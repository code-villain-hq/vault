//! The repositories a guest is added to. Admins and members have them all.
//!
//! Each change makes the guests it concerns start over from a fresh
//! snapshot: the guest whose repositories changed, and the guests who share
//! one of those repositories with them (they see each other).

use code_villain_sync::{lock_workspace, notify};
use layer_domain::Actor;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{Layer, SyncError};

/// Puts someone in exactly these repositories.
pub async fn set_repositories(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
    repositories: &[Uuid],
) -> Result<(), SyncError> {
    lock_workspace(db, actor.workspace_id).await?;
    let mut wanted = repositories.to_vec();
    wanted.sort();
    wanted.dedup();
    let known: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM repositories WHERE workspace_id = $1 AND id = ANY($2)",
    )
    .bind(actor.workspace_id)
    .bind(&wanted)
    .fetch_one(&mut *db)
    .await?;
    if known != wanted.len() as i64 {
        return Err(SyncError::Invalid("repository not found".into()));
    }
    let removed: Vec<Uuid> = sqlx::query_scalar(
        "DELETE FROM repository_access WHERE workspace_id = $1 AND user_id = $2 \
         AND NOT repository_id = ANY($3) RETURNING repository_id",
    )
    .bind(actor.workspace_id)
    .bind(user_id)
    .bind(&wanted)
    .fetch_all(&mut *db)
    .await?;
    let added: Vec<Uuid> = sqlx::query_scalar(
        "INSERT INTO repository_access (workspace_id, repository_id, user_id) \
         SELECT $1, unnest($3::uuid[]), $2 ON CONFLICT DO NOTHING RETURNING repository_id",
    )
    .bind(actor.workspace_id)
    .bind(user_id)
    .bind(&wanted)
    .fetch_all(&mut *db)
    .await?;
    let touched: Vec<Uuid> = removed.into_iter().chain(added).collect();
    if touched.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE workspace_members SET access_version = access_version + 1 \
         WHERE workspace_id = $1 AND (user_id = $2 OR (role = 'guest' AND user_id IN \
         (SELECT user_id FROM repository_access WHERE repository_id = ANY($3))))",
    )
    .bind(actor.workspace_id)
    .bind(user_id)
    .bind(&touched)
    .execute(&mut *db)
    .await?;
    notify::<Layer>(db, actor.workspace_id).await
}

/// Someone's role changed. Guests see the admins and members, so every
/// guest starts over, and so does the person, who may have become or
/// stopped being one.
pub async fn role_changed(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), SyncError> {
    sqlx::query(
        "UPDATE workspace_members SET access_version = access_version + 1 \
         WHERE workspace_id = $1 AND (user_id = $2 OR role = 'guest')",
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(&mut *db)
    .await?;
    Ok(())
}

/// A repository is going: its guests start over, so its notes, which stay
/// in the workspace, leave their copy with it.
pub(crate) async fn repository_going(
    db: &mut PgConnection,
    actor: Actor,
    repository_id: Uuid,
) -> Result<(), SyncError> {
    sqlx::query(
        "UPDATE workspace_members SET access_version = access_version + 1 \
         WHERE workspace_id = $1 AND user_id IN \
         (SELECT user_id FROM repository_access WHERE repository_id = $2)",
    )
    .bind(actor.workspace_id)
    .bind(repository_id)
    .execute(&mut *db)
    .await?;
    Ok(())
}
