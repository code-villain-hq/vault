//! The company's people and groups (code/kit/scim), which its account sends
//! to `/api/scim/v2` with the `scim.token` setting. The workspace has the
//! groups the settings name (`forge-users`, `forge-admins` and
//! `forge-guests` unless `[groups]` says otherwise) as members, admins and
//! guests; a team linked to a group (`Team.group_name`) has the group's
//! people in it. Forge's agents are added in Forge and stay.

use anyhow::anyhow;
use code_villain_scim::Directory;
use forge_domain::{Actor, Role, is_agent_email};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{accounts, company, members};

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
            .map_err(|e| anyhow!("{e}"))?;
        company::finish_setting_up(db, workspace, user).await
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

    async fn stays(db: &mut PgConnection, user: Uuid) -> anyhow::Result<bool> {
        Ok(
            sqlx::query_scalar("SELECT kind = 'agent' FROM users WHERE id = $1")
                .bind(user)
                .fetch_optional(&mut *db)
                .await?
                .unwrap_or(false),
        )
    }

    fn refuses_email(email: &str) -> Option<&'static str> {
        is_agent_email(email).then_some("that address is kept for Forge's agents")
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

/// `GET /api/w/{id}/groups`: the company's groups, without their `-users`,
/// `-admins` and `-guests`, for a team to follow. Not there when the account
/// doesn't send its groups.
pub async fn list(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    member: crate::auth::Member,
) -> Result<axum::Json<Vec<String>>, crate::error::AppError> {
    member.require_admin()?;
    if state.scim_token.is_none() {
        return Err(crate::error::AppError::not_found("groups are off"));
    }
    let mut db = state.pool.acquire().await?;
    Ok(axum::Json(code_villain_scim::group_names(&mut db).await?))
}
