use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::account::Role;

/// A member of the workspace, as clients sync it (for assignees, leads,
/// authors). Read-only through sync: membership changes go through the
/// account API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct User {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub email: String,
    pub role: Role,
    /// When they joined the workspace.
    pub created_at: DateTime<Utc>,
    pub kind: UserKind,
    /// For an agent: the issue it's working on (FOR-24). It says when it
    /// starts and finishes; moving the issue on ends it too.
    pub working_on_id: Option<Uuid>,
    pub working_since: Option<DateTime<Utc>>,
    /// Its latest sign of life while working. An hour without one means the
    /// run stopped (`WORK_LEASE_MINUTES` in the sync engine).
    pub working_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UserKind {
    Person,
    /// Acts through API tokens (for example a coding agent using the MCP
    /// server), so its changes are attributed to it.
    Agent,
}
