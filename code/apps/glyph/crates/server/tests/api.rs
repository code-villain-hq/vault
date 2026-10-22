//! The HTTP API end to end: requests go through the real router to a fresh
//! Postgres database per test.

mod common;

use axum::http::StatusCode;
use common::App;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = "../../migrations")]
async fn signing_up_creates_a_workspace_you_administer(pool: PgPool) {
    let app = App::new(pool);
    let (session, _) = app.signup("Ada", "Analytical Engines").await;

    let me = app.get("/api/me", &session).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["user"]["email"], "ada@example.com");
    assert_eq!(me.body["workspaces"][0]["name"], "Analytical Engines");
    assert_eq!(me.body["workspaces"][0]["url_key"], "analytical-engines");
    assert_eq!(me.body["workspaces"][0]["role"], "admin");
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_url_keys_are_unique(pool: PgPool) {
    let app = App::new(pool);
    let (first, _) = app.signup("Ada", "Acme").await;
    let (second, _) = app.signup("Grace", "Acme").await;

    assert_eq!(
        app.get("/api/me", &first).await.body["workspaces"][0]["url_key"],
        "acme"
    );
    assert_eq!(
        app.get("/api/me", &second).await.body["workspaces"][0]["url_key"],
        "acme-2"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn logging_out_ends_the_session(pool: PgPool) {
    let app = App::new(pool);
    let (session, _) = app.signup("Ada", "Acme").await;

    let response = app
        .call("POST", "/api/auth/logout", Some(&session), None)
        .await;
    assert_eq!(response.status, StatusCode::NO_CONTENT);
    assert_eq!(
        response.session.as_deref(),
        Some(""),
        "the cookie is cleared"
    );
    assert_eq!(
        app.get("/api/me", &session).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn workspace_routes_need_membership(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let (grace, _) = app.signup("Grace", "Navy").await;
    let bootstrap = format!("/api/w/{acme}/sync/bootstrap");

    assert_eq!(
        app.call("GET", &bootstrap, None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.get(&bootstrap, &grace).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.get("/api/w/not-a-uuid/sync/bootstrap", &ada)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.get(&bootstrap, &ada).await.status, StatusCode::OK);

    let translation = json!({ "id": uuid::Uuid::now_v7(), "operations": [
        { "action": "create", "model": "Translation", "id": uuid::Uuid::now_v7(), "data": { "lang": "en", "namespace": "common", "key": "hello", "value": "Hello" } }
    ]});
    let transactions = format!("/api/w/{acme}/sync/transactions");
    assert_eq!(
        app.post(&transactions, Some(&grace), translation.clone())
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.post(&transactions, Some(&ada), translation)
            .await
            .status,
        StatusCode::OK
    );
    let records = app.get(&bootstrap, &ada).await.body;
    assert_eq!(records["records"]["Translation"][0]["value"], "Hello");
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_manage_people(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let ada_id = app.get("/api/me", &ada).await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let demote = app
        .call(
            "PATCH",
            &format!("/api/w/{acme}/members/{ada_id}"),
            Some(&bob),
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(demote.status, StatusCode::FORBIDDEN);
    assert_eq!(
        app.get(&format!("/api/w/{acme}/members"), &bob)
            .await
            .status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_workspace_always_keeps_an_admin(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let id_of = async |session: &str| {
        app.get("/api/me", session).await.body["user"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (ada_id, bob_id) = (id_of(&ada).await, id_of(&bob).await);
    let member_path = |id: &str| format!("/api/w/{acme}/members/{id}");

    let demote_self = app
        .call(
            "PATCH",
            &member_path(&ada_id),
            Some(&ada),
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(demote_self.status, StatusCode::CONFLICT);
    assert_eq!(
        app.call("DELETE", &member_path(&ada_id), Some(&ada), None)
            .await
            .status,
        StatusCode::CONFLICT
    );

    let promote = app
        .call(
            "PATCH",
            &member_path(&bob_id),
            Some(&ada),
            Some(json!({ "role": "admin" })),
        )
        .await;
    assert_eq!(promote.status, StatusCode::NO_CONTENT);
    let demote_self = app
        .call(
            "PATCH",
            &member_path(&ada_id),
            Some(&ada),
            Some(json!({ "role": "member" })),
        )
        .await;
    assert_eq!(demote_self.status, StatusCode::NO_CONTENT);

    // Removed people lose access at once.
    assert_eq!(
        app.call("DELETE", &member_path(&ada_id), Some(&bob), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_workspace_starts_with_english_as_its_source(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let records = app
        .get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
        .await
        .body;
    let languages = records["records"]["Language"].as_array().unwrap();
    assert_eq!(languages.len(), 1);
    assert_eq!(languages[0]["code"], "en");
    assert_eq!(languages[0]["is_source"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn removing_someone_tells_clients(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let token = app.invite(&ada, &acme, "member").await;
    let grace = app.join("Grace", &token).await;
    let bootstrap = format!("/api/w/{acme}/sync/bootstrap");
    let users = app.get(&bootstrap, &ada).await.body["records"]["User"].clone();
    assert_eq!(users.as_array().unwrap().len(), 2);
    let grace_id = app.get("/api/me", &grace).await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sync_id = app.get(&bootstrap, &ada).await.body["sync_id"]
        .as_i64()
        .unwrap();

    let removed = app
        .call(
            "DELETE",
            &format!("/api/w/{acme}/members/{grace_id}"),
            Some(&ada),
            None,
        )
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.body);
    assert_eq!(
        app.get(&bootstrap, &grace).await.status,
        StatusCode::NOT_FOUND
    );
    let after = app.get(&bootstrap, &ada).await.body;
    assert_eq!(after["records"]["User"].as_array().unwrap().len(), 1);
    assert!(after["sync_id"].as_i64().unwrap() > sync_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_server_can_serve_the_web_app(pool: PgPool) {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.html"),
        "<!doctype html><title>Glyph</title>",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/app.js"), "console.log(1)").unwrap();
    let app = glyph_server::app(glyph_server::AppState::new(pool, false), Some(dir.path()));

    let get = |path: &'static str| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            (status, String::from_utf8_lossy(&bytes).to_string())
        }
    };
    assert_eq!(
        get("/assets/app.js").await,
        (StatusCode::OK, "console.log(1)".to_string())
    );
    let (status, body) = get("/acme/translations").await;
    assert_eq!(
        (status, body.contains("<title>Glyph</title>")),
        (StatusCode::OK, true),
        "app routes get index.html"
    );
    assert_eq!(get("/api/health").await.0, StatusCode::OK);
    let (status, body) = get("/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body.contains("no such endpoint"),
        "unknown API paths stay JSON errors"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_desktop_app_signs_in_with_a_token_from_its_own_origin(pool: PgPool) {
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::ServiceExt;

    let app = App::new(pool);
    // The token the desktop app gets from signing in (`oidc.rs`).
    let (token, _) = app.signup("Ada", "Acme").await;
    let send = async |request: Request<Body>| app.router.clone().oneshot(request).await.unwrap();

    let me = send(
        Request::get("/api/me")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(me.status(), StatusCode::OK);

    // WebSockets can't carry headers, so the token comes as a subprotocol.
    let socket_style = send(
        Request::get("/api/me")
            .header(
                header::SEC_WEBSOCKET_PROTOCOL,
                format!("glyph, bearer.{token}"),
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(socket_style.status(), StatusCode::OK);

    let preflight = |origin: &'static str| {
        Request::options("/api/me")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .body(Body::empty())
            .unwrap()
    };
    let allowed = send(preflight("tauri://localhost")).await;
    assert_eq!(
        allowed.headers()["access-control-allow-origin"],
        "tauri://localhost"
    );
    let elsewhere = send(preflight("https://example.com")).await;
    assert!(
        elsewhere
            .headers()
            .get("access-control-allow-origin")
            .is_none(),
        "other sites can't call the API"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_can_rename_themselves_and_teammates_see_it(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;

    let renamed = app
        .call(
            "PUT",
            "/api/me",
            Some(&bob),
            Some(json!({ "name": "  Robert  " })),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(renamed.body["user"]["name"], "Robert");

    let users = app
        .get(&format!("/api/w/{acme}/sync/bootstrap"), &ada)
        .await
        .body["records"]["User"]
        .clone();
    let names: Vec<&str> = users
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Robert"), "{names:?}");
    let logged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sync_log WHERE model = 'User' AND action = 'update' AND data->>'name' = 'Robert'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(logged, 1, "clients hear about it");

    let empty = app
        .call("PUT", "/api/me", Some(&bob), Some(json!({ "name": " " })))
        .await;
    assert_eq!(empty.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "../../migrations")]
async fn admins_rename_the_workspace_and_its_url_stays(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let bob = app
        .join("Bob", &app.invite(&ada, &acme, "member").await)
        .await;
    let path = format!("/api/w/{acme}");

    let refused = app
        .call("PATCH", &path, Some(&bob), Some(json!({ "name": "Bobco" })))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    let renamed = app
        .call(
            "PATCH",
            &path,
            Some(&ada),
            Some(json!({ "name": "Acme Labs" })),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    let workspace = &renamed.body["workspaces"][0];
    assert_eq!(workspace["name"], "Acme Labs");
    assert_eq!(workspace["url_key"], "acme", "links keep working");
    assert_eq!(
        app.get("/api/me", &bob).await.body["workspaces"][0]["name"],
        "Acme Labs"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_guest_works_only_in_the_languages_an_admin_adds_them_to(pool: PgPool) {
    let app = App::new(pool.clone());
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let created = app
        .post(
            &format!("/api/w/{acme}/sync/transactions"),
            Some(&ada),
            json!({ "id": Uuid::now_v7(), "operations": [
                { "action": "create", "model": "Language", "id": Uuid::now_v7(), "data": { "code": "sv" } },
                { "action": "create", "model": "Language", "id": Uuid::now_v7(), "data": { "code": "de" } },
            ] }),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK);
    let gus = app
        .join("Gus", &app.invite(&ada, &acme, "guest").await)
        .await;
    let gus_id = app.get("/api/me", &gus).await.body["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let swedish: Uuid = sqlx::query_scalar("SELECT id FROM languages WHERE code = 'sv'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let access = format!("/api/w/{acme}/members/{gus_id}/access");

    let by_guest = app
        .call(
            "PUT",
            &access,
            Some(&gus),
            Some(json!({ "items": [swedish] })),
        )
        .await;
    assert_eq!(by_guest.status, StatusCode::FORBIDDEN);
    let by_admin = app
        .call(
            "PUT",
            &access,
            Some(&ada),
            Some(json!({ "items": [swedish] })),
        )
        .await;
    assert_eq!(by_admin.status, StatusCode::NO_CONTENT);
    let members = app.get(&format!("/api/w/{acme}/members"), &ada).await.body;
    let row = members
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user"]["id"] == gus_id.as_str())
        .unwrap();
    assert_eq!(row["role"], "guest");
    assert_eq!(row["access"], json!([swedish]));
    // A guest's Settings are their account's alone.
    assert_eq!(
        app.get(&format!("/api/w/{acme}/members"), &gus)
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    let run = |target: &'static str| {
        let (app, gus, acme) = (&app, &gus, &acme);
        async move {
            app.post(
                &format!("/api/w/{acme}/runs"),
                Some(gus),
                json!({ "kind": "suggest", "source_lang": "en", "target_lang": target }),
            )
            .await
            .status
        }
    };
    assert_eq!(run("de").await, StatusCode::NOT_FOUND);
    // Swedish is theirs: what stops it is only that there's nothing to do.
    assert_ne!(run("sv").await, StatusCode::NOT_FOUND);
}
