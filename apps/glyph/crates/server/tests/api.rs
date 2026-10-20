//! The HTTP API end to end: requests go through the real router to a fresh
//! Postgres database per test.

mod common;

use axum::http::StatusCode;
use common::App;
use glyph_server::accounts;
use serde_json::{Value, json};
use sqlx::PgPool;

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
async fn signup_rejects_bad_input_and_taken_emails(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;

    let cases = [
        (
            json!({ "name": "Ada", "email": "ADA@example.com", "password": "correct horse", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "bo@example.com", "password": "short", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "not-an-email", "password": "correct horse", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": " ", "email": "bo@example.com", "password": "correct horse", "workspace_name": "X" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "bo@example.com", "password": "correct horse" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "name": "Bo", "email": "bo@example.com", "password": "correct horse", "workspace_name": "X", "invite_token": "t" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ];
    for (body, expected) in cases {
        let response = app.post("/api/auth/signup", None, body.clone()).await;
        assert_eq!(response.status, expected, "{body} gave {}", response.body);
        assert!(response.session.is_none());
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn signing_up_doesnt_reveal_who_has_an_account(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let signup = async |email: &str, extra: Value| {
        let mut body = json!({ "name": "Someone", "email": email, "password": "correct horse" });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        app.post("/api/auth/signup", None, body).await
    };

    let taken = signup(" Ada@Example.com", json!({ "workspace_name": "X" })).await;
    assert_eq!(taken.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(taken.body["message"], accounts::SIGNUP_REFUSED);
    assert!(taken.session.is_none());
    // With an invite too.
    let invite = app.invite(&ada, &acme, "member").await;
    let joining = signup("ada@example.com", json!({ "invite_token": invite })).await;
    assert_eq!(
        (joining.status, &joining.body),
        (taken.status, &taken.body),
        "the same answer"
    );
    // The same status as any other refusal.
    let short = signup(
        "bo@example.com",
        json!({ "workspace_name": "X", "password": "short" }),
    )
    .await;
    assert_eq!(short.status, taken.status);
    assert!(
        !taken
            .body
            .to_string()
            .to_lowercase()
            .contains("already an account"),
        "{}",
        taken.body
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn sign_ups_are_limited_per_client(pool: PgPool) {
    let app = App::new(pool);
    let here: std::net::SocketAddr = "203.0.113.7:50000".parse().unwrap();
    let signup = async |client: std::net::SocketAddr, name: &str, password: &str| {
        let email = format!("{}@example.com", name.to_lowercase());
        app.call_from(
            client,
            "POST",
            "/api/auth/signup",
            None,
            Some(json!({ "name": name, "email": email, "password": password, "workspace_name": name })),
        )
        .await
    };

    // Five tries an hour, whether they work or not.
    for (name, password) in [
        ("Ada", "correct horse"),
        ("Bo", "short"),
        ("Ada", "correct horse"),
        ("Cy", "correct horse"),
        ("Di", "short"),
    ] {
        let response = signup(here, name, password).await;
        assert_ne!(response.status, StatusCode::TOO_MANY_REQUESTS, "{name}");
    }
    let sixth = signup(here, "Eve", "correct horse").await;
    assert_eq!(
        sixth.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        sixth.body
    );
    assert_eq!(
        sixth.body["message"],
        "too many sign-ups from here; try again in 60 minutes"
    );
    assert!(sixth.session.is_none());
    let eve: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'eve@example.com'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(eve, 0);

    // Other clients aren't affected, and signing in isn't either.
    let elsewhere = signup(
        "198.51.100.2:50000".parse().unwrap(),
        "Eve",
        "correct horse",
    )
    .await;
    assert_eq!(elsewhere.status, StatusCode::OK, "{}", elsewhere.body);
    let login = app
        .call_from(
            here,
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "ada@example.com", "password": "correct horse" })),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn login_checks_the_password(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;

    let wrong = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "wrong horse" }),
        )
        .await;
    let unknown = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "bob@example.com", "password": "correct horse" }),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong.body["message"], unknown.body["message"],
        "don't reveal which emails have accounts"
    );

    let right = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": " Ada@Example.com", "password": "correct horse" }),
        )
        .await;
    assert_eq!(right.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/me", &right.session.unwrap()).await.status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn too_many_wrong_passwords_lock_the_account_for_a_while(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;
    let login = async |password: &str| {
        app.post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": password }),
        )
        .await
    };
    for _ in 0..10 {
        assert_eq!(login("wrong horse").await.status, StatusCode::UNAUTHORIZED);
    }
    let locked = login("correct horse").await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(
        locked.body["message"]
            .as_str()
            .unwrap()
            .contains("try again in 15 minutes"),
        "{}",
        locked.body
    );

    // Other accounts are unaffected.
    app.signup("Bob", "Bobco").await;
    let bob = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "bob@example.com", "password": "correct horse" }),
        )
        .await;
    assert_eq!(bob.status, StatusCode::OK);
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
async fn invite_links_let_people_join(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let token = app.invite(&ada, &acme, "member").await;

    let preview = app
        .call("GET", &format!("/api/invites/{token}"), None, None)
        .await;
    assert_eq!(preview.status, StatusCode::OK);
    assert_eq!(
        preview.body,
        json!({ "workspace_name": "Acme", "workspace_url_key": "acme", "invited_by": "Ada", "role": "member" })
    );

    // A new account joins through the link instead of creating a workspace.
    let bob = app.join("Bob", &token).await;
    let me = app.get("/api/me", &bob).await.body;
    assert_eq!(me["workspaces"].as_array().unwrap().len(), 1);
    assert_eq!(me["workspaces"][0]["id"], acme.as_str());
    assert_eq!(me["workspaces"][0]["role"], "member");

    // An existing account accepts the same link while signed in.
    let (grace, _) = app.signup("Grace", "Navy").await;
    let accepted = app
        .call(
            "POST",
            &format!("/api/invites/{token}/accept"),
            Some(&grace),
            None,
        )
        .await;
    assert_eq!(accepted.status, StatusCode::OK);
    assert_eq!(accepted.body["workspaces"].as_array().unwrap().len(), 2);

    let members = app.get(&format!("/api/w/{acme}/members"), &bob).await.body;
    let names: Vec<&str> = members
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["user"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Ada", "Bob", "Grace"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_invite_links_stop_working(pool: PgPool) {
    let app = App::new(pool);
    let (ada, acme) = app.signup("Ada", "Acme").await;
    let token = app.invite(&ada, &acme, "member").await;

    let invites = app.get(&format!("/api/w/{acme}/invites"), &ada).await.body;
    assert_eq!(invites.as_array().unwrap().len(), 1);
    let invite_id = invites[0]["id"].as_str().unwrap();
    let revoked = app
        .call(
            "DELETE",
            &format!("/api/w/{acme}/invites/{invite_id}"),
            Some(&ada),
            None,
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);

    assert_eq!(
        app.call("GET", &format!("/api/invites/{token}"), None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let signup = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Bob", "email": "bob@example.com", "password": "correct horse", "invite_token": token }),
        )
        .await;
    assert_eq!(signup.status, StatusCode::NOT_FOUND);
    assert!(
        app.get(&format!("/api/w/{acme}/invites"), &ada)
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
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

    let invite = app
        .post(
            &format!("/api/w/{acme}/invites"),
            Some(&bob),
            json!({ "role": "member" }),
        )
        .await;
    assert_eq!(invite.status, StatusCode::FORBIDDEN);
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
async fn changing_the_password_signs_out_other_sessions(pool: PgPool) {
    let app = App::new(pool);
    let (here, _) = app.signup("Ada", "Acme").await;
    let elsewhere = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "correct horse" }),
        )
        .await
        .session
        .unwrap();

    let wrong = app
        .call(
            "PUT",
            "/api/me/password",
            Some(&here),
            Some(json!({ "current_password": "nope nope", "new_password": "battery staple" })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);

    let changed = app
        .call(
            "PUT",
            "/api/me/password",
            Some(&here),
            Some(json!({ "current_password": "correct horse", "new_password": "battery staple" })),
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/me", &here).await.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/me", &elsewhere).await.status,
        StatusCode::UNAUTHORIZED
    );

    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "battery staple" }),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reset_password_sets_a_new_one_and_signs_out_everywhere(pool: PgPool) {
    let app = App::new(pool);
    let (session, _) = app.signup("Ada", "Acme").await;

    let password = accounts::reset_password(&app.pool, "ADA@example.com")
        .await
        .unwrap();

    assert_eq!(
        app.get("/api/me", &session).await.status,
        StatusCode::UNAUTHORIZED
    );
    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": password }),
        )
        .await;
    assert_eq!(login.status, StatusCode::OK);
    assert!(
        accounts::reset_password(&app.pool, "nobody@example.com")
            .await
            .is_err()
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
    app.signup("Ada", "Acme").await;
    let send = async |request: Request<Body>| app.router.clone().oneshot(request).await.unwrap();

    let login = send(
        Request::post("/api/auth/login")
            .header(header::ORIGIN, "tauri://localhost")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-glyph-client", "desktop")
            .body(Body::from(
                json!({ "email": "ada@example.com", "password": "correct horse" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let headers = login.headers();
    assert!(headers.get(header::SET_COOKIE).is_none(), "no cookie");
    assert_eq!(headers["access-control-allow-origin"], "tauri://localhost");
    assert!(
        headers["access-control-expose-headers"]
            .to_str()
            .unwrap()
            .contains("x-glyph-session")
    );
    let token = headers["x-glyph-session"].to_str().unwrap().to_string();

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
async fn many_wrong_passwords_at_once_still_hit_the_limit(pool: PgPool) {
    let app = App::new(pool);
    app.signup("Ada", "Acme").await;
    let tries = (0..30).map(|_| {
        app.post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "wrong horse" }),
        )
    });
    let statuses: Vec<StatusCode> = futures::future::join_all(tries)
        .await
        .into_iter()
        .map(|r| r.status)
        .collect();
    let checked = statuses
        .iter()
        .filter(|s| **s == StatusCode::UNAUTHORIZED)
        .count();
    assert!(checked <= 10, "{checked} passwords were checked");
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS));
}
