mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::App;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

#[sqlx::test(migrations = "../../migrations")]
async fn attachments_and_tickets_follow_their_original_issue_access(pool: PgPool) {
    let data = tempfile::tempdir().unwrap();
    let app = App::with_state(
        forge_server::AppState::new(pool.clone(), false).with_data_dir(data.path()),
    );
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let guest = app
        .join("Guest", &app.invite(&ada, &workspace, "guest").await)
        .await;
    let user = app.get("/api/me", &guest).await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let team: Uuid = sqlx::query_scalar("SELECT id FROM teams WHERE workspace_id = $1")
        .bind(workspace.parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    let issue = Uuid::now_v7();
    let response = app.post(&format!("/api/w/{workspace}/sync/transactions"), Some(&ada),
        json!({ "id": Uuid::now_v7(), "operations": [{ "action": "create", "model": "Issue", "id": issue,
          "data": { "team_id": team, "title": "Private attachment" } }] })).await;
    assert_eq!(response.status, StatusCode::OK);
    let uploaded = app
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/w/{workspace}/attachments?name=private.txt&issue_id={issue}"
            ))
            .header(header::COOKIE, format!("forge_session={ada}"))
            .body(Body::from("private contents"))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(uploaded.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(uploaded.into_body(), usize::MAX)
        .await
        .unwrap();
    let uploaded: Value = serde_json::from_slice(&bytes).unwrap();
    let url = uploaded["url"].as_str().unwrap();
    assert_eq!(app.get(url, &guest).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.post(&format!("{url}/link"), Some(&guest), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let access = format!("/api/w/{workspace}/members/{user}/access");
    assert_eq!(
        app.call("PUT", &access, Some(&ada), Some(json!({"items":[team]})))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(app.get(url, &guest).await.status, StatusCode::OK);
    let ticket = app
        .post(&format!("{url}/link"), Some(&guest), json!({}))
        .await;
    assert_eq!(ticket.status, StatusCode::OK);
    assert_eq!(
        app.call("PUT", &access, Some(&ada), Some(json!({"items":[]})))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(app.get(url, &guest).await.status, StatusCode::NOT_FOUND);
    let ticketed = app
        .call("GET", ticket.body["url"].as_str().unwrap(), None, None)
        .await;
    assert_eq!(
        ticketed.status,
        StatusCode::NOT_FOUND,
        "previously issued tickets recheck access"
    );
    assert_eq!(app.get(url, &ada).await.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_draft_attachment_is_bound_only_by_its_uploader(pool: PgPool) {
    let data = tempfile::tempdir().unwrap();
    let app = App::with_state(
        forge_server::AppState::new(pool.clone(), false).with_data_dir(data.path()),
    );
    let (ada, workspace) = app.signup("Ada", "Studio").await;
    let team: Uuid = sqlx::query_scalar("SELECT id FROM teams WHERE workspace_id = $1")
        .bind(workspace.parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    let uploaded = app
        .router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/w/{workspace}/attachments?name=draft.txt&team_id={team}"
            ))
            .header(header::COOKIE, format!("forge_session={ada}"))
            .body(Body::from("draft"))
            .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(uploaded.into_body(), usize::MAX)
        .await
        .unwrap();
    let uploaded: Value = serde_json::from_slice(&bytes).unwrap();
    let bob = app
        .join("Bob", &app.invite(&ada, &workspace, "member").await)
        .await;
    let copied_issue = Uuid::now_v7();
    let copied = app.post(&format!("/api/w/{workspace}/sync/transactions"), Some(&bob),
        json!({ "id": Uuid::now_v7(), "operations": [{ "action":"create", "model":"Issue", "id":copied_issue,
          "data": {"team_id":team,"title":"Copied URL","description":format!("[draft]({})",uploaded["url"].as_str().unwrap())} }] })).await;
    assert_eq!(copied.status, StatusCode::OK, "{}", copied.body);
    let copied_parent: Option<Uuid> =
        sqlx::query_scalar("SELECT issue_id FROM attachments WHERE id = $1")
            .bind(uploaded["id"].as_str().unwrap().parse::<Uuid>().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        copied_parent, None,
        "another member cannot claim a draft file"
    );
    let issue = Uuid::now_v7();
    let response = app.post(&format!("/api/w/{workspace}/sync/transactions"), Some(&ada),
        json!({ "id": Uuid::now_v7(), "operations": [{ "action":"create", "model":"Issue", "id":issue,
          "data": {"team_id":team,"title":"Draft","description":format!("[draft]({})",uploaded["url"].as_str().unwrap())} }] })).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    let parent: Option<Uuid> = sqlx::query_scalar("SELECT issue_id FROM attachments WHERE id = $1")
        .bind(uploaded["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(parent, Some(issue));
}
