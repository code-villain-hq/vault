//! Settle directory role transfers before treating them as departures.
use crate::{Directory, reconcile, store};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(crate) async fn depart<D: Directory>(
    db: &mut PgConnection,
    workspace: Uuid,
    user: Uuid,
) -> anyhow::Result<()> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM scim_users WHERE user_id = $1 AND active)",
    )
    .bind(user)
    .fetch_one(&mut *db)
    .await?;
    if active {
        sqlx::query("INSERT INTO scim_departures (workspace_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(workspace).bind(user).execute(&mut *db).await?;
        let due: bool = sqlx::query_scalar("SELECT remove_after <= now() FROM scim_departures WHERE workspace_id = $1 AND user_id = $2")
            .bind(workspace).bind(user).fetch_one(&mut *db).await?;
        if !due {
            return Ok(());
        }
    }
    // Disabled/deleted people leave immediately. A true group departure
    // ends its sessions too; a transfer cancelled the pending removal first.
    D::remove_member(db, workspace, user).await?;
    store::end_sessions(db, user).await?;
    Ok(())
}

/// Finish due removals, rechecking groups under the same lock as SCIM writes.
/// Each person settles on their own: one whose removal fails (the app's
/// `remove_member` refusing, say) is tried again next time, and doesn't keep
/// everyone else in the workspace with their sessions.
pub async fn settle_departures<D: Directory>(pool: &PgPool) -> anyhow::Result<()> {
    // Read outside the lock: a group update since may have kept someone,
    // which reconciling them under it sees.
    let users: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT user_id FROM scim_departures WHERE remove_after <= now()",
    )
    .fetch_all(pool)
    .await?;
    for user in users {
        if let Err(error) = settle::<D>(pool, user).await {
            tracing::warn!(%error, %user, "couldn't settle a directory departure");
        }
    }
    Ok(())
}

/// One person's due removals, and what follows their groups, together.
async fn settle<D: Directory>(pool: &PgPool, user: Uuid) -> anyhow::Result<()> {
    let mut db = pool.begin().await?;
    store::lock(&mut db).await?;
    reconcile::person::<D>(&mut db, user, None).await?;
    D::groups_changed(&mut db, &[user]).await?;
    db.commit().await?;
    Ok(())
}

/// Every server runs this beside its feed; restart cannot forget a removal.
pub async fn watch_departures<D: Directory>(pool: PgPool) {
    let mut ticks = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        ticks.tick().await;
        if let Err(error) = settle_departures::<D>(&pool).await {
            tracing::warn!(%error, "couldn't settle directory departures");
        }
    }
}
