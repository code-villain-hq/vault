//! The Code Villain account's groups: Pocket ID sends people and groups over
//! SCIM, and a workspace linked to a group follows it.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::App;
use layer_server::AppState;
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

async fn link(app: &App, admin: &str, workspace: &str, group: Option<&str>) -> StatusCode {
    app.call(
        "PUT",
        &format!("/api/w/{workspace}/group"),
        Some(admin),
        Some(json!({ "group": group })),
    )
    .await
    .status
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
async fn a_linked_workspace_follows_its_groups(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    let admins = push_group(&app, "layer-admins", &[&bob]).await;
    push_group(&app, "layer", &[]).await;
    push_group(&app, "layer-guests", &[]).await;

    // Only an admin links, and only to a group there is.
    let groups = app.get(&format!("/api/w/{studio}/group"), &ada).await.body;
    assert_eq!(groups["groups"], json!(["layer"]));
    assert_eq!(groups["group"], Value::Null);
    assert_eq!(
        link(&app, &ada, &studio, Some("forje")).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        link(&app, &ada, &studio, Some("layer")).await,
        StatusCode::NO_CONTENT
    );
    let people = members(&app, &ada, &studio).await;
    assert_eq!(
        people[1],
        ("Bob".into(), "admin".into(), json!("layer-admins"))
    );
    assert_eq!(people[0].2, Value::Null, "Ada came by signing up");

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
            .contains("layer-admins")
    );
    assert_eq!(
        app.call("DELETE", &path, Some(&ada), None).await.status,
        StatusCode::CONFLICT
    );

    // Moved to layer: a member.
    let (status, _) = scim(
        &app,
        "PUT",
        &format!("/Groups/{admins}"),
        Some(group("layer-admins", &[])),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let layer = scim(&app, "GET", "/Groups", None).await.1["Resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["displayName"] == "layer")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    scim(
        &app,
        "PUT",
        &format!("/Groups/{layer}"),
        Some(group("layer", &[&bob])),
    )
    .await;
    assert_eq!(
        members(&app, &ada, &studio).await[1],
        ("Bob".into(), "member".into(), json!("layer"))
    );

    // Out of the groups: out of the workspace.
    scim(
        &app,
        "PUT",
        &format!("/Groups/{layer}"),
        Some(group("layer", &[])),
    )
    .await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invited_people_stay_until_a_group_takes_them(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let token = app.invite(&ada, &studio, "member").await;
    app.join("Cleo", &token).await;
    push_group(&app, "layer", &[]).await;
    assert_eq!(
        link(&app, &ada, &studio, Some("layer")).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        members(&app, &ada, &studio).await[1],
        ("Cleo".into(), "member".into(), Value::Null)
    );

    // The account sends Cleo by her email, in layer-guests.
    let cleo = push_user(&app, "sub-cleo", "Cleo").await;
    let cleo_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'cleo@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cleo, cleo_id.to_string(), "the same account");
    push_group(&app, "layer-guests", &[&cleo]).await;
    assert_eq!(
        members(&app, &ada, &studio).await[1],
        ("Cleo".into(), "guest".into(), json!("layer-guests"))
    );

    // Unlinked, everyone stays, as though invited.
    assert_eq!(
        link(&app, &ada, &studio, None).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        members(&app, &ada, &studio).await[1],
        ("Cleo".into(), "guest".into(), Value::Null)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn someone_turned_off_is_signed_out_and_leaves(pool: PgPool) {
    let app = app(pool.clone());
    let (ada, studio) = app.signup("Ada", "Studio").await;
    let bob = push_user(&app, "sub-bob", "Bob").await;
    push_group(&app, "layer", &[&bob]).await;
    link(&app, &ada, &studio, Some("layer")).await;
    assert_eq!(members(&app, &ada, &studio).await.len(), 2);

    let (status, _) = scim(&app, "DELETE", &format!("/Users/{bob}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(members(&app, &ada, &studio).await.len(), 1);
    // The account stays; they can't sign in to it but through Pocket ID.
    let accounts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'bob@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(accounts, 1);
}
