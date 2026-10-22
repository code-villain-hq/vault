//! The company's account sends its people and groups over SCIM; the
//! workspace follows raven-users, -admins and -guests (or the groups the
//! settings name).

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use code_villain_scim::Groups;
use common::App;
use raven_server::company;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const TOKEN: &str = "a-long-scim-token";

fn app(pool: PgPool) -> App {
    App::with_state(common::state(pool).with_scim(TOKEN))
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

/// The server starting for the company, following `raven-*` groups.
async fn start(app: &App) {
    start_with(app, &Groups::new("raven", None, None, None)).await;
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
    let admins = push_group(&app, "Raven admins", &[&ada_id, &bob]).await;
    push_group(&app, "raven-users", &[]).await;
    push_group(&app, "raven-guests", &[]).await;
    // A group named for the tool alone gives no role.
    let dana = push_user(&app, "sub-dana", "Dana").await;
    push_group(&app, "raven", &[&dana]).await;
    start(&app).await;

    let people = members(&app, &ada, &studio).await;
    assert_eq!(
        people,
        vec![
            ("Ada".into(), "admin".into(), json!("raven-admins")),
            ("Bob".into(), "admin".into(), json!("raven-admins")),
        ],
        "Dana is only in raven"
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
            .contains("raven-admins")
    );
    assert_eq!(
        app.call("DELETE", &path, Some(&ada), None).await.status,
        StatusCode::CONFLICT
    );

    // Moved to raven-users: a member.
    let (status, _) = scim(
        &app,
        "PUT",
        &format!("/Groups/{admins}"),
        Some(group("raven-admins", &[&ada_id])),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let raven = scim(&app, "GET", "/Groups", None).await.1["Resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["displayName"] == "raven-users")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    scim(
        &app,
        "PUT",
        &format!("/Groups/{raven}"),
        Some(group("raven-users", &[&bob])),
    )
    .await;
    assert_eq!(
        members(&app, &ada, &studio).await[1],
        ("Bob".into(), "member".into(), json!("raven-users"))
    );

    // Out of the groups: out of the workspace.
    scim(
        &app,
        "PUT",
        &format!("/Groups/{raven}"),
        Some(group("raven-users", &[])),
    )
    .await;
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
        &Groups::new("raven", Some("Acme Engineers"), Some("acme-leads"), None),
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

    // Before the account has sent any groups, nobody leaves.
    start(&app).await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 2);

    // The account sends Ada and Cleo by their emails.
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let cleo = push_user(&app, "sub-cleo", "Cleo").await;
    let cleo_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'cleo@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cleo, cleo_id.to_string(), "the same account");
    push_group(&app, "raven-admins", &[&ada_id]).await;
    push_group(&app, "raven-guests", &[&cleo]).await;
    app.join("Dan", &app.invite(&ada, &studio, "member").await)
        .await;

    // Starting again, Dan leaves: he's in no group.
    start(&app).await;
    let people = members(&app, &ada, &studio).await;
    let names: Vec<&str> = people.iter().map(|(name, _, _)| name.as_str()).collect();
    assert_eq!(names, ["Ada", "Cleo"]);
    assert_eq!(
        people[1],
        ("Cleo".into(), "guest".into(), json!("raven-guests"))
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn someone_turned_off_is_signed_out_and_leaves(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let ada_id = push_user(&app, "sub-ada", "Ada").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    push_group(&app, "raven-admins", &[&ada_id]).await;
    push_group(&app, "raven-users", &[&bob]).await;
    start(&app).await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 2);

    let (status, _) = scim(&app, "DELETE", &format!("/Users/{bob}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
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
async fn a_new_server_makes_the_companys_workspace(pool: PgPool) {
    let app = app(pool.clone());
    let groups = Groups::new("raven", None, None, None);
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
    let watched = || async {
        sqlx::query_scalar::<_, String>("SELECT key FROM apps WHERE is_self")
            .fetch_all(&pool)
            .await
            .unwrap()
    };
    assert!(watched().await.is_empty(), "nobody to add it yet");

    // Its first person brings Raven itself.
    let ada = push_user(&app, "sub-ada", "Ada").await;
    push_group(&app, "raven-admins", &[&ada]).await;
    assert_eq!(watched().await, ["raven"]);
    let bob = push_user(&app, "sub-bob", "Bob").await;
    push_group(&app, "raven-users", &[&bob]).await;
    assert_eq!(watched().await, ["raven"], "only once");

    // A database with more than one company's is refused.
    let (_, _) = app.signup("Eve", "Elsewhere").await;
    let refused = company::start(&pool, "Acme Inc", &groups)
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("Elsewhere"), "{refused}");
}
