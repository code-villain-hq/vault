//! Invite links: admins create them, anyone with one can see which workspace
//! it's for, and signing up or accepting with it joins the workspace.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use raven_domain::{CreateInviteRequest, CreatedInvite, Invite, InvitePreview, Me, Role};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::accounts::{add_member, load_me};
use crate::auth::{CurrentUser, Member, hash_token, new_token};
use crate::error::AppError;
use crate::state::AppState;

const INVITE_DAYS: i64 = 7;

pub struct ValidInvite {
    pub workspace_id: Uuid,
    pub role: Role,
}

/// The invite behind a token, if it exists, hasn't expired and wasn't revoked.
pub async fn find_valid(
    db: impl PgExecutor<'_>,
    token: &str,
) -> Result<Option<ValidInvite>, AppError> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT workspace_id, role FROM invites \
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(hash_token(token))
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|(workspace_id, role)| {
        Some(ValidInvite {
            workspace_id,
            role: Role::parse(&role)?,
        })
    }))
}

pub fn gone() -> AppError {
    AppError::not_found("this invite link has expired or was revoked")
}

pub async fn create(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<CreateInviteRequest>,
) -> Result<Json<CreatedInvite>, AppError> {
    member.require_admin()?;
    let token = new_token();
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO invites (id, workspace_id, token_hash, role, created_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(member.actor.workspace_id)
    .bind(hash_token(&token))
    .bind(request.role.as_str())
    .bind(member.actor.user_id)
    .bind(Utc::now() + Duration::days(INVITE_DAYS))
    .execute(&state.pool)
    .await?;

    let invite = active(&state, member.actor.workspace_id)
        .await?
        .into_iter()
        .find(|invite| invite.id == id)
        .ok_or_else(|| AppError::internal("new invite not found"))?;
    Ok(Json(CreatedInvite { invite, token }))
}

pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Vec<Invite>>, AppError> {
    member.require_admin()?;
    Ok(Json(active(&state, member.actor.workspace_id).await?))
}

#[derive(sqlx::FromRow)]
struct InviteRow {
    id: Uuid,
    role: String,
    created_by: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

async fn active(state: &AppState, workspace_id: Uuid) -> Result<Vec<Invite>, AppError> {
    let rows: Vec<InviteRow> = sqlx::query_as(
        "SELECT i.id, i.role, u.name AS created_by, i.created_at, i.expires_at FROM invites i \
         JOIN users u ON u.id = i.created_by \
         WHERE i.workspace_id = $1 AND i.revoked_at IS NULL AND i.expires_at > now() \
         ORDER BY i.created_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(Invite {
                id: row.id,
                role: Role::parse(&row.role)?,
                created_by: row.created_by,
                created_at: row.created_at,
                expires_at: row.expires_at,
            })
        })
        .collect())
}

pub async fn revoke(
    State(state): State<AppState>,
    member: Member,
    Path((_, invite_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let revoked = sqlx::query(
        "UPDATE invites SET revoked_at = now() WHERE id = $1 AND workspace_id = $2 AND revoked_at IS NULL",
    )
    .bind(invite_id)
    .bind(member.actor.workspace_id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if revoked == 0 {
        return Err(AppError::not_found("invite not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Which workspace an invite is for. Needs no sign-in: the token is the secret.
pub async fn preview(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<InvitePreview>, AppError> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT w.name, w.url_key, u.name, i.role FROM invites i \
         JOIN workspaces w ON w.id = i.workspace_id JOIN users u ON u.id = i.created_by \
         WHERE i.token_hash = $1 AND i.revoked_at IS NULL AND i.expires_at > now()",
    )
    .bind(hash_token(&token))
    .fetch_optional(&state.pool)
    .await?;
    let (workspace_name, workspace_url_key, invited_by, role) = row.ok_or_else(gone)?;
    Ok(Json(InvitePreview {
        workspace_name,
        workspace_url_key,
        invited_by,
        role: Role::parse(&role).ok_or_else(gone)?,
    }))
}

/// Joins the invite's workspace as the signed-in user.
pub async fn accept(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(token): Path<String>,
) -> Result<Json<Me>, AppError> {
    let invite = find_valid(&state.pool, &token).await?.ok_or_else(gone)?;
    let mut db = state.pool.begin().await?;
    add_member(&mut db, invite.workspace_id, user.id, invite.role).await?;
    db.commit().await?;
    Ok(Json(load_me(&state.pool, user.id).await?))
}
