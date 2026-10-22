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
    assert_eq!(providers.body, json!({ "oidc": false, "company": null }));

    let signup = app
        .post(
            "/api/auth/signup",
            None,
            json!({ "name": "Eve", "email": "eve@example.com", "password": "correct horse", "workspace_name": "Mine" }),
        )
        .await;
    assert!(signup.status.is_client_error(), "{}", signup.status);

    local::start(&pool, "Ada", "Notes").await.unwrap();
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
async fn attachments_on_this_mac_open_with_the_apps_token_or_a_download_link(pool: PgPool) {
    use axum::body::Body;
    use axum::http::{Request, header};
    use serde_json::Value;
    use tower::ServiceExt;

    let data = tempfile::tempdir().unwrap();
    let app = App::with_state(
        AppState::new(pool.clone(), false)
            .on_this_mac()
            .with_data_dir(data.path()),
    );
    let person = local::start(&pool, "Ada", "Notes").await.unwrap();
    let token = local::session(&pool, person).await.unwrap();
    let me = app.get("/api/me", &token).await;
    let notes = me.body["workspaces"][0]["id"].as_str().unwrap().to_string();

    let send = async |request: Request<Body>| app.router.clone().oneshot(request).await.unwrap();
    let bearer = format!("Bearer {token}");
    let json = async |response: axum::response::Response| -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    };
    let upload = async |name: &str| {
        let uploaded = send(
            Request::post(format!("/api/w/{notes}/attachments?name={name}"))
                .header(header::AUTHORIZATION, &bearer)
                .header(header::CONTENT_TYPE, "application/pdf")
                .body(Body::from(b"%PDF fake".to_vec()))
                .unwrap(),
        )
        .await;
        assert_eq!(uploaded.status(), StatusCode::OK);
        json(uploaded).await["url"].as_str().unwrap().to_string()
    };
    let get = async |url: &str, authorization: Option<&str>| {
        let mut request = Request::get(url);
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        send(request.body(Body::empty()).unwrap()).await
    };

    // The app sends its session as a bearer token, as it does everywhere.
    let report = upload("report.pdf").await;
    let fetched = get(&report, Some(&bearer)).await;
    assert_eq!(fetched.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(fetched.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"%PDF fake");
    assert_eq!(get(&report, None).await.status(), StatusCode::UNAUTHORIZED);

    // The browser it opens a link in has no session: a download link works
    // there once.
    let ask = async |url: &str, authorization: Option<&str>| {
        let mut request = Request::post(format!("{url}/link"));
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        send(request.body(Body::empty()).unwrap()).await
    };
    assert_eq!(ask(&report, None).await.status(), StatusCode::UNAUTHORIZED);
    let link = json(ask(&report, Some(&bearer)).await).await["url"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(link.starts_with(&format!("{report}?ticket=")));
    let opened = get(&link, None).await;
    assert_eq!(opened.status(), StatusCode::OK);
    assert!(
        opened.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );
    assert_eq!(get(&link, None).await.status(), StatusCode::UNAUTHORIZED);

    // A ticket opens only the attachment it was made for.
    let other = upload("other.pdf").await;
    let link = json(ask(&report, Some(&bearer)).await).await["url"]
        .as_str()
        .unwrap()
        .to_string();
    let ticket = link.split_once("?ticket=").unwrap().1;
    let elsewhere = get(&format!("{other}?ticket={ticket}"), None).await;
    assert_eq!(elsewhere.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(get(&link, None).await.status(), StatusCode::OK);

    // No links to attachments that aren't there.
    let missing = format!("/api/w/{notes}/attachments/{}", uuid::Uuid::now_v7());
    assert_eq!(
        ask(&missing, Some(&bearer)).await.status(),
        StatusCode::NOT_FOUND
    );
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
