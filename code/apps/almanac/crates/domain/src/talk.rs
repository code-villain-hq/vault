use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::topic::TopicPatch;

/// A talk with a topic, in the panel beside its page. The server starts
/// it with the first question; you can delete it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Talk {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub topic_id: Uuid,
    /// The first question, short.
    pub title: String,
    pub last_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TalkCreate {
    pub topic_id: Uuid,
    pub title: String,
    pub last_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TalkPatch {
    pub title: Option<String>,
    pub last_at: Option<DateTime<Utc>>,
}

/// Where an answer's points come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, optional_fields)]
pub struct Source {
    pub title: String,
    pub url: Option<String>,
}

/// What was said in a talk: what you asked (`you`) and Almanac's answers
/// (`almanac`). Only the server writes them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Message {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub talk_id: Uuid,
    pub topic_id: Uuid,
    pub role: String,
    pub text: String,
    pub sources: Vec<Source>,
    /// What the answer kept, in words: "Kept on the page under Mieten."
    pub kept: Option<String>,
    /// Why an answer stopped, when it didn't finish.
    pub problem: Option<String>,
    /// The topic an off-topic question fits better, by name.
    pub elsewhere: Option<String>,
    /// A topic the workspace's own topic proposes, to start with a click.
    pub proposal: Option<Proposal>,
    /// What the answer changed in topics you named, for Undo.
    pub changes: Vec<TopicChange>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A topic Almanac would start: what it's called, what it sticks to, how
/// its page looks, and what it already heard from you, which the topic
/// keeps once it's started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Proposal {
    pub name: String,
    pub sticks_to: String,
    pub icon: String,
    pub group_name: String,
    /// `cards` or `document`.
    pub shape: String,
    /// For a document, its title.
    pub title: Option<String>,
    pub starters: Vec<String>,
    /// What you told Almanac, kept as the topic's first points.
    pub points: Vec<ProposedPoint>,
    /// Reminders it offers once the topic is started.
    pub reminders: Vec<ProposedReminder>,
    /// The topic, once it was started.
    pub topic_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProposedPoint {
    pub theme: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProposedReminder {
    pub theme: Option<String>,
    pub title: String,
    pub due_on: NaiveDate,
    pub at: Option<NaiveTime>,
    pub every_days: Option<i32>,
}

/// One topic an answer changed: what it said it changed, and the fields
/// before and after.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TopicChange {
    pub topic_id: Uuid,
    /// The topic's name after the change.
    pub name: String,
    /// What changed, in words: "Moved it to Hobby."
    pub said: String,
    pub before: TopicPatch,
    pub after: TopicPatch,
    /// Set once Undo put the fields back.
    pub undone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct MessageCreate {
    pub talk_id: Uuid,
    pub topic_id: Uuid,
    pub role: String,
    pub text: String,
    pub sources: Option<Vec<Source>>,
    pub kept: Option<String>,
    pub problem: Option<String>,
    pub elsewhere: Option<String>,
    pub proposal: Option<Proposal>,
    pub changes: Option<Vec<TopicChange>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct MessagePatch {
    pub kept: Option<String>,
    pub elsewhere: Option<String>,
    pub proposal: Option<Proposal>,
    pub changes: Option<Vec<TopicChange>>,
}

/// A question in a topic (`POST /api/w/{id}/topics/{topic}/ask`). Without
/// a talk, it starts one. The answer comes back as server-sent events:
/// `talk` (the talk's id), `text` (each piece), then `done` (the answer's
/// id) or `problem` (why there's none).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct AskRequest {
    pub talk_id: Option<Uuid>,
    pub text: String,
}

impl Validate for TalkCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        title(&self.title)
    }
}

impl Validate for TalkPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.title.as_deref().map(title).transpose()?;
        Ok(())
    }
}

impl Validate for MessageCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        if !matches!(self.role.as_str(), "you" | "almanac") {
            return Err(ValidationError("A message is yours or Almanac's.".into()));
        }
        Ok(())
    }
}

impl Validate for MessagePatch {
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

fn title(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.chars().count() > 200 {
        return Err(ValidationError(
            "A talk's title is 1 to 200 characters.".into(),
        ));
    }
    Ok(())
}
