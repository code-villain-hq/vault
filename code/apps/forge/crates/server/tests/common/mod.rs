//! Shared helpers for the HTTP tests: requests through the real router.
#![allow(dead_code)]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use forge_domain::Role;
use forge_server::{AppState, accounts, auth, router};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub struct App {
    pub router: Router,
    pub pool: PgPool,
    /// Requests come from a new address each, unless a test says otherwise.
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
        Self::with_state(AppState::new(pool.clone(), false))
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
            request = request.header(header::COOKIE, format!("forge_session={token}"));
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

    pub async fn get(&self, path: &str, session: &str) -> Response {
        self.call("GET", path, Some(session), None).await
    }

    pub async fn post(&self, path: &str, session: Option<&str>, body: Value) -> Response {
        self.call("POST", path, session, Some(body)).await
    }

    /// Someone with a new workspace, as the company's account would bring
    /// them in; returns their session and workspace id.
    pub async fn signup(&self, name: &str, workspace: &str) -> (String, String) {
        let user_id = self.person(name).await;
        let mut db = self.pool.begin().await.unwrap();
        let workspace_id = accounts::create_workspace(&mut db, workspace, user_id)
            .await
            .unwrap();
        db.commit().await.unwrap();
        let session = auth::start_session(&self.pool, user_id).await.unwrap();
        (session, workspace_id.to_string())
    }

    /// Where someone joins as `role`: what [`App::join`] takes. People come
    /// from the company's account, so there are no invite links; the admin
    /// stays in the signature to keep tests reading as before.
    pub async fn invite(&self, _admin: &str, workspace_id: &str, role: &str) -> String {
        format!("{workspace_id}/{role}")
    }

    /// Someone joins the workspace [`App::invite`] named; returns their session.
    pub async fn join(&self, name: &str, invite: &str) -> String {
        let (workspace_id, role) = invite.split_once('/').unwrap();
        let workspace_id: Uuid = workspace_id.parse().unwrap();
        let role = match role {
            "admin" => Role::Admin,
            "guest" => Role::Guest,
            _ => Role::Member,
        };
        let user_id = self.person(name).await;
        let mut db = self.pool.begin().await.unwrap();
        accounts::add_member(&mut db, workspace_id, user_id, role)
            .await
            .unwrap();
        db.commit().await.unwrap();
        auth::start_session(&self.pool, user_id).await.unwrap()
    }

    /// The person called `name`, at `<name>@example.com`.
    pub async fn person(&self, name: &str) -> Uuid {
        let email = format!("{}@example.com", name.to_lowercase());
        sqlx::query_scalar(
            "INSERT INTO users (id, email, name) VALUES ($1, $2, $3)
             ON CONFLICT (email) DO UPDATE SET name = users.name RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(email)
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
}
