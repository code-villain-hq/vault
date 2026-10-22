//! Signing in with the company's account, against a stand-in for its
//! OpenID Connect provider.

mod common;

use axum::body::Body;
use axum::extract::Form;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use common::App;
use jsonwebtoken::{EncodingKey, Header};
use raven_server::oidc::OidcLogin;
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
                    let basic = format!("Basic {}", STANDARD.encode("raven:secret"));
                    assert_eq!(headers[header::AUTHORIZATION], basic.as_str());
                    assert_eq!(form["grant_type"], "authorization_code");
                    assert_eq!(form["redirect_uri"], "http://raven.test/api/auth/oidc/callback");
                    let mut code = form["code"].split('.');
                    let (who, nonce, challenge) =
                        (code.next().unwrap(), code.next().unwrap(), code.next().unwrap());
                    let verified = URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes()));
                    if verified != challenge {
                        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant" })));
                    }
                    let mut claims = json!({
                        "iss": iss, "aud": "raven", "exp": chrono::Utc::now().timestamp() + 60,
                        "nonce": nonce,
                    });
                    let person = match who {
                        "ada" => json!({ "sub": "sub-ada", "email": "Ada@Example.com", "email_verified": true, "name": "Ada L." }),
                        "newbie" => json!({ "sub": "sub-new", "email": "new@example.com", "preferred_username": "newbie" }),
                        "unverified" => json!({ "sub": "sub-unverified", "email": "bob@example.com", "email_verified": false }),
                        "ada-again" => json!({ "sub": "sub-other", "email": "ada@example.com", "email_verified": true }),
                        "for-forge" => json!({ "sub": "sub-ada", "email": "ada@example.com", "aud": ["forge"] }),
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
    /// Raven for Acme Inc, signing in with its account.
    app: App,
    /// The same database, for putting people in as the company's account
    /// would.
    company: App,
}

async fn setup(pool: PgPool) -> Setup {
    let issuer = fake_provider().await;
    let login =
        OidcLogin::new(issuer, "raven".into(), "secret".into(), "http://raven.test").unwrap();
    let state = common::state(pool.clone())
        .for_company("Acme Inc")
        .with_oidc_login(login);
    Setup {
        app: App::with_state(state),
        company: App::new(pool),
    }
}

struct Started {
    state: String,
    nonce: String,
    challenge: String,
    /// The flow's cookie: nonce and verifier.
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
        location.contains("/authorize?response_type=code&client_id=raven"),
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
        .strip_prefix("raven_oidc_login=")
        .unwrap()
        .to_string();
    Started {
        state: param(&location, "state"),
        nonce: param(&location, "nonce"),
        challenge: param(&location, "code_challenge"),
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
            .header(header::COOKIE, format!("raven_oidc_login={cookie}"))
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
        .find_map(|c| c.strip_prefix("raven_session="))
        .map(|c| c.split(';').next().unwrap().to_string());
    (location, session)
}

async fn sign_in(app: &App, who: &str, query: &str) -> (String, Option<String>) {
    let started = start(app, query).await;
    back(app, who, &started, &started.cookie).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_companys_people_sign_in_with_its_account_only(pool: PgPool) {
    let Setup { app, company } = setup(pool).await;
    company.signup("Ada", "Acme").await;

    let providers = app
        .call("GET", "/api/auth/providers", None, None)
        .await
        .body;
    assert_eq!(providers, json!({ "oidc": true, "company": "Acme Inc" }));
    for path in ["/api/auth/login", "/api/auth/signup"] {
        let response = app
            .post(
                path,
                None,
                json!({ "email": "ada@example.com", "password": "correct horse" }),
            )
            .await;
        assert!(
            response.status.is_client_error(),
            "{path}: {}",
            response.status
        );
    }

    let (to, session) = sign_in(&app, "ada", "?redirect=/acme/forge/technical").await;
    assert_eq!(to, "/acme/forge/technical");
    let session = session.unwrap();
    let me = app.get("/api/me", &session).await.body;
    assert_eq!(me["user"]["email"], "ada@example.com");
    assert_eq!(me["workspaces"][0]["name"], "Acme");
    let change = app
        .call(
            "PUT",
            "/api/me/password",
            Some(&session),
            Some(json!({ "current_password": "correct horse", "new_password": "battery staple" })),
        )
        .await;
    assert!(change.status.is_client_error(), "{}", change.status);

    // Linked now: the company's account signs in to it whatever its email.
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
async fn signing_in_makes_nobody(pool: PgPool) {
    let Setup { app, company } = setup(pool).await;
    company.signup("Ada", "Acme").await;

    let (to, session) = sign_in(&app, "newbie", "").await;
    assert_eq!(to, "/login?error=oidc_no_access");
    assert!(session.is_none());
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(people, 1, "only Ada");

    // Known, but in no workspace: the company took them out of its groups.
    company.person("Newbie").await;
    sqlx::query("UPDATE users SET email = 'new@example.com' WHERE name = 'Newbie'")
        .execute(&app.pool)
        .await
        .unwrap();
    let (to, session) = sign_in(&app, "newbie", "").await;
    assert_eq!(to, "/login?error=oidc_no_access");
    assert!(session.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_a_verified_email_links_and_only_once(pool: PgPool) {
    let Setup { app, company } = setup(pool).await;
    let (bob, bobco) = company.signup("Bob", "Bobco").await;
    let (to, _) = sign_in(&app, "unverified", "").await;
    assert_eq!(to, "/login?error=oidc_no_email");

    let invite = company.invite(&bob, &bobco, "member").await;
    company.join("Ada", &invite).await;
    sign_in(&app, "ada", "").await;
    let (to, session) = sign_in(&app, "ada-again", "").await;
    assert_eq!(
        to, "/login?error=oidc_taken",
        "Ada's account has its company account"
    );
    assert!(session.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn tokens_must_be_for_raven_current_and_from_this_sign_in(pool: PgPool) {
    let Setup { app, company } = setup(pool).await;
    company.signup("Ada", "Acme").await;
    for who in ["for-forge", "stale"] {
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
    let Setup { app, company } = setup(pool).await;
    company.signup("Ada", "Acme").await;
    let secret = "the desktop app's secret";
    let challenge = hex::encode(Sha256::digest(secret.as_bytes()));

    let (to, session) = sign_in(&app, "ada", &format!("?desktop={challenge}&redirect=/acme")).await;
    assert!(session.is_none(), "the browser isn't signed in");
    let code = to
        .strip_prefix("/signed-in?code=")
        .unwrap_or_else(|| panic!("{to}"));
    let (code, next) = code.split_once("&next=").unwrap();
    assert_eq!(next, "%2Facme");

    let collect = |secret: &str| {
        Request::post("/api/auth/oidc/desktop")
            .header("x-raven-client", "desktop")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "code": code, "secret": secret }).to_string(),
            ))
            .unwrap()
    };
    let response = app.router.clone().oneshot(collect(secret)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(header::SET_COOKIE).is_none());
    let token = response.headers()["x-raven-session"]
        .to_str()
        .unwrap()
        .to_string();
    let me = app.get("/api/me", &token).await.body;
    assert_eq!(me["user"]["email"], "ada@example.com");

    let again = app.router.clone().oneshot(collect(secret)).await.unwrap();
    assert_eq!(again.status(), StatusCode::UNAUTHORIZED, "codes work once");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_desktop_sign_in_goes_back_to_the_app(pool: PgPool) {
    let Setup { app, .. } = setup(pool).await;
    let challenge = hex::encode(Sha256::digest(b"the desktop app's secret"));
    let (to, session) = sign_in(&app, "newbie", &format!("?desktop={challenge}")).await;
    assert_eq!(to, "/signed-in?error=oidc_no_access");
    assert!(session.is_none());

    let started = start(&app, &format!("?desktop={challenge}")).await;
    let tampered = Started {
        state: format!("{}0", started.state),
        ..started
    };
    let (to, _) = back(&app, "ada", &tampered, &tampered.cookie).await;
    assert_eq!(
        to, "/login?error=oidc_expired",
        "a trip that can't be read isn't the app's"
    );
}
