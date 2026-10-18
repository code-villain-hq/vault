//! The Forgejo connection against a stand-in for Forgejo's API: connecting
//! it, then checks that link pull requests and move the issues they work on.

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use chrono::{Duration, Utc};
use common::App;
use forge_server::{AppState, forgejo};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const TOKEN: &str = "read-token";
const REPOSITORY: &str = "code-villain/vault";

/// What the stand-in Forgejo knows.
#[derive(Default)]
struct Forgejo {
    /// Pull requests on code-villain/vault, as Forgejo describes them.
    pull_requests: Vec<Value>,
    /// Commit statuses per commit.
    statuses: HashMap<String, Vec<&'static str>>,
    /// Answers 503 to everything.
    down: bool,
}

type Shared = Arc<Mutex<Forgejo>>;

fn refuse(headers: &HeaderMap, forgejo: &Forgejo) -> Option<Response> {
    if forgejo.down {
        return Some(StatusCode::SERVICE_UNAVAILABLE.into_response());
    }
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        != Some(&format!("token {TOKEN}"))
    {
        return Some(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "message": "invalid token" })),
            )
                .into_response(),
        );
    }
    None
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "message": "The target couldn't be found." })),
    )
        .into_response()
}

async fn fake_forgejo(shared: Shared) -> String {
    let repo = shared.clone();
    let list = shared.clone();
    let one = shared.clone();
    let status = shared.clone();
    let app = axum::Router::new()
        .route(
            "/api/v1/repos/{owner}/{name}",
            get(
                move |headers: HeaderMap, Path((owner, name)): Path<(String, String)>| async move {
                    let forgejo = repo.lock().unwrap();
                    if let Some(refused) = refuse(&headers, &forgejo) {
                        return refused;
                    }
                    if format!("{owner}/{name}") != REPOSITORY {
                        return not_found();
                    }
                    Json(json!({ "full_name": REPOSITORY })).into_response()
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{name}/pulls",
            get(
                move |headers: HeaderMap, Query(query): Query<HashMap<String, String>>| async move {
                    let forgejo = list.lock().unwrap();
                    if let Some(refused) = refuse(&headers, &forgejo) {
                        return refused;
                    }
                    if query["page"] != "1" {
                        return Json(json!([])).into_response();
                    }
                    let mut found: Vec<Value> = forgejo
                        .pull_requests
                        .iter()
                        .filter(|pr| pr["state"] == query["state"].as_str())
                        .cloned()
                        .collect();
                    found.sort_by(|a, b| {
                        b["updated_at"].as_str().cmp(&a["updated_at"].as_str())
                    });
                    Json(Value::Array(found)).into_response()
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{name}/pulls/{number}",
            get(
                move |headers: HeaderMap, Path((_, _, number)): Path<(String, String, i64)>| async move {
                    let forgejo = one.lock().unwrap();
                    if let Some(refused) = refuse(&headers, &forgejo) {
                        return refused;
                    }
                    match forgejo.pull_requests.iter().find(|pr| pr["number"] == number) {
                        Some(pr) => Json(pr.clone()).into_response(),
                        None => not_found(),
                    }
                },
            ),
        )
        .route(
            "/api/v1/repos/{owner}/{name}/commits/{sha}/status",
            get(
                move |headers: HeaderMap, Path((_, _, sha)): Path<(String, String, String)>| async move {
                    let forgejo = status.lock().unwrap();
                    if let Some(refused) = refuse(&headers, &forgejo) {
                        return refused;
                    }
                    let statuses: Vec<Value> = forgejo
                        .statuses
                        .get(&sha)
                        .into_iter()
                        .flatten()
                        .map(|s| json!({ "status": s, "context": "ci" }))
                        .collect();
                    Json(json!({ "state": "", "statuses": statuses })).into_response()
                },
            ),
        )
        .fallback(|| async { not_found() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

struct Setup {
    app: App,
    admin: String,
    workspace: String,
    server: String,
    forgejo: Shared,
}

/// A workspace with FOR-1 in Backlog and FOR-2 in Todo, and a stand-in
/// Forgejo, not yet connected.
async fn setup(pool: PgPool) -> Setup {
    let forgejo = Shared::default();
    let server = fake_forgejo(forgejo.clone()).await;
    let app = App::with_state(AppState::new(pool, false));
    let (admin, workspace) = app.signup("Ada", "Forge").await;
    let snapshot = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &admin)
        .await
        .body;
    let team = snapshot["records"]["Team"][0]["id"].clone();
    let backlog = state_id(&snapshot, "Backlog");
    let todo = state_id(&snapshot, "Todo");
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": { "team_id": team, "title": "Rate limits", "state_id": backlog } },
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": { "team_id": team, "title": "Docs", "state_id": todo } },
    ]});
    let response = app
        .post(
            &format!("/api/w/{workspace}/sync/transactions"),
            Some(&admin),
            tx,
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    Setup {
        app,
        admin,
        workspace,
        server,
        forgejo,
    }
}

fn state_id(snapshot: &Value, name: &str) -> Value {
    snapshot["records"]["WorkflowState"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap()["id"]
        .clone()
}

impl Setup {
    async fn connect(&self) {
        let response = self
            .app
            .call(
                "PUT",
                &format!("/api/w/{}/forgejo", self.workspace),
                Some(&self.admin),
                Some(
                    json!({ "server": self.server, "repositories": [REPOSITORY], "token": TOKEN }),
                ),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    }

    async fn check(&self) {
        forgejo::check(&self.app.pool, self.workspace.parse().unwrap())
            .await
            .unwrap();
    }

    fn add(&self, pr: Value) {
        self.forgejo.lock().unwrap().pull_requests.push(pr);
    }

    /// Changes pull request `number` on Forgejo.
    fn change(&self, number: i64, change: impl FnOnce(&mut Value)) {
        let mut forgejo = self.forgejo.lock().unwrap();
        let pr = forgejo
            .pull_requests
            .iter_mut()
            .find(|pr| pr["number"] == number)
            .unwrap();
        change(pr);
        pr["updated_at"] = json!(Utc::now().to_rfc3339());
    }

    fn merge(&self, number: i64) {
        self.change(number, |pr| {
            pr["state"] = json!("closed");
            pr["merged"] = json!(true);
            pr["merged_at"] = json!(Utc::now().to_rfc3339());
            pr["closed_at"] = json!(Utc::now().to_rfc3339());
        });
    }

    fn close(&self, number: i64) {
        self.change(number, |pr| {
            pr["state"] = json!("closed");
            pr["closed_at"] = json!(Utc::now().to_rfc3339());
        });
    }

    async fn snapshot(&self) -> Value {
        self.app
            .get(
                &format!("/api/w/{}/sync/bootstrap", self.workspace),
                &self.admin,
            )
            .await
            .body
    }

    /// (identifier number, status name) of every issue.
    async fn statuses(&self) -> Vec<(i64, String)> {
        let snapshot = self.snapshot().await;
        let states = snapshot["records"]["WorkflowState"].as_array().unwrap();
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

    /// Every link, as (issue number, pull request number, link).
    async fn links(&self) -> Vec<(i64, i64, Value)> {
        let snapshot = self.snapshot().await;
        let issues = snapshot["records"]["Issue"].as_array().unwrap();
        let mut out: Vec<(i64, i64, Value)> = snapshot["records"]["IssueLink"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let issue = issues.iter().find(|i| i["id"] == l["issue_id"]).unwrap();
                (
                    issue["number"].as_i64().unwrap(),
                    l["number"].as_i64().unwrap(),
                    l.clone(),
                )
            })
            .collect();
        out.sort_by_key(|(issue, number, _)| (*issue, *number));
        out
    }

    async fn status(&self, session: &str) -> Value {
        let response = self
            .app
            .get(&format!("/api/w/{}/forgejo", self.workspace), session)
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        response.body
    }
}

/// An open pull request pushed with `refs/for/main` (AGit: no branch).
fn pull_request(number: i64, title: &str, body: &str) -> Value {
    let now = Utc::now().to_rfc3339();
    json!({
        "number": number,
        "title": title,
        "body": body,
        "html_url": format!("https://git.codevillain.eu/{REPOSITORY}/pulls/{number}"),
        "state": "open",
        "merged": false,
        "merged_at": null,
        "closed_at": null,
        "draft": false,
        "mergeable": true,
        "flow": 1,
        "head": { "ref": format!("refs/pull/{number}/head"), "sha": format!("{number:040x}") },
        "user": { "login": "code-villain" },
        "created_at": now,
        "updated_at": now,
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn admins_connect_forgejo_and_nobody_sees_the_token(pool: PgPool) {
    let s = setup(pool).await;
    let path = format!("/api/w/{}/forgejo", s.workspace);
    assert_eq!(s.status(&s.admin).await["connected"], false);

    // Forgejo doesn't answer for one of the repositories: nothing is saved.
    let refused = s
        .app
        .call(
            "PUT",
            &path,
            Some(&s.admin),
            Some(json!({ "server": s.server, "repositories": [REPOSITORY, "code-villain/secret"], "token": TOKEN })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refused.body["message"]
            .as_str()
            .unwrap()
            .contains("code-villain/secret"),
        "{}",
        refused.body
    );
    let wrong_token = s
        .app
        .call(
            "PUT",
            &path,
            Some(&s.admin),
            Some(json!({ "server": s.server, "repositories": [REPOSITORY], "token": "nope" })),
        )
        .await;
    assert_eq!(wrong_token.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        wrong_token.body["message"]
            .as_str()
            .unwrap()
            .contains("refused the token"),
        "{}",
        wrong_token.body
    );
    assert_eq!(s.status(&s.admin).await["connected"], false);

    s.connect().await;
    let invite = s.app.invite(&s.admin, &s.workspace, "member").await;
    let member = s.app.join("Grace", &invite).await;
    let seen = s.status(&member).await;
    assert_eq!(seen["connected"], true);
    assert_eq!(seen["server"], s.server);
    assert_eq!(seen["repositories"], json!([REPOSITORY]));
    assert!(!seen.to_string().contains(TOKEN));

    // Members can't change it.
    let as_member = s
        .app
        .call(
            "PUT",
            &path,
            Some(&member),
            Some(json!({ "server": s.server, "repositories": [REPOSITORY], "token": TOKEN })),
        )
        .await;
    assert_eq!(as_member.status, StatusCode::FORBIDDEN);
    let delete_as_member = s.app.call("DELETE", &path, Some(&member), None).await;
    assert_eq!(delete_as_member.status, StatusCode::FORBIDDEN);

    // Changing the repositories keeps the token for the same server, but
    // never hands it to another one.
    let kept = s
        .app
        .call(
            "PUT",
            &path,
            Some(&s.admin),
            Some(json!({ "server": s.server, "repositories": [REPOSITORY] })),
        )
        .await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.body);
    let elsewhere = s
        .app
        .call(
            "PUT",
            &path,
            Some(&s.admin),
            Some(json!({ "server": "https://elsewhere.example", "repositories": [REPOSITORY] })),
        )
        .await;
    assert_eq!(elsewhere.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_agit_pull_request_works_on_issues_only_after_a_closing_word(pool: PgPool) {
    let s = setup(pool).await;
    s.add(pull_request(
        10,
        "FOR-2: Notify the reviewer",
        "Fixes FOR-1",
    ));
    s.connect().await;
    s.check().await;

    let links = s.links().await;
    assert_eq!(links.len(), 2);
    let (_, _, works) = &links[0];
    assert_eq!(works["works_on"], true);
    assert_eq!(works["provider"], "forgejo");
    assert_eq!(works["checks"], "none");
    assert_eq!(works["author"], "code-villain");
    let (_, _, mentions) = &links[1];
    assert_eq!(mentions["works_on"], false);

    // No checks reported doesn't hold FOR-1 back from review.
    assert_eq!(
        s.statuses().await,
        [(1, "In Review".into()), (2, "Todo".into())]
    );
    let actors: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT u.name FROM sync_log l JOIN users u ON u.id = l.actor_id \
         WHERE l.model IN ('IssueLink', 'Issue') AND u.kind = 'agent'",
    )
    .fetch_all(&s.app.pool)
    .await
    .unwrap();
    assert_eq!(actors, ["Forgejo"]);
    let status = s.status(&s.admin).await;
    assert_eq!(status["up_to_date"], true);
    assert!(status["checked_at"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn pull_requests_closed_before_connecting_only_link(pool: PgPool) {
    let s = setup(pool).await;
    let yesterday = (Utc::now() - Duration::days(1)).to_rfc3339();
    let mut old = pull_request(3, "Rate limits", "Fixes FOR-1");
    old["state"] = json!("closed");
    old["merged"] = json!(true);
    old["merged_at"] = json!(yesterday);
    old["closed_at"] = json!(yesterday);
    old["updated_at"] = json!(yesterday);
    s.add(old);
    s.connect().await;
    s.check().await;

    let links = s.links().await;
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].2["state"], "merged");
    assert_eq!(
        s.statuses().await,
        [(1, "Backlog".into()), (2, "Todo".into())]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_issue_is_done_once_one_pull_request_merged_and_none_is_open(pool: PgPool) {
    let s = setup(pool).await;
    s.add(pull_request(10, "Rate limits", "Fixes FOR-1"));
    s.add(pull_request(11, "Rate limit docs", "Fixes FOR-1"));
    s.connect().await;
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Review".into()));

    // Merged while the other is open: it stays.
    s.merge(10);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Review".into()));

    // The other closed without merging: done.
    s.close(11);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "Done".into()));
    let links = s.links().await;
    assert_eq!(links[0].2["state"], "merged");
    assert_eq!(links[1].2["state"], "closed");

    // Seen again, nothing changes; reopening one doesn't undo Done either.
    s.check().await;
    s.change(11, |pr| {
        pr["state"] = json!("open");
        pr["closed_at"] = json!(null);
    });
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "Done".into()));
    assert_eq!(s.links().await[1].2["state"], "open");
}

#[sqlx::test(migrations = "../../migrations")]
async fn merging_the_last_open_pull_request_completes_the_issue(pool: PgPool) {
    let s = setup(pool).await;
    s.add(pull_request(10, "Rate limits", "Fixes FOR-1"));
    s.add(pull_request(11, "Rate limit docs", "Fixes FOR-1"));
    s.connect().await;
    s.check().await;

    // Closed while the other is open: nothing changes.
    s.close(10);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Review".into()));

    // Merged between two checks, never seen open again: done.
    s.merge(11);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "Done".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn all_closed_without_merging_sends_the_issue_back(pool: PgPool) {
    let s = setup(pool).await;
    s.add(pull_request(10, "Rate limits", "Fixes FOR-1"));
    s.connect().await;
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Review".into()));

    s.close(10);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Progress".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_canceled_issue_stays_canceled(pool: PgPool) {
    let s = setup(pool).await;
    s.add(pull_request(10, "Docs", "Fixes FOR-2"));
    s.forgejo
        .lock()
        .unwrap()
        .statuses
        .insert(format!("{:040x}", 10), vec!["pending"]);
    s.connect().await;
    s.check().await;
    let snapshot = s.snapshot().await;
    let issue = snapshot["records"]["Issue"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["number"] == 2)
        .unwrap()["id"]
        .clone();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "update", "model": "Issue", "id": issue, "data": { "state_id": state_id(&snapshot, "Canceled") } },
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

    s.merge(10);
    s.check().await;
    assert_eq!(s.statuses().await[1], (2, "Canceled".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn checks_and_conflicts_hold_issues_back_and_send_them_back(pool: PgPool) {
    let s = setup(pool).await;
    let sha = format!("{:040x}", 10);
    s.add(pull_request(10, "Rate limits", "Fixes FOR-1"));
    s.forgejo
        .lock()
        .unwrap()
        .statuses
        .insert(sha.clone(), vec!["success", "pending"]);
    s.connect().await;
    s.check().await;
    // Checks running: held back.
    assert_eq!(s.statuses().await[0], (1, "In Progress".into()));
    let link = s.links().await[0].2.clone();
    assert_eq!(link["checks"], "running");
    assert_eq!(link["blocker"], "checks_running");
    assert_eq!(link["held"], true);

    // They pass: into review.
    s.forgejo
        .lock()
        .unwrap()
        .statuses
        .insert(sha.clone(), vec!["success", "success"]);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Review".into()));
    assert_eq!(s.links().await[0].2["checks"], "passed");

    // Conflicts: back to work, held.
    s.change(10, |pr| pr["mergeable"] = json!(false));
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Progress".into()));
    let link = s.links().await[0].2.clone();
    assert_eq!(link["blocker"], "conflicts");
    assert_eq!(link["held"], true);

    // Fixed, but a check failed: still held.
    s.change(10, |pr| pr["mergeable"] = json!(true));
    s.forgejo
        .lock()
        .unwrap()
        .statuses
        .insert(sha, vec!["success", "failure"]);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Progress".into()));
    assert_eq!(s.links().await[0].2["checks"], "failed");
}

#[sqlx::test(migrations = "../../migrations")]
async fn drafts_start_the_work_and_marking_ready_asks_for_review(pool: PgPool) {
    let s = setup(pool).await;
    let mut draft = pull_request(12, "WIP: Docs", "Fixes FOR-2");
    draft["draft"] = json!(true);
    s.add(draft);
    s.connect().await;
    s.check().await;
    assert_eq!(s.statuses().await[1], (2, "In Progress".into()));
    assert_eq!(s.links().await[0].2["draft"], true);

    s.change(12, |pr| {
        pr["draft"] = json!(false);
        pr["title"] = json!("Docs");
    });
    s.check().await;
    assert_eq!(s.statuses().await[1], (2, "In Review".into()));

    // Turned back into a draft, its only open one: back to work.
    s.change(12, |pr| pr["draft"] = json!(true));
    s.check().await;
    assert_eq!(s.statuses().await[1], (2, "In Progress".into()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn when_forgejo_does_not_answer_nothing_changes(pool: PgPool) {
    let s = setup(pool).await;
    s.add(pull_request(10, "Rate limits", "Fixes FOR-1"));
    s.connect().await;
    s.check().await;
    let checked_at = s.status(&s.admin).await["checked_at"].clone();

    s.merge(10);
    s.forgejo.lock().unwrap().down = true;
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Review".into()));
    let status = s.status(&s.admin).await;
    assert_eq!(status["checked_at"], checked_at);
    assert!(
        status["check_error"].as_str().unwrap().contains("503"),
        "{status}"
    );

    // Back again: the merge it missed completes the issue.
    s.forgejo.lock().unwrap().down = false;
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "Done".into()));
    assert_eq!(s.status(&s.admin).await["check_error"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_old_check_is_not_up_to_date(pool: PgPool) {
    let s = setup(pool).await;
    s.connect().await;
    s.check().await;
    sqlx::query("UPDATE forgejo_connections SET checked_at = now() - interval '8 minutes'")
        .execute(&s.app.pool)
        .await
        .unwrap();
    assert_eq!(s.status(&s.admin).await["up_to_date"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn disconnecting_stops_checks_and_keeps_links(pool: PgPool) {
    let s = setup(pool).await;
    s.add(pull_request(10, "Rate limits", "Fixes FOR-1"));
    s.connect().await;
    s.check().await;
    let response = s
        .app
        .call(
            "DELETE",
            &format!("/api/w/{}/forgejo", s.workspace),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(response.status, StatusCode::NO_CONTENT);

    s.merge(10);
    s.check().await;
    assert_eq!(s.statuses().await[0], (1, "In Review".into()));
    assert_eq!(s.links().await.len(), 1);
    assert_eq!(s.status(&s.admin).await["connected"], false);
}
