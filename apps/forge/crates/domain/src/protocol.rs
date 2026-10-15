//! What clients and the server send each other.
//!
//! Writes: the client POSTs a [`Transaction`] and gets a [`TransactionResult`].
//! Reads: the client downloads a [`Bootstrap`] snapshot once, then follows the
//! change log over a WebSocket, which carries [`ServerMessage`]s.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::ModelKind;

/// A group of changes that succeed or fail together. The client chooses the
/// id; sending the same transaction twice applies it once.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Transaction {
    pub id: Uuid,
    pub operations: Vec<Operation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum Operation {
    /// Create a record. The client chooses the id (a UUIDv7), so it can
    /// create records while offline.
    Create {
        model: ModelKind,
        id: Uuid,
        #[ts(type = "Record<string, unknown>")]
        data: Map<String, Value>,
    },
    /// Change some fields of a record. Fields not in `data` are untouched.
    Update {
        model: ModelKind,
        id: Uuid,
        #[ts(type = "Record<string, unknown>")]
        data: Map<String, Value>,
    },
    /// Delete a record. Deleting a record that no longer exists does nothing.
    Delete { model: ModelKind, id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TransactionResult {
    /// The sync id of the transaction's last change, or null if it changed
    /// nothing. Once the client has seen this sync id, the change is confirmed.
    #[ts(type = "number | null")]
    pub sync_id: Option<i64>,
}

/// A snapshot of every synced record in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Bootstrap {
    pub epoch: Uuid,
    /// The snapshot includes every change up to and including this sync id.
    #[ts(type = "number")]
    pub sync_id: i64,
    /// Records by model.
    pub records: BTreeMap<ModelKind, Vec<Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SyncAction {
    Insert,
    Update,
    Delete,
}

/// One change from the log.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SyncEntry {
    #[ts(type = "number")]
    pub sync_id: i64,
    pub transaction_id: Uuid,
    pub model: ModelKind,
    pub model_id: Uuid,
    pub action: SyncAction,
    /// The whole record after the change; null for deletes.
    #[ts(type = "Record<string, unknown> | null")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ServerMessage {
    /// New changes, in order.
    Delta {
        entries: Vec<SyncEntry>,
        #[ts(type = "number")]
        sync_id: i64,
    },
    /// The client's copy can't be caught up (for example, the server's
    /// history started over). Download a new snapshot.
    Resync,
}

/// The body of every error response.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
