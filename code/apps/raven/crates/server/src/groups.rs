//! The Code Villain account's groups (code/kit/scim). Pocket ID sends its
//! people and groups to `/api/scim/v2` when `RAVEN_SCIM_TOKEN` is set. A
//! workspace linked to a tool's groups has `<tool>-users` as members, `<tool>-admins`
//! as admins and `<tool>-guests` as guests.

use anyhow::anyhow;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use code_villain_scim::Directory;
use raven_domain::{Actor, LinkGroupRequest, Role, WorkspaceGroup};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;
use crate::{accounts, members};

pub struct RavenDirectory;

impl Directory for RavenDirectory {
    async fn add_member(
        db: &mut PgConnection,
        workspace: Uuid,
        user: Uuid,
        role: code_villain_scim::Role,
    ) -> anyhow::Result<()> {
        accounts::add_member(db, workspace, user, role_of(role))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn change_role(
        db: &mut PgConnection,
        workspace: Uuid,
        user: Uuid,
        role: code_villain_scim::Role,
    ) -> anyhow::Result<()> {
        let current = members::lock_and_get_role(db, workspace, user)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        members::set_role(db, actor(workspace, user), user, current, role_of(role))
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    async fn remove_member(
        db: &mut PgConnection,
        workspace: Uuid,
        user: Uuid,
    ) -> anyhow::Result<()> {
        members::lock_and_get_role(db, workspace, user)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        members::take_out(db, actor(workspace, user), user)
            .await
            .map_err(|e| anyhow!("{e}"))
    }
}

fn role_of(role: code_villain_scim::Role) -> Role {
    match role {
        code_villain_scim::Role::Admin => Role::Admin,
        code_villain_scim::Role::Member => Role::Member,
        code_villain_scim::Role::Guest => Role::Guest,
    }
}

fn actor(workspace_id: Uuid, user_id: Uuid) -> Actor {
    Actor {
        user_id,
        workspace_id,
    }
}

/// `GET /api/w/{id}/group`: the group the workspace follows, and the groups
/// it could. Not there when the account doesn't send its groups.
pub async fn get(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<WorkspaceGroup>, AppError> {
    member.require_admin()?;
    if state.scim_token.is_none() {
        return Err(AppError::not_found("groups are off"));
    }
    let mut db = state.pool.acquire().await?;
    Ok(Json(WorkspaceGroup {
        group: code_villain_scim::linked_group(&mut db, member.actor.workspace_id).await?,
        groups: code_villain_scim::group_names(&mut db).await?,
    }))
}

/// `PUT /api/w/{id}/group`: follows a group, or none. Its people join at
/// once; unlinking keeps everyone as they are, as though invited.
pub async fn link(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<LinkGroupRequest>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    if state.scim_token.is_none() {
        return Err(AppError::not_found("groups are off"));
    }
    let mut db = state.pool.begin().await?;
    let group = request
        .group
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty());
    if let Some(group) = group
        && !code_villain_scim::group_names(&mut db)
            .await?
            .iter()
            .any(|g| g == group)
    {
        return Err(AppError::invalid(format!(
            "there's no group {group} in the Code Villain account"
        )));
    }
    code_villain_scim::link_workspace::<RavenDirectory>(&mut db, member.actor.workspace_id, group)
        .await
        .map_err(|e| AppError::invalid(e.to_string()))?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
