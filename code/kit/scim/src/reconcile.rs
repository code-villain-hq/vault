//! Turning groups into roles: who should be in each workspace that follows
//! groups, and as what, compared with who is.

use std::collections::{BTreeMap, BTreeSet};

use sqlx::PgConnection;
use uuid::Uuid;

use crate::{Directory, Role, group_key, store};

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

/// The role someone's groups give them in each workspace that follows groups
/// (or only in `only`), with the group it comes from, and what they have now;
/// then the joins, changes and leaves that close the gap. Someone whose role
/// didn't come from a group is left here: [`follow_groups`] takes them out.
/// Pocket ID sends people before their groups, so a newcomer isn't in one yet.
pub(crate) async fn person<D: Directory>(
    db: &mut PgConnection,
    user: Uuid,
    only: Option<Uuid>,
) -> anyhow::Result<()> {
    let from_groups: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT w.id, g.name, CASE g.name WHEN w.admins_group THEN 'admin' \
         WHEN w.users_group THEN 'member' ELSE 'guest' END FROM workspaces w \
         JOIN scim_groups g ON g.name IN (w.users_group, w.admins_group, w.guests_group) \
         JOIN scim_group_members m ON m.group_id = g.id AND m.user_id = $1 \
         JOIN scim_users s ON s.user_id = m.user_id AND s.active \
         WHERE w.users_group IS NOT NULL AND ($2::uuid IS NULL OR w.id = $2)",
    )
    .bind(user)
    .bind(only)
    .fetch_all(&mut *db)
    .await?;
    // Someone in more than one of a workspace's groups has the highest role.
    let mut wanted: BTreeMap<Uuid, (Role, String)> = BTreeMap::new();
    for (workspace, name, role) in from_groups {
        let role = Role::parse(&role).unwrap_or(Role::Guest);
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
        if wanted.contains_key(&workspace) {
            sqlx::query("DELETE FROM scim_departures WHERE workspace_id = $1 AND user_id = $2")
                .bind(workspace)
                .bind(user)
                .execute(&mut *db)
                .await?;
        }
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
            (None, Some((_, Some(_)))) => {
                crate::departures::depart::<D>(db, workspace, user).await?
            }
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

/// The three groups that give a workspace's roles: its members, its admins
/// and its guests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Groups {
    pub users: String,
    pub admins: String,
    pub guests: String,
}

impl Groups {
    /// `<app>-users`, `<app>-admins` and `<app>-guests`, or the names given,
    /// read as names ([`group_key`]): "Forge admins" is forge-admins.
    pub fn new(app: &str, users: Option<&str>, admins: Option<&str>, guests: Option<&str>) -> Self {
        let name = |given: Option<&str>, ending: &str| {
            given
                .map(group_key)
                .filter(|g| !g.is_empty())
                .unwrap_or_else(|| format!("{app}-{ending}"))
        };
        Groups {
            users: name(users, "users"),
            admins: name(admins, "admins"),
            guests: name(guests, "guests"),
        }
    }
}

/// Makes a workspace follow `groups` and brings its people in line: the
/// groups' people join with their roles and, once the account has sent its
/// groups, everyone else leaves, but for those [`Directory::stays`] keeps.
/// The server does this as it starts, with the groups its settings name.
pub async fn follow_groups<D: Directory>(
    db: &mut PgConnection,
    workspace: Uuid,
    groups: &Groups,
) -> anyhow::Result<()> {
    store::lock(db).await?;
    sqlx::query(
        "UPDATE workspaces SET users_group = $2, admins_group = $3, guests_group = $4 WHERE id = $1",
    )
    .bind(workspace)
    .bind(&groups.users)
    .bind(&groups.admins)
    .bind(&groups.guests)
    .execute(&mut *db)
    .await?;
    let users: Vec<Uuid> = sqlx::query_scalar(
        "SELECT m.user_id FROM scim_group_members m JOIN scim_groups g ON g.id = m.group_id \
         WHERE g.name IN ($2, $3, $4) \
         UNION SELECT user_id FROM workspace_members WHERE workspace_id = $1",
    )
    .bind(workspace)
    .bind(&groups.users)
    .bind(&groups.admins)
    .bind(&groups.guests)
    .fetch_all(&mut *db)
    .await?;
    for &user in &users {
        person::<D>(db, user, Some(workspace)).await?;
    }
    // Until the account has sent its groups nobody is in one, and leaving
    // would empty the workspace.
    let sent: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM scim_groups)")
        .fetch_one(&mut *db)
        .await?;
    if sent {
        let strays: Vec<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM workspace_members WHERE workspace_id = $1 AND from_group IS NULL",
        )
        .bind(workspace)
        .fetch_all(&mut *db)
        .await?;
        for user in strays {
            if !D::stays(db, user).await? {
                D::remove_member(db, workspace, user).await?;
                store::end_sessions(db, user).await?;
            }
        }
    }
    D::groups_changed(db, &users).await
}

/// The group someone's role in a workspace comes from, if it does: then the
/// role is changed in the company's account, not here.
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
