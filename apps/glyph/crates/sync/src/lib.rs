//! The server side of Glyph's sync engine: a copy of Forge's, with the same
//! protocol and tables, and Glyph's own models.
//!
//! Every write goes through [`apply_transaction`], whether it comes from the
//! UI, an import or the translation worker. It updates the model tables and
//! appends each change to `sync_log`. Clients download a [`bootstrap`]
//! snapshot once and then follow the log with [`entries_after`].

mod apply;
mod error;
mod read;

pub use apply::{Origin, apply_in, apply_transaction, lock_workspace, record_change};
pub use error::SyncError;
pub use read::{bootstrap, current_sync_id, entries_after, workspace_epoch};

/// The Postgres channel notified, with the workspace id as payload, whenever
/// a workspace's log grows.
pub const NOTIFY_CHANNEL: &str = "glyph_sync";

/// The database migrations, in `/migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
