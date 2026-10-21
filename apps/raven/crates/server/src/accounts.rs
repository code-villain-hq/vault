//! Signing up, signing in and out, the signed-in user, and passwords.
//! Glyph's, without workspaces: everyone signed in to Raven sees every app.
//! The first person to come sets Raven up; after that, people sign in with
//! Code Villain (or, without it, use the account they have).

use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{
    CLIENT_HEADER, CurrentUser, SESSION_HEADER, clear_session_cookie, hash_password, hash_token,
    session_cookie, session_token, start_session, verify_password,
};
use crate::error::AppError;
use crate::limits::ClientAddress;
use crate::state::AppState;

const MIN_PASSWORD: usize = 10;
const MAX_PASSWORD: usize = 200;
const MAX_NAME: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

/// The signed-in user.
#[derive(Debug, Clone, Serialize)]
pub struct Me {
    pub user: UserInfo,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignupRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

pub fn normalize_email(email: &str) -> Result<String, AppError> {
    let email = email.trim().to_lowercase();
    let valid = email.len() <= 254
        && !email.contains(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });
    if valid {
        Ok(email)
    } else {
        Err(AppError::invalid("that doesn't look like an email address"))
    }
}

pub fn check_password(password: &str) -> Result<(), AppError> {
    let length = password.chars().count();
    if length < MIN_PASSWORD {
        return Err(AppError::invalid(format!(
            "password must be at least {MIN_PASSWORD} characters"
        )));
    }
    if length > MAX_PASSWORD {
        return Err(AppError::invalid(format!(
            "password must be at most {MAX_PASSWORD} characters"
        )));
    }
    Ok(())
}

/// Trims a name and checks it isn't empty or too long.
pub fn clean_name(name: &str, what: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid(format!("{what} can't be empty")));
    }
    if name.chars().count() > MAX_NAME {
        return Err(AppError::invalid(format!(
            "{what} is longer than {MAX_NAME} characters"
        )));
    }
    Ok(name.to_string())
}

/// With Code Villain sign-in on, it's the only way in: no passwords.
fn passwords_open(state: &AppState) -> Result<(), AppError> {
    match state.oidc_login {
        Some(_) => Err(AppError::not_found("sign in with Code Villain instead")),
        None => Ok(()),
    }
}

/// Whether anyone has an account yet. Until someone has, the sign-in page
/// offers to set Raven up.
pub async fn has_accounts(pool: &PgPool) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(pool)
        .await?)
}

/// Creates the first account, at most five tries an hour from one client.
/// Once there is one, nobody else can sign up with a password.
pub async fn signup(
    State(state): State<AppState>,
    client: ClientAddress,
    headers: HeaderMap,
    Json(request): Json<SignupRequest>,
) -> Result<Response, AppError> {
    passwords_open(&state)?;
    state
        .signup_attempts
        .count(&client.key())
        .map_err(AppError::too_many_signups)?;
    let name = clean_name(&request.name, "name")?;
    let email = normalize_email(&request.email)?;
    check_password(&request.password)?;
    let password_hash = hash_password(request.password).await?;

    let mut db = state.pool.begin().await?;
    // One sign-up at a time, so two people can't both be first.
    sqlx::query("LOCK TABLE users IN EXCLUSIVE MODE")
        .execute(&mut *db)
        .await?;
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&mut *db)
        .await?;
    if taken {
        return Err(AppError::forbidden(
            "Raven is set up already: sign in with your account",
        ));
    }
    let user_id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, name, password_hash) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind(&email)
        .bind(&name)
        .bind(&password_hash)
        .execute(&mut *db)
        .await?;
    let token = start_session(&mut *db, user_id).await?;
    db.commit().await?;
    signed_in(&state, &headers, &token, user_id).await
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, AppError> {
    passwords_open(&state)?;
    let email = normalize_email(&request.email).unwrap_or_default();
    let attempt = state
        .password_attempts
        .begin(&email)
        .map_err(AppError::too_many)?;
    let account: Option<(Uuid, Option<String>)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    let (user_id, hash) = account.map_or((None, None), |(id, hash)| (Some(id), hash));

    // Check a password even when there's no account, so both take as long.
    let matches = verify_password(request.password, hash).await;
    let Some(user_id) = user_id.filter(|_| matches) else {
        attempt.failed();
        return Err(AppError::unauthorized("wrong email or password"));
    };
    attempt.succeeded();

    let token = start_session(&state.pool, user_id).await?;
    signed_in(&state, &headers, &token, user_id).await
}

/// Ends the current session, if there is one, and clears the cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(hash_token(&token))
            .execute(&state.pool)
            .await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(SET_COOKIE, clear_session_cookie(state.cookie_secure))],
    )
        .into_response())
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> Result<Json<Me>, AppError> {
    Ok(Json(load_me(&state.pool, user.id).await?))
}

/// Sets a new random password and ends every session; for the
/// `reset-password` command.
pub async fn reset_password(pool: &PgPool, email: &str) -> anyhow::Result<String> {
    let email = normalize_email(email).map_err(|e| anyhow::anyhow!(e.message))?;
    let password = readable_password();
    let hash = hash_password(password.clone())
        .await
        .map_err(|_| anyhow::anyhow!("couldn't hash the password"))?;

    let mut db = pool.begin().await?;
    let user_id: Option<Uuid> =
        sqlx::query_scalar("UPDATE users SET password_hash = $1 WHERE email = $2 RETURNING id")
            .bind(hash)
            .bind(&email)
            .fetch_optional(&mut *db)
            .await?;
    let user_id =
        user_id.ok_or_else(|| anyhow::anyhow!("nobody has an account with the email {email}"))?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *db)
        .await?;
    db.commit().await?;
    Ok(password)
}

/// 20 random letters and digits in groups of four, avoiding look-alikes.
fn readable_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 20];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    let chars: Vec<char> = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    chars
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// The response to signing in: who you are, and the session as a cookie,
/// or for the desktop app as a header it keeps in the keychain.
pub async fn signed_in(
    state: &AppState,
    headers: &HeaderMap,
    token: &str,
    user_id: Uuid,
) -> Result<Response, AppError> {
    let me = load_me(&state.pool, user_id).await?;
    if headers.get(CLIENT_HEADER).is_some_and(|v| v == "desktop") {
        return Ok(([(SESSION_HEADER, token)], Json(me)).into_response());
    }
    Ok((
        [(SET_COOKIE, session_cookie(token, state.cookie_secure))],
        Json(me),
    )
        .into_response())
}

pub async fn load_me(pool: &PgPool, user_id: Uuid) -> Result<Me, AppError> {
    let (id, email, name): (Uuid, String, String) =
        sqlx::query_as("SELECT id, email, name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(Me {
        user: UserInfo { id, email, name },
    })
}
