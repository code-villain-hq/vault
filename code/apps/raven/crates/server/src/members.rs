//! The people in a workspace. Admins and members can see them; admins change
//! roles, put guests in apps and remove people. A workspace always keeps at
//! least one admin. Someone whose role comes from a group of the Code
//! Villain account ([`crate::groups`]) keeps it until the group changes.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use raven_domain::{
    Actor, MemberInfo, ModelKind, Role, SyncAction, UpdateAccessRequest, UpdateMemberRequest,
    UserInfo,
};
use raven_sync::{record_change, role_changed, set_apps};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

/// Id, email, name, role, when they joined, their apps, and the group their role
/// comes from.
type MemberRow = (
    Uuid,
    String,
    String,
    String,
    DateTime<Utc>,
    Vec<Uuid>,
    Option<String>,
);

pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Vec<MemberInfo>>, AppError> {
    // A guest's Settings are their account's alone.
    member.require_not_guest()?;
    let rows: Vec<MemberRow> = sqlx::query_as(
        "SELECT u.id, u.email, u.name, m.role, m.created_at, \
         ARRAY(SELECT a.app_id FROM app_access a WHERE a.user_id = m.user_id \
               AND a.workspace_id = m.workspace_id ORDER BY a.app_id), m.from_group \
         FROM workspace_members m \
         JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 ORDER BY m.created_at",
    )
    .bind(member.actor.workspace_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|(id, email, name, role, joined_at, access, from_group)| {
                Some(MemberInfo {
                    user: UserInfo { id, email, name },
                    role: Role::parse(&role)?,
                    joined_at,
                    access,
                    from_group,
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
    not_from_group(&mut db, member.actor.workspace_id, user_id).await?;
    if current == Role::Admin && request.role != Role::Admin {
        ensure_another_admin(&mut db, member.actor.workspace_id).await?;
    }
    set_role(&mut db, member.actor, user_id, current, request.role).await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Gives someone a new role and tells clients.
pub async fn set_role(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
    current: Role,
    role: Role,
) -> Result<(), AppError> {
    sqlx::query("UPDATE workspace_members SET role = $1 WHERE workspace_id = $2 AND user_id = $3")
        .bind(role.as_str())
        .bind(actor.workspace_id)
        .bind(user_id)
        .execute(&mut *db)
        .await?;
    if role != current {
        role_changed(db, actor.workspace_id, user_id).await?;
    }
    log_user(db, actor, user_id, SyncAction::Update).await
}

/// Puts someone in exactly the apps asked for. Only a guest's matter.
pub async fn update_access(
    State(state): State<AppState>,
    member: Member,
    Path((_, user_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateAccessRequest>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let mut db = state.pool.begin().await?;
    lock_and_get_role(&mut db, member.actor.workspace_id, user_id).await?;
    set_apps(&mut db, member.actor, user_id, &request.items).await?;
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
    not_from_group(&mut db, member.actor.workspace_id, user_id).await?;
    take_out(&mut db, member.actor, user_id).await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Takes someone out of a workspace.
pub async fn take_out(db: &mut PgConnection, actor: Actor, user_id: Uuid) -> Result<(), AppError> {
    log_user(db, actor, user_id, SyncAction::Delete).await?;
    sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
        .bind(actor.workspace_id)
        .bind(user_id)
        .execute(&mut *db)
        .await?;
    Ok(())
}

/// Refuses to change a role that comes from a group: it's changed in the
/// Code Villain account.
async fn not_from_group(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    match code_villain_scim::role_source(db, workspace_id, user_id).await? {
        Some(group) => Err(AppError::conflict(format!(
            "their role comes from {group}: it's changed in the Code Villain account"
        ))),
        None => Ok(()),
    }
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
pub async fn lock_and_get_role(
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
