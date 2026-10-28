//! Almanac on this Mac: one person with no password, a first workspace that
//! starts empty, the app's own server taking no sign-ins, and `serve`
//! telling the Mac when a reminder comes due and stopping when asked.

mod common;

use std::time::Duration;

use almanac_server::claude::Claude;
use almanac_server::local;
use axum::http::StatusCode;
use common::App;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "../../migrations")]
async fn starting_on_this_mac_makes_an_empty_home(pool: PgPool) {
    assert_eq!(local::person(&pool).await.unwrap(), None);
    let person = local::start(&pool, "Ada Lovelace", "Home").await.unwrap();
    assert_eq!(local::person(&pool).await.unwrap(), Some(person));
    let side = local::new_workspace(&pool, person, "Side project")
        .await
        .unwrap();

    let app = App::with_state(common::state(pool.clone()).on_this_mac());
    let session = local::session(&pool, person).await.unwrap();
    let me = app.get("/api/me", &session).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["user"]["name"], "Ada Lovelace");
    let workspaces = me.body["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 2);
    assert!(workspaces.iter().all(|w| w["role"] == "admin"));
    assert!(
        app.synced(&session, &side.to_string(), "Topic")
            .await
            .is_empty()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_apps_own_server_takes_no_sign_ups_or_sign_ins(pool: PgPool) {
    let app = App::with_state(common::state(pool.clone()).on_this_mac());
    let providers = app.call("GET", "/api/auth/providers", None, None).await;
    assert_eq!(providers.body, json!({ "oidc": false, "company": null }));

    let signup = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Eve", "email": "eve@example.com", "password": "correct horse", "workspace_name": "Mine" }),
        )
        .await;
    assert!(signup.status.is_client_error(), "{}", signup.status);

    local::start(&pool, "Ada", "Home").await.unwrap();
    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": local::EMAIL, "password": "" }),
        )
        .await;
    assert!(login.status.is_client_error(), "{}", login.status);
}

#[sqlx::test(migrations = "../../migrations")]
async fn serve_tells_the_mac_once_per_due_date_and_stops_when_asked(pool: PgPool) {
    let person = local::start(&pool, "Ada", "Home").await.unwrap();
    let workspace: Uuid = sqlx::query_scalar("SELECT workspace_id FROM workspace_members")
        .fetch_one(&pool)
        .await
        .unwrap();
    let topic = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO topics (id, workspace_id, name, position) VALUES ($1, $2, 'Pflanzen', 1)",
    )
    .bind(topic)
    .bind(workspace)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO reminders (id, workspace_id, topic_id, title, due_on, every_days) \
         VALUES ($1, $2, $3, 'Water the monstera', current_date - 1, 7)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace)
    .bind(topic)
    .execute(&pool)
    .await
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = almanac_server::AppState::new(pool.clone(), false, Claude::new(None)).on_this_mac();
    let mut due = state.due.subscribe();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(almanac_server::serve(state, None, listener, async {
        let _ = stopped.await;
    }));

    let providers = reqwest::get(format!("http://{address}/api/auth/providers"))
        .await
        .unwrap();
    assert_eq!(providers.status(), StatusCode::OK);

    let told = tokio::time::timeout(Duration::from_secs(10), due.recv())
        .await
        .expect("a due reminder is told")
        .unwrap();
    assert_eq!(told.title, "Water the monstera");
    assert_eq!(told.topic, "Pflanzen");
    let marked: bool = sqlx::query_scalar("SELECT notified_for = due_on FROM reminders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(marked, "told once for this due date");
    let _ = person;

    stop.send(()).unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(10), server).await;
    assert!(stopped.unwrap().unwrap().is_ok());
}
