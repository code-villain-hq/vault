//! The server side of the studio's sync engine, shared by every app (see the
//! "Sync engine" spec in Forge). An app brings its models ([`Model`]) and its
//! rules ([`Rules`]); the engine keeps the log and serves it.
//!
//! Every write goes through [`apply_transaction`], whether it comes from the
//! UI, an agent or the server itself. It updates the model tables and appends
//! each change to `sync_log`. Clients download a [`bootstrap`] snapshot once
//! and then follow the log with [`follow`], which [`listen`] wakes.
//!
//! The tables it needs are in `schema.sql`.

mod apply;
mod error;
mod feed;
mod listen;
mod model;
mod protocol;
mod read;
mod scope;

pub use apply::{
    Origin, Rules, apply_in, apply_operation, apply_transaction, current, insert, lock_workspace,
    log, notify, record_change,
};
pub use error::SyncError;
pub use feed::{Position, follow};
pub use listen::listen;
pub use model::{Actor, GuestView, Model, ValidationError, Write};
pub use protocol::{
    ApiError, Bootstrap, Operation, ServerMessage, SyncAction, SyncEntry, Transaction,
    TransactionResult,
};
pub use read::{bootstrap, current_sync_id, entries_after, workspace_epoch};
pub use scope::served_epoch;
