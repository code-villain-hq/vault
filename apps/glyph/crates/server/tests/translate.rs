//! Translating with a model: runs, suggestions and the models API, against
//! a fake model server.

mod common;

use std::time::Duration;

use axum::Json;
use axum::http::StatusCode;
use axum::routing::{get, post};
use common::App;
use glyph_llm::{Model, Models};
use glyph_server::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// A model that translates `x` as `sv:x`, after `delay`.
async fn fake_model(delay: Duration) -> String {
    let reply = move |Json(body): Json<Value>| async move {
        tokio::time::sleep(delay).await;
        let prompt = body["messages"][1]["content"].as_str().unwrap_or_default();
        let quoted = prompt
            .split("Text to translate: ")
            .nth(1)
            .and_then(|rest| rest.lines().next())
            .unwrap_or("\"?\"");
        let text: String = serde_json::from_str(quoted).unwrap_or_default();
        let answer = json!({ "translation": format!("sv:{text}"), "confidence": 0.9 }).to_string();
        Json(json!({ "choices": [{ "message": { "content": answer } }] }))
    };
    let app = axum::Router::new()
        .route("/v1/chat/completions", post(reply))
        .route("/health", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://localhost:{port}")
}

fn models(endpoint: &str) -> Models {
    Models {
        llm_binary: "llama-server".into(),
        models: vec![Model {
            name: "fake".into(),
            endpoint: endpoint.into(),
            ..Model::default()
        }],
    }
}

async fn app_with(pool: PgPool, endpoint: &str) -> App {
    App::with_state(AppState::new(pool, false).with_models(models(endpoint)))
}

/// A workspace with Swedish and three English keys; returns the session and
/// workspace id.
async fn workspace(app: &App) -> (String, String) {
    let (session, ws) = app.signup("Ada", "Studio").await;
    let mut operations = vec![json!({
        "action": "create", "model": "Language", "id": Uuid::now_v7(), "data": { "code": "sv" }
    })];
    for (key, value) in [("hello", "Hello"), ("bye", "Goodbye"), ("thanks", "Thanks")] {
        operations.push(json!({
            "action": "create", "model": "Translation", "id": Uuid::now_v7(),
            "data": { "lang": "en", "namespace": "common", "key": key, "value": value }
        }));
    }
    transact(app, &session, &ws, operations).await;
    (session, ws)
}

async fn transact(app: &App, session: &str, ws: &str, operations: Vec<Value>) -> common::Response {
    app.post(
        &format!("/api/w/{ws}/sync/transactions"),
        Some(session),
        json!({ "id": Uuid::now_v7(), "operations": operations }),
    )
    .await
}

/// Waits for a run to finish and returns it.
async fn finished(pool: &PgPool, id: &str) -> Value {
    let id: Uuid = id.parse().unwrap();
    for _ in 0..250 {
        let run: Value =
            sqlx::query_scalar("SELECT to_jsonb(r.*) FROM translation_runs r WHERE id = $1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap();
        if ["done", "failed", "cancelled"].contains(&run["status"].as_str().unwrap()) {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the run didn't finish");
}

async fn rows(pool: &PgPool, sql: &'static str) -> Vec<Value> {
    sqlx::query_scalar(sql).fetch_all(pool).await.unwrap()
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn a_suggest_run_proposes_the_missing_keys(pool: PgPool) {
    let app = app_with(pool.clone(), &fake_model(Duration::ZERO).await).await;
    let (session, ws) = workspace(&app).await;

    let started = app
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "suggest", "source_lang": "en", "target_lang": "sv" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.body);
    assert_eq!(started.body["total"], 3);
    assert_eq!(started.body["model"], "fake");

    let run = finished(&pool, started.body["id"].as_str().unwrap()).await;
    assert_eq!(run["status"], "done", "{run}");
    assert_eq!(run["done"], 3);

    let suggestions = rows(
        &pool,
        "SELECT jsonb_build_object('key', key, 'value', value, 'model', model) FROM suggestions ORDER BY key",
    )
    .await;
    assert_eq!(
        suggestions,
        [
            json!({ "key": "bye", "value": "sv:Goodbye", "model": "fake" }),
            json!({ "key": "hello", "value": "sv:Hello", "model": "fake" }),
            json!({ "key": "thanks", "value": "sv:Thanks", "model": "fake" }),
        ]
    );

    // Everything waiting already: nothing left to suggest.
    let again = app
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "suggest", "source_lang": "en", "target_lang": "sv" }),
        )
        .await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Clients hear about it all through sync.
    let log = rows(
        &pool,
        "SELECT to_jsonb(model) FROM sync_log WHERE model IN ('Suggestion', 'TranslationRun')",
    )
    .await;
    assert!(log.iter().filter(|m| *m == "Suggestion").count() == 3);
    assert!(log.iter().any(|m| m == "TranslationRun"));
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn accepting_writes_the_translation_and_rejecting_only_deletes(pool: PgPool) {
    let app = app_with(pool.clone(), &fake_model(Duration::ZERO).await).await;
    let (session, ws) = workspace(&app).await;
    let started = app
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "suggest", "source_lang": "en", "target_lang": "sv",
                    "keys": [{ "namespace": "common", "key": "hello" }, { "namespace": "common", "key": "bye" }] }),
        )
        .await;
    finished(&pool, started.body["id"].as_str().unwrap()).await;
    let ids: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, key FROM suggestions ORDER BY key")
        .fetch_all(&pool)
        .await
        .unwrap();
    let (bye, hello) = (ids[0].0, ids[1].0);

    // Clients can't make suggestions up, or change them.
    let made_up = transact(
        &app,
        &session,
        &ws,
        vec![json!({
            "action": "update", "model": "Suggestion", "id": hello, "data": { "value": "Hej" }
        })],
    )
    .await;
    assert_eq!(made_up.status, StatusCode::UNPROCESSABLE_ENTITY);

    let accepted = transact(
        &app,
        &session,
        &ws,
        vec![
            json!({ "action": "create", "model": "Translation", "id": Uuid::now_v7(),
                "data": { "lang": "sv", "namespace": "common", "key": "hello", "value": "sv:Hello",
                          "confidence": 0.9, "translated_by": "fake" } }),
            json!({ "action": "delete", "model": "Suggestion", "id": hello }),
            json!({ "action": "delete", "model": "Suggestion", "id": bye }),
        ],
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.body);
    let sv = rows(&pool, "SELECT jsonb_build_object('key', key, 'by', translated_by) FROM translations WHERE lang = 'sv'").await;
    assert_eq!(sv, [json!({ "key": "hello", "by": "fake" })]);
    assert!(
        rows(&pool, "SELECT to_jsonb(id) FROM suggestions")
            .await
            .is_empty()
    );
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn a_fill_run_writes_translations_and_keeps_what_people_wrote(pool: PgPool) {
    let app = app_with(pool.clone(), &fake_model(Duration::ZERO).await).await;
    let (session, ws) = workspace(&app).await;
    transact(
        &app,
        &session,
        &ws,
        vec![json!({
            "action": "create", "model": "Translation", "id": Uuid::now_v7(),
            "data": { "lang": "sv", "namespace": "common", "key": "thanks", "value": "Tack" }
        })],
    )
    .await;

    let started = app
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "fill", "source_lang": "en", "target_lang": "sv", "namespace": "common" }),
        )
        .await;
    assert_eq!(started.body["total"], 2, "{}", started.body);
    let run = finished(&pool, started.body["id"].as_str().unwrap()).await;
    assert_eq!(run["status"], "done");

    let sv = rows(
        &pool,
        "SELECT jsonb_build_object('key', key, 'value', value, 'by', translated_by, 'sure', confidence) \
         FROM translations WHERE lang = 'sv' ORDER BY key",
    )
    .await;
    assert_eq!(
        sv,
        [
            json!({ "key": "bye", "value": "sv:Goodbye", "by": "fake", "sure": 0.9 }),
            json!({ "key": "hello", "value": "sv:Hello", "by": "fake", "sure": 0.9 }),
            json!({ "key": "thanks", "value": "Tack", "by": "human", "sure": null }),
        ]
    );
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn a_run_without_its_model_fails_and_says_why(pool: PgPool) {
    // Nothing listens on port 9 here.
    let app = app_with(pool.clone(), "http://127.0.0.1:9").await;
    let (session, ws) = workspace(&app).await;
    let started = app
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "suggest", "source_lang": "en", "target_lang": "sv" }),
        )
        .await;
    let run = finished(&pool, started.body["id"].as_str().unwrap()).await;
    assert_eq!(run["status"], "failed");
    assert!(
        run["error"]
            .as_str()
            .unwrap()
            .contains("Couldn't reach the model"),
        "{run}"
    );

    // With no models at all, nothing starts.
    let bare = App::new(pool.clone());
    let (session, ws) = bare.signup("Bo", "Other").await;
    let refused = bare
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "suggest", "source_lang": "en", "target_lang": "sv" }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refused.body["message"]
            .as_str()
            .unwrap()
            .contains("models file")
    );
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn a_cancelled_run_stops_after_the_key_it_is_on(pool: PgPool) {
    let app = app_with(pool.clone(), &fake_model(Duration::from_millis(300)).await).await;
    let (session, ws) = workspace(&app).await;
    let started = app
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "suggest", "source_lang": "en", "target_lang": "sv" }),
        )
        .await;
    let id = started.body["id"].as_str().unwrap();
    let cancelled = app
        .post(
            &format!("/api/w/{ws}/runs/{id}/cancel"),
            Some(&session),
            json!({}),
        )
        .await;
    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
    assert_eq!(cancelled.body["status"], "cancelled");

    tokio::time::sleep(Duration::from_millis(700)).await;
    let run = finished(&pool, id).await;
    assert_eq!(run["status"], "cancelled");
    assert!(run["done"].as_i64().unwrap() < 3);
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn deleting_a_language_takes_its_suggestions_and_runs(pool: PgPool) {
    let app = app_with(pool.clone(), &fake_model(Duration::ZERO).await).await;
    let (session, ws) = workspace(&app).await;
    let started = app
        .post(
            &format!("/api/w/{ws}/runs"),
            Some(&session),
            json!({ "kind": "suggest", "source_lang": "en", "target_lang": "sv" }),
        )
        .await;
    finished(&pool, started.body["id"].as_str().unwrap()).await;
    let sv: Uuid = sqlx::query_scalar("SELECT id FROM languages WHERE code = 'sv'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let deleted = transact(
        &app,
        &session,
        &ws,
        vec![json!({ "action": "delete", "model": "Language", "id": sv })],
    )
    .await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.body);
    assert!(
        rows(&pool, "SELECT to_jsonb(id) FROM suggestions")
            .await
            .is_empty()
    );
    assert!(
        rows(&pool, "SELECT to_jsonb(id) FROM translation_runs")
            .await
            .is_empty()
    );
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn runs_left_going_by_a_restart_are_marked_failed(pool: PgPool) {
    let app = app_with(pool.clone(), "http://127.0.0.1:9").await;
    let (_, ws) = workspace(&app).await;
    let ws: Uuid = ws.parse().unwrap();
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO translation_runs (id, workspace_id, kind, source_lang, target_lang, status, started_by) \
         VALUES ($1, $2, 'fill', 'en', 'sv', 'running', $3)",
    )
    .bind(Uuid::now_v7())
    .bind(ws)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    glyph_server::translate::fail_interrupted(&pool)
        .await
        .unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM translation_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
}

#[sqlx::test(migrator = "glyph_sync::MIGRATOR")]
async fn the_models_api_says_which_answer(pool: PgPool) {
    let app = app_with(pool.clone(), &fake_model(Duration::ZERO).await).await;
    let (session, _) = app.signup("Ada", "Studio").await;
    let listed = app.get("/api/models", &session).await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.body[0]["name"], "fake");
    assert_eq!(listed.body[0]["reachable"], true);
    assert_eq!(listed.body[0]["can_start"], false);

    let start = app
        .post("/api/models/fake/start", Some(&session), json!({}))
        .await;
    assert_eq!(start.status, StatusCode::UNPROCESSABLE_ENTITY);
    let stop = app
        .post("/api/models/fake/stop", Some(&session), json!({}))
        .await;
    assert_eq!(stop.status, StatusCode::UNPROCESSABLE_ENTITY);
    let missing = app
        .post("/api/models/nope/start", Some(&session), json!({}))
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
