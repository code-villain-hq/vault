//! The company's account sends its people and groups over SCIM; the
//! workspace follows forge-users, -admins and -guests (or the groups the
//! settings name), and a linked team follows its own groups.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use code_villain_scim::Groups;
use common::App;
use forge_server::{AppState, company};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const TOKEN: &str = "a-long-scim-token";

fn app(pool: PgPool) -> App {
    App::with_state(AppState::new(pool, false).with_scim(TOKEN))
}

/// A request as Pocket ID sends it.
async fn scim(app: &App, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    scim_with(app, Some(TOKEN), method, path, body).await
}

async fn scim_with(
    app: &App,
    token: Option<&str>,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/api/scim/v2{path}"))
        .header(header::CONTENT_TYPE, "application/scim+json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn push_user(app: &App, sub: &str, name: &str) -> String {
    let (status, body) = scim(
        app,
        "POST",
        "/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "externalId": sub,
            "userName": name.to_lowercase(),
            "displayName": name,
            "active": true,
            "emails": [{ "value": format!("{}@example.com", name.to_lowercase()), "primary": true }],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_string()
}

async fn push_group(app: &App, name: &str, members: &[&str]) -> String {
    let (status, body) = scim(app, "POST", "/Groups", Some(group(name, members))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_string()
}

fn group(name: &str, members: &[&str]) -> Value {
    json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
        "externalId": format!("pocket-{name}"),
        "displayName": name,
        "members": members.iter().map(|m| json!({ "value": m })).collect::<Vec<_>>(),
    })
}

/// The server starting for the company, following `forge-*` groups.
async fn start(app: &App) {
    start_with(app, &Groups::new("forge", None, None, None)).await;
}

async fn start_with(app: &App, groups: &Groups) {
    company::start(&app.pool, "Studio", groups).await.unwrap();
}

/// Each member's name, role and the group it comes from.
async fn members(app: &App, admin: &str, workspace: &str) -> Vec<(String, String, Value)> {
    let body = app
        .get(&format!("/api/w/{workspace}/members"), admin)
        .await
        .body;
    body.as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["user"]["name"].as_str().unwrap().to_string(),
                m["role"].as_str().unwrap().to_string(),
                m["from_group"].clone(),
            )
        })
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn scim_needs_its_token_and_is_off_without_one(pool: PgPool) {
    let off = App::new(pool.clone());
    assert_ne!(scim(&off, "GET", "/Users", None).await.0, StatusCode::OK);
    let on = app(pool);
    assert_eq!(
        scim_with(&on, None, "GET", "/Users", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        scim_with(&on, Some("wrong"), "GET", "/Users", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, list) = scim(&on, "GET", "/Users?startIndex=1&count=1000", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["totalResults"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_workspace_follows_the_companys_groups(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    let admins = push_group(&app, "Forge admins", &[&ada_id, &bob]).await;
    push_group(&app, "forge-users", &[]).await;
    push_group(&app, "forge-guests", &[]).await;
    // A group named for the tool alone gives no role.
    let dana = push_user(&app, "sub-dana", "Dana").await;
    push_group(&app, "forge", &[&dana]).await;
    start(&app).await;

    let people = members(&app, &ada, &studio).await;
    assert_eq!(
        people,
        vec![
            ("Ada".into(), "admin".into(), json!("forge-admins")),
            ("Bob".into(), "admin".into(), json!("forge-admins")),
        ],
        "Dana is only in forge"
    );

    // Their role is changed in the account, not here.
    let path = format!("/api/w/{studio}/members/{bob}");
    let refused = app
        .call(
            "PATCH",
            &path,
            Some(&ada),
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert!(
        refused.body["message"]
            .as_str()
            .unwrap()
            .contains("forge-admins")
    );
    assert_eq!(
        app.call("DELETE", &path, Some(&ada), None).await.status,
        StatusCode::CONFLICT
    );

    // Moved to forge-users: a member.
    let (status, _) = scim(
        &app,
        "PUT",
        &format!("/Groups/{admins}"),
        Some(group("forge-admins", &[&ada_id])),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let forge = scim(&app, "GET", "/Groups", None).await.1["Resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["displayName"] == "forge-users")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    scim(
        &app,
        "PUT",
        &format!("/Groups/{forge}"),
        Some(group("forge-users", &[&bob])),
    )
    .await;
    assert_eq!(
        members(&app, &ada, &studio).await[1],
        ("Bob".into(), "member".into(), json!("forge-users"))
    );

    // Out of the groups: out of the workspace.
    scim(
        &app,
        "PUT",
        &format!("/Groups/{forge}"),
        Some(group("forge-users", &[])),
    )
    .await;
    settle(&app).await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_settings_can_name_other_groups(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    push_group(&app, "Acme Leads", &[&ada_id]).await;
    push_group(&app, "acme-engineers", &[&bob]).await;
    start_with(
        &app,
        &Groups::new("forge", Some("Acme Engineers"), Some("acme-leads"), None),
    )
    .await;
    assert_eq!(
        members(&app, &ada, &studio).await,
        vec![
            ("Ada".into(), "admin".into(), json!("acme-leads")),
            ("Bob".into(), "member".into(), json!("acme-engineers")),
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_outside_the_groups_leave_once_the_account_sends_them(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    app.join("Cleo", &app.invite(&ada, &studio, "member").await)
        .await;
    let agent = app
        .post(
            &format!("/api/w/{studio}/agents"),
            Some(&ada),
            json!({ "name": "Claude Code" }),
        )
        .await;
    assert_eq!(agent.status, StatusCode::OK, "{}", agent.body);

    // Before the account has sent any groups, nobody leaves.
    start(&app).await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 3);

    // The account sends Ada and Cleo by their emails.
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let cleo = push_user(&app, "sub-cleo", "Cleo").await;
    let cleo_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'cleo@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cleo, cleo_id.to_string(), "the same account");
    push_group(&app, "forge-admins", &[&ada_id]).await;
    push_group(&app, "forge-guests", &[&cleo]).await;
    app.join("Dan", &app.invite(&ada, &studio, "member").await)
        .await;

    // Starting again, Dan leaves: he's in no group. The agent stays.
    start(&app).await;
    let people = members(&app, &ada, &studio).await;
    let names: Vec<&str> = people.iter().map(|(name, _, _)| name.as_str()).collect();
    assert_eq!(names, ["Ada", "Cleo", "Claude Code"]);
    assert_eq!(
        people[1],
        ("Cleo".into(), "guest".into(), json!("forge-guests"))
    );

    // Agents' addresses aren't for people.
    let (status, _) = scim(
        &app,
        "POST",
        "/Users",
        Some(json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "externalId": "sub-mallory",
            "userName": "mallory",
            "active": true,
            "emails": [{ "value": format!("forge-ai@{studio}.agents.forge.invalid"), "primary": true }],
        })),
    )
    .await;
    assert!(status.is_client_error(), "{status}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn someone_turned_off_is_signed_out_and_leaves(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    push_group(&app, "forge-admins", &[&ada_id]).await;
    push_group(&app, "forge-users", &[&bob]).await;
    start(&app).await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 2);

    let (status, _) = scim(&app, "DELETE", &format!("/Users/{bob}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    settle(&app).await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 1);
    // The account stays; they can't sign in to it but through the company's.
    let accounts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'bob@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(accounts, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_linked_team_has_its_groups_people(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    let cleo = push_user(&app, "sub-cleo", "Cleo").await;
    push_group(&app, "forge-admins", &[&ada_id]).await;
    push_group(&app, "forge-users", &[&bob, &cleo]).await;
    let design = push_group(&app, "design-users", &[&bob]).await;
    start(&app).await;

    let bootstrap = format!("/api/w/{studio}/sync/bootstrap");
    let team = app.get(&bootstrap, &ada).await.body["records"]["Team"][0]["id"].clone();
    let in_team = |records: &Value| -> Vec<String> {
        let mut people: Vec<String> = records["records"]["TeamMember"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["team_id"] == team)
            .map(|m| m["user_id"].as_str().unwrap().to_string())
            .collect();
        people.sort();
        people
    };
    // Members join every team.
    assert_eq!(in_team(&app.get(&bootstrap, &ada).await.body).len(), 3);

    let linked = app
        .post(
            &format!("/api/w/{studio}/sync/transactions"),
            Some(&ada),
            json!({ "id": uuid::Uuid::now_v7(), "operations": [
                { "action": "update", "model": "Team", "id": team, "data": { "group_name": "design" } }
            ]}),
        )
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.body);
    assert_eq!(
        in_team(&app.get(&bootstrap, &ada).await.body),
        vec![bob.clone()]
    );

    // Nobody joins it by hand.
    let access = app
        .call(
            "PUT",
            &format!("/api/w/{studio}/members/{cleo}/access"),
            Some(&ada),
            Some(json!({ "items": [team] })),
        )
        .await;
    assert_eq!(access.status, StatusCode::NO_CONTENT);
    assert_eq!(
        in_team(&app.get(&bootstrap, &ada).await.body),
        vec![bob.clone()]
    );

    // The group changes, and the team with it.
    scim(
        &app,
        "PUT",
        &format!("/Groups/{design}"),
        Some(group("design-users", &[&cleo])),
    )
    .await;
    assert_eq!(
        in_team(&app.get(&bootstrap, &ada).await.body),
        vec![cleo.clone()]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_server_makes_the_companys_workspace(pool: PgPool) {
    let app = app(pool.clone());
    let groups = Groups::new("forge", None, None, None);
    let workspace = company::start(&pool, " Acme Inc ", &groups).await.unwrap();
    assert_eq!(
        company::start(&pool, "Acme Inc", &groups).await.unwrap(),
        workspace,
        "the same one on every start"
    );
    let (name, url_key): (String, String) =
        sqlx::query_as("SELECT name, url_key FROM workspaces WHERE id = $1")
            .bind(workspace)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), url_key.as_str()), ("Acme Inc", "acme-inc"));
    let teams = || async {
        sqlx::query_scalar::<_, String>("SELECT name FROM teams")
            .fetch_all(&pool)
            .await
            .unwrap()
    };
    assert!(teams().await.is_empty(), "nobody to make it yet");

    // Its first person brings its first team.
    let ada = push_user(&app, "sub-ada", "Ada").await;
    push_group(&app, "forge-admins", &[&ada]).await;
    assert_eq!(teams().await, ["Acme Inc"]);
    let bob = push_user(&app, "sub-bob", "Bob").await;
    push_group(&app, "forge-users", &[&bob]).await;
    assert_eq!(teams().await, ["Acme Inc"], "only once");

    // A database with more than one company's is refused.
    let (_, _) = app.signup("Eve", "Elsewhere").await;
    let refused = company::start(&pool, "Acme Inc", &groups)
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("Elsewhere"), "{refused}");
}

async fn settle(app: &App) {
    sqlx::query("UPDATE scim_departures SET remove_after = now() - interval '1 second'")
        .execute(&app.pool)
        .await
        .unwrap();
    code_villain_scim::settle_departures::<forge_server::groups::ForgeDirectory>(&app.pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_role_transfer_keeps_the_session_but_a_departure_ends_it(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    let admins = push_group(&app, "forge-admins", &[&ada_id, &bob]).await;
    let users = push_group(&app, "forge-users", &[]).await;
    start(&app).await;
    let bob_id: uuid::Uuid = bob.parse().unwrap();
    let token = forge_server::auth::start_session(&pool, bob_id)
        .await
        .unwrap();
    let issue = uuid::Uuid::now_v7();
    let team: uuid::Uuid = sqlx::query_scalar("SELECT id FROM teams WHERE workspace_id = $1")
        .bind(studio.parse::<uuid::Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    let created = app.post(&format!("/api/w/{studio}/sync/transactions"), Some(&ada),
      json!({ "id": uuid::Uuid::now_v7(), "operations": [{ "action": "create", "model": "Issue", "id": issue,
        "data": { "team_id": team, "title": "Keep my assignment", "assignee_id": bob_id } }] })).await;
    assert_eq!(created.status, StatusCode::OK);
    scim(
        &app,
        "PUT",
        &format!("/Groups/{admins}"),
        Some(group("forge-admins", &[&ada_id])),
    )
    .await;
    assert_eq!(app.get("/api/me", &token).await.status, StatusCode::OK);
    scim(
        &app,
        "PUT",
        &format!("/Groups/{users}"),
        Some(group("forge-users", &[&bob])),
    )
    .await;
    settle(&app).await;
    assert_eq!(app.get("/api/me", &token).await.status, StatusCode::OK);
    let assignee: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT assignee_id FROM issues WHERE id = $1")
            .bind(issue)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(assignee, Some(bob_id));
    scim(
        &app,
        "PUT",
        &format!("/Groups/{users}"),
        Some(group("forge-users", &[])),
    )
    .await;
    settle(&app).await;
    assert_eq!(
        app.get("/api/me", &token).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(members(&app, &ada, &studio).await.len(), 1);
}
