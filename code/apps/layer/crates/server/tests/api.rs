//! The HTTP API end to end: requests go through the real router to a fresh
//! Postgres database per test.

mod common;

use axum::http::StatusCode;
use common::App;
use layer_server::accounts;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// One create, as the desktop app or the web app sends it.
async fn create(
    app: &App,
    session: &str,
    workspace: &str,
    model: &str,
    data: Value,
) -> common::Response {
    app.sync(
        session,
        workspace,
        json!([{ "action": "create", "model": model, "id": Uuid::now_v7(), "data": data }]),
    )
    .await
}

fn repository(remote: &str) -> Value {
    json!({ "remote_key": remote, "name": "api", "default_branch": "main" })
}

fn note(repo: &str, text: &str) -> Value {
    json!({ "repo": repo, "text": text, "source": "agent" })
}

#[sqlx::test(migrations = "../../migrations")]
async fn setting_up_layer_makes_the_first_workspace(pool: PgPool) {
    let app = App::new(pool);
    let setup = app.call("GET", "/api/auth/providers", None, None).await;
    assert_eq!(setup.body["setup"], true);

    let (session, _) = app.signup("Ada", "Code Villain").await;
    let me = app.get("/api/me", &session).await;
    assert_eq!(me.body["workspaces"][0]["url_key"], "code-villain");
    assert_eq!(me.body["workspaces"][0]["role"], "admin");

    // Layer is set up once; everyone after comes with an invite.
    let again = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Eve", "email": "eve@example.com", "password": "correct horse", "workspace_name": "Mine" }),
        )
        .await;
    assert_eq!(again.status, StatusCode::FORBIDDEN, "{}", again.body);
    let setup = app.call("GET", "/api/auth/providers", None, None).await;
    assert_eq!(setup.body["setup"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invite_links_let_people_join_and_share_the_repositories(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    let added = create(
        &app,
        &ada,
        &workspace,
        "Repository",
        repository("git@github.com:code-villain/api.git"),
    )
    .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);

    let token = app.invite(&ada, &workspace, "member").await;
    let grace = app.join("Grace", &token).await;
    let me = app.get("/api/me", &grace).await;
    assert_eq!(me.body["workspaces"][0]["id"], workspace.as_str());
    assert_eq!(me.body["workspaces"][0]["role"], "member");

    let repositories = app.records(&grace, &workspace, "Repository").await;
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0]["remote_key"], "github.com/code-villain/api");
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_routes_need_membership(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let token = app.invite(&ada, &studio, "member").await;
    let grace = app.join("Grace", &token).await;

    // A second workspace, only Ada's.
    let ada_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'ada@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut db = pool.begin().await.unwrap();
    let side = accounts::create_workspace(&mut db, "Side project", ada_id)
        .await
        .unwrap();
    db.commit().await.unwrap();

    for path in [
        format!("/api/w/{side}/sync/bootstrap"),
        format!("/api/w/{side}/members"),
        format!("/api/w/{side}/invites"),
    ] {
        assert_eq!(
            app.get(&path, &grace).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let write = create(
        &app,
        &grace,
        &side.to_string(),
        "Repository",
        repository("github.com/ada/side"),
    )
    .await;
    assert_eq!(write.status, StatusCode::NOT_FOUND, "{}", write.body);
    assert!(
        app.records(&ada, &side.to_string(), "Repository")
            .await
            .is_empty()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn members_write_repositories_and_notes_and_admins_the_connections(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    let token = app.invite(&ada, &workspace, "member").await;
    let grace = app.join("Grace", &token).await;

    let repo = create(
        &app,
        &grace,
        &workspace,
        "Repository",
        repository("https://github.com/code-villain/api"),
    )
    .await;
    assert_eq!(repo.status, StatusCode::OK, "{}", repo.body);
    let remembered = create(
        &app,
        &grace,
        &workspace,
        "Note",
        note(
            "github.com/code-villain/api",
            "Run the migrations before the tests.",
        ),
    )
    .await;
    assert_eq!(remembered.status, StatusCode::OK, "{}", remembered.body);
    let models = create(
        &app,
        &grace,
        &workspace,
        "AgentModel",
        json!({
            "agent_id": "claude", "models": [{ "value": "opus", "name": "Opus" }],
        }),
    )
    .await;
    assert_eq!(models.status, StatusCode::OK, "{}", models.body);

    let jira = json!({ "kind": "jira", "settings": { "base_url": "https://acme.atlassian.net", "flow": {} } });
    let refused = create(&app, &grace, &workspace, "Connection", jira.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
    let logs = json!({ "name": "Requests", "bucket": "acme-logs", "prefix": "requests/" });
    let refused = create(&app, &grace, &workspace, "LogSource", logs.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);

    let connected = create(&app, &ada, &workspace, "Connection", jira).await;
    assert_eq!(connected.status, StatusCode::OK, "{}", connected.body);
    let sourced = create(&app, &ada, &workspace, "LogSource", logs).await;
    assert_eq!(sourced.status, StatusCode::OK, "{}", sourced.body);
    assert_eq!(app.records(&grace, &workspace, "Connection").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn no_token_or_email_reaches_a_shared_connection(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    for settings in [
        json!({ "base_url": "https://acme.atlassian.net", "flow": {}, "token": "secret" }),
        json!({ "base_url": "https://acme.atlassian.net", "flow": {}, "email": "ada@acme.com" }),
    ] {
        let refused = create(
            &app,
            &ada,
            &workspace,
            "Connection",
            json!({ "kind": "jira", "settings": settings }),
        )
        .await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            refused.body
        );
    }

    // Nor through a change: the settings are checked against the record's kind.
    let id = Uuid::now_v7();
    let connected = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "create", "model": "Connection", "id": id, "data": {
                "kind": "slack", "settings": { "channel": "#layer" },
            }}]),
        )
        .await;
    assert_eq!(connected.status, StatusCode::OK, "{}", connected.body);
    let changed = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "update", "model": "Connection", "id": id, "data": {
                "settings": { "channel": "#layer", "bot_token": "xoxb-secret" },
            }}]),
        )
        .await;
    assert_eq!(
        changed.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        changed.body
    );
    let as_jira = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "update", "model": "Connection", "id": id, "data": {
                "settings": { "base_url": "https://acme.atlassian.net", "flow": {} },
            }}]),
        )
        .await;
    assert_eq!(
        as_jira.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        as_jira.body
    );
    let kept = app.records(&ada, &workspace, "Connection").await;
    assert_eq!(kept[0]["settings"], json!({ "channel": "#layer" }));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_repository_is_one_whichever_way_it_was_cloned(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    let token = app.invite(&ada, &workspace, "member").await;
    let grace = app.join("Grace", &token).await;

    let added = create(
        &app,
        &ada,
        &workspace,
        "Repository",
        repository("git@github.com:code-villain/api.git"),
    )
    .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    let again = create(
        &app,
        &grace,
        &workspace,
        "Repository",
        repository("https://github.com/code-villain/api"),
    )
    .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.body);
    assert!(
        again
            .body
            .to_string()
            .contains("already has that repository"),
        "{}",
        again.body
    );

    let repositories = app.records(&ada, &workspace, "Repository").await;
    let ada_id = app.get("/api/me", &ada).await.body["user"]["id"].clone();
    assert_eq!(repositories[0]["added_by"], ada_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn null_clears_a_repositorys_settings(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    let id = Uuid::now_v7();
    let mut data = repository("github.com/code-villain/api");
    data["group_name"] = json!("backend");
    data["check_command"] = json!("bun run check");
    let added = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "create", "model": "Repository", "id": id, "data": data }]),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);

    let cleared = app
        .sync(
            &ada,
            &workspace,
            json!([{ "action": "update", "model": "Repository", "id": id,
            "data": { "group_name": null, "check_command": null } }]),
        )
        .await;
    assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.body);
    let repository = &app.records(&ada, &workspace, "Repository").await[0];
    assert_eq!(repository["group_name"], Value::Null);
    assert_eq!(repository["check_command"], Value::Null);
    assert_eq!(repository["name"], "api");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_repository_keeps_at_most_fifty_notes(pool: PgPool) {
    let app = App::new(pool);
    let (ada, workspace) = app.signup("Ada", "Code Villain").await;
    let operations: Vec<Value> = (0..50)
        .map(|n| {
            json!({ "action": "create", "model": "Note", "id": Uuid::now_v7(),
                         "data": note("github.com/code-villain/api", &format!("Note {n}.")) })
        })
        .collect();
    let fifty = app.sync(&ada, &workspace, Value::Array(operations)).await;
    assert_eq!(fifty.status, StatusCode::OK, "{}", fifty.body);

    let one_more = create(
        &app,
        &ada,
        &workspace,
        "Note",
        note("github.com/code-villain/api", "One more."),
    )
    .await;
    assert_eq!(
        one_more.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        one_more.body
    );
    // Another repository's notes are counted apart.
    let other = create(
        &app,
        &ada,
        &workspace,
        "Note",
        note("github.com/code-villain/web", "Its own."),
    )
    .await;
    assert_eq!(other.status, StatusCode::OK, "{}", other.body);
}
