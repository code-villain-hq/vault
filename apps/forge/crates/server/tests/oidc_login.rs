//! Signing in with Code Villain, against a stand-in for the studio's OpenID
//! Connect provider.

mod common;

use axum::body::Body;
use axum::extract::Form;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use common::App;
use forge_server::AppState;
use forge_server::github::login::GitHubLogin;
use forge_server::oidc::OidcLogin;
use jsonwebtoken::{EncodingKey, Header};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use tower::ServiceExt;

/// The provider as far as signing in goes. Its codes are
/// `<who>.<nonce>.<code_challenge>`, so it can put the sign-in's nonce in
/// the ID token and check the PKCE verifier; `who` picks the person.
async fn fake_provider() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/api/oidc/token"),
    });
    let iss = issuer.clone();
    let app = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || async move { Json(discovery) }),
        )
        .route(
            "/api/oidc/token",
            post(
                move |headers: HeaderMap, Form(form): Form<HashMap<String, String>>| async move {
                    let basic = format!("Basic {}", STANDARD.encode("forge:secret"));
                    assert_eq!(headers[header::AUTHORIZATION], basic.as_str());
                    assert_eq!(form["grant_type"], "authorization_code");
                    assert_eq!(form["redirect_uri"], "http://forge.test/api/auth/oidc/callback");
                    let mut code = form["code"].split('.');
                    let (who, nonce, challenge) =
                        (code.next().unwrap(), code.next().unwrap(), code.next().unwrap());
                    let verified = URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes()));
                    if verified != challenge {
                        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant" })));
                    }
                    let mut claims = json!({
                        "iss": iss, "aud": "forge", "exp": chrono::Utc::now().timestamp() + 60,
                        "nonce": nonce,
                    });
                    let person = match who {
                        "ada" => json!({ "sub": "sub-ada", "email": "Ada@Example.com", "email_verified": true, "name": "Ada L." }),
                        "newbie" => json!({ "sub": "sub-new", "email": "new@example.com", "preferred_username": "newbie" }),
                        "unverified" => json!({ "sub": "sub-unverified", "email": "bob@example.com", "email_verified": false }),
                        "ada-again" => json!({ "sub": "sub-other", "email": "ada@example.com", "email_verified": true }),
                        "for-glyph" => json!({ "sub": "sub-ada", "email": "ada@example.com", "aud": ["glyph"] }),
                        "stale" => json!({ "sub": "sub-ada", "email": "ada@example.com", "exp": 1 }),
                        other => panic!("nobody is {other}"),
                    };
                    for (key, value) in person.as_object().unwrap() {
                        claims[key] = value.clone();
                    }
                    let id_token = jsonwebtoken::encode(
                        &Header::default(),
                        &claims,
                        &EncodingKey::from_secret(b"the provider's key"),
                    )
                    .unwrap();
                    (StatusCode::OK, Json(json!({ "access_token": "at", "id_token": id_token })))
                },
            ),
        );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    issuer
}

struct Setup {
    /// Forge with Code Villain sign-in on.
    app: App,
    /// The same database, with passwords: people from before.
    before: App,
}

async fn setup(pool: PgPool) -> Setup {
    let issuer = fake_provider().await;
    let login =
        OidcLogin::new(issuer, "forge".into(), "secret".into(), "http://forge.test").unwrap();
    let github = GitHubLogin::new("id".into(), "secret".into(), String::new(), String::new());
    let state = AppState::new(pool.clone(), false)
        .with_github_login(github)
        .with_oidc_login(login);
    Setup {
        app: App::with_state(state),
        before: App::new(pool),
    }
}

struct Started {
    /// The whole URL sent to the provider.
    location: String,
    state: String,
    nonce: String,
    challenge: String,
    /// The flow's cookie: nonce, verifier, and the invite if any.
    cookie: String,
}

fn param(url: &str, name: &str) -> String {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no {name} in {url}"))
        .to_string()
}

async fn start(app: &App, query: &str) -> Started {
    let response = app
        .router
        .clone()
        .oneshot(
            Request::get(format!("/api/auth/oidc{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.contains("/authorize?response_type=code&client_id=forge"),
        "{location}"
    );
    assert!(
        location.contains("code_challenge_method=S256"),
        "{location}"
    );
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("forge_oidc_login=")
        .unwrap()
        .to_string();
    Started {
        state: param(&location, "state"),
        nonce: param(&location, "nonce"),
        challenge: param(&location, "code_challenge"),
        location,
        cookie,
    }
}

/// Where the provider's return sends the browser, and the session it starts.
async fn back(app: &App, who: &str, started: &Started, cookie: &str) -> (String, Option<String>) {
    let code = format!("{who}.{}.{}", started.nonce, started.challenge);
    let response = app
        .router
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/auth/oidc/callback?code={code}&state={}",
                started.state
            ))
            .header(header::COOKIE, format!("forge_oidc_login={cookie}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    let session = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|c| c.to_str().ok())
        .find_map(|c| c.strip_prefix("forge_session="))
        .map(|c| c.split(';').next().unwrap().to_string());
    (location, session)
}

async fn sign_in(app: &App, who: &str, query: &str) -> (String, Option<String>) {
    let started = start(app, query).await;
    back(app, who, &started, &started.cookie).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_from_before_keep_their_account_and_passwords_close(pool: PgPool) {
    let Setup { app, before } = setup(pool).await;
    before.signup("Ada", "Acme").await;

    let providers = app
        .call("GET", "/api/auth/providers", None, None)
        .await
        .body;
    assert_eq!(
        providers,
        json!({ "oidc": true, "password": false, "github": false })
    );
    let login = app
        .post(
            "/api/auth/login",
            None,
            json!({ "email": "ada@example.com", "password": "correct horse" }),
        )
        .await;
    assert_eq!(login.status, StatusCode::NOT_FOUND);
    let signup = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Eve", "email": "eve@example.com", "password": "correct horse", "workspace_name": "Evil" }),
        )
        .await;
    assert_eq!(signup.status, StatusCode::NOT_FOUND);

    let (to, session) = sign_in(&app, "ada", "?redirect=/acme/inbox").await;
    assert_eq!(to, "/acme/inbox");
    let me = app.get("/api/me", &session.unwrap()).await.body;
    assert_eq!(me["user"]["email"], "ada@example.com");
    assert_eq!(me["workspaces"][0]["name"], "Acme");

    // Linked now: the studio account signs in to it whatever its email.
    let sub: Option<String> =
        sqlx::query_scalar("SELECT oidc_sub FROM users WHERE email = 'ada@example.com'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(sub.as_deref(), Some("sub-ada"));
    let (to, session) = sign_in(&app, "ada", "").await;
    assert_eq!(to, "/");
    assert!(session.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_people_come_through_invites_and_confirm_joining(pool: PgPool) {
    let Setup { app, before } = setup(pool).await;
    let (ada, acme) = before.signup("Ada", "Acme").await;

    let (to, session) = sign_in(&app, "newbie", "").await;
    assert_eq!(to, "/login?error=oidc_no_account");
    assert!(session.is_none());

    let invite = before.invite(&ada, &acme, "member").await;
    let started = start(&app, &format!("?invite={invite}")).await;
    assert!(
        !started.location.contains(&invite),
        "the invite never goes to the provider"
    );
    let (to, session) = back(&app, "newbie", &started, &started.cookie).await;
    assert_eq!(to, format!("/join/{invite}"), "they confirm joining there");
    let session = session.unwrap();
    let me = app.get("/api/me", &session).await.body;
    assert_eq!(me["user"]["name"], "newbie");
    assert_eq!(me["user"]["email"], "new@example.com");
    assert!(me["workspaces"].as_array().unwrap().is_empty());

    let joined = app
        .post(
            &format!("/api/invites/{invite}/accept"),
            Some(&session),
            json!({}),
        )
        .await;
    assert_eq!(joined.status, StatusCode::OK, "{}", joined.body);
    assert_eq!(joined.body["workspaces"][0]["role"], "member");
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_a_verified_email_links_and_only_once(pool: PgPool) {
    let Setup { app, before } = setup(pool).await;
    before.signup("Bob", "Bobco").await;
    let (to, _) = sign_in(&app, "unverified", "").await;
    assert_eq!(to, "/login?error=oidc_no_email");

    before.signup("Ada", "Acme").await;
    sign_in(&app, "ada", "").await;
    let (to, session) = sign_in(&app, "ada-again", "").await;
    assert_eq!(
        to, "/login?error=oidc_taken",
        "Ada's account has its studio account"
    );
    assert!(session.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn tokens_must_be_for_forge_current_and_from_this_sign_in(pool: PgPool) {
    let Setup { app, before } = setup(pool).await;
    before.signup("Ada", "Acme").await;
    for who in ["for-glyph", "stale"] {
        let (to, session) = sign_in(&app, who, "").await;
        assert_eq!(to, "/login?error=oidc_refused", "{who}");
        assert!(session.is_none());
    }
    // A nonce from another sign-in.
    let first = start(&app, "").await;
    let second = start(&app, "").await;
    let mixed = Started {
        nonce: first.nonce.clone(),
        ..second
    };
    let (to, _) = back(&app, "ada", &mixed, &mixed.cookie).await;
    assert_eq!(to, "/login?error=oidc_refused");
    // A verifier that isn't this sign-in's.
    let started = start(&app, "").await;
    let cookie = format!("{}.{}", started.nonce, "0".repeat(64));
    let (to, _) = back(&app, "ada", &started, &cookie).await;
    assert_eq!(to, "/login?error=oidc_refused");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sign_ins_must_finish_where_they_started(pool: PgPool) {
    let Setup { app, .. } = setup(pool).await;
    let started = start(&app, "").await;
    let (to, session) = back(&app, "ada", &started, "someone-elses-nonce.x").await;
    assert_eq!(to, "/login?error=oidc_other_browser");
    assert!(session.is_none());

    let tampered = Started {
        state: format!("{}0", started.state),
        ..started
    };
    let (to, _) = back(&app, "ada", &tampered, &tampered.cookie).await;
    assert_eq!(to, "/login?error=oidc_expired", "tampered state");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_desktop_app_collects_its_session_once(pool: PgPool) {
    let Setup { app, before } = setup(pool).await;
    before.signup("Ada", "Acme").await;
    let secret = "the desktop app's secret";
    let challenge = hex::encode(Sha256::digest(secret.as_bytes()));

    let (to, session) = sign_in(&app, "ada", &format!("?desktop={challenge}&redirect=/acme")).await;
    assert!(session.is_none(), "the browser isn't signed in");
    let code = to
        .strip_prefix("forge://signed-in?code=")
        .unwrap_or_else(|| panic!("{to}"));
    let (code, next) = code.split_once("&next=").unwrap();
    assert_eq!(next, "%2Facme");

    let collect = |secret: &str| {
        Request::post("/api/auth/oidc/desktop")
            .header("x-forge-client", "desktop")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "code": code, "secret": secret }).to_string(),
            ))
            .unwrap()
    };
    let response = app.router.clone().oneshot(collect(secret)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(header::SET_COOKIE).is_none());
    let token = response.headers()["x-forge-session"]
        .to_str()
        .unwrap()
        .to_string();
    let me = app.get("/api/me", &token).await.body;
    assert_eq!(me["user"]["email"], "ada@example.com");

    let again = app.router.clone().oneshot(collect(secret)).await.unwrap();
    assert_eq!(again.status(), StatusCode::UNAUTHORIZED, "codes work once");
}

#[sqlx::test(migrations = "../../migrations")]
async fn github_sign_in_is_off(pool: PgPool) {
    let Setup { app, .. } = setup(pool).await;
    let response = app
        .router
        .clone()
        .oneshot(
            Request::get("/api/auth/github")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.headers()[header::LOCATION],
        "/login?error=github_not_set_up"
    );
}
