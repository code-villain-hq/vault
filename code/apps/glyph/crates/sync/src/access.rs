//! The languages a guest is added to. Admins and members have them all.
//!
//! Each change makes the guests it concerns start over from a fresh
//! snapshot: the guest whose languages changed, and the guests who share one
//! of those languages with them (they see each other).

use code_villain_sync::{lock_workspace, notify};
use glyph_domain::Actor;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{Glyph, SyncError};

/// Puts someone in exactly these languages.
pub async fn set_languages(
    db: &mut PgConnection,
    actor: Actor,
    user_id: Uuid,
    languages: &[Uuid],
) -> Result<(), SyncError> {
    lock_workspace(db, actor.workspace_id).await?;
    let mut wanted = languages.to_vec();
    wanted.sort();
    wanted.dedup();
    let known: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM languages WHERE workspace_id = $1 AND id = ANY($2)",
    )
    .bind(actor.workspace_id)
    .bind(&wanted)
    .fetch_one(&mut *db)
    .await?;
    if known != wanted.len() as i64 {
        return Err(SyncError::Invalid("language not found".into()));
    }
    let removed: Vec<Uuid> = sqlx::query_scalar(
        "DELETE FROM language_access WHERE workspace_id = $1 AND user_id = $2 \
         AND NOT language_id = ANY($3) RETURNING language_id",
    )
    .bind(actor.workspace_id)
    .bind(user_id)
    .bind(&wanted)
    .fetch_all(&mut *db)
    .await?;
    let added: Vec<Uuid> = sqlx::query_scalar(
        "INSERT INTO language_access (workspace_id, language_id, user_id) \
         SELECT $1, unnest($3::uuid[]), $2 ON CONFLICT DO NOTHING RETURNING language_id",
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
         (SELECT user_id FROM language_access WHERE language_id = ANY($3))))",
    )
    .bind(actor.workspace_id)
    .bind(user_id)
    .bind(&touched)
    .execute(&mut *db)
    .await?;
    notify::<Glyph>(db, actor.workspace_id).await
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

/// Every guest starts over, as when the source language changes.
pub async fn every_guest_starts_over(
    db: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<(), SyncError> {
    sqlx::query(
        "UPDATE workspace_members SET access_version = access_version + 1 \
         WHERE workspace_id = $1 AND role = 'guest'",
    )
    .bind(workspace_id)
    .execute(&mut *db)
    .await?;
    Ok(())
}
