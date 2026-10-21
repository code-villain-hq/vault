//! Drift checks end to end, with a scripted model and a real git repository.

mod common;

use std::process::Command;
use std::sync::Arc;

use axum::http::StatusCode;
use common::App;
use forge_ai::ScriptedProvider;
use forge_domain::Actor;
use forge_server::AppState;
use forge_server::drift::{self, DriftSettings};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A repository where rate limiting is already implemented.
fn repository(root: &std::path::Path) -> std::path::PathBuf {
    let repo = root.join("api");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("src/limits.rs"),
        "/// Allows 100 requests a minute per token.\npub fn rate_limit(token: &str) -> bool {\n    true\n}\n",
    )
    .unwrap();
    std::fs::write(repo.join("README.md"), "Not part of the service.\n").unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "Add rate limiting",
    ]);
    repo
}

struct Setup {
    app: App,
    admin: String,
    workspace: String,
    actor: Actor,
    service: Uuid,
    _root: tempfile::TempDir,
}

async fn setup(pool: PgPool, provider: Option<Arc<ScriptedProvider>>) -> Setup {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(&std::fs::canonicalize(root.path()).unwrap());
    let settings = DriftSettings {
        repos_root: Some(std::fs::canonicalize(root.path()).unwrap()),
        check_every: None,
        max_turns: 20,
        github_clones: None,
        triage_new_issues: false,
    };
    let mut state = AppState::new(pool.clone(), false);
    if let Some(provider) = provider {
        state = state.with_ai(provider, settings);
    }
    let app = App::with_state(state);
    let (admin, workspace) = app.signup("Ada", "Forge").await;
    let actor = Actor {
        user_id: app.get("/api/me", &admin).await.body["user"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        workspace_id: workspace.parse().unwrap(),
    };
    let snapshot = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &admin)
        .await
        .body;
    let team = snapshot["records"]["Team"][0]["id"].clone();
    let todo = snapshot["records"]["WorkflowState"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Todo")
        .unwrap()["id"]
        .clone();

    let service = Uuid::now_v7();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Service", "id": service, "data": {
            "name": "API", "repository": repo.to_str().unwrap(), "paths": ["src/**"], "team_id": team,
        }},
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": {
            "team_id": team, "title": "Add rate limiting", "state_id": todo, "service_id": service,
        }},
    ]});
    let created = app
        .post(
            &format!("/api/w/{workspace}/sync/transactions"),
            Some(&admin),
            tx,
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    Setup {
        app,
        admin,
        workspace,
        actor,
        service,
        _root: root,
    }
}

/// What the model does in the happy path: look, find FOR-1 done in the
/// code, suggest closing it, record the current state, finish.
fn script() -> Vec<Value> {
    vec![
        ScriptedProvider::tool_call("t1", "list_files", json!({})),
        ScriptedProvider::tool_call("t2", "read_file", json!({ "path": "src/limits.rs" })),
        ScriptedProvider::tool_call(
            "t3",
            "propose_change",
            json!({
                "title": "FOR-1 is done in the code but still open",
                "explanation": "`src/limits.rs:2` implements `rate_limit`, added in the latest commit.",
                "issue": "FOR-1",
                "changes": [
                    { "action": "set_issue_status", "issue": "FOR-1", "status": "Done" },
                    { "action": "comment_on_issue", "issue": "FOR-1", "body": "Implemented in src/limits.rs." },
                ],
            }),
        ),
        ScriptedProvider::tool_call(
            "t4",
            "update_current_state",
            json!({
                "current_state": "Limits each token to 100 requests a minute.",
            }),
        ),
        ScriptedProvider::tool_call(
            "t5",
            "finish",
            json!({ "summary": "FOR-1 is implemented; suggested closing it." }),
        ),
    ]
}

async fn records(s: &Setup, model: &str) -> Vec<Value> {
    let body = s
        .app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body;
    body["records"][model]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[sqlx::test(migrations = "../../migrations")]
async fn checks_need_a_model_and_a_repository(pool: PgPool) {
    let s = setup(pool, None).await;
    let check = s
        .app
        .call(
            "POST",
            &format!("/api/w/{}/services/{}/check", s.workspace, s.service),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(check.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        check.body["message"]
            .as_str()
            .unwrap()
            .contains("ANTHROPIC_API_KEY")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_check_suggests_fixes_that_people_can_accept(pool: PgPool) {
    let provider = Arc::new(ScriptedProvider::new(script()));
    let s = setup(pool, Some(provider.clone())).await;

    let check = s
        .app
        .call(
            "POST",
            &format!("/api/w/{}/services/{}/check", s.workspace, s.service),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(check.status, StatusCode::OK, "{}", check.body);
    let again = s
        .app
        .call(
            "POST",
            &format!("/api/w/{}/services/{}/check", s.workspace, s.service),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(
        again.status,
        StatusCode::CONFLICT,
        "one at a time per service"
    );

    let settings = DriftSettings {
        repos_root: Some(std::fs::canonicalize(s._root.path()).unwrap()),
        check_every: None,
        max_turns: 20,
        github_clones: None,
        triage_new_issues: false,
    };
    assert!(
        drift::run_next(&s.app.pool, provider.as_ref(), &settings)
            .await
            .unwrap()
    );
    assert!(
        !drift::run_next(&s.app.pool, provider.as_ref(), &settings)
            .await
            .unwrap(),
        "nothing left"
    );

    // The model saw the tracker context and the code it asked for.
    let requests = provider.requests.lock().unwrap().clone();
    let first = requests[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(first.contains("FOR-1 Add rate limiting [Todo]"), "{first}");
    assert!(
        first.contains("Add rate limiting"),
        "the git history is included"
    );
    let read = requests[2]["messages"].as_array().unwrap().last().unwrap()["content"][0]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(read.contains("pub fn rate_limit"), "{read}");

    let run = &records(&s, "AiRun").await[0];
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["suggestions"], 1);
    assert_eq!(
        run["summary"],
        "FOR-1 is implemented; suggested closing it."
    );
    assert!(run["input_tokens"].as_i64().unwrap() > 0);

    let service = &records(&s, "Service").await[0];
    assert_eq!(
        service["current_state"],
        "Limits each token to 100 requests a minute."
    );

    let suggestion = records(&s, "Suggestion").await[0].clone();
    assert_eq!(suggestion["status"], "open");
    assert_eq!(
        suggestion["changes"],
        json!([
            "Move FOR-1 to Done",
            "Comment on FOR-1: Implemented in src/limits.rs."
        ])
    );
    let forge_ai = suggestion["created_by"].as_str().unwrap();
    let users = records(&s, "User").await;
    let ai_user = users.iter().find(|u| u["id"] == forge_ai).unwrap();
    assert_eq!(
        (ai_user["name"].as_str(), ai_user["kind"].as_str()),
        (Some("Forge AI"), Some("agent"))
    );

    // Ada created the issue, so she hears about the suggestion.
    let notified: Vec<(String, Uuid)> = sqlx::query_as("SELECT type, user_id FROM notifications")
        .fetch_all(&s.app.pool)
        .await
        .unwrap();
    assert!(
        notified.contains(&("suggestion".into(), s.actor.user_id)),
        "{notified:?}"
    );

    // Nothing changed yet; accepting applies the changes as Ada.
    let issue = &records(&s, "Issue").await[0];
    let states = records(&s, "WorkflowState").await;
    let state_name = |id: &Value| states.iter().find(|st| &st["id"] == id).unwrap()["name"].clone();
    assert_eq!(state_name(&issue["state_id"]), "Todo");

    let id = suggestion["id"].as_str().unwrap();
    let accepted = s
        .app
        .call(
            "POST",
            &format!("/api/w/{}/suggestions/{id}/accept", s.workspace),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(accepted.status, StatusCode::NO_CONTENT, "{}", accepted.body);

    let issue = &records(&s, "Issue").await[0];
    assert_eq!(state_name(&issue["state_id"]), "Done");
    let comment = &records(&s, "Comment").await[0];
    assert_eq!(
        comment["author_id"],
        json!(s.actor.user_id),
        "applied as the person who accepted"
    );
    let suggestion = &records(&s, "Suggestion").await[0];
    assert_eq!(suggestion["status"], "accepted");
    assert_eq!(suggestion["resolved_by"], json!(s.actor.user_id));

    let twice = s
        .app
        .call(
            "POST",
            &format!("/api/w/{}/suggestions/{id}/accept", s.workspace),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_proposals_go_back_to_the_model_and_runs_record_failures(pool: PgPool) {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ScriptedProvider::tool_call("t1", "read_file", json!({ "path": "README.md" })),
        ScriptedProvider::tool_call(
            "t2",
            "propose_change",
            json!({
                "title": "x", "explanation": "y", "changes": [{ "action": "set_issue_status", "issue": "FOR-99", "status": "Done" }],
            }),
        ),
        // The script ends here: the model API "fails".
    ]));
    let s = setup(pool, Some(provider.clone())).await;
    s.app
        .call(
            "POST",
            &format!("/api/w/{}/services/{}/check", s.workspace, s.service),
            Some(&s.admin),
            None,
        )
        .await;
    let settings = DriftSettings {
        repos_root: Some(std::fs::canonicalize(s._root.path()).unwrap()),
        check_every: None,
        max_turns: 20,
        github_clones: None,
        triage_new_issues: false,
    };
    drift::run_next(&s.app.pool, provider.as_ref(), &settings)
        .await
        .unwrap();

    let requests = provider.requests.lock().unwrap().clone();
    let results = |n: usize| {
        requests[n]["messages"].as_array().unwrap().last().unwrap()["content"][0].clone()
    };
    assert_eq!(
        results(1)["is_error"],
        true,
        "README.md is outside the service"
    );
    assert_eq!(results(2)["is_error"], true, "FOR-99 doesn't exist");

    let run = &records(&s, "AiRun").await[0];
    assert_eq!(run["status"], "failed");
    assert!(run["error"].as_str().unwrap().contains("ran out"), "{run}");
    assert!(records(&s, "Suggestion").await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn repositories_must_be_under_the_allowed_root(pool: PgPool) {
    let settings = DriftSettings {
        repos_root: Some(std::fs::canonicalize(std::env::temp_dir()).unwrap()),
        ..Default::default()
    };
    assert!(settings.resolve_repository("/etc").is_err());
    assert!(settings.resolve_repository("../etc").is_err());
    assert!(settings.resolve_repository("github:villain/forge").is_err());

    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    let name = repo.path().file_name().unwrap().to_str().unwrap();
    let found = std::fs::canonicalize(repo.path()).unwrap();
    assert_eq!(
        settings.resolve_repository(name).unwrap(),
        found,
        "relative to the root"
    );
    assert_eq!(
        settings
            .resolve_repository(repo.path().to_str().unwrap())
            .unwrap(),
        found
    );
    assert!(
        DriftSettings::default().resolve_repository("/tmp").is_err(),
        "off unless configured"
    );
    drop(pool);
}

const RATE_LIMITS: &str = "\
## Purpose

Rate limits for the API.

## Requirements

### Requirement: Tokens are limited
The API SHALL allow 100 requests a minute per token.

#### Scenario: Too many
- **WHEN** a token sends a 101st request within a minute
- **THEN** it gets 429

### Requirement: Limits reset
The limit MUST reset every minute.

#### Scenario: A new minute
- **WHEN** a minute has passed
- **THEN** the token may send 100 more
";

#[sqlx::test(migrations = "../../migrations")]
async fn a_check_gives_each_requirement_a_verdict(pool: PgPool) {
    let verdict = |requirement: &str, verdict: &str, evidence: &str| json!({ "spec": "Rate limits", "requirement": requirement, "verdict": verdict, "evidence": evidence });
    let provider = Arc::new(ScriptedProvider::new(vec![
        ScriptedProvider::tool_call(
            "t1",
            "report_verdicts",
            json!({ "verdicts": [
                verdict("Tokens are limited", "met", "src/limits.rs:2"),
                verdict("Tokens are throttled", "met", "x"),
            ]}),
        ),
        ScriptedProvider::tool_call(
            "t2",
            "report_verdicts",
            json!({ "verdicts": [verdict("Tokens are limited", "met", "`src/limits.rs:2` limits each token.")] }),
        ),
        ScriptedProvider::tool_call(
            "t3",
            "report_verdicts",
            json!({ "verdicts": [verdict("Requirement: Limits reset", "not_met", "Nothing resets the count.")] }),
        ),
        ScriptedProvider::tool_call(
            "t4",
            "propose_change",
            json!({
                "title": "Limits never reset", "explanation": "Nothing in `src/limits.rs` resets the count.",
                "spec": "Rate limits", "requirement": "Limits reset", "changes": [],
            }),
        ),
        ScriptedProvider::tool_call(
            "t5",
            "finish",
            json!({ "summary": "One requirement isn't met." }),
        ),
        // The next check: the code now resets limits.
        ScriptedProvider::tool_call(
            "t6",
            "report_verdicts",
            json!({ "verdicts": [verdict("Limits reset", "met", "`src/limits.rs:3`")] }),
        ),
        ScriptedProvider::tool_call("t7", "finish", json!({ "summary": "All met." })),
    ]));
    let s = setup(pool, Some(provider.clone())).await;
    let spec = Uuid::now_v7();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Spec", "id": spec, "data": {
            "title": "Rate limits", "body": RATE_LIMITS, "service_ids": [s.service], "status": "active",
        }},
    ]});
    let created = s
        .app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let settings = DriftSettings {
        repos_root: Some(std::fs::canonicalize(s._root.path()).unwrap()),
        max_turns: 20,
        ..Default::default()
    };
    let check = async || {
        let queued = s
            .app
            .call(
                "POST",
                &format!("/api/w/{}/services/{}/check", s.workspace, s.service),
                Some(&s.admin),
                None,
            )
            .await;
        assert_eq!(queued.status, StatusCode::OK, "{}", queued.body);
        assert!(
            drift::run_next(&s.app.pool, provider.as_ref(), &settings)
                .await
                .unwrap()
        );
    };
    check().await;

    // The model was told which requirements to judge.
    let requests = provider.requests.lock().unwrap().clone();
    let first = requests[0]["messages"][0]["content"].as_str().unwrap();
    assert!(
        first.contains("to give a verdict on:\n- Tokens are limited\n- Limits reset"),
        "{first}"
    );
    let result = |n: usize| {
        requests[n]["messages"].as_array().unwrap().last().unwrap()["content"][0].clone()
    };
    assert_eq!(result(1)["is_error"], true, "no such requirement");
    assert!(
        result(1)["content"]
            .as_str()
            .unwrap()
            .contains("its requirements are \"Tokens are limited\", \"Limits reset\""),
        "{}",
        result(1)
    );
    assert!(
        result(2)["content"]
            .as_str()
            .unwrap()
            .contains("Still without a verdict: \"Limits reset\" (Rate limits)"),
        "{}",
        result(2)
    );

    let verdicts = |all: Vec<Value>| -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = all
            .iter()
            .map(|v| {
                (
                    v["requirement"].as_str().unwrap().to_string(),
                    v["verdict"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        out.sort();
        out
    };
    let first_run = records(&s, "RequirementVerdict").await;
    assert_eq!(
        verdicts(first_run.clone()),
        [
            ("Limits reset".into(), "not_met".into()),
            ("Tokens are limited".into(), "met".into())
        ]
    );
    let limited = first_run
        .iter()
        .find(|v| v["requirement"] == "Tokens are limited")
        .unwrap();
    assert_eq!(limited["spec_id"], json!(spec));
    assert_eq!(limited["evidence"], "`src/limits.rs:2` limits each token.");
    assert!(
        limited["requirement_text"]
            .as_str()
            .unwrap()
            .starts_with("### Requirement: Tokens are limited\nThe API SHALL")
    );
    assert_eq!(limited["commit_sha"].as_str().unwrap().len(), 40);
    let spec_record = records(&s, "Spec").await[0].clone();
    assert_eq!(limited["spec_updated_at"], spec_record["updated_at"]);

    let suggestion = &records(&s, "Suggestion").await[0];
    assert_eq!(suggestion["spec_id"], json!(spec));
    assert_eq!(suggestion["requirement"], "Limits reset");

    // A later check replaces the verdict instead of adding one.
    check().await;
    let second_run = records(&s, "RequirementVerdict").await;
    assert_eq!(
        verdicts(second_run.clone()),
        [
            ("Limits reset".into(), "met".into()),
            ("Tokens are limited".into(), "met".into())
        ]
    );
    let reset = |all: &[Value]| {
        all.iter()
            .find(|v| v["requirement"] == "Limits reset")
            .unwrap()["id"]
            .clone()
    };
    assert_eq!(reset(&first_run), reset(&second_run));

    // People can't write verdicts; deleting the spec deletes them.
    let forged = s
        .app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            json!({ "id": Uuid::now_v7(), "operations": [
                { "action": "update", "model": "RequirementVerdict", "id": reset(&second_run), "data": { "verdict": "not_met" } },
            ]}),
        )
        .await;
    assert_ne!(forged.status, StatusCode::OK);
    let deleted = s
        .app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            json!({ "id": Uuid::now_v7(), "operations": [
                { "action": "delete", "model": "Spec", "id": spec },
            ]}),
        )
        .await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.body);
    assert!(records(&s, "RequirementVerdict").await.is_empty());
    assert_eq!(records(&s, "Suggestion").await[0]["spec_id"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_check_can_read_long_specs_and_sees_project_and_workspace_ones(pool: PgPool) {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ScriptedProvider::tool_call("t1", "read_spec", json!({ "spec": "Rate limits" })),
        ScriptedProvider::tool_call(
            "t2",
            "read_decision",
            json!({ "decision": "Tokens, not IPs" }),
        ),
        ScriptedProvider::tool_call("t3", "finish", json!({ "summary": "Read them." })),
    ]));
    let s = setup(pool, Some(provider.clone())).await;
    let team = records(&s, "Team").await[0]["id"].clone();
    let (launch, other) = (Uuid::now_v7(), Uuid::now_v7());
    let long = format!(
        "Limits per token.\n\n{}\n\nThe last line: limits reset every minute.",
        "Filler. ".repeat(1200)
    );
    let decision = |title: &str, project: Option<Uuid>| {
        json!({ "action": "create", "model": "Decision", "id": Uuid::now_v7(), "data": {
            "title": title, "decision": format!("{title}, decided."), "status": "accepted", "project_id": project,
        }})
    };
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Project", "id": launch, "data": { "name": "Launch" } },
        { "action": "create", "model": "Project", "id": other, "data": { "name": "Other" } },
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": {
            "team_id": team, "title": "Limit the public API", "service_id": s.service, "project_id": launch,
        }},
        { "action": "create", "model": "Spec", "id": Uuid::now_v7(), "data": {
            "title": "Rate limits", "body": long, "service_ids": [s.service], "status": "active",
        }},
        { "action": "create", "model": "Spec", "id": Uuid::now_v7(), "data": {
            "title": "Launch checklist", "body": "Everything is limited.", "project_id": launch, "status": "active",
        }},
        decision("Tokens, not IPs", Some(launch)),
        decision("Postgres everywhere", None),
        decision("Something else entirely", Some(other)),
    ]});
    let created = s
        .app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    let settings = DriftSettings {
        repos_root: Some(std::fs::canonicalize(s._root.path()).unwrap()),
        max_turns: 20,
        ..Default::default()
    };
    let queued = s
        .app
        .call(
            "POST",
            &format!("/api/w/{}/services/{}/check", s.workspace, s.service),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(queued.status, StatusCode::OK, "{}", queued.body);
    assert!(
        drift::run_next(&s.app.pool, provider.as_ref(), &settings)
            .await
            .unwrap()
    );

    let requests = provider.requests.lock().unwrap().clone();
    let first = requests[0]["messages"][0]["content"].as_str().unwrap();
    // The long spec is cut, saying how to read the rest.
    assert!(!first.contains("The last line"), "{first}");
    assert!(
        first.contains("(cut off; read it in full with read_spec"),
        "{first}"
    );
    // The issue's project and the whole workspace count; other projects don't.
    assert!(
        first.contains("- Spec \"Launch checklist\" (project Launch"),
        "{first}"
    );
    assert!(
        first.contains("- Decision \"Tokens, not IPs\" (project Launch"),
        "{first}"
    );
    assert!(
        first.contains("- Decision \"Postgres everywhere\" (workspace-wide"),
        "{first}"
    );
    assert!(!first.contains("Something else entirely"), "{first}");

    let result = |n: usize| {
        requests[n]["messages"].as_array().unwrap().last().unwrap()["content"][0]["content"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(
        result(1).contains("The last line: limits reset every minute."),
        "{}",
        result(1)
    );
    assert!(
        result(2).contains("Tokens, not IPs, decided."),
        "{}",
        result(2)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_model_runs_left_over_are_marked_failed(pool: PgPool) {
    let s = setup(pool.clone(), None).await;
    let run = async |status: &str, kind: &str| {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO ai_runs (id, workspace_id, service_id, trigger, status, kind, created_by) \
             VALUES ($1, $2, $3, 'manual', $4, $5, $6)",
        )
        .bind(id)
        .bind(s.actor.workspace_id)
        // One active drift check per service; triage runs are about an issue.
        .bind((kind == "drift").then_some(s.service))
        .bind(status)
        .bind(kind)
        .bind(s.actor.user_id)
        .execute(&pool)
        .await
        .unwrap();
        id
    };
    let (running, queued) = (run("running", "drift").await, run("queued", "triage").await);

    // A server without a model only clears up, then stops.
    forge_server::drift::worker(AppState::new(pool.clone(), false)).await;

    let outcome = async |id: Uuid| -> (String, Option<String>) {
        sqlx::query_as("SELECT status, error FROM ai_runs WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    assert_eq!(
        outcome(running).await,
        (
            "failed".into(),
            Some("The server restarted during the check.".into())
        )
    );
    assert_eq!(
        outcome(queued).await,
        (
            "failed".into(),
            Some("No model is set up to run triage.".into())
        )
    );
}
