use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NotificationType {
    /// Someone assigned an issue to you.
    Assigned,
    /// Someone commented on an issue you created, are assigned to, or commented on.
    Comment,
    /// An issue you created was completed.
    Completed,
    /// Forge's AI suggests a change that concerns you.
    Suggestion,
    /// Someone mentioned you in a comment or an issue's description.
    Mention,
    /// Another open issue's spec change wants to change a requirement that
    /// an issue of yours changes first. Its issue is the other one.
    SpecConflict,
}

/// Something that happened that one person should know about. Only that
/// person syncs it. The server creates notifications; their recipient can
/// mark them read or delete them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notification {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Who it's for.
    pub user_id: Uuid,
    #[serde(rename = "type")]
    pub kind: NotificationType,
    pub issue_id: Option<Uuid>,
    pub comment_id: Option<Uuid>,
    pub suggestion_id: Option<Uuid>,
    /// Who caused it.
    pub actor_id: Option<Uuid>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct NotificationPatch {
    pub read_at: Option<Option<DateTime<Utc>>>,
}

impl Validate for NotificationPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}
