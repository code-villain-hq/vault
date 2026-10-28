//! Settings → Claude: the key Almanac answers with. It's kept in
//! `claude_key`, which never syncs, and never sent back: only its last four
//! characters, to tell keys apart. Admins set and remove it.

use almanac_domain::{ClaudeKeyInfo, SetClaudeKeyRequest};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

const STORED: &str = "SELECT api_key FROM claude_key";
const STORE: &str = "INSERT INTO claude_key (api_key) VALUES ($1) \
     ON CONFLICT (only_one) DO UPDATE SET api_key = excluded.api_key, updated_at = now()";
const FORGET: &str = "DELETE FROM claude_key";

pub async fn show(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<ClaudeKeyInfo>, AppError> {
    member.require_not_guest()?;
    let stored: Option<String> = sqlx::query_scalar(STORED)
        .fetch_optional(&state.pool)
        .await?;
    let ends_with = stored.as_deref().map(|key| {
        let chars: Vec<char> = key.chars().collect();
        chars[chars.len().saturating_sub(4)..].iter().collect()
    });
    Ok(Json(ClaudeKeyInfo {
        set: stored.is_some(),
        ends_with,
        from_settings: state.claude.has_settings_key(),
    }))
}

pub async fn set(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<SetClaudeKeyRequest>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let key = request.api_key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) || key.len() > 400 {
        return Err(AppError::invalid(
            "That doesn't look like a key. Copy it again from the Claude Console.",
        ));
    }
    sqlx::query(STORE).bind(key).execute(&state.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn forget(State(state): State<AppState>, member: Member) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    sqlx::query(FORGET).execute(&state.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}
