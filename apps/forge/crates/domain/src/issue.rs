use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules::{self, optional};

/// 0 = none, 1 = urgent, 2 = high, 3 = medium, 4 = low (the same scale as Linear).
pub type Priority = i16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Issue {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub team_id: Uuid,
    /// Unique within the team; with the team's key it makes the identifier
    /// (`FOR-12`). Assigned by the server.
    pub number: i32,
    pub title: String,
    /// Markdown.
    pub description: String,
    pub state_id: Uuid,
    pub priority: Priority,
    pub assignee_id: Option<Uuid>,
    /// The issue this is a sub-issue of.
    pub parent_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// The service the issue is about.
    pub service_id: Option<Uuid>,
    pub label_ids: Vec<Uuid>,
    pub due_date: Option<NaiveDate>,
    /// Manual order in lists.
    pub sort_order: f64,
    /// When the issue moved into a completed or canceled state. Set by the server.
    pub completed_at: Option<DateTime<Utc>>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What a client sends to create an issue. The id travels with the
/// operation. Without a state, the issue starts in the team's backlog.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct IssueCreate {
    pub team_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub state_id: Option<Uuid>,
    pub priority: Option<Priority>,
    pub assignee_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub label_ids: Option<Vec<Uuid>>,
    pub due_date: Option<NaiveDate>,
    pub sort_order: Option<f64>,
}

/// What a client sends to change an issue. Only the fields present change;
/// null clears a field that can be empty.
#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct IssuePatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub state_id: Option<Uuid>,
    pub priority: Option<Priority>,
    pub assignee_id: Option<Option<Uuid>>,
    pub parent_id: Option<Option<Uuid>>,
    pub project_id: Option<Option<Uuid>>,
    pub service_id: Option<Option<Uuid>>,
    pub label_ids: Option<Vec<Uuid>>,
    pub due_date: Option<Option<NaiveDate>>,
    pub sort_order: Option<f64>,
}

impl Validate for IssueCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.title, "title", rules::TITLE)?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })?;
        optional(&self.priority, |p| rules::priority(*p))
    }
}

impl Validate for IssuePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.title, |t| rules::text(t, "title", rules::TITLE))?;
        optional(&self.description, |d| {
            rules::long_text(d, "description", rules::DESCRIPTION)
        })?;
        optional(&self.priority, |p| rules::priority(*p))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RelationType {
    /// The issue blocks the related issue.
    Blocks,
    Related,
    /// The issue is a duplicate of the related issue.
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IssueRelation {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub issue_id: Uuid,
    pub related_issue_id: Uuid,
    #[serde(rename = "type")]
    pub kind: RelationType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct IssueRelationCreate {
    pub issue_id: Uuid,
    pub related_issue_id: Uuid,
    #[serde(rename = "type")]
    pub kind: RelationType,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct IssueRelationPatch {
    #[serde(rename = "type")]
    pub kind: Option<RelationType>,
}

impl Validate for IssueRelationCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.issue_id == self.related_issue_id {
            return rules::invalid("an issue can't be related to itself");
        }
        Ok(())
    }
}

impl Validate for IssueRelationPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Comment {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub issue_id: Uuid,
    /// Set by the server. Only the author can edit or delete a comment.
    pub author_id: Uuid,
    /// Markdown.
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CommentCreate {
    pub issue_id: Uuid,
    pub body: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct CommentPatch {
    pub body: Option<String>,
}

const COMMENT: usize = 50_000;

impl Validate for CommentCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.body, "comment", COMMENT)
    }
}

impl Validate for CommentPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        optional(&self.body, |b| rules::text(b, "comment", COMMENT))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use crate::ModelKind;

    fn create(value: serde_json::Value) -> Result<(), String> {
        ModelKind::Issue
            .validate_create(value.as_object().unwrap())
            .map_err(|e| e.0)
    }

    fn patch(value: serde_json::Value) -> Result<(), String> {
        ModelKind::Issue
            .validate_patch(value.as_object().unwrap())
            .map_err(|e| e.0)
    }

    #[test]
    fn create_needs_a_team_and_a_title() {
        let team = Uuid::now_v7();
        assert!(create(json!({ "team_id": team, "title": "Fix login" })).is_ok());
        assert!(create(json!({ "team_id": team, "title": "   " })).is_err());
        assert!(create(json!({ "title": "No team" })).is_err());
    }

    #[test]
    fn create_rejects_unknown_and_server_owned_fields() {
        let team = Uuid::now_v7();
        for field in [
            "colour",
            "number",
            "workspace_id",
            "created_by",
            "completed_at",
        ] {
            assert!(
                create(json!({ "team_id": team, "title": "x", field: 1 })).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn patch_checks_only_present_fields_and_allows_clearing() {
        assert!(patch(json!({})).is_ok());
        assert!(
            patch(json!({ "assignee_id": null, "due_date": null, "project_id": null })).is_ok()
        );
        assert!(patch(json!({ "due_date": "2026-10-20" })).is_ok());
        assert!(patch(json!({ "due_date": "next week" })).is_err());
        assert!(patch(json!({ "priority": 9 })).is_err());
        assert!(patch(json!({ "title": "" })).is_err());
        assert!(
            patch(json!({ "team_id": Uuid::now_v7() })).is_err(),
            "moving teams isn't supported"
        );
    }
}
