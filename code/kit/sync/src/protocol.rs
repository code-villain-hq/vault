//! What clients and the server send each other, for any app's models `M`.
//! `client/src/protocol.ts` is the same for TypeScript.
//!
//! Writes: the client POSTs a [`Transaction`] and gets a [`TransactionResult`].
//! Reads: the client downloads a [`Bootstrap`] snapshot once, then follows the
//! change log over a WebSocket, which carries [`ServerMessage`]s.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

/// A group of changes that succeed or fail together. The client chooses the
/// id; sending the same transaction twice applies it once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction<M> {
    pub id: Uuid,
    pub operations: Vec<Operation<M>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Operation<M> {
    /// Create a record. The client chooses the id (a UUIDv7), so it can
    /// create records while offline.
    Create {
        model: M,
        id: Uuid,
        data: Map<String, Value>,
    },
    /// Change some fields of a record. Fields not in `data` are untouched.
    Update {
        model: M,
        id: Uuid,
        data: Map<String, Value>,
    },
    /// Delete a record. Deleting a record that no longer exists does nothing.
    Delete { model: M, id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    /// The sync id of the transaction's last change, or null if it changed
    /// nothing. Once the client has seen this sync id, the change is confirmed.
    pub sync_id: Option<i64>,
}

/// A snapshot of every synced record in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "M: Deserialize<'de> + Ord"))]
pub struct Bootstrap<M> {
    pub epoch: Uuid,
    /// The snapshot includes every change up to and including this sync id.
    pub sync_id: i64,
    /// Records by model.
    pub records: BTreeMap<M, Vec<Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    Insert,
    Update,
    Delete,
}

/// One change from the log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEntry<M> {
    pub sync_id: i64,
    pub transaction_id: Uuid,
    pub model: M,
    pub model_id: Uuid,
    pub action: SyncAction,
    /// The whole record after the change; null for deletes.
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage<M> {
    /// New changes, in order.
    Delta {
        entries: Vec<SyncEntry<M>>,
        sync_id: i64,
    },
    /// The client's copy can't be caught up (for example, the server's
    /// history started over). Download a new snapshot.
    Resync,
}

/// The body of every error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
