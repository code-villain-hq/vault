//! Drift checks: runs of Forge's AI over a service, and the suggestions they
//! produce. Both are written by the server only; clients sync them to show
//! progress and to let people accept or dismiss suggestions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::rules;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AiRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AiRunTrigger {
    Manual,
    Schedule,
    Push,
    /// An issue arrived in triage.
    NewIssue,
}

/// What a run of Forge's AI does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AiRunKind {
    /// Compare a service's code with the tracker.
    Drift,
    /// Suggest the properties of an issue in triage, and spot duplicates.
    Triage,
}

/// One run of Forge's AI: a drift check of a service, or the triage of an issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AiRun {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: AiRunKind,
    pub service_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub trigger: AiRunTrigger,
    pub status: AiRunStatus,
    pub model: String,
    /// Markdown: what the run found.
    pub summary: String,
    pub error: Option<String>,
    #[ts(type = "number")]
    pub input_tokens: i64,
    #[ts(type = "number")]
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub suggestions: i32,
    pub created_by: Uuid,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiRunCreate {
    pub kind: Option<AiRunKind>,
    pub service_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub trigger: AiRunTrigger,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiRunPatch {
    /// Cleared when the service or issue is deleted.
    pub service_id: Option<Option<Uuid>>,
    pub issue_id: Option<Option<Uuid>>,
    pub status: Option<AiRunStatus>,
    pub model: Option<String>,
    pub summary: Option<String>,
    pub error: Option<Option<String>>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub suggestions: Option<i32>,
    pub started_at: Option<Option<DateTime<Utc>>>,
    pub finished_at: Option<Option<DateTime<Utc>>>,
}

impl Validate for AiRunCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

impl Validate for AiRunPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SuggestionStatus {
    Open,
    Accepted,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SuggestionKind {
    /// The tracker and the code disagree.
    Drift,
    /// Properties for an issue in triage.
    Triage,
    /// A new description for an issue, proposed by an agent: agents propose
    /// changes to an issue's scope, people decide.
    Description,
}

/// A change Forge's AI proposes, because the tracker and the code disagree
/// or to triage a new issue, or a new description an agent proposes for an
/// issue. People accept it (its operations are applied as them) or dismiss it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Suggestion {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: SuggestionKind,
    pub title: String,
    /// Markdown: what drifted, and the evidence.
    pub body: String,
    pub status: SuggestionStatus,
    /// The proposed changes, in words.
    pub changes: Vec<String>,
    /// The same changes as sync operations, applied on accept.
    #[ts(type = "Array<unknown>")]
    pub operations: Value,
    pub service_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub decision_id: Option<Uuid>,
    /// The spec and requirement a drift finding is about, if any.
    pub spec_id: Option<Uuid>,
    pub requirement: Option<String>,
    pub run_id: Option<Uuid>,
    pub created_by: Uuid,
    pub resolved_by: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionCreate {
    pub kind: Option<SuggestionKind>,
    pub title: String,
    pub body: String,
    pub changes: Vec<String>,
    pub operations: Value,
    pub service_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub decision_id: Option<Uuid>,
    pub spec_id: Option<Uuid>,
    pub requirement: Option<String>,
    pub run_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionPatch {
    /// Cleared when what they point at is deleted.
    pub service_id: Option<Option<Uuid>>,
    pub issue_id: Option<Option<Uuid>>,
    pub decision_id: Option<Option<Uuid>>,
    pub spec_id: Option<Option<Uuid>>,
    pub run_id: Option<Option<Uuid>>,
    pub status: Option<SuggestionStatus>,
    pub resolved_by: Option<Option<Uuid>>,
    pub resolved_at: Option<Option<DateTime<Utc>>>,
}

impl Validate for SuggestionCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        rules::text(&self.title, "title", rules::TITLE)?;
        rules::long_text(&self.body, "explanation", rules::DESCRIPTION)
    }
}

impl Validate for SuggestionPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}
