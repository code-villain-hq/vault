//! Who is making a request: sessions and the extractors handlers use to
//! require a signed-in user or a workspace member. Token helpers live in
//! `code-villain-auth`.

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use code_villain_auth::SessionNames;
use glyph_domain::{Actor, Role};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

pub const NAMES: SessionNames = SessionNames {
    cookie: "glyph_session",
    client_header: "x-glyph-client",
    session_header: "x-glyph-session",
    bearer_protocol_prefix: "bearer.",
    sync_protocol: "glyph",
};

pub const SESSION_COOKIE: &str = NAMES.cookie;
pub const CLIENT_HEADER: &str = NAMES.client_header;
pub const SESSION_HEADER: &str = NAMES.session_header;
pub const BEARER_PROTOCOL_PREFIX: &str = NAMES.bearer_protocol_prefix;
pub const SYNC_PROTOCOL: &str = NAMES.sync_protocol;

pub use code_villain_auth::{hash_token, new_token};

/// Starts a session and returns its token, for the cookie.
pub async fn start_session(db: impl PgExecutor<'_>, user_id: Uuid) -> Result<String, AppError> {
    Ok(code_villain_auth::start_session(db, user_id).await?)
}

pub fn session_cookie(token: &str, secure: bool) -> HeaderValue {
    code_villain_auth::session_cookie(NAMES, token, secure)
}

pub fn clear_session_cookie(secure: bool) -> HeaderValue {
    code_villain_auth::clear_session_cookie(NAMES, secure)
}

/// The session token from an `Authorization: Bearer` header or the session cookie.
pub fn session_token(headers: &HeaderMap) -> Option<String> {
    code_villain_auth::session_token(NAMES, headers)
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
        let id = code_villain_auth::user_for_session(&state.pool, &token_hash)
            .await?
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

    /// A guest may read the source language and languages they are assigned.
    pub async fn require_reads(&self, pool: &sqlx::PgPool, language: &str) -> Result<(), AppError> {
        if self.role != Role::Guest {
            return Ok(());
        }
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM languages l WHERE l.workspace_id = $1 AND l.code = $2 \
             AND (l.is_source OR l.id IN (SELECT language_id FROM language_access WHERE user_id = $3)))"
        ).bind(self.actor.workspace_id).bind(language).bind(self.actor.user_id)
         .fetch_one(pool).await?;
        if visible {
            Ok(())
        } else {
            Err(AppError::not_found("language not found"))
        }
    }

    /// For a guest, translating into a language they haven't been added to
    /// is refused, as their copy has no such language.
    pub async fn require_translates(
        &self,
        pool: &sqlx::PgPool,
        language: &str,
    ) -> Result<(), AppError> {
        if self.role != Role::Guest {
            return Ok(());
        }
        let theirs: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM languages l JOIN language_access a \
             ON a.language_id = l.id AND a.user_id = $3 \
             WHERE l.workspace_id = $1 AND l.code = $2)",
        )
        .bind(self.actor.workspace_id)
        .bind(language)
        .bind(self.actor.user_id)
        .fetch_one(pool)
        .await?;
        if theirs {
            Ok(())
        } else {
            Err(AppError::not_found("language not found"))
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
        let role = code_villain_auth::member_role(&state.pool, workspace_id, user.id)
            .await?
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
    Ok(code_villain_auth::still_member(db, token_hash, actor.workspace_id, actor.user_id).await?)
}
