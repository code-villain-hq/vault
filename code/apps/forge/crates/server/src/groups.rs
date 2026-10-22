//! The Code Villain account's groups (code/kit/scim). Pocket ID sends its
//! people and groups to `/api/scim/v2` when `FORGE_SCIM_TOKEN` is set. A
//! workspace linked to a tool's groups has `<tool>-users` as members, `<tool>-admins`
//! as admins and `<tool>-guests` as guests; a team linked to a group
//! (`Team.group_name`) has the group's people in the workspace.

use anyhow::anyhow;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use code_villain_scim::Directory;
use forge_domain::{Actor, LinkGroupRequest, Role, WorkspaceGroup};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;
use crate::{accounts, members};

pub struct ForgeDirectory;

impl Directory for ForgeDirectory {
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

    /// Puts them in the teams linked to their groups, and out of the others.
    async fn groups_changed(db: &mut PgConnection, users: &[Uuid]) -> anyhow::Result<()> {
        let workspaces: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT workspace_id FROM teams WHERE group_name IS NOT NULL \
             AND workspace_id IN (SELECT workspace_id FROM workspace_members WHERE user_id = ANY($1))",
        )
        .bind(users)
        .fetch_all(&mut *db)
        .await?;
        for workspace in workspaces {
            // The first of them stands for the change in the log.
            let Some(&first) = users.first() else { break };
            forge_sync::follow_groups(db, actor(workspace, first), None, Some(users)).await?;
        }
        Ok(())
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
    code_villain_scim::link_workspace::<ForgeDirectory>(&mut db, member.actor.workspace_id, group)
        .await
        .map_err(|e| AppError::invalid(e.to_string()))?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
