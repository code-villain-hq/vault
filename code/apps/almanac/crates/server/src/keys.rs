//! Settings → Claude: who answers. On this Mac it's Claude Code, as you're
//! signed in to it, and the only choice is whether it may search the web
//! (`claude_choices`). A server answers with a key, kept in `claude_key`,
//! which never syncs, and never sent back: only its last four characters,
//! to tell keys apart. Admins set and remove both.

use almanac_domain::{ClaudeCodeInfo, ClaudeKeyInfo, SetClaudeKeyRequest, SetClaudeWebRequest};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::acp::{self, Failure};
use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

const STORED: &str = "SELECT api_key FROM claude_key";
const STORE: &str = "INSERT INTO claude_key (api_key) VALUES ($1) \
     ON CONFLICT (only_one) DO UPDATE SET api_key = excluded.api_key, updated_at = now()";
const FORGET: &str = "DELETE FROM claude_key";
const WEB: &str = "SELECT web FROM claude_choices";
const STORE_WEB: &str = "INSERT INTO claude_choices (web) VALUES ($1) \
     ON CONFLICT (only_one) DO UPDATE SET web = excluded.web, updated_at = now()";

pub async fn show(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<ClaudeKeyInfo>, AppError> {
    member.require_not_guest()?;
    let web: Option<bool> = sqlx::query_scalar(WEB).fetch_optional(&state.pool).await?;
    let web = web.unwrap_or(true);
    if let Some(claude_code) = &state.claude_code {
        let (state, version, problem) = match claude_code.check().await {
            Ok(version) => ("ready", version, None),
            Err(Failure::NotInstalled) => ("not_installed", None, None),
            Err(Failure::SignedOut) => ("signed_out", None, None),
            Err(Failure::Trouble(said)) => ("trouble", None, Some(said)),
        };
        return Ok(Json(ClaudeKeyInfo {
            via: "claude_code".into(),
            claude_code: Some(ClaudeCodeInfo {
                state: state.into(),
                version,
                install: acp::INSTALL.into(),
                problem,
            }),
            web,
            set: false,
            ends_with: None,
            from_settings: false,
        }));
    }
    let stored: Option<String> = sqlx::query_scalar(STORED)
        .fetch_optional(&state.pool)
        .await?;
    let ends_with = stored.as_deref().map(|key| {
        let chars: Vec<char> = key.chars().collect();
        chars[chars.len().saturating_sub(4)..].iter().collect()
    });
    Ok(Json(ClaudeKeyInfo {
        via: "key".into(),
        claude_code: None,
        web: false,
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

/// `PUT /api/w/{id}/claude/web`: whether Claude Code may search the web.
pub async fn set_web(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<SetClaudeWebRequest>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    if state.claude_code.is_none() {
        return Err(AppError::invalid(
            "Only Claude Code on this Mac searches the web for Almanac.",
        ));
    }
    sqlx::query(STORE_WEB)
        .bind(request.web)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
