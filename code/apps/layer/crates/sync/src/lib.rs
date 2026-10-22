//! The server side of Layer's sync engine.
//!
//! The engine itself is the studio's ([`code_villain_sync`], in
//! `code/kit/sync`), the same one Forge uses; this crate adds Layer's rules
//! ([`Layer`]) and names the engine's calls with Layer's types.
//!
//! Only a workspace's shared setup syncs: its repositories, connections,
//! log sources, agent models and notes, and its members.
//!
//! Every write goes through [`apply_transaction`]. It updates the model
//! tables and appends each change to `sync_log`. Clients download a
//! [`bootstrap`] snapshot once and then follow the log with
//! [`entries_after`].

mod access;
mod rules;

use layer_domain::{Actor, Bootstrap, ModelKind, SyncAction, SyncEntry, Transaction};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub use access::{role_changed, set_repositories};
pub use code_villain_sync::{Origin, SyncError, current_sync_id, lock_workspace, workspace_epoch};
pub use rules::Layer;

/// The Postgres channel notified, with the workspace id as payload, whenever
/// a workspace's log grows.
pub const NOTIFY_CHANNEL: &str = "layer_sync";

/// The database migrations, in `/migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// Applies a client's transaction atomically and returns the sync id of its
/// last change, or `None` if it changed nothing. Retrying is safe.
pub async fn apply_transaction(
    pool: &PgPool,
    actor: Actor,
    transaction: &Transaction,
) -> Result<Option<i64>, SyncError> {
    code_villain_sync::apply_transaction::<Layer>(pool, actor, transaction).await
}

/// Applies a transaction inside the caller's database transaction, for
/// server code that changes synced records alongside other work.
pub async fn apply_in(
    db: &mut PgConnection,
    actor: Actor,
    transaction: &Transaction,
    origin: Origin,
) -> Result<Option<i64>, SyncError> {
    code_villain_sync::apply_in::<Layer>(db, actor, transaction, origin).await
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
    code_villain_sync::record_change::<Layer>(db, actor, model, model_id, action, data).await
}

/// A consistent snapshot of every record the viewer syncs in the workspace.
pub async fn bootstrap(pool: &PgPool, viewer: Actor) -> Result<Bootstrap, SyncError> {
    code_villain_sync::bootstrap::<Layer>(pool, viewer).await
}

/// Up to `limit` log entries after `after` that the viewer may see, and the
/// position to continue from.
pub async fn entries_after(
    pool: &PgPool,
    viewer: Actor,
    after: i64,
    limit: i64,
) -> Result<(Vec<SyncEntry>, i64), SyncError> {
    code_villain_sync::entries_after::<Layer>(pool, viewer, after, limit).await
}
