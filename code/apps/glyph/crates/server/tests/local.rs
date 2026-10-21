//! Workspaces on this Mac: one person with no password, the app's own
//! server taking no sign-ins, models added in Settings, and `serve`
//! stopping when asked.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::App;
use glyph_llm::Models;
use glyph_server::{AppState, local};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn starting_on_this_mac_makes_you_admin_of_a_workspace(pool: PgPool) {
    assert_eq!(local::person(&pool).await.unwrap(), None);
    let person = local::start(&pool, "Ada Lovelace", "Docs").await.unwrap();
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
    assert_eq!(providers.body, json!({ "oidc": false, "password": false }));

    let signup = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Eve", "email": "eve@example.com", "password": "correct horse", "workspace_name": "Mine" }),
        )
        .await;
    assert_eq!(signup.status, StatusCode::NOT_FOUND);

    local::start(&pool, "Ada", "Docs").await.unwrap();
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
async fn models_added_on_this_mac_are_written_to_its_file(pool: PgPool) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("glyph").join("models.toml");
    let app = App::with_state(
        AppState::new(pool.clone(), false)
            .on_this_mac()
            .with_models_file(&file),
    );
    let person = local::start(&pool, "Ada", "Docs").await.unwrap();
    let session = local::session(&pool, person).await.unwrap();

    let download = json!({
        "name": "viking",
        "hf_repo": "mradermacher/Viking_7B_V.4-GGUF",
        "hf_file": "Viking_7B_V.4.Q4_K_M.gguf",
    });
    let added = app
        .post("/api/models", Some(&session), download.clone())
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    assert_eq!(added.body["name"], "viking");
    assert_eq!(added.body["can_start"], true);
    let endpoint = added.body["endpoint"].as_str().unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:74"), "{endpoint}");

    let again = app.post("/api/models", Some(&session), download).await;
    assert_eq!(again.status, StatusCode::CONFLICT);

    let server = json!({ "name": "ollama", "endpoint": "http://localhost:11434/" });
    let added = app.post("/api/models", Some(&session), server).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    assert_eq!(added.body["endpoint"], "http://127.0.0.1:11434");
    assert_eq!(added.body["can_start"], false);

    let neither = json!({ "name": "nothing" });
    let refused = app.post("/api/models", Some(&session), neither).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);

    let saved = Models::load(&file).unwrap();
    let names: Vec<_> = saved.models.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["viking", "ollama"]);

    let removed = app
        .call("DELETE", "/api/models/viking", Some(&session), None)
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let list = app.get("/api/models", &session).await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);
    assert_eq!(Models::load(&file).unwrap().models.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_server_keeps_its_models_in_its_file(pool: PgPool) {
    let app = App::new(pool);
    let (session, _) = app.signup("Ada", "Docs").await;
    let added = app
        .post(
            "/api/models",
            Some(&session),
            json!({ "name": "ollama", "endpoint": "http://localhost:11434" }),
        )
        .await;
    assert_eq!(added.status, StatusCode::FORBIDDEN);
    let removed = app
        .call("DELETE", "/api/models/ollama", Some(&session), None)
        .await;
    assert_eq!(removed.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn serve_stops_when_asked(pool: PgPool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(glyph_server::serve(
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
