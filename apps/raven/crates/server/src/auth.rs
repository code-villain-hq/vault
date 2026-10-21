//! Who is making a request: sessions, password hashing, and the extractor
//! handlers use to require a signed-in user. Glyph's, without workspaces and
//! the desktop app.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::header::{AUTHORIZATION, COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "raven_session";
const SESSION_DAYS: i64 = 60;

/// A random 256-bit token, hex-encoded, for sessions and ingest keys.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    hex::encode(bytes)
}

/// Tokens are stored only as hashes, so a leaked database can't be used to sign in.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)
}

/// A real hash of a throwaway password, checked when an account has no
/// password, so response times don't reveal which emails have accounts.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$KCbv1GHXuPgJBJiiwePHxA$NL6gvMeQB6J1KbyTIKFTYyGswt6zjU4UHX2+wfWh8B4";

pub async fn verify_password(password: String, hash: Option<String>) -> bool {
    tokio::task::spawn_blocking(move || {
        let has_hash = hash.is_some();
        let hash = hash.unwrap_or_else(|| DUMMY_HASH.into());
        let matches = PasswordHash::new(&hash).is_ok_and(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        });
        has_hash && matches
    })
    .await
    .unwrap_or(false)
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
