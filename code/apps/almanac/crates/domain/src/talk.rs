use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};

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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct MessagePatch {
    pub kept: Option<String>,
    pub elsewhere: Option<String>,
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
