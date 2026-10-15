//! Signing in with GitHub, against a stand-in for GitHub's OAuth and API.

mod common;

use axum::Json;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::routing::{get, post};
use common::App;
use forge_server::AppState;
use forge_server::github::login::GitHubLogin;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

/// GitHub as far as signing in goes: code "ada" is Ada's account, code
/// "newbie" belongs to someone without a Forge account.
async fn fake_github() -> String {
    let token = |headers: &HeaderMap| {
        headers[header::AUTHORIZATION]
            .to_str()
            .unwrap()
            .trim_start_matches("Bearer ")
            .to_string()
    };
    let app = axum::Router::new()
        .route(
            "/login/oauth/access_token",
            post(|Json(body): Json<Value>| async move {
                assert_eq!(body["client_secret"], "secret");
                Json(json!({ "access_token": format!("token-{}", body["code"].as_str().unwrap()) }))
            }),
        )
        .route(
            "/user",
            get(move |headers: HeaderMap| async move {
                Json(match token(&headers).as_str() {
                    "token-ada" => json!({ "id": 1, "login": "ada", "name": "Ada L." }),
                    _ => json!({ "id": 2, "login": "newbie", "name": null }),
                })
            }),
        )
        .route(
            "/user/emails",
            get(move |headers: HeaderMap| async move {
                Json(match token(&headers).as_str() {
                    "token-ada" => json!([
                        { "email": "unverified@example.com", "primary": false, "verified": false },
                        { "email": "Ada@Example.com", "primary": true, "verified": true },
                    ]),
                    _ => json!([{ "email": "new@example.com", "primary": true, "verified": true }]),
                })
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

async fn setup(pool: PgPool) -> App {
    let github = fake_github().await;
    let login = GitHubLogin::new("client".into(), "secret".into(), github.clone(), github);
    App::with_state(AppState::new(pool.clone(), false).with_github_login(login))
}

struct Started {
    /// The whole URL sent to GitHub.
    location: String,
    state: String,
    /// The flow's cookie: the nonce, and the invite if any.
    cookie: String,
}

/// Starts signing in (`/api/auth/github...`) or connecting
/// (`/api/auth/github/connect...`, with `session`).
async fn begin(app: &App, path: &str, session: Option<&str>) -> Started {
    let mut request = Request::get(path);
    if let Some(session) = session {
        request = request.header(header::COOKIE, format!("forge_session={session}"));
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.contains("/login/oauth/authorize?client_id=client&state="),
        "{location}"
    );
    let state = location
        .split("state=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_string();
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_string();
    Started {
        location,
        state,
        cookie,
    }
}

async fn start(app: &App, query: &str) -> Started {
    begin(app, &format!("/api/auth/github{query}"), None).await
}

/// Where GitHub's return sends the browser, and the session it starts.
async fn back(
    app: &App,
    code: &str,
    started: &Started,
    cookie: &str,
    session: Option<&str>,
) -> (String, Option<String>) {
    let mut cookies = format!("forge_github_login={cookie}");
    if let Some(session) = session {
        cookies.push_str(&format!("; forge_session={session}"));
    }
    let response = app
        .router
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/auth/github/callback?code={code}&state={}",
                started.state
            ))
            .header(header::COOKIE, cookies)
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

async fn callback(app: &App, code: &str, started: &Started) -> (String, Option<String>) {
    back(app, code, started, &started.cookie, None).await
}

async fn github_id(app: &App, email: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT github_id FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_email_match_never_takes_over_an_account_with_a_password(pool: PgPool) {
    let app = setup(pool).await;
    // Whoever signed up with ada@example.com set the password, which proves
    // nothing about owning the address.
    app.signup("Ada", "Acme").await;

    let started = start(&app, "").await;
    let (to, session) = callback(&app, "ada", &started).await;
    assert_eq!(to, "/login?error=github_use_password");
    assert!(session.is_none());
    assert_eq!(github_id(&app, "ada@example.com").await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_connect_github_while_signed_in_then_sign_in_with_it(pool: PgPool) {
    let app = setup(pool).await;
    let (ada, _) = app.signup("Ada", "Acme").await;

    let started = begin(
        &app,
        "/api/auth/github/connect?redirect=/acme/settings/account",
        Some(&ada),
    )
    .await;
    // Finishing in a browser where someone else (or no one) is signed in fails.
    let (to, _) = back(&app, "ada", &started, &started.cookie, None).await;
    assert_eq!(
        to,
        "/acme/settings/account?github_error=github_other_browser"
    );
    assert_eq!(github_id(&app, "ada@example.com").await, None);

    let started = begin(&app, "/api/auth/github/connect", Some(&ada)).await;
    let (to, session) = back(&app, "ada", &started, &started.cookie, Some(&ada)).await;
    assert_eq!(to, "/");
    assert!(session.is_none(), "already signed in");
    assert_eq!(github_id(&app, "ada@example.com").await, Some(1));
    let me = app.get("/api/me", &ada).await.body;
    assert_eq!(me["github_connected"], true);

    // Someone else can't connect the same GitHub account, and hears why
    // where they started.
    let (bob, _) = app.signup("Bob", "Bobco").await;
    let started = begin(
        &app,
        "/api/auth/github/connect?redirect=/bobco/settings/account",
        Some(&bob),
    )
    .await;
    let (to, _) = back(&app, "ada", &started, &started.cookie, Some(&bob)).await;
    assert_eq!(to, "/bobco/settings/account?github_error=github_taken");

    let started = start(&app, "?redirect=/acme/inbox").await;
    let (to, session) = callback(&app, "ada", &started).await;
    assert_eq!(to, "/acme/inbox");
    let me = app.get("/api/me", &session.unwrap()).await.body;
    assert_eq!(me["user"]["email"], "ada@example.com");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sign_ins_must_finish_where_they_started(pool: PgPool) {
    let app = setup(pool).await;
    let started = start(&app, "").await;
    let (to, session) = back(&app, "ada", &started, "someone-elses-nonce", None).await;
    assert_eq!(to, "/login?error=github_other_browser");
    assert!(session.is_none());

    let tampered = Started {
        state: format!("{}0", started.state),
        ..started
    };
    let (to, session) = callback(&app, "ada", &tampered).await;
    assert_eq!(to, "/login?error=github_expired", "tampered state");
    assert!(session.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn redirects_stay_on_this_site(pool: PgPool) {
    let app = setup(pool).await;
    let (ada, _) = app.signup("Ada", "Acme").await;
    let started = begin(&app, "/api/auth/github/connect", Some(&ada)).await;
    back(&app, "ada", &started, &started.cookie, Some(&ada)).await;

    let started = start(&app, "?redirect=/%09/evil.example").await;
    let (to, _) = callback(&app, "ada", &started).await;
    assert_eq!(to, "/");
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_people_come_through_invites_and_confirm_joining(pool: PgPool) {
    let app = setup(pool).await;
    let (ada, acme) = app.signup("Ada", "Acme").await;

    let started = start(&app, "").await;
    let (to, session) = callback(&app, "newbie", &started).await;
    assert_eq!(to, "/login?error=github_no_account");
    assert!(session.is_none());

    let invite = app.invite(&ada, &acme, "member").await;
    let started = start(&app, &format!("?invite={invite}")).await;
    assert!(
        !started.location.contains(&invite),
        "the invite never goes to GitHub"
    );
    let (to, session) = callback(&app, "newbie", &started).await;
    assert_eq!(to, format!("/join/{invite}"), "they confirm joining there");
    let session = session.unwrap();
    let me = app.get("/api/me", &session).await.body;
    assert_eq!(me["user"]["name"], "newbie");
    assert_eq!(me["user"]["email"], "new@example.com");
    assert_eq!(me["has_password"], false);
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

    // Without a password yet, they set one without the current one.
    let set = app
        .call(
            "PUT",
            "/api/me/password",
            Some(&session),
            Some(json!({ "current_password": "", "new_password": "a good password" })),
        )
        .await;
    assert_eq!(set.status, StatusCode::NO_CONTENT, "{}", set.body);
}
