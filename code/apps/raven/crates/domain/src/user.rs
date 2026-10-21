use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::account::Role;

/// A member of the workspace, as clients sync it. Read-only through sync:
/// membership changes go through the account API.
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
}
