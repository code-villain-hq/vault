//! Turning groups into roles: who should be in each linked workspace, and
//! as what, compared with who is.

use std::collections::{BTreeMap, BTreeSet};

use sqlx::PgConnection;
use uuid::Uuid;

use crate::{Directory, Role, store};

/// Brings each of these people's workspaces in line with their groups.
pub(crate) async fn people<D: Directory>(
    db: &mut PgConnection,
    users: &[Uuid],
) -> anyhow::Result<()> {
    for &user in users {
        person::<D>(db, user, None).await?;
    }
    D::groups_changed(db, users).await
}

/// The role someone's groups give them in each linked workspace (or only in
/// `only`), with the group it comes from, and what they have now; then the
/// joins, changes and leaves that close the gap. Someone who came by invite
/// and isn't in the groups stays as they are.
async fn person<D: Directory>(
    db: &mut PgConnection,
    user: Uuid,
    only: Option<Uuid>,
) -> anyhow::Result<()> {
    let from_groups: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT w.id, w.group_name, g.name FROM workspaces w \
         JOIN scim_groups g ON g.name IN (w.group_name, w.group_name || '-admins', w.group_name || '-guests') \
         JOIN scim_group_members m ON m.group_id = g.id AND m.user_id = $1 \
         JOIN scim_users s ON s.user_id = m.user_id AND s.active \
         WHERE w.group_name IS NOT NULL AND ($2::uuid IS NULL OR w.id = $2)",
    )
    .bind(user)
    .bind(only)
    .fetch_all(&mut *db)
    .await?;
    // Someone in more than one of a workspace's groups has the highest role.
    let mut wanted: BTreeMap<Uuid, (Role, String)> = BTreeMap::new();
    for (workspace, base, name) in from_groups {
        let role = if name == base {
            Role::Member
        } else if name.ends_with("-admins") {
            Role::Admin
        } else {
            Role::Guest
        };
        if wanted.get(&workspace).is_none_or(|(best, _)| role > *best) {
            wanted.insert(workspace, (role, name));
        }
    }

    let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT workspace_id, role, from_group FROM workspace_members \
         WHERE user_id = $1 AND ($2::uuid IS NULL OR workspace_id = $2)",
    )
    .bind(user)
    .bind(only)
    .fetch_all(&mut *db)
    .await?;
    let current: BTreeMap<Uuid, (Option<Role>, Option<String>)> = rows
        .into_iter()
        .map(|(workspace, role, from)| (workspace, (Role::parse(&role), from)))
        .collect();

    let workspaces: BTreeSet<Uuid> = wanted.keys().chain(current.keys()).copied().collect();
    for workspace in workspaces {
        match (wanted.get(&workspace), current.get(&workspace)) {
            (Some((role, from)), None) => {
                D::add_member(db, workspace, user, *role).await?;
                set_source(db, workspace, user, Some(from)).await?;
            }
            (Some((role, from)), Some((now, now_from))) => {
                if Some(*role) != *now {
                    D::change_role(db, workspace, user, *role).await?;
                }
                if now_from.as_deref() != Some(from.as_str()) {
                    set_source(db, workspace, user, Some(from)).await?;
                }
            }
            (None, Some((_, Some(_)))) => D::remove_member(db, workspace, user).await?,
            (None, _) => {}
        }
    }
    Ok(())
}

async fn set_source(
    db: &mut PgConnection,
    workspace: Uuid,
    user: Uuid,
    from: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE workspace_members SET from_group = $3 WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace)
    .bind(user)
    .bind(from)
    .execute(db)
    .await
    .map(|_| ())
}

/// Links a workspace to a group, or unlinks it with `None`, and brings its
/// people in line at once. Unlinking keeps everyone, with their roles, as
/// though they'd been invited.
pub async fn link_workspace<D: Directory>(
    db: &mut PgConnection,
    workspace: Uuid,
    group: Option<&str>,
) -> anyhow::Result<()> {
    store::lock(db).await?;
    sqlx::query("UPDATE workspaces SET group_name = $2 WHERE id = $1")
        .bind(workspace)
        .bind(group)
        .execute(&mut *db)
        .await?;
    if group.is_none() {
        sqlx::query("UPDATE workspace_members SET from_group = NULL WHERE workspace_id = $1")
            .bind(workspace)
            .execute(&mut *db)
            .await?;
    }
    let users: Vec<Uuid> = sqlx::query_scalar(
        "SELECT m.user_id FROM scim_group_members m JOIN scim_groups g ON g.id = m.group_id \
         WHERE g.name IN ($2, $2 || '-admins', $2 || '-guests') \
         UNION SELECT user_id FROM workspace_members WHERE workspace_id = $1 AND from_group IS NOT NULL",
    )
    .bind(workspace)
    .bind(group)
    .fetch_all(&mut *db)
    .await?;
    for &user in &users {
        person::<D>(db, user, Some(workspace)).await?;
    }
    D::groups_changed(db, &users).await
}

/// The group a workspace is linked to.
pub async fn linked_group(
    db: &mut PgConnection,
    workspace: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT group_name FROM workspaces WHERE id = $1")
        .bind(workspace)
        .fetch_optional(db)
        .await
        .map(Option::flatten)
}

/// The group someone's role in a workspace comes from, if it does: then the
/// role is changed in the Code Villain account, not here.
pub async fn role_source(
    db: &mut PgConnection,
    workspace: Uuid,
    user: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT from_group FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace)
    .bind(user)
    .fetch_optional(db)
    .await
    .map(Option::flatten)
}
