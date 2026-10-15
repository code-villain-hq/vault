//! The server side of Forge's sync engine (see the "Sync engine" spec in Forge).
//!
//! Every write goes through [`apply_transaction`], whether it comes from the
//! UI, an agent or an accepted AI suggestion. It updates the model tables and
//! appends each change to `sync_log`. Clients download a [`bootstrap`]
//! snapshot once and then follow the log with [`entries_after`].

mod apply;
mod error;
mod read;
mod spec_change;
mod work;

pub use apply::{Origin, apply_in, apply_transaction, lock_workspace, record_change};
pub use error::SyncError;
pub use read::{
    bootstrap, current_sync_id, entries_after, issue_events, spec_versions, workspace_epoch,
};
pub use spec_change::{Standing, spec_change_standing};
pub use work::{WORK_LEASE_MINUTES, finish_work, renew_work, start_work};

/// The Postgres channel notified, with the workspace id as payload, whenever
/// a workspace's log grows.
pub const NOTIFY_CHANNEL: &str = "forge_sync";

/// The database migrations, in `/migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
