//! The people in a workspace. Everyone can see them; admins change roles and
//! remove people. A workspace always keeps at least one admin.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use raven_domain::{Actor, MemberInfo, ModelKind, Role, SyncAction, UpdateMemberRequest, UserInfo};
use raven_sync::record_change;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Vec<MemberInfo>>, AppError> {
    let rows: Vec<(Uuid, String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT u.id, u.email, u.name, m.role, m.created_at FROM workspace_members m \
         JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 ORDER BY m.created_at",
    )
    .bind(member.actor.workspace_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|(id, email, name, role, joined_at)| {
                Some(MemberInfo {
                    user: UserInfo { id, email, name },
                    role: Role::parse(&role)?,
                    joined_at,
                })
            })
            .collect(),
    ))
}

pub async fn update(
    State(state): State<AppState>,
    member: Member,
    Path((_, user_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateMemberRequest>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let mut db = state.pool.begin().await?;
    let current = lock_and_get_role(&mut db, member.actor.workspace_id, user_id).await?;
    if current == Role::Admin && request.role != Role::Admin {
        ensure_another_admin(&mut db, member.actor.workspace_id).await?;
    }
    sqlx::query("UPDATE workspace_members SET role = $1 WHERE workspace_id = $2 AND user_id = $3")
        .bind(request.role.as_str())
        .bind(member.actor.workspace_id)
        .bind(user_id)
        .execute(&mut *db)
        .await?;
    log_user(&mut db, member.actor, user_id, SyncAction::Update).await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove(
    State(state): State<AppState>,
    member: Member,
    Path((_, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let mut db = state.pool.begin().await?;
    if lock_and_get_role(&mut db, member.actor.workspace_id, user_id).await? == Role::Admin {
        ensure_another_admin(&mut db, member.actor.workspace_id).await?;
    }
    log_user(&mut db, member.actor, user_id, SyncAction::Delete).await?;
    sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
        .bind(member.actor.workspace_id)
        .bind(user_id)
        .execute(&mut *db)
        .await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Tells clients a workspace member was added, changed or removed. Call it
/// after adding or changing the membership, and before removing it.
pub async fn log_user(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
    action: SyncAction,
) -> Result<(), AppError> {
    let data: Option<Value> = match action {
        SyncAction::Delete => None,
        SyncAction::Insert | SyncAction::Update => Some(
            sqlx::query_scalar("SELECT to_jsonb(u.*) FROM workspace_users u WHERE u.id = $1 AND u.workspace_id = $2")
                .bind(user_id)
                .bind(actor.workspace_id)
                .fetch_one(&mut *db)
                .await?,
        ),
    };
    record_change(db, actor, ModelKind::User, user_id, action, data).await?;
    Ok(())
}

/// Locks the workspace's membership against concurrent changes and returns
/// the person's role.
async fn lock_and_get_role(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Role, AppError> {
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace_id)
        .execute(&mut *db)
        .await?;
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_optional(&mut *db)
    .await?;
    role.as_deref()
        .and_then(Role::parse)
        .ok_or_else(|| AppError::not_found("member not found"))
}

async fn ensure_another_admin(db: &mut PgConnection, workspace_id: Uuid) -> Result<(), AppError> {
    let admins: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_members WHERE workspace_id = $1 AND role = 'admin'",
    )
    .bind(workspace_id)
    .fetch_one(&mut *db)
    .await?;
    if admins <= 1 {
        return Err(AppError::conflict("a workspace needs at least one admin"));
    }
    Ok(())
}
