//! The server side of Postrider's sync engine.
//!
//! The engine itself is the studio's ([`code_villain_sync`], in
//! `code/kit/sync`); this crate adds Postrider's rules ([`Postrider`]) and
//! names the engine's calls with Postrider's types.
//!
//! Only Postrider's own records sync: the mail accounts, the calendar
//! server, and you. Mail and events stay on their servers.
//!
//! Every write goes through [`apply_transaction`] or [`apply_in`]. It
//! updates the model tables and appends each change to `sync_log`. Clients
//! download a [`bootstrap`] snapshot once and then follow the log.

mod rules;

use postrider_domain::{Actor, Bootstrap, ModelKind, SyncAction, Transaction};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub use code_villain_sync::{Origin, SyncError, current_sync_id, lock_workspace, workspace_epoch};
pub use rules::Postrider;

/// The Postgres channel notified, with the workspace id as payload, whenever
/// a workspace's log grows.
pub const NOTIFY_CHANNEL: &str = "postrider_sync";

/// The database migrations, in `/migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// Applies a client's transaction atomically and returns the sync id of its
/// last change, or `None` if it changed nothing. Retrying is safe.
pub async fn apply_transaction(
    pool: &PgPool,
    actor: Actor,
    transaction: &Transaction,
) -> Result<Option<i64>, SyncError> {
    code_villain_sync::apply_transaction::<Postrider>(pool, actor, transaction).await
}

/// Applies a transaction inside the caller's database transaction, for
/// server code that changes synced records, such as adding an account.
pub async fn apply_in(
    db: &mut PgConnection,
    actor: Actor,
    transaction: &Transaction,
    origin: Origin,
) -> Result<Option<i64>, SyncError> {
    code_villain_sync::apply_in::<Postrider>(db, actor, transaction, origin).await
}

/// Records a change the server made to a record outside of a transaction,
/// such as you joining your workspace (a `User`).
pub async fn record_change(
    db: &mut PgConnection,
    actor: Actor,
    model: ModelKind,
    model_id: Uuid,
    action: SyncAction,
    data: Option<Value>,
) -> Result<i64, SyncError> {
    code_villain_sync::record_change::<Postrider>(db, actor, model, model_id, action, data).await
}

/// A consistent snapshot of every record the viewer syncs in the workspace.
pub async fn bootstrap(pool: &PgPool, viewer: Actor) -> Result<Bootstrap, SyncError> {
    code_villain_sync::bootstrap::<Postrider>(pool, viewer).await
}
