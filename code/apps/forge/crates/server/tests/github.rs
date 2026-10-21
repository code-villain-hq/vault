//! The GitHub connection with signed webhook payloads like GitHub sends,
//! against a stand-in for GitHub's API.

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::body::Body;
use axum::extract::Query;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::routing::{get, post};
use common::App;
use forge_server::AppState;
use forge_server::github::GitHubApp;
use forge_server::github::login::GitHubLogin;
use hmac::{Hmac, KeyInit, Mac};
use jsonwebtoken::EncodingKey;
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "webhook-secret";
const INSTALLATION: i64 = 42;

struct Setup {
    app: App,
    admin: String,
    workspace: String,
    /// Pull request #7 as GitHub sees it.
    github: Arc<Mutex<PullRequest>>,
    /// Forge's side of the connection, to wait for what it does in the
    /// background.
    connection: GitHubApp,
    _data: tempfile::TempDir,
}

/// Pull request #7 on GitHub.
struct PullRequest {
    /// Whether it could be merged: null while GitHub hasn't worked it out.
    mergeable: Value,
    draft: bool,
    /// Its latest commit's check runs: (status, conclusion).
    checks: Vec<(&'static str, Option<&'static str>)>,
    /// How many times GitHub answers that its latest commit has no check
    /// runs before they start.
    checks_start_after: usize,
    /// The check runs of the commit it goes into, on main.
    main_checks: Vec<(&'static str, Option<&'static str>)>,
}

fn check_runs(runs: &[(&str, Option<&str>)]) -> Json<Value> {
    let runs: Vec<Value> = runs
        .iter()
        .map(|(status, conclusion)| json!({ "status": status, "conclusion": conclusion }))
        .collect();
    Json(json!({ "total_count": runs.len(), "check_runs": runs }))
}

/// The repositories installation 42 can read, with their default branches.
const REPOSITORIES: [(&str, &str); 3] = [
    ("villain/docs", "main"),
    ("villain/forge", "main"),
    ("villain/layer", "trunk"),
];

/// GitHub's API as far as installation 42, its repositories and pull
/// request #7 on villain/forge go, plus who can access installation 42:
/// OAuth code "owner" belongs to someone who can, "stranger" to someone
/// who can't, and "expired" is refused. Everything else, cloning included,
/// is "Not Found".
async fn fake_github(pr: Arc<Mutex<PullRequest>>) -> String {
    let checks = pr.clone();
    let main_checks = pr.clone();
    let mut app = axum::Router::new()
        .route(
            "/login/oauth/access_token",
            post(|Json(body): Json<Value>| async move {
                assert_eq!(body["client_id"], "client");
                assert_eq!(body["client_secret"], "client-secret");
                Json(match body["code"].as_str().unwrap() {
                    "expired" => json!({ "error": "bad_verification_code" }),
                    code => json!({ "access_token": format!("token-{code}") }),
                })
            }),
        )
        .route(
            "/user/installations",
            get(
                |headers: HeaderMap, Query(query): Query<HashMap<String, usize>>| async move {
                    // A full page of other accounts' installations first,
                    // then installation 42 for its owner.
                    let owner = headers[header::AUTHORIZATION] == "Bearer token-owner";
                    let ids: Vec<i64> = match query["page"] {
                        1 => (1000..1100).collect(),
                        2 if owner => vec![INSTALLATION],
                        _ => vec![],
                    };
                    let installations: Vec<Value> =
                        ids.iter().map(|id| json!({ "id": id })).collect();
                    Json(json!({ "total_count": 101, "installations": installations }))
                },
            ),
        )
        .route(
            "/app/installations/{id}/access_tokens",
            post(|| async { Json(json!({ "token": "installation-token" })) }),
        )
        .route(
            "/app/installations/{id}",
            get(|| async {
                Json(json!({ "id": INSTALLATION, "account": { "login": "villain" } }))
            }),
        )
        .route(
            "/installation/repositories",
            get(|Query(page): Query<HashMap<String, usize>>| async move {
                let (per_page, page) = (page["per_page"], page["page"]);
                let repositories: Vec<Value> = REPOSITORIES
                    .iter()
                    .skip((page - 1) * per_page)
                    .take(per_page)
                    .map(|(name, branch)| json!({ "full_name": name, "default_branch": branch }))
                    .collect();
                Json(json!({ "total_count": REPOSITORIES.len(), "repositories": repositories }))
            }),
        )
        .route(
            "/repos/villain/forge/pulls/7",
            get(move || async move {
                let pr = pr.lock().unwrap();
                Json(json!({
                    "number": 7, "state": "open", "merged": false, "draft": pr.draft,
                    "mergeable": pr.mergeable, "head": { "sha": "abc123" },
                    "base": { "sha": "main456" },
                }))
            }),
        )
        .route(
            "/repos/villain/forge/commits/abc123/check-runs",
            get(move || async move {
                let mut pr = checks.lock().unwrap();
                if pr.checks_start_after > 0 {
                    pr.checks_start_after -= 1;
                    return check_runs(&[]);
                }
                check_runs(&pr.checks)
            }),
        )
        .route(
            "/repos/villain/forge/commits/main456/check-runs",
            get(move || async move { check_runs(&main_checks.lock().unwrap().main_checks) }),
        )
        .route(
            "/repos/villain/forge/commits/abc123/status",
            get(|| async { Json(json!({ "state": "pending", "total_count": 0, "statuses": [] })) }),
        )
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "message": "Not Found" })),
            )
        });
    for (name, branch) in REPOSITORIES {
        app = app.route(
            &format!("/repos/{name}"),
            get(
                move || async move { Json(json!({ "full_name": name, "default_branch": branch })) },
            ),
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

async fn setup(pool: PgPool) -> Setup {
    setup_with(pool, false).await
}

/// With `sign_in`, the server also has GitHub sign-in (OAuth) set up.
async fn setup_with(pool: PgPool, sign_in: bool) -> Setup {
    let data = tempfile::tempdir().unwrap();
    let pr = Arc::new(Mutex::new(PullRequest {
        mergeable: json!(true),
        draft: false,
        checks: vec![],
        checks_start_after: 0,
        main_checks: vec![],
    }));
    let api = fake_github(pr.clone()).await;
    let github = GitHubApp::new(
        "1".into(),
        "forge-test".into(),
        EncodingKey::from_secret(b"shared with the stand-in"),
        SECRET.into(),
        data.path().to_path_buf(),
    )
    .with_test_api(api.clone());
    let mut state = AppState::new(pool.clone(), false).with_github(github.clone());
    if sign_in {
        let login = GitHubLogin::new("client".into(), "client-secret".into(), api.clone(), api);
        state = state.with_github_login(login);
    }
    let app = App::with_state(state);
    let (admin, workspace) = app.signup("Ada", "Forge").await;

    // As if the admin had installed the app on the "villain" organisation.
    sqlx::query("INSERT INTO github_installations (installation_id, workspace_id, account) VALUES ($1, $2, 'villain')")
        .bind(INSTALLATION)
        .bind(workspace.parse::<Uuid>().unwrap())
        .execute(&app.pool)
        .await
        .unwrap();

    let snapshot = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &admin)
        .await
        .body;
    let team = snapshot["records"]["Team"][0]["id"].clone();
    let state_id = |name: &str| {
        snapshot["records"]["WorkflowState"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()["id"]
            .clone()
    };
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": { "team_id": team, "title": "Rate limits", "state_id": state_id("Backlog") } },
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": { "team_id": team, "title": "Docs", "state_id": state_id("Todo") } },
    ]});
    assert_eq!(
        app.post(
            &format!("/api/w/{workspace}/sync/transactions"),
            Some(&admin),
            tx
        )
        .await
        .status,
        StatusCode::OK
    );
    Setup {
        app,
        admin,
        workspace,
        github: pr,
        connection: github,
        _data: data,
    }
}

fn signature(body: &[u8], secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

async fn deliver(s: &Setup, event: &str, payload: Value, secret: &str) -> StatusCode {
    let body = payload.to_string();
    let request = Request::builder()
        .method("POST")
        .uri("/api/github/webhook")
        .header("content-type", "application/json")
        .header("x-github-event", event)
        .header("x-hub-signature-256", signature(body.as_bytes(), secret))
        .body(Body::from(body))
        .unwrap();
    let status = s
        .app
        .router
        .clone()
        .oneshot(request)
        .await
        .unwrap()
        .status();
    // What the delivery set off, such as asking GitHub whether a pull
    // request could be merged, is part of its outcome.
    s.connection.idle().await;
    status
}

fn pull_request(action: &str, state: &str, merged: bool) -> Value {
    json!({
        "action": action,
        "installation": { "id": INSTALLATION },
        "repository": { "full_name": "villain/forge" },
        "pull_request": {
            "number": 7,
            "title": "Rate limits and docs",
            "body": "Fixes FOR-2. Adds a limiter.",
            "html_url": "https://github.com/villain/forge/pull/7",
            "state": state,
            "merged": merged,
            "head": { "ref": "for-1-rate-limits" },
            "user": { "login": "ada" },
        },
    })
}

/// (identifier number, status name) of every issue.
async fn statuses(s: &Setup) -> Vec<(i64, String)> {
    let snapshot = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    let states = snapshot["records"]["WorkflowState"]
        .as_array()
        .unwrap()
        .clone();
    let mut out: Vec<(i64, String)> = snapshot["records"]["Issue"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let state = states.iter().find(|s| s["id"] == i["state_id"]).unwrap();
            (
                i["number"].as_i64().unwrap(),
                state["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    out.sort();
    out
}

async fn links(s: &Setup) -> Vec<Value> {
    let snapshot = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    snapshot["records"]["IssueLink"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// (identifier number, whether the link works on it) of every link.
async fn works_on(s: &Setup) -> Vec<(i64, bool)> {
    let snapshot = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    let issues = snapshot["records"]["Issue"].as_array().unwrap().clone();
    let mut out: Vec<(i64, bool)> = snapshot["records"]["IssueLink"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let issue = issues.iter().find(|i| i["id"] == l["issue_id"]).unwrap();
            (
                issue["number"].as_i64().unwrap(),
                l["works_on"].as_bool().unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

#[sqlx::test(migrations = "../../migrations")]
async fn pull_requests_link_and_move_the_issues_they_work_on(pool: PgPool) {
    let s = setup(pool).await;

    assert_eq!(
        deliver(
            &s,
            "pull_request",
            pull_request("opened", "open", false),
            SECRET
        )
        .await,
        StatusCode::NO_CONTENT
    );
    // Opened for review, and it could be merged: committed work waiting for
    // someone to check it.
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
    let opened = links(&s).await;
    assert_eq!(opened.len(), 2, "one link per mentioned issue");
    assert!(
        opened
            .iter()
            .all(|l| l["state"] == "open" && l["kind"] == "pull_request" && l["number"] == 7)
    );

    assert_eq!(
        deliver(
            &s,
            "pull_request",
            pull_request("closed", "closed", true),
            SECRET
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(statuses(&s).await, [(1, "Done".into()), (2, "Done".into())]);
    let merged = links(&s).await;
    assert_eq!(merged.len(), 2, "updated, not duplicated");
    assert!(merged.iter().all(|l| l["state"] == "merged"));

    // The GitHub agent did it, so history says so.
    let actors: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT u.name FROM sync_log l JOIN users u ON u.id = l.actor_id WHERE l.model = 'IssueLink'",
    )
    .fetch_all(&s.app.pool)
    .await
    .unwrap();
    assert_eq!(actors, ["GitHub"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_pull_request_that_only_mentions_an_issue_links_it_but_leaves_it_alone(pool: PgPool) {
    let s = setup(pool).await;
    let mention = |action: &str, state: &str, merged: bool| {
        let mut pr = pull_request(action, state, merged);
        pr["pull_request"]["title"] = json!("Rate limits (FOR-1, see FOR-2)");
        pr["pull_request"]["body"] =
            json!("Fixes the limiter. Follow-up filed as FOR-2; fixing FOR-2 comes later.");
        pr
    };

    deliver(&s, "pull_request", mention("opened", "open", false), SECRET).await;
    // FOR-1 is in the branch name; FOR-2 is only mentioned.
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "Todo".into())]
    );
    assert_eq!(works_on(&s).await, [(1, true), (2, false)], "both linked");

    deliver(
        &s,
        "pull_request",
        mention("closed", "closed", true),
        SECRET,
    )
    .await;
    assert_eq!(statuses(&s).await, [(1, "Done".into()), (2, "Todo".into())]);
    assert!(links(&s).await.iter().all(|l| l["state"] == "merged"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn editing_a_pull_request_changes_which_issues_it_works_on(pool: PgPool) {
    let s = setup(pool).await;
    let mut pr = pull_request("opened", "open", false);
    pr["pull_request"]["body"] = json!("See FOR-2.");
    deliver(&s, "pull_request", pr.clone(), SECRET).await;
    assert_eq!(works_on(&s).await, [(1, true), (2, false)]);

    pr["action"] = json!("edited");
    pr["pull_request"]["body"] = json!("Fixes FOR-2 as well.");
    deliver(&s, "pull_request", pr.clone(), SECRET).await;
    assert_eq!(works_on(&s).await, [(1, true), (2, true)]);

    pr["pull_request"]["body"] = json!("FOR-2 comes later.");
    deliver(&s, "pull_request", pr, SECRET).await;
    assert_eq!(works_on(&s).await, [(1, true), (2, false)]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn closing_without_merging_sends_the_issue_back_to_work(pool: PgPool) {
    let s = setup(pool).await;
    let agent = agent_on_issue_1(&s).await;
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
    // Someone took FOR-2 out of review already.
    let todo = state_id(&s, "Todo").await;
    update_issue(&s, 2, json!({ "state_id": todo })).await;

    deliver(
        &s,
        "pull_request",
        pull_request("closed", "closed", false),
        SECRET,
    )
    .await;
    // Nothing waits for review anymore; FOR-2 stays where it was put.
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "Todo".into())]
    );
    assert!(links(&s).await.iter().all(|l| l["state"] == "closed"));
    assert_eq!(
        wakeups(&s, agent).await,
        [("assigned".into(), 1), ("sent_back".into(), 1)]
    );

    // Reopened, it asks for review again.
    deliver(
        &s,
        "pull_request",
        pull_request("reopened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn turning_a_pull_request_back_into_a_draft_sends_the_issue_back_to_work(pool: PgPool) {
    let s = setup(pool).await;
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );

    s.github.lock().unwrap().draft = true;
    let mut draft = pull_request("converted_to_draft", "open", false);
    draft["pull_request"]["draft"] = json!(true);
    deliver(&s, "pull_request", draft, SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );
    // The default branch moving doesn't put a draft's issues back in review.
    deliver(&s, "push", push_to_main(), SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );

    s.github.lock().unwrap().draft = false;
    deliver(
        &s,
        "pull_request",
        pull_request("ready_for_review", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_draft_pull_request_waits_until_it_is_ready_for_review(pool: PgPool) {
    let s = setup(pool).await;
    s.github.lock().unwrap().draft = true;
    let mut draft = pull_request("opened", "open", false);
    draft["pull_request"]["draft"] = json!(true);
    deliver(&s, "pull_request", draft, SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );

    s.github.lock().unwrap().draft = false;
    deliver(
        &s,
        "pull_request",
        pull_request("ready_for_review", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn pushes_link_commits(pool: PgPool) {
    let s = setup(pool).await;
    let push = json!({
        "ref": "refs/heads/feature",
        "installation": { "id": INSTALLATION },
        "repository": { "full_name": "villain/forge", "default_branch": "main" },
        "commits": [{
            "id": "a1b2c3d4", "message": "Fix the limiter for FOR-1\n\nDetails.", "url": "https://github.com/villain/forge/commit/a1b2c3d4",
            "author": { "username": "ada" }, "added": [], "modified": ["src/limits.rs"], "removed": [],
        }],
    });
    assert_eq!(
        deliver(&s, "push", push, SECRET).await,
        StatusCode::NO_CONTENT
    );

    let links = links(&s).await;
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["kind"], "commit");
    assert_eq!(links[0]["title"], "Fix the limiter for FOR-1");
    assert_eq!(links[0]["sha"], "a1b2c3d4");
    assert_eq!(links[0]["works_on"], false, "no closing word");
    assert_eq!(
        statuses(&s).await,
        [(1, "Backlog".into()), (2, "Todo".into())],
        "a mention moves nothing"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_closing_commit_only_links_its_issue(pool: PgPool) {
    let s = setup(pool).await;
    let push = |to: &str, sha: &str| {
        json!({
            "ref": format!("refs/heads/{to}"),
            "installation": { "id": INSTALLATION },
            "repository": { "full_name": "villain/forge", "default_branch": "main" },
            "commits": [{
                "id": sha, "message": "Fixes FOR-1, see FOR-2", "url": format!("https://github.com/villain/forge/commit/{sha}"),
                "author": { "username": "ada" }, "added": [], "modified": [], "removed": [],
            }],
        })
    };

    deliver(&s, "push", push("for-1-rate-limits", "a1"), SECRET).await;
    deliver(&s, "push", push("main", "b2"), SECRET).await;
    // Pushing "Fixes FOR-1" can't skip review: Done comes with a merged pull request.
    assert_eq!(
        statuses(&s).await,
        [(1, "Backlog".into()), (2, "Todo".into())]
    );
    assert_eq!(links(&s).await.len(), 4, "each commit linked to both");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_push_to_the_default_branch_marks_the_services_it_changed(pool: PgPool) {
    let s = setup(pool).await;
    let (api, docs) = (Uuid::now_v7(), Uuid::now_v7());
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Service", "id": api, "data": { "name": "API", "repository": "github:villain/forge", "paths": ["src/**"] } },
        { "action": "create", "model": "Service", "id": docs, "data": { "name": "Docs", "repository": "github:villain/forge", "paths": ["docs/**"] } },
    ]});
    s.app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;
    let push = |to: &str| {
        json!({
            "ref": format!("refs/heads/{to}"),
            "installation": { "id": INSTALLATION },
            "repository": { "full_name": "villain/forge", "default_branch": "main" },
            "commits": [{
                "id": "a1", "message": "Tune the limiter", "url": "https://github.com/villain/forge/commit/a1",
                "author": { "username": "ada" }, "added": [], "modified": ["src/limits.rs"], "removed": [],
            }],
        })
    };
    let changed = async || -> Vec<(String, bool)> {
        sqlx::query_as("SELECT name, code_changed_at IS NOT NULL FROM services ORDER BY name")
            .fetch_all(&s.app.pool)
            .await
            .unwrap()
    };

    deliver(&s, "push", push("feature"), SECRET).await;
    assert_eq!(
        changed().await,
        [("API".into(), false), ("Docs".into(), false)],
        "not on the default branch"
    );
    deliver(&s, "push", push("main"), SECRET).await;
    assert_eq!(
        changed().await,
        [("API".into(), true), ("Docs".into(), false)]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn webhooks_must_be_signed_and_for_a_linked_installation(pool: PgPool) {
    let s = setup(pool).await;
    assert_eq!(
        deliver(
            &s,
            "pull_request",
            pull_request("opened", "open", false),
            "wrong"
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    assert!(links(&s).await.is_empty());

    let mut elsewhere = pull_request("opened", "open", false);
    elsewhere["installation"]["id"] = json!(99);
    assert_eq!(
        deliver(&s, "pull_request", elsewhere, SECRET).await,
        StatusCode::NO_CONTENT
    );
    assert!(links(&s).await.is_empty(), "not our installation");
}

#[sqlx::test(migrations = "../../migrations")]
async fn installations_track_their_repositories(pool: PgPool) {
    let s = setup(pool).await;
    let added = json!({
        "action": "added",
        "installation": { "id": INSTALLATION },
        "repositories_added": [{ "full_name": "villain/forge" }, { "full_name": "villain/layer" }],
        "repositories_removed": [],
    });
    deliver(&s, "installation_repositories", added, SECRET).await;
    let removed = json!({
        "action": "removed",
        "installation": { "id": INSTALLATION },
        "repositories_added": [],
        "repositories_removed": [{ "full_name": "villain/layer" }],
    });
    deliver(&s, "installation_repositories", removed, SECRET).await;

    let status = s
        .app
        .get(&format!("/api/w/{}/github", s.workspace), &s.admin)
        .await
        .body;
    assert_eq!(status["configured"], true);
    assert!(
        status["install_url"]
            .as_str()
            .unwrap()
            .starts_with("https://github.com/apps/forge-test/installations/new?state=")
    );
    assert_eq!(status["installations"][0]["account"], "villain");
    let repos: Vec<&str> = status["installations"][0]["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["full_name"].as_str().unwrap())
        .collect();
    assert_eq!(repos, ["villain/forge"]);

    deliver(
        &s,
        "installation",
        json!({ "action": "deleted", "installation": { "id": INSTALLATION } }),
        SECRET,
    )
    .await;
    let status = s
        .app
        .get(&format!("/api/w/{}/github", s.workspace), &s.admin)
        .await
        .body;
    assert!(status["installations"].as_array().unwrap().is_empty());
}

/// Each repository Forge knows about: (owner/name, default branch).
async fn repositories(s: &Setup) -> Vec<(String, String)> {
    sqlx::query_as("SELECT full_name, default_branch FROM github_repositories ORDER BY full_name")
        .fetch_all(&s.app.pool)
        .await
        .unwrap()
}

fn repository(name: &str, branch: &str) -> (String, String) {
    (name.to_string(), branch.to_string())
}

/// Whether Forge has tried to clone a repository: the stand-in for GitHub
/// can't be cloned from, so every try ends in an error.
async fn tried_to_clone(s: &Setup, name: &str) -> bool {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT mirror_error FROM github_repositories WHERE full_name = $1",
    )
    .bind(name)
    .fetch_one(&s.app.pool)
    .await
    .unwrap()
    .is_some()
}

#[sqlx::test(migrations = "../../migrations")]
async fn repositories_added_later_get_their_real_default_branch_and_a_clone(pool: PgPool) {
    let s = setup(pool).await;
    let added = json!({
        "action": "added",
        "installation": { "id": INSTALLATION },
        "repositories_added": [{ "full_name": "villain/layer" }],
        "repositories_removed": [],
    });
    deliver(&s, "installation_repositories", added, SECRET).await;
    assert_eq!(
        repositories(&s).await,
        [repository("villain/layer", "trunk")]
    );
    assert!(tried_to_clone(&s, "villain/layer").await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_installation_event_leaves_known_default_branches_alone(pool: PgPool) {
    let s = setup(pool).await;
    sqlx::query("INSERT INTO github_repositories (installation_id, full_name, default_branch) VALUES ($1, 'villain/forge', 'develop')")
        .bind(INSTALLATION)
        .execute(&s.app.pool)
        .await
        .unwrap();
    let event = json!({
        "action": "new_permissions_accepted",
        "installation": { "id": INSTALLATION },
        "repositories": [{ "full_name": "villain/forge" }],
    });
    deliver(&s, "installation", event, SECRET).await;
    assert_eq!(
        repositories(&s).await,
        [repository("villain/forge", "develop")]
    );
    assert!(
        !tried_to_clone(&s, "villain/forge").await,
        "known repositories aren't cloned again"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_push_corrects_the_stored_default_branch(pool: PgPool) {
    let s = setup(pool).await;
    sqlx::query("INSERT INTO github_repositories (installation_id, full_name, default_branch) VALUES ($1, 'villain/forge', 'develop')")
        .bind(INSTALLATION)
        .execute(&s.app.pool)
        .await
        .unwrap();
    deliver(&s, "push", push_to_main(), SECRET).await;
    assert_eq!(
        repositories(&s).await,
        [repository("villain/forge", "main")]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn updating_the_copies_asks_github_for_the_default_branch(pool: PgPool) {
    let s = setup(pool).await;
    // As recorded before Forge asked: a guess.
    sqlx::query("INSERT INTO github_repositories (installation_id, full_name, default_branch) VALUES ($1, 'villain/layer', 'main')")
        .bind(INSTALLATION)
        .execute(&s.app.pool)
        .await
        .unwrap();
    let response = s
        .app
        .post(
            &format!("/api/w/{}/github/refresh", s.workspace),
            Some(&s.admin),
            json!({}),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    s.connection.idle().await;
    assert_eq!(
        repositories(&s).await,
        [repository("villain/layer", "trunk")]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn connecting_records_every_page_of_repositories(pool: PgPool) {
    let s = setup(pool).await;
    let state = install_state(&s).await;
    let response = s
        .app
        .get(
            &format!("/api/github/setup?installation_id={INSTALLATION}&state={state}"),
            &s.admin,
        )
        .await;
    assert!(response.status.is_redirection(), "{}", response.status);
    // Two to a page with the stand-in, so this takes two.
    assert_eq!(
        repositories(&s).await,
        [
            repository("villain/docs", "main"),
            repository("villain/forge", "main"),
            repository("villain/layer", "trunk"),
        ]
    );
}

/// The signed `state` from the admin's install link.
async fn install_state(s: &Setup) -> String {
    let status = s
        .app
        .get(&format!("/api/w/{}/github", s.workspace), &s.admin)
        .await
        .body;
    let install_url = status["install_url"].as_str().unwrap();
    install_url.split_once("state=").unwrap().1.to_string()
}

/// The admin comes back from installing the app on GitHub, which adds
/// `extra` (such as the OAuth code) to the setup URL. Installation 42
/// isn't linked before.
async fn finish_installing(s: &Setup, extra: &str) -> common::Response {
    sqlx::query("DELETE FROM github_installations")
        .execute(&s.app.pool)
        .await
        .unwrap();
    let state = install_state(s).await;
    let response = s
        .app
        .get(
            &format!("/api/github/setup?installation_id={INSTALLATION}&state={state}{extra}"),
            &s.admin,
        )
        .await;
    s.connection.idle().await;
    response
}

/// Whether installation 42 is linked to the workspace.
async fn linked(s: &Setup) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM github_installations WHERE installation_id = $1 AND workspace_id = $2)",
    )
    .bind(INSTALLATION)
    .bind(s.workspace.parse::<Uuid>().unwrap())
    .fetch_one(&s.app.pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_github_sign_in_the_installer_must_be_able_to_access_the_installation(pool: PgPool) {
    let s = setup_with(pool, true).await;
    let status = s
        .app
        .get(&format!("/api/w/{}/github", s.workspace), &s.admin)
        .await
        .body;
    assert_eq!(status["confirms_ownership"], true);

    // GitHub lists installation 42 for its owner, on the second page.
    let response = finish_installing(&s, "&code=owner&setup_action=install").await;
    assert!(response.status.is_redirection(), "{}", response.body);
    assert!(linked(&s).await);
    assert_eq!(repositories(&s).await.len(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_installation_the_installer_cannot_access_is_not_linked(pool: PgPool) {
    let s = setup_with(pool, true).await;
    let response = finish_installing(&s, "&code=stranger&setup_action=install").await;
    assert_eq!(response.status, StatusCode::FORBIDDEN, "{}", response.body);
    assert!(
        response.body["message"]
            .as_str()
            .unwrap()
            .contains("can't access that installation")
    );
    assert!(!linked(&s).await);

    let response = finish_installing(&s, "&code=expired&setup_action=install").await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!linked(&s).await, "GitHub refused the code");
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_github_sign_in_an_installation_needs_github_to_say_who_installed_it(pool: PgPool) {
    let s = setup_with(pool, true).await;
    let response = finish_installing(&s, "&setup_action=install").await;
    assert_eq!(
        response.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        response.body
    );
    assert!(
        response.body["message"]
            .as_str()
            .unwrap()
            .contains("Request user authorization (OAuth) during installation")
    );
    assert!(!linked(&s).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_github_sign_in_an_admin_links_installations_as_before(pool: PgPool) {
    let s = setup(pool).await;
    let status = s
        .app
        .get(&format!("/api/w/{}/github", s.workspace), &s.admin)
        .await
        .body;
    assert_eq!(status["confirms_ownership"], false);
    let response = finish_installing(&s, "&setup_action=install").await;
    assert!(response.status.is_redirection(), "{}", response.body);
    assert!(linked(&s).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn installs_that_github_sends_to_the_sign_in_callback_go_on_to_setup(pool: PgPool) {
    let s = setup_with(pool, true).await;
    sqlx::query("DELETE FROM github_installations")
        .execute(&s.app.pool)
        .await
        .unwrap();
    // With "Request user authorization (OAuth) during installation" on,
    // GitHub comes back to the OAuth callback instead of the setup URL.
    let query = format!(
        "code=owner&installation_id={INSTALLATION}&setup_action=install&state={}",
        install_state(&s).await
    );
    let request = Request::get(format!("/api/auth/github/callback?{query}"))
        .header(header::COOKIE, format!("forge_session={}", s.admin))
        .body(Body::empty())
        .unwrap();
    let response = s.app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    assert_eq!(location, format!("/api/github/setup?{query}"));

    let response = s.app.get(location, &s.admin).await;
    s.connection.idle().await;
    assert!(response.status.is_redirection(), "{}", response.body);
    assert!(linked(&s).await);
}

/// (identifier number, blocker) of every pull request link.
async fn blockers(s: &Setup) -> Vec<(i64, Value)> {
    let snapshot = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    let issues = snapshot["records"]["Issue"].as_array().unwrap().clone();
    let mut out: Vec<(i64, Value)> = snapshot["records"]["IssueLink"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let issue = issues.iter().find(|i| i["id"] == l["issue_id"]).unwrap();
            (issue["number"].as_i64().unwrap(), l["blocker"].clone())
        })
        .collect();
    out.sort_by_key(|(number, _)| *number);
    out
}

/// One update to issue FOR-`number`, as the admin.
async fn update_issue(s: &Setup, number: i64, data: Value) {
    let snapshot = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    let issue = snapshot["records"]["Issue"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["number"] == number)
        .unwrap()["id"]
        .clone();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "update", "model": "Issue", "id": issue, "data": data },
    ]});
    let response = s
        .app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
}

async fn state_id(s: &Setup, name: &str) -> Value {
    let snapshot = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    snapshot["records"]["WorkflowState"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap()["id"]
        .clone()
}

/// Adds the agent "Claude Code" with a runner and assigns FOR-1 to it;
/// returns the agent's id.
async fn agent_on_issue_1(s: &Setup) -> Uuid {
    let base = format!("/api/w/{}", s.workspace);
    let created = s
        .app
        .post(
            &format!("{base}/agents"),
            Some(&s.admin),
            json!({ "name": "Claude Code" }),
        )
        .await;
    let agent = created.body["agent_id"].as_str().unwrap().to_string();
    let runner = s
        .app
        .call(
            "PUT",
            &format!("{base}/agents/{agent}/runner"),
            Some(&s.admin),
            Some(json!({ "url": "http://127.0.0.1:9/wake" })),
        )
        .await;
    assert_eq!(runner.status, StatusCode::OK, "{}", runner.body);
    update_issue(s, 1, json!({ "assignee_id": agent })).await;
    agent.parse().unwrap()
}

/// The agent's wake-ups, oldest first: (event, issue number).
async fn wakeups(s: &Setup, agent: Uuid) -> Vec<(String, i32)> {
    sqlx::query_as(
        "SELECT w.event, i.number FROM agent_wakeups w JOIN issues i ON i.id = w.issue_id \
         WHERE w.user_id = $1 ORDER BY w.created_at, w.id",
    )
    .bind(agent)
    .fetch_all(&s.app.pool)
    .await
    .unwrap()
}

fn push_to_main() -> Value {
    json!({
        "ref": "refs/heads/main",
        "installation": { "id": INSTALLATION },
        "repository": { "full_name": "villain/forge", "default_branch": "main" },
        "commits": [],
    })
}

fn checks_finished(event: &str) -> Value {
    json!({
        "action": "completed",
        "installation": { "id": INSTALLATION },
        "repository": { "full_name": "villain/forge" },
        event: { "status": "completed", "pull_requests": [{ "number": 7 }] },
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_pull_request_with_conflicts_waits_until_they_are_resolved(pool: PgPool) {
    let s = setup(pool).await;
    s.github.lock().unwrap().mergeable = json!(false);
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        blockers(&s).await,
        [(1, json!("conflicts")), (2, json!("conflicts"))]
    );
    // Started, but not ready for review.
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );

    // The author merges the default branch in and pushes.
    s.github.lock().unwrap().mergeable = json!(true);
    deliver(
        &s,
        "pull_request",
        pull_request("synchronize", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
    assert_eq!(blockers(&s).await, [(1, Value::Null), (2, Value::Null)]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_conflict_after_the_default_branch_moves_sends_the_issue_back_to_its_agent(pool: PgPool) {
    let s = setup(pool).await;
    let agent = agent_on_issue_1(&s).await;
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );

    // Another pull request lands, and #7 now conflicts with it.
    s.github.lock().unwrap().mergeable = json!(false);
    deliver(&s, "push", push_to_main(), SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );
    assert_eq!(
        blockers(&s).await,
        [(1, json!("conflicts")), (2, json!("conflicts"))]
    );
    // Woken once, saying why: not also as "sent back".
    assert_eq!(
        wakeups(&s, agent).await,
        [("assigned".into(), 1), ("pr_blocked".into(), 1)]
    );

    // Resolved: back in review, without waking it again.
    s.github.lock().unwrap().mergeable = json!(true);
    deliver(
        &s,
        "pull_request",
        pull_request("synchronize", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
    assert_eq!(wakeups(&s, agent).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn checks_must_pass_before_review_and_failing_ones_send_it_back(pool: PgPool) {
    let s = setup(pool).await;
    s.github.lock().unwrap().checks = vec![("in_progress", None), ("completed", Some("success"))];
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        blockers(&s).await,
        [(1, json!("checks_running")), (2, json!("checks_running"))]
    );
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );

    s.github.lock().unwrap().checks = vec![
        ("completed", Some("success")),
        ("completed", Some("skipped")),
    ];
    deliver(&s, "check_suite", checks_finished("check_suite"), SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );

    s.github.lock().unwrap().checks = vec![("completed", Some("failure"))];
    deliver(&s, "check_run", checks_finished("check_run"), SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );
    assert_eq!(
        blockers(&s).await,
        [(1, json!("checks_failed")), (2, json!("checks_failed"))]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_issue_sent_back_by_a_person_waits_for_its_assignee(pool: PgPool) {
    let s = setup(pool).await;
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
    let working = state_id(&s, "In Progress").await;
    update_issue(&s, 1, json!({ "state_id": working })).await;

    // The assignee pushes a first fix; its checks run, then pass.
    s.github.lock().unwrap().checks = vec![("queued", None)];
    deliver(
        &s,
        "pull_request",
        pull_request("synchronize", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        blockers(&s).await,
        [(1, json!("checks_running")), (2, json!("checks_running"))]
    );
    s.github.lock().unwrap().checks = vec![("completed", Some("success"))];
    deliver(&s, "check_suite", checks_finished("check_suite"), SECRET).await;
    assert_eq!(blockers(&s).await, [(1, Value::Null), (2, Value::Null)]);

    // Running checks didn't send FOR-2 back, and passing ones don't hand
    // FOR-1 over: its assignee does that.
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Review".into())]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn checks_that_have_not_started_yet_hold_the_issue_back(pool: PgPool) {
    let s = setup(pool).await;
    // The repository runs checks, but GitHub hasn't started the pull
    // request's yet when it opens.
    {
        let mut github = s.github.lock().unwrap();
        github.main_checks = vec![("completed", Some("success"))];
        github.checks_start_after = 2;
        github.checks = vec![("queued", None)];
    }
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        blockers(&s).await,
        [(1, json!("checks_running")), (2, json!("checks_running"))]
    );
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );

    s.github.lock().unwrap().checks = vec![("completed", Some("success"))];
    deliver(&s, "check_suite", checks_finished("check_suite"), SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
    assert_eq!(blockers(&s).await, [(1, Value::Null), (2, Value::Null)]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_pull_request_whose_checks_never_start_still_goes_to_review(pool: PgPool) {
    let s = setup(pool).await;
    s.github.lock().unwrap().main_checks = vec![("completed", Some("success"))];
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    // Forge waited a while for them, then let it through.
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
    assert_eq!(blockers(&s).await, [(1, Value::Null), (2, Value::Null)]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_commit_status_changing_asks_github_again(pool: PgPool) {
    let s = setup(pool).await;
    s.github.lock().unwrap().checks = vec![("in_progress", None)];
    deliver(
        &s,
        "pull_request",
        pull_request("opened", "open", false),
        SECRET,
    )
    .await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Progress".into()), (2, "In Progress".into())]
    );

    // A status event says only which commit; Forge asks about the open pull requests.
    s.github.lock().unwrap().checks = vec![("completed", Some("success"))];
    let status = json!({
        "installation": { "id": INSTALLATION },
        "repository": { "full_name": "villain/forge" },
        "sha": "abc123", "state": "success",
    });
    deliver(&s, "status", status, SECRET).await;
    assert_eq!(
        statuses(&s).await,
        [(1, "In Review".into()), (2, "In Review".into())]
    );
}
