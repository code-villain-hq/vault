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
    /// An agent ran the check outside Forge and reports it through MCP.
    Agent,
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

/// One part of an agent's check (its orchestrator, or a worker) and what
/// it used, as the agent reported it. `None` is unavailable, never 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UsagePart {
    /// What the part was, e.g. "Orchestrator" or "Worker: Accounts".
    pub name: String,
    #[ts(type = "number | null")]
    pub calls: Option<i64>,
    /// Input tokens read without the cache.
    #[ts(type = "number | null")]
    pub input: Option<i64>,
    #[ts(type = "number | null")]
    pub cache_writes: Option<i64>,
    #[ts(type = "number | null")]
    pub cache_reads: Option<i64>,
    #[ts(type = "number | null")]
    pub output: Option<i64>,
    /// The output is an estimate, not a measurement.
    #[serde(default)]
    pub output_estimated: bool,
}

/// An agent's check's verdicts so far, by kind, and the planned requirements
/// still without one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VerdictTally {
    pub met: i32,
    pub not_met: i32,
    pub unclear: i32,
    pub skipped: i32,
    /// Covered by open work.
    pub covered: i32,
    /// Planned, not judged yet.
    pub unjudged: i32,
}

/// One run of Forge's AI: a drift check of a service, or the triage of an
/// issue. Or a drift check an agent ran outside Forge and reported (trigger
/// `agent`), which can cover several services and a range of commits.
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
    /// The services an agent's check covers.
    pub service_ids: Vec<Uuid>,
    pub commit_from: Option<String>,
    pub commit_to: Option<String>,
    /// The checker's name and version.
    pub checker: Option<String>,
    pub full_audit: Option<bool>,
    /// The agent's own key for its check.
    pub run_key: Option<String>,
    /// When the agent last started or reported into its check.
    pub last_report_at: Option<DateTime<Utc>>,
    pub tally: Option<VerdictTally>,
    /// What an agent's check used, per part; `None` when it didn't say.
    pub usage: Option<Vec<UsagePart>>,
    pub created_by: Uuid,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What an agent's check found about one requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CheckVerdict {
    Met,
    NotMet,
    Unclear,
    /// Not judged on purpose, e.g. outside the commits checked.
    Skipped,
    /// Already covered by open work: an issue says so.
    Covered,
}

/// One requirement in an agent's check: planned, or judged. Kept per check,
/// with the spec's title and version and the requirement's text as reported.
/// Not synced; a check's page asks for them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CheckResult {
    pub spec_id: Option<Uuid>,
    pub spec_title: String,
    pub spec_updated_at: DateTime<Utc>,
    pub requirement: String,
    pub requirement_text: String,
    /// `None` while it's planned and not judged yet.
    pub verdict: Option<CheckVerdict>,
    /// Markdown: the evidence, or why.
    pub why: String,
    /// The open issue that covers it.
    pub issue_id: Option<Uuid>,
    pub reported_at: DateTime<Utc>,
}

/// What a drift check's page shows beyond the run itself: the requirements
/// it judged (or planned), and the issues Forge saw its agent file and
/// comment on while it ran, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DriftCheckDetails {
    pub results: Vec<CheckResult>,
    pub filed: Vec<Uuid>,
    pub commented: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiRunCreate {
    pub kind: Option<AiRunKind>,
    pub service_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub trigger: AiRunTrigger,
    pub model: Option<String>,
    pub status: Option<AiRunStatus>,
    pub service_ids: Option<Vec<Uuid>>,
    pub commit_from: Option<String>,
    pub commit_to: Option<String>,
    pub checker: Option<String>,
    pub full_audit: Option<bool>,
    pub run_key: Option<String>,
    pub last_report_at: Option<DateTime<Utc>>,
    pub tally: Option<VerdictTally>,
    pub started_at: Option<DateTime<Utc>>,
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
    pub service_ids: Option<Vec<Uuid>>,
    pub last_report_at: Option<Option<DateTime<Utc>>>,
    pub tally: Option<Option<VerdictTally>>,
    pub usage: Option<Option<Vec<UsagePart>>>,
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
