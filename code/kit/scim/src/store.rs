//! The people and groups, in `scim_users`, `scim_groups` and
//! `scim_group_members`, and the accounts behind the people.

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::ScimError;
use crate::resources::{GroupIn, GroupRow, UserIn, UserRow};

/// Changes arrive one at a time: two syncs at once would undo each other.
pub async fn lock(db: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(7281)")
        .execute(db)
        .await
        .map(|_| ())
}

type UserTuple = (Uuid, String, String, String, String, bool, DateTime<Utc>);

const USER_COLUMNS: &str = "SELECT s.user_id, s.external_id, s.user_name, u.name, u.email, s.active, s.created_at \
     FROM scim_users s JOIN users u ON u.id = s.user_id";

fn user_row(t: UserTuple) -> UserRow {
    UserRow {
        id: t.0,
        external_id: t.1,
        user_name: t.2,
        name: t.3,
        email: t.4,
        active: t.5,
        created_at: t.6,
    }
}

pub async fn list_users(
    db: &mut PgConnection,
    offset: i64,
    limit: i64,
) -> Result<(i64, Vec<UserRow>), sqlx::Error> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM scim_users")
        .fetch_one(&mut *db)
        .await?;
    let rows: Vec<UserTuple> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{USER_COLUMNS} ORDER BY s.created_at, s.user_id OFFSET $1 LIMIT $2"
    )))
    .bind(offset)
    .bind(limit)
    .fetch_all(&mut *db)
    .await?;
    Ok((total, rows.into_iter().map(user_row).collect()))
}

pub async fn user(db: &mut PgConnection, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    let row: Option<UserTuple> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{USER_COLUMNS} WHERE s.user_id = $1"
    )))
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(user_row))
}

/// Finds or makes the account for someone new: the one signing in already
/// uses (`oidc_sub`), else the one with their email that no studio account
/// has yet, else a new one. Signing in later finds it by `oidc_sub`.
pub async fn create_user(db: &mut PgConnection, user: &UserIn) -> Result<Uuid, ScimError> {
    let external_id = user
        .external_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            ScimError::typed(
                StatusCode::BAD_REQUEST,
                "invalidValue",
                "externalId is needed",
            )
        })?;
    let known: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM scim_users WHERE external_id = $1")
            .bind(external_id)
            .fetch_optional(&mut *db)
            .await?;
    if let Some(id) = known {
        replace_user(db, id, user).await?;
        return Ok(id);
    }
    let linked: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE oidc_sub = $1")
        .bind(external_id)
        .fetch_optional(&mut *db)
        .await?;
    let id = match linked {
        Some(id) => id,
        None => {
            let email = user.email().ok_or_else(|| {
                ScimError::typed(
                    StatusCode::BAD_REQUEST,
                    "invalidValue",
                    "an email is needed: accounts here are known by it",
                )
            })?;
            let existing: Option<(Uuid, bool)> =
                sqlx::query_as("SELECT id, oidc_sub IS NULL FROM users WHERE email = $1")
                    .bind(&email)
                    .fetch_optional(&mut *db)
                    .await?;
            match existing {
                Some((id, true)) => {
                    sqlx::query("UPDATE users SET oidc_sub = $1 WHERE id = $2")
                        .bind(external_id)
                        .bind(id)
                        .execute(&mut *db)
                        .await?;
                    id
                }
                Some((_, false)) => {
                    return Err(ScimError::typed(
                        StatusCode::CONFLICT,
                        "uniqueness",
                        "an account with this email belongs to another studio account",
                    ));
                }
                None => {
                    let id = Uuid::now_v7();
                    sqlx::query(
                        "INSERT INTO users (id, email, name, oidc_sub) VALUES ($1, $2, $3, $4)",
                    )
                    .bind(id)
                    .bind(&email)
                    .bind(user.shown_name())
                    .bind(external_id)
                    .execute(&mut *db)
                    .await?;
                    id
                }
            }
        }
    };
    sqlx::query(
        "INSERT INTO scim_users (user_id, external_id, user_name, active) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (user_id) DO UPDATE SET external_id = $2, user_name = $3, active = $4, updated_at = now()",
    )
    .bind(id)
    .bind(external_id)
    .bind(&user.user_name)
    .bind(user.active.unwrap_or(true))
    .execute(&mut *db)
    .await?;
    Ok(id)
}

/// Keeps what the account says of someone. Their name here is theirs to
/// change, so it's set only when the account is made.
pub async fn replace_user(
    db: &mut PgConnection,
    id: Uuid,
    user: &UserIn,
) -> Result<bool, sqlx::Error> {
    let changed = sqlx::query(
        "UPDATE scim_users SET user_name = $2, active = $3, \
         external_id = COALESCE($4, external_id), updated_at = now() WHERE user_id = $1",
    )
    .bind(id)
    .bind(&user.user_name)
    .bind(user.active.unwrap_or(true))
    .bind(user.external_id.as_deref().filter(|e| !e.is_empty()))
    .execute(db)
    .await?
    .rows_affected();
    Ok(changed == 1)
}

/// Forgets that the account sent someone, and takes them out of its groups.
pub async fn delete_user(db: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query("DELETE FROM scim_group_members WHERE user_id = $1")
        .bind(id)
        .execute(&mut *db)
        .await?;
    let deleted = sqlx::query("DELETE FROM scim_users WHERE user_id = $1")
        .bind(id)
        .execute(&mut *db)
        .await?
        .rows_affected();
    Ok(deleted == 1)
}

/// Signs someone out everywhere: their sessions, on the web and on desktops.
pub async fn end_sessions(db: &mut PgConnection, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(id)
        .execute(db)
        .await
        .map(|_| ())
}

type GroupTuple = (Uuid, Option<String>, String, Vec<Uuid>, DateTime<Utc>);

const GROUP_COLUMNS: &str = "SELECT g.id, g.external_id, g.name, \
     ARRAY(SELECT m.user_id FROM scim_group_members m WHERE m.group_id = g.id ORDER BY m.user_id), \
     g.created_at FROM scim_groups g";

fn group_row(t: GroupTuple) -> GroupRow {
    GroupRow {
        id: t.0,
        external_id: t.1,
        name: t.2,
        members: t.3,
        created_at: t.4,
    }
}

pub async fn list_groups(
    db: &mut PgConnection,
    offset: i64,
    limit: i64,
) -> Result<(i64, Vec<GroupRow>), sqlx::Error> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM scim_groups")
        .fetch_one(&mut *db)
        .await?;
    let rows: Vec<GroupTuple> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{GROUP_COLUMNS} ORDER BY g.created_at, g.id OFFSET $1 LIMIT $2"
    )))
    .bind(offset)
    .bind(limit)
    .fetch_all(&mut *db)
    .await?;
    Ok((total, rows.into_iter().map(group_row).collect()))
}

pub async fn group(db: &mut PgConnection, id: Uuid) -> Result<Option<GroupRow>, sqlx::Error> {
    let row: Option<GroupTuple> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{GROUP_COLUMNS} WHERE g.id = $1"
    )))
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(group_row))
}

pub async fn group_member_ids(db: &mut PgConnection, id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT user_id FROM scim_group_members WHERE group_id = $1")
        .bind(id)
        .fetch_all(db)
        .await
}

/// A group's name, from the friendly name Pocket ID sends as `displayName`
/// (it doesn't send the group's own name): lowercase, with anything that
/// isn't a letter or digit as one `-`. "Forge admins", `forge_admins` and
/// `forge-admins` are all `forge-admins`.
pub fn group_key(display_name: &str) -> String {
    let mut key = String::new();
    for c in display_name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            key.push(c);
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    key.trim_end_matches('-').to_string()
}

fn group_name(group: &GroupIn) -> Result<String, ScimError> {
    let name = group_key(&group.display_name);
    if name.is_empty() {
        return Err(ScimError::typed(
            StatusCode::BAD_REQUEST,
            "invalidValue",
            "displayName is needed",
        ));
    }
    Ok(name)
}

pub async fn create_group(db: &mut PgConnection, group: &GroupIn) -> Result<Uuid, ScimError> {
    let name = group_name(group)?;
    let external_id = group.external_id.as_deref().filter(|e| !e.is_empty());
    let known: Option<Uuid> = match external_id {
        Some(external_id) => {
            sqlx::query_scalar("SELECT id FROM scim_groups WHERE external_id = $1")
                .bind(external_id)
                .fetch_optional(&mut *db)
                .await?
        }
        None => None,
    };
    let id = match known {
        Some(id) => id,
        None => {
            let id = Uuid::now_v7();
            sqlx::query("INSERT INTO scim_groups (id, external_id, name) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(external_id)
                .bind(&name)
                .execute(&mut *db)
                .await?;
            id
        }
    };
    replace_group(db, id, group).await?;
    Ok(id)
}

/// Sets a group's name and exactly its people. A member this server doesn't
/// know is left out: Pocket ID sends people before their groups.
pub async fn replace_group(
    db: &mut PgConnection,
    id: Uuid,
    group: &GroupIn,
) -> Result<bool, ScimError> {
    let name = group_name(group)?;
    let changed = sqlx::query(
        "UPDATE scim_groups SET name = $2, external_id = COALESCE($3, external_id), \
         updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(&name)
    .bind(group.external_id.as_deref().filter(|e| !e.is_empty()))
    .execute(&mut *db)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(false);
    }
    let members: Vec<Uuid> = group
        .members
        .iter()
        .filter_map(|m| Uuid::parse_str(&m.value).ok())
        .collect();
    sqlx::query("DELETE FROM scim_group_members WHERE group_id = $1")
        .bind(id)
        .execute(&mut *db)
        .await?;
    sqlx::query(
        "INSERT INTO scim_group_members (group_id, user_id) \
         SELECT $1, s.user_id FROM scim_users s WHERE s.user_id = ANY($2) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(&members)
    .execute(&mut *db)
    .await?;
    Ok(true)
}

pub async fn delete_group(db: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query("DELETE FROM scim_groups WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?
        .rows_affected();
    Ok(deleted == 1)
}

/// The groups a workspace or a team can link to: each base name once, so
/// `forge` stands for `forge-users`, `forge-admins` and `forge-guests`. A
/// group without one of those endings links nowhere.
pub async fn group_names(db: &mut PgConnection) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT regexp_replace(name, '-(users|admins|guests)$', '') AS base \
         FROM scim_groups WHERE name ~ '.-(users|admins|guests)$' ORDER BY base",
    )
    .fetch_all(db)
    .await
}

/// Whether someone is in a group of this name and turned on in the account.
pub async fn in_group(db: &mut PgConnection, user: Uuid, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM scim_group_members m \
         JOIN scim_groups g ON g.id = m.group_id \
         JOIN scim_users s ON s.user_id = m.user_id AND s.active \
         WHERE m.user_id = $1 AND g.name = $2)",
    )
    .bind(user)
    .bind(name)
    .fetch_one(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::group_key;

    #[test]
    fn a_friendly_name_is_matched_as_a_name() {
        assert_eq!(group_key("Forge admins"), "forge-admins");
        assert_eq!(group_key("forge_admins"), "forge-admins");
        assert_eq!(group_key(" Forge -- Users "), "forge-users");
        assert_eq!(group_key("forge-guests"), "forge-guests");
        assert_eq!(group_key("  "), "");
    }
}
