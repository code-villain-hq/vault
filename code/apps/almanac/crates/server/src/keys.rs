//! Settings → Claude: who answers. On this Mac it's Claude Code, as you're
//! signed in to it, or ChatGPT, signed in to from here; the choices are
//! which, and whether it may search the web (`claude_choices`). A server answers with a key, kept in `claude_key`,
//! which never syncs, and never sent back: only its last four characters,
//! to tell keys apart. Admins set and remove both.

use almanac_domain::{
    ClaudeCodeInfo, ClaudeKeyInfo, SetAnswersRequest, SetClaudeKeyRequest, SetClaudeWebRequest,
};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::acp::{Adapter, Failure};
use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

/// The key set in Settings, which `claude.rs` answers with.
pub(crate) const STORED: &str = "SELECT api_key FROM claude_key";
const STORE: &str = "INSERT INTO claude_key (api_key) VALUES ($1) \
     ON CONFLICT (only_one) DO UPDATE SET api_key = excluded.api_key, updated_at = now()";
const FORGET: &str = "DELETE FROM claude_key";
/// Whether Claude may search the web, which a talk reads too.
pub(crate) const WEB: &str = "SELECT web FROM claude_choices";
const ANSWERS: &str = "SELECT answers FROM claude_choices";
const STORE_ANSWERS: &str = "INSERT INTO claude_choices (answers) VALUES ($1) \
     ON CONFLICT (only_one) DO UPDATE SET answers = excluded.answers, updated_at = now()";
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
        let answers: Option<String> = sqlx::query_scalar(ANSWERS)
            .fetch_optional(&state.pool)
            .await?;
        let chatgpt = match &state.chatgpt {
            Some(chatgpt) => Some(how_it_is(chatgpt).await),
            None => None,
        };
        return Ok(Json(ClaudeKeyInfo {
            via: "claude_code".into(),
            claude_code: Some(how_it_is(claude_code).await),
            answers: Some(answers.unwrap_or_else(|| "claude_code".into())),
            chatgpt,
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
        chatgpt: None,
        answers: None,
        web: false,
        set: stored.is_some(),
        ends_with,
        from_settings: state.claude.has_settings_key(),
    }))
}

/// Whether it can answer, as Settings shows it.
async fn how_it_is(adapter: &Adapter) -> ClaudeCodeInfo {
    let (state, version, problem) = match adapter.check().await {
        Ok(version) => ("ready", version, None),
        Err(Failure::NotInstalled) => ("not_installed", None, None),
        Err(Failure::SignedOut) => ("signed_out", None, None),
        Err(Failure::Trouble(said)) => ("trouble", None, Some(said)),
    };
    ClaudeCodeInfo {
        state: state.into(),
        version,
        install: adapter.kind().install().into(),
        problem,
    }
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
            "Only Claude Code or ChatGPT on this Mac searches the web for Almanac.",
        ));
    }
    sqlx::query(STORE_WEB)
        .bind(request.web)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/w/{id}/claude/answers`: who answers on this Mac.
pub async fn set_answers(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<SetAnswersRequest>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let known = match request.answers.as_str() {
        "claude_code" => state.claude_code.is_some(),
        "chatgpt" => state.chatgpt.is_some(),
        _ => false,
    };
    if !known {
        return Err(AppError::invalid(
            "Only Claude Code or ChatGPT on this Mac answers for Almanac.",
        ));
    }
    sqlx::query(STORE_ANSWERS)
        .bind(&request.answers)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/w/{id}/claude/chatgpt/sign-in`: signs ChatGPT in for
/// Almanac, in the browser. It answers once that's done there.
pub async fn sign_in_chatgpt(
    State(state): State<AppState>,
    member: Member,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let Some(chatgpt) = &state.chatgpt else {
        return Err(AppError::invalid("ChatGPT answers only on this Mac."));
    };
    match chatgpt.sign_in().await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(Failure::NotInstalled) => Err(AppError::invalid(
            "Install ChatGPT's adapter first, with the command above.",
        )),
        Err(Failure::SignedOut) => Err(AppError::invalid(
            "The sign-in didn't finish. Try again, and finish it in the browser.",
        )),
        Err(Failure::Trouble(said)) => Err(AppError::invalid(said)),
    }
}
