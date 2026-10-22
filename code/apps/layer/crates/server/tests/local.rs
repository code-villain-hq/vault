//! Layer on this Mac: one person with no password, workspaces of their own,
//! the app's own server taking no sign-ins, and `serve` stopping when asked.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::App;
use layer_server::local;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn starting_on_this_mac_makes_one_person_with_their_workspaces(pool: PgPool) {
    assert_eq!(local::person(&pool).await.unwrap(), None);
    let person = local::start(&pool, "Ada Lovelace", "This Mac")
        .await
        .unwrap();
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

    // The person is admin, so they set up the connections too.
    let connected = app
        .sync(&session, &side.to_string(), json!([{ "action": "create", "model": "Connection",
            "id": uuid::Uuid::now_v7(),
            "data": { "kind": "github", "settings": { "api_url": "https://api.github.com", "web_url": "https://github.com" } },
        }]))
        .await;
    assert_eq!(connected.status, StatusCode::OK, "{}", connected.body);
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

    local::start(&pool, "Ada", "This Mac").await.unwrap();
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
async fn serve_answers_and_stops_when_asked(pool: PgPool) {
    local::start(&pool, "Ada", "This Mac").await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(layer_server::serve(
        common::state(pool.clone()).on_this_mac(),
        layer_server::Web::None,
        listener,
        async {
            let _ = stopped.await;
        },
    ));

    let health = reqwest::get(format!("http://{address}/api/health"))
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    stop.send(()).unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(10), server).await;
    assert!(stopped.unwrap().unwrap().is_ok());
}
