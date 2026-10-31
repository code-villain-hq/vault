use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};
use crate::topic::{Cell, Facet, Field, TopicPatch};

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
    /// The photos a question came with, in order. Their bytes don't sync:
    /// `GET /api/w/{id}/photos/{photo}` has each one.
    pub photos: Vec<Photo>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A topic Almanac would start: what it's called, what it sticks to, the
/// template its blocks start from, and what it already heard from you,
/// which the topic keeps once it's started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Proposal {
    pub name: String,
    pub sticks_to: String,
    pub icon: String,
    pub group_name: String,
    /// One of [`crate::TEMPLATE_NAMES`].
    pub template: String,
    /// For a collection, what its cards are: "Plants".
    #[serde(default)]
    #[ts(optional)]
    pub cards: Option<String>,
    /// For a collection, the fields its rows have.
    #[serde(default)]
    pub fields: Vec<Field>,
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
    /// The template's block it goes in, by title ("You"); the first that
    /// keeps points when left out.
    #[serde(default)]
    #[ts(optional)]
    pub block: Option<String>,
    #[serde(default)]
    pub theme: String,
    /// In a collection, the row's name.
    pub text: String,
    /// In a collection, the row's cells, by field.
    #[serde(default)]
    pub cells: BTreeMap<String, Cell>,
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

/// A photo a question came with, by its id, and its size in pixels, so a
/// talk can make room for it before it arrives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Photo {
    pub id: Uuid,
    pub width: i32,
    pub height: i32,
}

/// A photo sent with a question: its type (`image/jpeg`, `image/png`,
/// `image/webp` or `image/gif`), its bytes in base64, and its size.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PhotoUpload {
    pub media_type: String,
    pub data: String,
    pub width: i32,
    pub height: i32,
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
    /// The page laid out anew, when they asked for that.
    #[serde(default)]
    pub layout: Option<PageLayout>,
    /// Set once Undo put the fields back.
    pub undone: bool,
}

/// A topic's page an answer laid out anew: what moved, block by block, and
/// the blocks and points as they were, for Undo to put back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PageLayout {
    /// A line per block it changed, in the page's new order, then the
    /// blocks it removed.
    pub moves: Vec<LayoutMove>,
    /// The blocks it changed or removed, as they were.
    pub blocks: Vec<BlockWas>,
    /// The points it moved or reworded, as they were.
    pub points: Vec<PointWas>,
    /// The blocks and points it started, which Undo takes off again.
    pub made_blocks: Vec<Uuid>,
    pub made_points: Vec<Uuid>,
    /// The collections whose rows or fields it changed, in the page's
    /// order, which the answer shows as their pictures.
    #[serde(default)]
    pub collections: Vec<Uuid>,
}

/// One block in "What moved": "Wie sie ist · new cards", then what it
/// holds now, "Verhalten 10, Streicheln 6".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LayoutMove {
    pub block: String,
    pub change: String,
    pub holds: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BlockWas {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub facets: Vec<Facet>,
    #[serde(default)]
    pub width: String,
    /// The view a person picked, which undo brings back with the block.
    #[serde(default)]
    pub view: String,
    /// A collection's fields.
    #[serde(default)]
    pub fields: Vec<Field>,
    pub position: i32,
    /// Whether the layout removed it.
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PointWas {
    pub id: Uuid,
    pub block_id: Uuid,
    pub theme: String,
    pub text: String,
    pub happened: String,
    #[serde(default)]
    pub facet: String,
    #[serde(default)]
    pub look: String,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub note: String,
    /// A row's cells.
    #[serde(default)]
    pub cells: BTreeMap<String, Cell>,
    #[serde(default)]
    pub done: bool,
    pub position: i32,
    /// Whether the answer removed it, which undo makes again with its
    /// source.
    #[serde(default)]
    pub removed: bool,
    #[serde(default)]
    #[ts(optional)]
    pub source: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub source_url: Option<String>,
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
    pub photos: Option<Vec<Photo>>,
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
    /// Photos to look at with the question: of a shampoo, a plant, a page.
    pub photos: Option<Vec<PhotoUpload>>,
    /// The section of the page open in front of them, by its theme
    /// ("Intifada"): the answer is about it, and keeps what it finds there.
    pub about: Option<String>,
    /// The collection's row whose page is open in front of them: the
    /// answer is about it, and keeps what it finds in its cells and under
    /// its name.
    pub row: Option<Uuid>,
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
