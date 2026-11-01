//! Session tokens and the headers/cookies that carry them.

use axum::http::header::{AUTHORIZATION, COOKIE, SEC_WEBSOCKET_PROTOCOL};
use axum::http::{HeaderMap, HeaderValue};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgExecutor;
use uuid::Uuid;

/// How long a signed-in session lasts.
pub const SESSION_DAYS: i64 = 60;

/// Names that differ per app (`forge_session`, `x-forge-client`, …).
#[derive(Debug, Clone, Copy)]
pub struct SessionNames {
    pub cookie: &'static str,
    /// Packaged desktop app: `x-<app>-client: desktop`.
    pub client_header: &'static str,
    /// Packaged desktop app: where the session token comes back.
    pub session_header: &'static str,
    /// WebSocket subprotocol prefix: `bearer.<token>`.
    pub bearer_protocol_prefix: &'static str,
    /// Sync WebSocket subprotocol: the app's key (`forge`, `glyph`, …).
    pub sync_protocol: &'static str,
}

/// A random 256-bit token, hex-encoded, for sessions (and similar secrets).
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    hex::encode(bytes)
}

/// Tokens are stored only as hashes, so a leaked database can't be used to sign in.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Starts a session and returns its token.
pub async fn start_session(db: impl PgExecutor<'_>, user_id: Uuid) -> Result<String, sqlx::Error> {
    let token = new_token();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(Utc::now() + Duration::days(SESSION_DAYS))
        .execute(db)
        .await?;
    Ok(token)
}

/// Ends a session by its token hash. No-op when nothing matches.
pub async fn end_session(db: impl PgExecutor<'_>, token_hash: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(db)
        .await?;
    Ok(())
}

pub fn session_cookie(names: SessionNames, token: &str, secure: bool) -> HeaderValue {
    cookie(names.cookie, token, SESSION_DAYS * 24 * 60 * 60, secure)
}

pub fn clear_session_cookie(names: SessionNames, secure: bool) -> HeaderValue {
    cookie(names.cookie, "", 0, secure)
}

fn cookie(name: &str, value: &str, max_age: i64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("a cookie made of hex and ASCII is a valid header")
}

/// Who this session token is for, if it is still current.
pub async fn user_for_session(
    db: impl PgExecutor<'_>,
    token_hash: &[u8],
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT user_id FROM sessions WHERE token_hash = $1 AND expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await
}

/// Whether a session is still valid and its user still in the workspace.
pub async fn still_member(
    db: impl PgExecutor<'_>,
    token_hash: &[u8],
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sessions s JOIN workspace_members m ON m.user_id = s.user_id \
         WHERE s.token_hash = $1 AND s.expires_at > now() AND m.workspace_id = $2 AND m.user_id = $3",
    )
    .bind(token_hash)
    .bind(workspace_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(found.is_some())
}

/// Workspace role for a member, if any.
pub async fn member_role(
    db: impl PgExecutor<'_>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// The session token from an `Authorization: Bearer` header, a WebSocket
/// bearer subprotocol, or the session cookie.
pub fn session_token(names: SessionNames, headers: &HeaderMap) -> Option<String> {
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
        .find_map(|p| p.trim().strip_prefix(names.bearer_protocol_prefix));
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
            (name == names.cookie && !value.is_empty()).then(|| value.to_string())
        })
}
