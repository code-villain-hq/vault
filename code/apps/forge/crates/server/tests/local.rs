//! Workspaces on this Mac: one person with no password, the app's own
//! server taking no sign-ins, and `serve` stopping when asked.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::App;
use forge_server::{AppState, local};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn starting_on_this_mac_makes_you_admin_of_a_workspace(pool: PgPool) {
    assert_eq!(local::person(&pool).await.unwrap(), None);
    let person = local::start(&pool, "Ada Lovelace", "Notes").await.unwrap();
    assert_eq!(local::person(&pool).await.unwrap(), Some(person));
    local::new_workspace(&pool, person, "Side project")
        .await
        .unwrap();

    let app = App::with_state(AppState::new(pool.clone(), false).on_this_mac());
    let session = local::session(&pool, person).await.unwrap();
    let me = app.get("/api/me", &session).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["user"]["name"], "Ada Lovelace");
    let workspaces = me.body["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 2);
    assert!(workspaces.iter().all(|w| w["role"] == "admin"));

    // Each launch gets its own session; the last one still works.
    let again = local::session(&pool, person).await.unwrap();
    assert_ne!(again, session);
    assert_eq!(app.get("/api/me", &session).await.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_apps_own_server_takes_no_sign_ups_or_sign_ins(pool: PgPool) {
    let app = App::with_state(AppState::new(pool.clone(), false).on_this_mac());
    let providers = app.call("GET", "/api/auth/providers", None, None).await;
    assert_eq!(
        providers.body,
        json!({ "oidc": false, "password": false, "github": false })
    );

    let signup = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Eve", "email": "eve@example.com", "password": "correct horse", "workspace_name": "Mine" }),
        )
        .await;
    assert_eq!(signup.status, StatusCode::NOT_FOUND);

    local::start(&pool, "Ada", "Notes").await.unwrap();
    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": local::EMAIL, "password": "" }),
        )
        .await;
    assert_eq!(login.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn serve_stops_when_asked(pool: PgPool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(forge_server::serve(
        AppState::new(pool, false).on_this_mac(),
        None,
        listener,
        async {
            let _ = stopped.await;
        },
    ));

    let providers = reqwest::get(format!("http://{address}/api/auth/providers"))
        .await
        .unwrap();
    assert_eq!(providers.status(), StatusCode::OK);

    stop.send(()).unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(10), server).await;
    assert!(stopped.unwrap().unwrap().is_ok());
}
