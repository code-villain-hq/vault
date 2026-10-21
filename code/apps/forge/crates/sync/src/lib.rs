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
mod work;

use forge_domain::{Actor, Bootstrap, ModelKind, SyncAction, SyncEntry, Transaction};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub use code_villain_sync::{Origin, SyncError, current_sync_id, lock_workspace, workspace_epoch};
pub use history::{issue_events, spec_versions};
pub use rules::Forge;
pub use spec_change::{Standing, spec_change_standing};
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

/// A consistent snapshot of every record the viewer syncs in the workspace.
pub async fn bootstrap(pool: &PgPool, viewer: Actor) -> Result<Bootstrap, SyncError> {
    code_villain_sync::bootstrap(pool, viewer).await
}

/// Up to `limit` log entries after `after` that the viewer may see, and the
/// position to continue from.
pub async fn entries_after(
    pool: &PgPool,
    viewer: Actor,
    after: i64,
    limit: i64,
) -> Result<(Vec<SyncEntry>, i64), SyncError> {
    code_villain_sync::entries_after(pool, viewer, after, limit).await
}
