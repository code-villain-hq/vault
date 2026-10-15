use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum IssueLinkKind {
    PullRequest,
    Commit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum IssueLinkState {
    Open,
    Merged,
    Closed,
    /// A commit, pushed.
    Pushed,
}

/// What keeps an open pull request from being merged. Until it's gone, the
/// issues it works on aren't ready for review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PullRequestBlocker {
    /// It conflicts with the branch it goes into.
    Conflicts,
    /// One of its checks failed.
    ChecksFailed,
    /// Its checks haven't finished yet.
    ChecksRunning,
}

/// A pull request or commit that mentions an issue (for example `FOR-12` in
/// a branch name, title or commit message). Written by the GitHub
/// connection; read-only for clients.
///
/// It works on the issue when the issue is named in its branch or after a
/// closing word ("Fixes FOR-12"), as on GitHub; otherwise it only mentions it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IssueLink {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub issue_id: Uuid,
    pub kind: IssueLinkKind,
    pub url: String,
    pub title: String,
    /// owner/name
    pub repository: String,
    pub number: Option<i32>,
    pub sha: Option<String>,
    pub state: IssueLinkState,
    /// GitHub login of the author.
    pub author: Option<String>,
    /// Works on the issue rather than only mentioning it: merging the pull
    /// request completes the issue.
    pub works_on: bool,
    /// For an open pull request: what keeps it from being merged, if
    /// anything. None when it's ready, or before Forge has asked GitHub.
    pub blocker: Option<PullRequestBlocker>,
    /// Forge is holding the issue back from review because of the blocker:
    /// it moves the issue to review once the blocker is gone.
    pub held: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueLinkCreate {
    pub issue_id: Uuid,
    pub kind: IssueLinkKind,
    pub url: String,
    pub title: String,
    pub repository: String,
    pub number: Option<i32>,
    pub sha: Option<String>,
    pub state: IssueLinkState,
    pub author: Option<String>,
    pub works_on: bool,
    #[serde(default)]
    pub blocker: Option<PullRequestBlocker>,
    #[serde(default)]
    pub held: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueLinkPatch {
    pub title: Option<String>,
    pub state: Option<IssueLinkState>,
    pub works_on: Option<bool>,
    pub blocker: Option<Option<PullRequestBlocker>>,
    pub held: Option<bool>,
}

impl Validate for IssueLinkCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

impl Validate for IssueLinkPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}
