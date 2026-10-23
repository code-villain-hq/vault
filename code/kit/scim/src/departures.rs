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
pub async fn settle_departures<D: Directory>(pool: &PgPool) -> anyhow::Result<()> {
    let mut db = pool.begin().await?;
    store::lock(&mut db).await?;
    let users: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT user_id FROM scim_departures WHERE remove_after <= now()",
    )
    .fetch_all(&mut *db)
    .await?;
    for &user in &users {
        reconcile::person::<D>(&mut db, user, None).await?;
    }
    D::groups_changed(&mut db, &users).await?;
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
