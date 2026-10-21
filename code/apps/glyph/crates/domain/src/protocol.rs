//! What clients and the server send each other: the studio's sync protocol
//! (`code/kit/sync`), for Glyph's models.
//!
//! Writes: the client POSTs a [`Transaction`] and gets a [`TransactionResult`].
//! Reads: the client downloads a [`Bootstrap`] snapshot once, then follows the
//! change log over a WebSocket, which carries [`ServerMessage`]s.

use crate::model::ModelKind;

pub use code_villain_sync::{ApiError, SyncAction, TransactionResult};

pub type Transaction = code_villain_sync::Transaction<ModelKind>;
pub type Operation = code_villain_sync::Operation<ModelKind>;
pub type Bootstrap = code_villain_sync::Bootstrap<ModelKind>;
pub type SyncEntry = code_villain_sync::SyncEntry<ModelKind>;
pub type ServerMessage = code_villain_sync::ServerMessage<ModelKind>;
