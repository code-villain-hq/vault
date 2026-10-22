//! Raven on this Mac: one person with no password, a first workspace that
//! watches the apps on this Mac, the app's own server taking no sign-ins,
//! and `serve` checking itself and stopping when asked.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::App;
use raven_server::local;
use raven_server::telemetry::Recorder;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn starting_on_this_mac_watches_the_apps_on_this_mac(pool: PgPool) {
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

    let first = workspaces[0]["id"].as_str().unwrap();
    let keys: Vec<_> = app
        .synced_apps(&session, first)
        .await
        .iter()
        .map(|a| a["key"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys, ["raven", "forge", "glyph", "forge-dev", "glyph-dev"]);
    // Another workspace starts empty, as a sign-up's.
    assert!(
        app.synced_apps(&session, &side.to_string())
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
async fn serve_checks_itself_records_its_calls_and_stops_when_asked(pool: PgPool) {
    local::start(&pool, "Ada", "This Mac").await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (recorder, records) = Recorder::new("test");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(raven_server::serve(
        raven_server::AppState::new(pool.clone(), false, recorder).on_this_mac(),
        None,
        listener,
        records,
        async {
            let _ = stopped.await;
        },
    ));

    let providers = reqwest::get(format!("http://{address}/api/auth/providers"))
        .await
        .unwrap();
    assert_eq!(providers.status(), StatusCode::OK);

    // Raven checks itself at the address it listens on, and keeps its own
    // calls with the app that is itself.
    let mut seen = (false, false);
    for _ in 0..100 {
        seen = sqlx::query_as(
            "SELECT \
               EXISTS (SELECT 1 FROM health_checks h JOIN apps a ON a.id = h.app_id WHERE a.is_self AND h.ok), \
               EXISTS (SELECT 1 FROM spans s JOIN apps a ON a.id = s.app_id \
                       WHERE a.is_self AND s.name = 'GET /api/auth/providers')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if seen == (true, true) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(seen, (true, true), "(checked itself, recorded its call)");

    stop.send(()).unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(10), server).await;
    assert!(stopped.unwrap().unwrap().is_ok());
}
