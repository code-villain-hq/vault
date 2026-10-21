//! Shared helpers for the HTTP tests: requests through the real router.
#![allow(dead_code)]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use layer_server::{AppState, router};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

/// A server's state for tests: no secure cookies.
pub fn state(pool: PgPool) -> AppState {
    AppState::new(pool, false)
}

pub struct App {
    pub router: Router,
    pub pool: PgPool,
    /// Requests come from a new address each, unless a test says otherwise,
    /// so limits per client (on signing up) don't get in the way.
    next_client: AtomicU32,
}

pub struct Response {
    pub status: StatusCode,
    /// The session token from a `Set-Cookie` header, if one was set.
    pub session: Option<String>,
    pub body: Value,
}

impl App {
    pub fn new(pool: PgPool) -> Self {
        Self::with_state(state(pool))
    }

    pub fn with_state(state: AppState) -> Self {
        App {
            pool: state.pool.clone(),
            router: router(state),
            next_client: AtomicU32::new(1),
        }
    }

    pub async fn call(
        &self,
        method: &str,
        path: &str,
        session: Option<&str>,
        body: Option<Value>,
    ) -> Response {
        let n = self.next_client.fetch_add(1, Ordering::Relaxed);
        let client = SocketAddr::from((Ipv4Addr::from(0x0a00_0000 | n), 40000));
        self.call_from(client, method, path, session, body).await
    }

    /// A request from `client`, as the server sees the connection.
    pub async fn call_from(
        &self,
        client: SocketAddr,
        method: &str,
        path: &str,
        session: Option<&str>,
        body: Option<Value>,
    ) -> Response {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .extension(ConnectInfo(client));
        if let Some(token) = session {
            request = request.header(header::COOKIE, format!("layer_session={token}"));
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let session = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|cookie| {
                cookie
                    .split(';')
                    .next()?
                    .split_once('=')
                    .map(|(_, v)| v.to_string())
            });
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Response {
            status,
            session,
            body,
        }
    }

    /// Writes to the workspace's synced records.
    pub async fn sync(&self, session: &str, workspace_id: &str, operations: Value) -> Response {
        self.post(
            &format!("/api/w/{workspace_id}/sync/transactions"),
            Some(session),
            json!({ "id": uuid::Uuid::now_v7(), "operations": operations }),
        )
        .await
    }

    /// The workspace's synced records of one model.
    pub async fn records(&self, session: &str, workspace_id: &str, model: &str) -> Vec<Value> {
        let bootstrap = self
            .get(&format!("/api/w/{workspace_id}/sync/bootstrap"), session)
            .await;
        assert_eq!(bootstrap.status, StatusCode::OK, "{}", bootstrap.body);
        bootstrap.body["records"][model]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    pub async fn get(&self, path: &str, session: &str) -> Response {
        self.call("GET", path, Some(session), None).await
    }

    pub async fn post(&self, path: &str, session: Option<&str>, body: Value) -> Response {
        self.call("POST", path, session, Some(body)).await
    }

    /// Signs someone up with a new workspace; returns their session and workspace id.
    pub async fn signup(&self, name: &str, workspace: &str) -> (String, String) {
        let email = format!("{}@example.com", name.to_lowercase());
        let response = self
            .post(
                "/api/auth/signup",
                None,
                json!({ "name": name, "email": email, "password": "correct horse", "workspace_name": workspace }),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        let workspace_id = response.body["workspaces"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        (response.session.expect("a session cookie"), workspace_id)
    }

    pub async fn invite(&self, admin: &str, workspace_id: &str, role: &str) -> String {
        let response = self
            .post(
                &format!("/api/w/{workspace_id}/invites"),
                Some(admin),
                json!({ "role": role }),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        response.body["token"].as_str().unwrap().to_string()
    }

    pub async fn join(&self, name: &str, token: &str) -> String {
        let email = format!("{}@example.com", name.to_lowercase());
        let response = self
            .post(
                "/api/auth/signup",
                None,
                json!({ "name": name, "email": email, "password": "correct horse", "invite_token": token }),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        response.session.unwrap()
    }
}
