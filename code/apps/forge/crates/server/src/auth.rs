//! Who is making a request: sessions, password hashing, and the extractors
//! handlers use to require a signed-in user or a workspace member.

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use axum::http::header::{AUTHORIZATION, COOKIE, SEC_WEBSOCKET_PROTOCOL};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use chrono::{Duration, Utc};
use forge_domain::{Actor, Role};
use sha2::{Digest, Sha256};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "forge_session";
/// Sent by the packaged desktop app, which keeps its session token itself
/// (in the system keychain) instead of in a cookie.
pub const CLIENT_HEADER: &str = "x-forge-client";
/// Where the desktop app gets its session token when it signs in.
pub const SESSION_HEADER: &str = "x-forge-session";
/// Browsers can't set headers on a WebSocket, so the desktop app offers its
/// token as a subprotocol, `bearer.<token>`, next to [`SYNC_PROTOCOL`].
pub const BEARER_PROTOCOL_PREFIX: &str = "bearer.";
pub const SYNC_PROTOCOL: &str = "forge";
const SESSION_DAYS: i64 = 60;

/// A random 256-bit token, hex-encoded, for sessions.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    hex::encode(bytes)
}

/// Tokens are stored only as hashes, so a leaked database can't be used to sign in.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Starts a session and returns its token, for the cookie.
pub async fn start_session(db: impl PgExecutor<'_>, user_id: Uuid) -> Result<String, AppError> {
    let token = new_token();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(Utc::now() + Duration::days(SESSION_DAYS))
        .execute(db)
        .await?;
    Ok(token)
}

pub fn session_cookie(token: &str, secure: bool) -> HeaderValue {
    cookie(token, SESSION_DAYS * 24 * 60 * 60, secure)
}

pub fn clear_session_cookie(secure: bool) -> HeaderValue {
    cookie("", 0, secure)
}

fn cookie(value: &str, max_age: i64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("a cookie made of hex and ASCII is a valid header")
}

/// The session token from an `Authorization: Bearer` header or the session cookie.
pub fn session_token(headers: &HeaderMap) -> Option<String> {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if let Some(token) = bearer {
        return Some(token.trim().to_string());
    }
    let protocol = headers
        .get_all(SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .find_map(|p| p.trim().strip_prefix(BEARER_PROTOCOL_PREFIX));
    if let Some(token) = protocol {
        return Some(token.to_string());
    }
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == SESSION_COOKIE && !value.is_empty()).then(|| value.to_string())
        })
}

/// The signed-in user. Rejects the request with 401 otherwise.
pub struct CurrentUser {
    pub id: Uuid,
    pub token_hash: Vec<u8>,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token =
            session_token(&parts.headers).ok_or_else(|| AppError::unauthorized("not signed in"))?;
        let token_hash = hash_token(&token);
        let user_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM sessions WHERE token_hash = $1 AND expires_at > now()",
        )
        .bind(&token_hash)
        .fetch_optional(&state.pool)
        .await?;
        let id = user_id
            .ok_or_else(|| AppError::unauthorized("your session has ended; sign in again"))?;
        Ok(CurrentUser { id, token_hash })
    }
}

/// The signed-in user as a member of the workspace in the path
/// (`/api/w/{workspace_id}/...`). Anyone else gets 404, so workspaces you
/// aren't in don't reveal that they exist.
pub struct Member {
    pub actor: Actor,
    pub role: Role,
    pub token_hash: Vec<u8>,
}

impl Member {
    pub fn require_admin(&self) -> Result<(), AppError> {
        match self.role {
            Role::Admin => Ok(()),
            Role::Member | Role::Guest => {
                Err(AppError::forbidden("only workspace admins can do that"))
            }
        }
    }

    /// For what spans the whole workspace, which a guest doesn't see.
    pub fn require_not_guest(&self) -> Result<(), AppError> {
        match self.role {
            Role::Guest => Err(AppError::forbidden("guests can't do that")),
            Role::Admin | Role::Member => Ok(()),
        }
    }

    /// For a guest, an issue outside their teams isn't there.
    pub async fn require_sees_issue(
        &self,
        pool: &sqlx::PgPool,
        issue: Uuid,
    ) -> Result<(), AppError> {
        if self.role != Role::Guest {
            return Ok(());
        }
        let mut db = pool.acquire().await?;
        if forge_sync::sees_issue(
            &mut db,
            self.actor.workspace_id,
            self.actor.user_id,
            Some(issue),
        )
        .await?
        {
            Ok(())
        } else {
            Err(AppError::not_found("issue not found"))
        }
    }
}

impl FromRequestParts<AppState> for Member {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        let not_found = || AppError::not_found("workspace not found");
        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found())?;
        let workspace_id: Uuid = params
            .get("workspace_id")
            .and_then(|id| id.parse().ok())
            .ok_or_else(not_found)?;
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(workspace_id)
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?;
        let role = role
            .as_deref()
            .and_then(Role::parse)
            .ok_or_else(not_found)?;
        Ok(Member {
            actor: Actor {
                user_id: user.id,
                workspace_id,
            },
            role,
            token_hash: user.token_hash,
        })
    }
}

/// Whether a session is still valid and its user still in the workspace.
/// Long-lived connections check this now and then.
pub async fn still_member(
    db: impl PgExecutor<'_>,
    token_hash: &[u8],
    actor: Actor,
) -> Result<bool, AppError> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sessions s JOIN workspace_members m ON m.user_id = s.user_id \
         WHERE s.token_hash = $1 AND s.expires_at > now() AND m.workspace_id = $2 AND m.user_id = $3",
    )
    .bind(token_hash)
    .bind(actor.workspace_id)
    .bind(actor.user_id)
    .fetch_optional(db)
    .await?;
    Ok(found.is_some())
}
