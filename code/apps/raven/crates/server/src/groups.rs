//! The company's people and groups (code/kit/scim), which its account sends
//! to `/api/scim/v2` with the `scim.token` setting. The workspace has the
//! groups the settings name (`raven-users`, `raven-admins` and
//! `raven-guests` unless `[groups]` says otherwise) as members, admins and
//! guests. A guest's apps are still set in Raven.

use anyhow::anyhow;
use code_villain_scim::Directory;
use raven_domain::{Actor, Role};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{accounts, company, members};

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
