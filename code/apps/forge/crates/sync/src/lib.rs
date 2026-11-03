//! The server side of Forge's sync engine (see the "Sync engine" spec in Forge).
//!
//! The engine itself is the studio's ([`code_villain_sync`], in
//! `code/kit/sync`); this crate adds Forge's rules ([`Forge`]) and its own
//! reads, and names the engine's calls with Forge's types.
//!
//! Every write goes through [`apply_transaction`], whether it comes from the
//! UI, an agent or an accepted AI suggestion. It updates the model tables and
//! appends each change to `sync_log`. Clients download a [`bootstrap`]
//! snapshot once and then follow the log with [`entries_after`].

mod history;
mod review;
mod rules;
mod spec_change;
mod team_members;
mod work;

use std::collections::BTreeMap;

use code_villain_sync::GuestView;
use forge_domain::{Actor, Bootstrap, ModelKind, SyncAction, SyncEntry, Transaction};
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use uuid::Uuid;

pub use code_villain_sync::{Origin, SyncError, current_sync_id, lock_workspace, workspace_epoch};
pub use history::{issue_events, spec_versions};
pub use rules::{Forge, sees_issue};
pub use spec_change::{Standing, spec_change_standing};
pub use team_members::{
    add_everyone, follow_groups, join_every_team, leave_every_team, role_changed, set_teams,
};
pub use work::{WORK_LEASE_MINUTES, finish_work, renew_work, start_work};

/// The Postgres channel notified, with the workspace id as payload, whenever
/// a workspace's log grows.
pub const NOTIFY_CHANNEL: &str = "forge_sync";

/// The database migrations, in `/migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// Applies a client's transaction atomically and returns the sync id of its
/// last change, or `None` if it changed nothing. Retrying is safe.
pub async fn apply_transaction(
    pool: &PgPool,
    actor: Actor,
    transaction: &Transaction,
) -> Result<Option<i64>, SyncError> {
    code_villain_sync::apply_transaction::<Forge>(pool, actor, transaction).await
}

/// Applies a transaction inside the caller's database transaction, for
/// server code that changes synced records alongside other work.
pub async fn apply_in(
    db: &mut PgConnection,
    actor: Actor,
    transaction: &Transaction,
    origin: Origin,
) -> Result<Option<i64>, SyncError> {
    code_villain_sync::apply_in::<Forge>(db, actor, transaction, origin).await
}

/// Records a change the server made to a record outside of a transaction,
/// such as someone joining the workspace (a `User`).
pub async fn record_change(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    model_id: Uuid,
    action: SyncAction,
    data: Option<Value>,
) -> Result<i64, SyncError> {
    code_villain_sync::record_change::<Forge>(db, actor, model, model_id, action, data).await
}

/// Whether the viewer sees only some of the workspace: for a guest, the
/// version of what they may see; `None` for admins and members.
pub async fn guest_access(db: &mut PgConnection, viewer: Actor) -> Result<Option<i64>, SyncError> {
    <Forge as code_villain_sync::Rules>::guest_access(db, viewer).await
}

/// A consistent snapshot of every record the viewer syncs in the workspace.
pub async fn bootstrap(pool: &PgPool, viewer: Actor) -> Result<Bootstrap, SyncError> {
    code_villain_sync::bootstrap::<Forge>(pool, viewer).await
}

/// The records of some kinds that the viewer syncs, as [`bootstrap`] has
/// them (oldest first), from one consistent snapshot. For server code that
/// needs a few kinds, not the whole workspace.
pub async fn records(
    pool: &PgPool,
    viewer: Actor,
    models: &[ModelKind],
) -> Result<BTreeMap<ModelKind, Vec<Value>>, SyncError> {
    let mut db = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *db)
        .await?;
    let guest = guest_access(&mut db, viewer).await?.is_some();
    let mut records = BTreeMap::new();
    for model in models {
        let Some(visible) = visible(*model, guest, "$2") else {
            records.insert(*model, Vec::new());
            continue;
        };
        let sql = format!(
            "SELECT to_jsonb(t.*) FROM {} AS t WHERE t.workspace_id = $1 AND {visible} \
             ORDER BY t.created_at",
            model.table()
        );
        // Safe: the table name and the conditions are constants from `ModelKind`.
        let rows: Vec<Value> = sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(viewer.workspace_id)
            .bind(viewer.user_id)
            .fetch_all(&mut *db)
            .await?;
        records.insert(*model, rows);
    }
    db.commit().await?;
    Ok(records)
}

/// The condition on `t`, a record of `model`, that keeps a read to what the
/// viewer syncs of it, as [`bootstrap`] does: their own notifications and,
/// for a guest, their teams' records. The viewer's user id is bound as
/// `viewer` (such as `"$2"`). `None` when they see none of it.
pub fn visible(model: ModelKind, guest: bool, viewer: &str) -> Option<String> {
    let mut only = Vec::new();
    if model.private_to_user() {
        only.push(format!("t.user_id = {viewer}"));
    }
    if guest {
        match model.guest_view() {
            GuestView::All => {}
            GuestView::Where(sql) => only.push(format!("({})", sql.replace("$viewer", viewer))),
            GuestView::Hidden => return None,
        }
    }
    Some(if only.is_empty() {
        "TRUE".into()
    } else {
        only.join(" AND ")
    })
}

/// Up to `limit` log entries after `after` that the viewer may see, and the
/// position to continue from.
pub async fn entries_after(
    pool: &PgPool,
    viewer: Actor,
    after: i64,
    limit: i64,
) -> Result<(Vec<SyncEntry>, i64), SyncError> {
    code_villain_sync::entries_after::<Forge>(pool, viewer, after, limit).await
}
