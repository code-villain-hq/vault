//! Triage by Forge's AI, with a scripted model, and by agents through MCP:
//! new issues in triage get a suggestion people can accept, and nobody is
//! notified about it.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::App;
use forge_ai::ScriptedProvider;
use forge_server::AppState;
use forge_server::drift::{self, DriftSettings, triage};
use forge_server::mcp::tools::{SearchIssues, SuggestTriage, search_issues, suggest_triage};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

struct Setup {
    app: App,
    admin: String,
    workspace: String,
    /// FOR-2, reported by Bob, waiting in triage.
    reported: Uuid,
}

fn settings() -> DriftSettings {
    DriftSettings {
        triage_new_issues: true,
        ..Default::default()
    }
}

async fn setup(pool: PgPool, provider: Option<Arc<ScriptedProvider>>) -> Setup {
    let mut state = AppState::new(pool.clone(), false);
    if let Some(provider) = provider {
        state = state.with_ai(provider, settings());
    }
    let app = App::with_state(state);
    let (admin, workspace) = app.signup("Ada", "Forge").await;
    let bob = app
        .join("Bob", &app.invite(&admin, &workspace, "member").await)
        .await;
    let snapshot = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &admin)
        .await
        .body;
    let team = snapshot["records"]["Team"][0]["id"].clone();
    let state = |name: &str| {
        snapshot["records"]["WorkflowState"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()["id"]
            .clone()
    };
    let write = async |who: &str, operations: Value| {
        let tx = json!({ "id": Uuid::now_v7(), "operations": operations });
        let response = app
            .post(
                &format!("/api/w/{workspace}/sync/transactions"),
                Some(who),
                tx,
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    };
    write(&admin, json!([
        { "action": "create", "model": "Label", "id": Uuid::now_v7(), "data": { "name": "Bug" } },
        { "action": "create", "model": "Label", "id": Uuid::now_v7(), "data": { "name": "Customer" } },
        { "action": "create", "model": "Service", "id": Uuid::now_v7(), "data": { "name": "Accounts", "description": "Sign-in, sessions and passwords." } },
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": { "team_id": team, "title": "Login locks people out after wrong passwords", "state_id": state("Done") } },
    ])).await;
    let reported = Uuid::now_v7();
    write(&bob, json!([
        { "action": "create", "model": "Issue", "id": reported, "data": { "team_id": team, "title": "Can't log in after mistyping my password", "state_id": state("Triage") } },
    ])).await;
    Setup {
        app,
        admin,
        workspace,
        reported,
    }
}

fn script() -> Vec<Value> {
    vec![
        ScriptedProvider::tool_call(
            "t1",
            "search_issues",
            json!({ "query": "login", "include_closed": true }),
        ),
        ScriptedProvider::tool_call(
            "t2",
            "suggest",
            json!({
                "explanation": "Sign-in is broken for this person; FOR-1 describes the same lockout.",
                "priority": "high",
                "labels": ["Bug"],
                "service": "Accounts",
                "duplicate_of": "FOR-1",
                "related": ["FOR-1", "FOR-1"],
            }),
        ),
    ]
}

async fn records(s: &Setup, model: &str) -> Vec<Value> {
    s.app
        .get(&format!("/api/w/{}/sync/bootstrap", s.workspace), &s.admin)
        .await
        .body["records"][model]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_issues_in_triage_get_a_suggestion_people_can_accept(pool: PgPool) {
    let provider = Arc::new(ScriptedProvider::new(script()));
    let s = setup(pool, Some(provider.clone())).await;

    assert_eq!(
        triage::queue_new_issues(&s.app.pool, "scripted")
            .await
            .unwrap(),
        1,
        "only the issue in triage"
    );
    assert_eq!(
        triage::queue_new_issues(&s.app.pool, "scripted")
            .await
            .unwrap(),
        0,
        "once"
    );
    assert!(
        drift::run_next(&s.app.pool, provider.as_ref(), &settings())
            .await
            .unwrap()
    );

    // The model saw the issue and what it could be given.
    let requests = provider.requests.lock().unwrap().clone();
    let first = requests[0]["messages"][0]["content"].as_str().unwrap();
    assert!(
        first.contains("Can't log in after mistyping my password"),
        "{first}"
    );
    assert!(first.contains("Bug"), "labels are listed");
    assert!(first.contains("Accounts: Sign-in, sessions and passwords."));
    let found =
        requests[1]["messages"].as_array().unwrap().last().unwrap()["content"][0]["content"]
            .as_str()
            .unwrap();
    assert!(
        found.contains("FOR-1"),
        "closed issues are searched: {found}"
    );

    let run = &records(&s, "AiRun").await[0];
    assert_eq!(run["kind"], "triage");
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["issue_id"], json!(s.reported));

    let suggestion = &records(&s, "Suggestion").await[0];
    assert_eq!(suggestion["kind"], "triage");
    assert_eq!(suggestion["title"], "Triage FOR-2");
    let changes: Vec<&str> = suggestion["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(
        changes,
        [
            "Priority: high",
            "Label: Bug",
            "Service: Accounts",
            "Duplicate of FOR-1: Login locks people out after wrong passwords",
        ]
    );
    let notified: i64 =
        sqlx::query_scalar("SELECT count(*) FROM notifications WHERE type = 'suggestion'")
            .fetch_one(&s.app.pool)
            .await
            .unwrap();
    assert_eq!(notified, 0, "triage suggestions show in Triage instead");

    // Someone labels the issue before anyone applies the suggestion.
    let customer = records(&s, "Label")
        .await
        .into_iter()
        .find(|l| l["name"] == "Customer")
        .unwrap()["id"]
        .clone();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "update", "model": "Issue", "id": s.reported, "data": { "label_ids": [customer] } },
    ]});
    let labelled = s
        .app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;
    assert_eq!(labelled.status, StatusCode::OK, "{}", labelled.body);

    let accepted = s
        .app
        .call(
            "POST",
            &format!(
                "/api/w/{}/suggestions/{}/accept",
                s.workspace,
                suggestion["id"].as_str().unwrap()
            ),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(accepted.status, StatusCode::NO_CONTENT, "{}", accepted.body);
    let issue = records(&s, "Issue")
        .await
        .into_iter()
        .find(|i| i["id"] == json!(s.reported))
        .unwrap();
    assert_eq!(issue["priority"], 2);
    assert_eq!(
        issue["label_ids"].as_array().unwrap().len(),
        2,
        "the label added meanwhile stays"
    );
    assert!(issue["service_id"].is_string());
    let relations = records(&s, "IssueRelation").await;
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0]["type"], "duplicate");
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_can_ask_for_triage_once_at_a_time(pool: PgPool) {
    let off = setup(pool.clone(), None).await;
    let url = format!("/api/w/{}/issues/{}/triage", off.workspace, off.reported);
    let refused = off.app.call("POST", &url, Some(&off.admin), None).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refused.body["message"]
            .as_str()
            .unwrap()
            .contains("ANTHROPIC_API_KEY")
    );
    drop(off);

    // A fresh workspace on a server with a model.
    let provider = Arc::new(ScriptedProvider::new(script()));
    let state = AppState::new(pool.clone(), false).with_ai(provider, settings());
    let app = App::with_state(state);
    let (admin, workspace) = app.signup("Eve", "Elsewhere").await;
    let team = app
        .get(&format!("/api/w/{workspace}/sync/bootstrap"), &admin)
        .await
        .body["records"]["Team"][0]["id"]
        .clone();
    let issue = Uuid::now_v7();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": issue, "data": { "team_id": team, "title": "Flaky sync" } },
    ]});
    app.post(
        &format!("/api/w/{workspace}/sync/transactions"),
        Some(&admin),
        tx,
    )
    .await;
    let url = format!("/api/w/{workspace}/issues/{issue}/triage");
    assert_eq!(
        app.call("POST", &url, Some(&admin), None).await.status,
        StatusCode::OK
    );
    assert_eq!(
        app.call("POST", &url, Some(&admin), None).await.status,
        StatusCode::CONFLICT
    );
    let other = format!("/api/w/{workspace}/issues/{}/triage", Uuid::now_v7());
    assert_eq!(
        app.call("POST", &other, Some(&admin), None).await.status,
        StatusCode::NOT_FOUND
    );
}

/// An agent added under Settings → Agents, acting through MCP.
async fn agent(s: &Setup, name: &str) -> forge_domain::Actor {
    let created = s
        .app
        .post(
            &format!("/api/w/{}/agents", s.workspace),
            Some(&s.admin),
            json!({ "name": name }),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.body);
    forge_domain::Actor {
        user_id: created.body["agent_id"].as_str().unwrap().parse().unwrap(),
        workspace_id: s.workspace.parse().unwrap(),
    }
}

fn needs_triage() -> SearchIssues {
    SearchIssues {
        query: None,
        team: None,
        status: None,
        assignee: None,
        project: None,
        service: None,
        include_closed: None,
        needs_triage: Some(true),
        limit: None,
    }
}

fn suggestion(identifier: &str, why: &str) -> SuggestTriage {
    SuggestTriage {
        identifier: identifier.into(),
        why: why.into(),
        priority: None,
        labels: None,
        service: None,
        project: None,
        assignee: None,
        duplicate_of: None,
        related: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_suggest_triage_without_a_model(pool: PgPool) {
    let s = setup(pool, None).await;
    let claude = agent(&s, "Claude Code").await;
    let pool = &s.app.pool;
    let waiting = async || -> Value {
        serde_json::from_str(&search_issues(pool, claude, needs_triage()).await.unwrap()).unwrap()
    };
    let found = waiting().await;
    assert_eq!(found["total"], 1, "only the issue in triage: {found}");
    assert_eq!(found["issues"][0]["identifier"], "FOR-2");

    let error = suggest_triage(pool, claude, suggestion("FOR-1", "Done already."))
        .await
        .unwrap_err();
    assert!(error.contains("isn't in triage"), "{error}");
    let error = suggest_triage(pool, claude, suggestion("FOR-2", " "))
        .await
        .unwrap_err();
    assert!(error.contains("why"), "{error}");
    let error = suggest_triage(
        pool,
        claude,
        SuggestTriage {
            assignee: Some("Nobody".into()),
            ..suggestion("FOR-2", "Someone should.")
        },
    )
    .await
    .unwrap_err();
    assert!(error.contains("no member"), "{error}");

    suggest_triage(
        pool,
        claude,
        SuggestTriage {
            priority: Some("high".into()),
            labels: Some(vec!["Bug".into()]),
            service: Some("Accounts".into()),
            assignee: Some("me".into()),
            duplicate_of: Some("FOR-1".into()),
            ..suggestion("FOR-2", "Sign-in is broken; FOR-1 is the same lockout.")
        },
    )
    .await
    .unwrap();
    let suggestion_ = &records(&s, "Suggestion").await[0];
    assert_eq!(suggestion_["kind"], "triage");
    assert_eq!(suggestion_["status"], "open");
    assert_eq!(
        suggestion_["created_by"],
        json!(claude.user_id),
        "credited to the agent"
    );
    assert!(suggestion_["run_id"].is_null());
    let changes: Vec<&str> = suggestion_["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(
        changes,
        [
            "Priority: high",
            "Label: Bug",
            "Service: Accounts",
            "Assignee: Claude Code",
            "Duplicate of FOR-1: Login locks people out after wrong passwords",
        ]
    );
    assert_eq!(waiting().await["total"], 0, "triaged now");

    let error = suggest_triage(pool, claude, suggestion("FOR-2", "Again."))
        .await
        .unwrap_err();
    assert!(error.contains("already has a triage suggestion"), "{error}");

    // Applying it assigns the issue to the agent, as the person applying.
    let accepted = s
        .app
        .call(
            "POST",
            &format!(
                "/api/w/{}/suggestions/{}/accept",
                s.workspace,
                suggestion_["id"].as_str().unwrap()
            ),
            Some(&s.admin),
            None,
        )
        .await;
    assert_eq!(accepted.status, StatusCode::NO_CONTENT, "{}", accepted.body);
    let issue = records(&s, "Issue")
        .await
        .into_iter()
        .find(|i| i["id"] == json!(s.reported))
        .unwrap();
    assert_eq!(issue["assignee_id"], json!(claude.user_id));
    assert_eq!(issue["priority"], 2);

    // Nothing to change is a suggestion too, so the issue stops waiting.
    let other = Uuid::now_v7();
    let team = records(&s, "Team").await[0]["id"].clone();
    let triage = records(&s, "WorkflowState")
        .await
        .into_iter()
        .find(|st| st["name"] == "Triage")
        .unwrap()["id"]
        .clone();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": other, "data": { "team_id": team, "title": "Typo on the sign-in page", "state_id": triage } },
    ]});
    s.app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;
    assert_eq!(waiting().await["total"], 1);
    let answer = suggest_triage(pool, claude, suggestion("FOR-3", "It's fine as filed."))
        .await
        .unwrap();
    assert!(answer.contains("leaving FOR-3 as it is"), "{answer}");
    assert_eq!(waiting().await["total"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn forge_ai_leaves_issues_an_agent_triaged_and_can_suggest_agents(pool: PgPool) {
    let provider = Arc::new(ScriptedProvider::new(vec![ScriptedProvider::tool_call(
        "t1",
        "suggest",
        json!({ "explanation": "A change to the code.", "assignee": "Claude Code" }),
    )]));
    let s = setup(pool, Some(provider.clone())).await;
    let claude = agent(&s, "Claude Code").await;

    let status = s
        .app
        .get(&format!("/api/w/{}/ai", s.workspace), &s.admin)
        .await;
    assert_eq!(status.body["configured"], true);

    // Forge AI suggests the agent, which it's told about.
    let url = format!("/api/w/{}/issues/{}/triage", s.workspace, s.reported);
    let asked = s.app.call("POST", &url, Some(&s.admin), None).await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.body);
    assert!(
        drift::run_next(&s.app.pool, provider.as_ref(), &settings())
            .await
            .unwrap()
    );
    let requests = provider.requests.lock().unwrap().clone();
    let first = requests[0]["messages"][0]["content"].as_str().unwrap();
    assert!(first.contains("# Agents"), "{first}");
    assert!(first.contains("Claude Code"), "{first}");
    assert!(
        !first.contains("Forge AI"),
        "Forge's own agents take no work"
    );
    let suggestions = records(&s, "Suggestion").await;
    assert_eq!(suggestions[0]["changes"], json!(["Assignee: Claude Code"]));

    // With that one waiting, nobody else suggests triage for the issue.
    let again = s.app.call("POST", &url, Some(&s.admin), None).await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.body);
    let error = suggest_triage(&s.app.pool, claude, suggestion("FOR-2", "Mine too."))
        .await
        .unwrap_err();
    assert!(error.contains("already has a triage suggestion"), "{error}");

    // A new issue an agent triaged first isn't queued for Forge AI.
    let team = records(&s, "Team").await[0]["id"].clone();
    let triage_state = records(&s, "WorkflowState")
        .await
        .into_iter()
        .find(|st| st["name"] == "Triage")
        .unwrap()["id"]
        .clone();
    let tx = json!({ "id": Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Issue", "id": Uuid::now_v7(), "data": { "team_id": team, "title": "Slow search", "state_id": triage_state } },
    ]});
    s.app
        .post(
            &format!("/api/w/{}/sync/transactions", s.workspace),
            Some(&s.admin),
            tx,
        )
        .await;
    suggest_triage(
        &s.app.pool,
        claude,
        SuggestTriage {
            priority: Some("low".into()),
            ..suggestion("FOR-3", "Polish.")
        },
    )
    .await
    .unwrap();
    assert_eq!(
        triage::queue_new_issues(&s.app.pool, "scripted")
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_app_can_tell_forge_ai_is_off(pool: PgPool) {
    let s = setup(pool, None).await;
    let status = s
        .app
        .get(&format!("/api/w/{}/ai", s.workspace), &s.admin)
        .await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["configured"], false);
}
