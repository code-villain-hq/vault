use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError};

/// A topic: what it's called and what it sticks to. Its page is its
/// [`Block`]s. Almanac picks the icon, the group and the template its
/// blocks start from when it starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Topic {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    /// A sentence or two on what the topic sticks to.
    pub sticks_to: String,
    /// A Lucide icon name, one of [`ICONS`].
    pub icon: String,
    /// The sidebar group it sits in, such as "Hobby". Empty for none.
    pub group_name: String,
    /// First questions, for the empty page.
    pub starters: Vec<String>,
    pub position: i32,
    /// `topic`, or `almanac` for the workspace's own topic, which knows
    /// the others and starts new ones. Set by the server.
    pub kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TopicCreate {
    pub name: String,
    pub sticks_to: Option<String>,
    pub icon: Option<String>,
    pub group_name: Option<String>,
    pub starters: Option<Vec<String>>,
    /// After the last topic when left out.
    pub position: Option<i32>,
    /// Only the server makes the workspace's own topic.
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TopicPatch {
    pub name: Option<String>,
    pub sticks_to: Option<String>,
    pub icon: Option<String>,
    pub group_name: Option<String>,
    pub starters: Option<Vec<String>>,
    pub position: Option<i32>,
}

/// How long what a topic sticks to may be, in characters: its brief, what
/// it's about and how its talks answer.
pub const STICKS_TO_MAX: usize = 4000;

/// The icons a topic can have: Lucide's names, which the web app draws.
pub const ICONS: &[&str] = &[
    "library",
    "book",
    "book-open",
    "sprout",
    "leaf",
    "flower",
    "trees",
    "cat",
    "dog",
    "paw-print",
    "brush",
    "palette",
    "pen-tool",
    "shower-head",
    "scissors",
    "heart-pulse",
    "pill",
    "dumbbell",
    "utensils",
    "chef-hat",
    "salad",
    "coffee",
    "presentation",
    "briefcase",
    "plane",
    "building",
    "graduation-cap",
    "languages",
    "music",
    "camera",
    "film",
    "gamepad",
    "code",
    "cpu",
    "wrench",
    "hammer",
    "home",
    "car",
    "bike",
    "map",
    "globe",
    "landmark",
    "scale",
    "wallet",
    "piggy-bank",
    "baby",
    "users",
    "calendar",
    "lightbulb",
    "flask-conical",
    "telescope",
    "mountain",
    "tent",
    "fish",
    "bird",
    "sun",
    "moon",
    "star",
    "sparkles",
];

pub const KINDS: &[&str] = &["topic", "almanac"];

/// How a block lays out its points. `today` has the steps of what's due,
/// each under what comes round (its theme, as its reminders are called);
/// `week` the days of the topic's reminders, and no points; `chips` short
/// facts; `shelf` a tile per thing they use (its theme); `cards` a card per
/// theme; `text` a section per theme; `list` one line each; `timeline` what
/// happened when, oldest first, each under the section it belongs to.
pub const BLOCK_KINDS: &[&str] = &[
    "today", "week", "chips", "shelf", "cards", "text", "list", "timeline",
];

/// The blocks a topic starts with, each its kind and title. A collection's
/// cards are named for what they are ("Plants") when Almanac knows.
pub const TEMPLATES: &[(&str, &[(&str, &str)])] = &[
    (
        "routine",
        &[
            ("today", "Today"),
            ("week", ""),
            ("chips", "You"),
            ("shelf", "Your shelf"),
            ("list", "Advice"),
        ],
    ),
    (
        "collection",
        &[("today", "Today"), ("week", ""), ("cards", "Notes")],
    ),
    (
        "study",
        &[
            ("list", "Reading"),
            ("chips", "Terms"),
            ("text", "What it says"),
        ],
    ),
];

/// The names of [`TEMPLATES`].
pub const TEMPLATE_NAMES: &[&str] = &["routine", "collection", "study"];

/// The blocks of a template, by its name.
pub fn template(name: &str) -> Option<&'static [(&'static str, &'static str)]> {
    TEMPLATES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, blocks)| *blocks)
}

/// A part of a topic's page: its kind, one of [`BLOCK_KINDS`], says how it
/// lays out the points kept in it; its title is the topic's own ("You",
/// "Terms"). A topic shows its blocks by position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Block {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub topic_id: Uuid,
    pub kind: String,
    /// Empty for none: the week is headed by its days.
    pub title: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct BlockCreate {
    pub topic_id: Uuid,
    pub kind: String,
    pub title: Option<String>,
    /// After the topic's last block when left out.
    pub position: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct BlockPatch {
    pub kind: Option<String>,
    pub title: Option<String>,
    pub position: Option<i32>,
}

/// A short point the page keeps, in a block, under a theme, with where it
/// comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Point {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub topic_id: Uuid,
    pub block_id: Uuid,
    /// The card, tile, section or routine it's under: a plant, a shampoo.
    /// Empty in chips and lists.
    pub theme: String,
    pub text: String,
    /// When it happened, on a timeline: a year ("1959"), a span
    /// ("1987–1993") or a day ("11. September 2001"). Empty elsewhere.
    pub happened: String,
    /// Where it's from, in words: "Engels, Zur Wohnungsfrage, 1872".
    pub source: Option<String>,
    pub source_url: Option<String>,
    /// The talk it was kept from.
    pub talk_id: Option<Uuid>,
    /// New points carry the unread dot until they're seen.
    pub seen: bool,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct PointCreate {
    pub topic_id: Uuid,
    pub block_id: Uuid,
    pub theme: Option<String>,
    pub text: String,
    pub happened: Option<String>,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub talk_id: Option<Uuid>,
    pub seen: Option<bool>,
    pub position: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct PointPatch {
    pub block_id: Option<Uuid>,
    pub theme: Option<String>,
    pub text: Option<String>,
    pub happened: Option<String>,
    pub source: Option<String>,
    pub source_url: Option<String>,
    /// The talk it now comes from, when a talk changed it.
    pub talk_id: Option<Uuid>,
    pub seen: Option<bool>,
    pub position: Option<i32>,
}

/// A date in a topic: watering, a wash day, a reading circle. Those that
/// notify reach the Mac when they're due.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Reminder {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub topic_id: Uuid,
    /// The card it belongs to, when it's about one: "Monstera".
    pub theme: Option<String>,
    pub title: String,
    /// When it's next due.
    pub due_on: NaiveDate,
    /// The time of day; none for the whole day.
    pub at: Option<NaiveTime>,
    /// Comes back this many days after it's done; none for once.
    pub every_days: Option<i32>,
    pub notify: bool,
    /// `offered`, `on`, `done` or `declined`.
    pub status: String,
    /// The answer that offered it.
    pub message_id: Option<Uuid>,
    /// The reminders it takes the place of, once it's on.
    pub replaces: Vec<Uuid>,
    /// The day it was last done for: the day it was due. Calendar keeps it
    /// there, ticked, while it's done or comes due after it.
    pub last_done_on: Option<NaiveDate>,
    /// The due date the Mac was last told about. Set by the server.
    pub notified_for: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ReminderCreate {
    pub topic_id: Uuid,
    pub theme: Option<String>,
    pub title: String,
    pub due_on: NaiveDate,
    pub at: Option<NaiveTime>,
    pub every_days: Option<i32>,
    pub notify: Option<bool>,
    pub status: Option<String>,
    pub message_id: Option<Uuid>,
    pub replaces: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ReminderPatch {
    pub theme: Option<String>,
    pub title: Option<String>,
    pub due_on: Option<NaiveDate>,
    pub at: Option<NaiveTime>,
    pub every_days: Option<i32>,
    pub notify: Option<bool>,
    pub status: Option<String>,
    pub last_done_on: Option<NaiveDate>,
    /// Only the server sets it.
    pub notified_for: Option<NaiveDate>,
}

pub const REMINDER_STATUSES: &[&str] = &["offered", "on", "done", "declined"];

/// Something Almanac wasn't sure about. It waits where it's about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Question {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub topic_id: Uuid,
    pub theme: Option<String>,
    pub text: String,
    /// The point it would add, when it's about keeping one.
    pub point: Option<String>,
    pub source: Option<String>,
    pub talk_id: Option<Uuid>,
    /// `open`, `added`, `left_out` or `answered`.
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct QuestionCreate {
    pub topic_id: Uuid,
    pub theme: Option<String>,
    pub text: String,
    pub point: Option<String>,
    pub source: Option<String>,
    pub talk_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct QuestionPatch {
    pub status: Option<String>,
}

pub const QUESTION_STATUSES: &[&str] = &["open", "added", "left_out", "answered"];

/// Starts a topic (`POST /api/w/{id}/topics`): a name, and what it sticks
/// to if you know. Almanac writes the rest. From a topic Almanac proposed
/// (`from_message`), it starts with what the proposal says, and any of the
/// fields here over it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct StartTopicRequest {
    pub name: String,
    pub sticks_to: Option<String>,
    pub icon: Option<String>,
    pub group_name: Option<String>,
    /// One of [`TEMPLATE_NAMES`]; Almanac picks one when it's left out.
    pub template: Option<String>,
    /// The answer whose proposal this starts.
    pub from_message: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StartedTopic {
    pub id: Uuid,
}

impl Validate for TopicCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        name(&self.name)?;
        one_of(self.kind.as_deref(), KINDS, "kind")?;
        topic_fields(
            self.sticks_to.as_deref(),
            self.icon.as_deref(),
            self.group_name.as_deref(),
            self.starters.as_deref(),
        )
    }
}

impl Validate for TopicPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.name.as_deref().map(name).transpose()?;
        topic_fields(
            self.sticks_to.as_deref(),
            self.icon.as_deref(),
            self.group_name.as_deref(),
            self.starters.as_deref(),
        )
    }
}

impl Validate for BlockCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        one_of(Some(&self.kind), BLOCK_KINDS, "kind")?;
        self.title.as_deref().map(block_title).transpose()?;
        Ok(())
    }
}

impl Validate for BlockPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        one_of(self.kind.as_deref(), BLOCK_KINDS, "kind")?;
        self.title.as_deref().map(block_title).transpose()?;
        Ok(())
    }
}

impl Validate for PointCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        self.theme.as_deref().map(theme).transpose()?;
        self.happened.as_deref().map(happened).transpose()?;
        words(&self.text, "A point needs some words.", 2000)
    }
}

impl Validate for PointPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.theme.as_deref().map(theme).transpose()?;
        self.happened.as_deref().map(happened).transpose()?;
        if let Some(text) = &self.text {
            words(text, "A point needs some words.", 2000)?;
        }
        Ok(())
    }
}

impl Validate for ReminderCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        words(&self.title, "A reminder needs a title.", 200)?;
        every(self.every_days)?;
        one_of(self.status.as_deref(), REMINDER_STATUSES, "status")
    }
}

impl Validate for ReminderPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        if let Some(title) = &self.title {
            words(title, "A reminder needs a title.", 200)?;
        }
        every(self.every_days)?;
        one_of(self.status.as_deref(), REMINDER_STATUSES, "status")
    }
}

impl Validate for QuestionCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        words(&self.text, "A question needs some words.", 2000)
    }
}

impl Validate for QuestionPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        one_of(self.status.as_deref(), QUESTION_STATUSES, "status")
    }
}

fn invalid(message: impl Into<String>) -> Result<(), ValidationError> {
    Err(ValidationError(message.into()))
}

fn name(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return invalid("A topic needs a name.");
    }
    if value.chars().count() > 80 {
        return invalid("That name is longer than 80 characters.");
    }
    Ok(())
}

fn words(value: &str, empty: &str, max: usize) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return invalid(empty);
    }
    if value.chars().count() > max {
        return invalid(format!("That's longer than {max} characters."));
    }
    Ok(())
}

fn block_title(value: &str) -> Result<(), ValidationError> {
    if value.chars().count() > 60 {
        return invalid("A block's title is at most 60 characters.");
    }
    Ok(())
}

fn theme(value: &str) -> Result<(), ValidationError> {
    if value.chars().count() > 120 {
        return invalid("A theme is at most 120 characters.");
    }
    Ok(())
}

fn happened(value: &str) -> Result<(), ValidationError> {
    if value.chars().count() > 60 {
        return invalid("When it happened is at most 60 characters.");
    }
    Ok(())
}

fn topic_fields(
    sticks_to: Option<&str>,
    icon: Option<&str>,
    group_name: Option<&str>,
    starters: Option<&[String]>,
) -> Result<(), ValidationError> {
    if sticks_to.is_some_and(|s| s.chars().count() > STICKS_TO_MAX) {
        return invalid("What it sticks to is longer than 4000 characters.");
    }
    if icon.is_some_and(|i| !ICONS.contains(&i)) {
        return invalid("Almanac doesn't know that icon.");
    }
    if group_name.is_some_and(|g| g.chars().count() > 40) {
        return invalid("A group's name is at most 40 characters.");
    }
    if starters.is_some_and(|s| s.len() > 6 || s.iter().any(|q| q.chars().count() > 200)) {
        return invalid("A topic has at most six first questions, each short.");
    }
    Ok(())
}

fn every(days: Option<i32>) -> Result<(), ValidationError> {
    if days.is_some_and(|d| !(1..=366).contains(&d)) {
        return invalid("A reminder comes back after 1 to 366 days.");
    }
    Ok(())
}

fn one_of(value: Option<&str>, allowed: &[&str], what: &str) -> Result<(), ValidationError> {
    match value {
        Some(v) if !allowed.contains(&v) => invalid(format!("Unknown {what}: {v}.")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_topic_needs_a_name_and_a_known_icon() {
        let topic = |name: &str, icon: Option<&str>| TopicCreate {
            name: name.into(),
            sticks_to: None,
            icon: icon.map(Into::into),
            group_name: None,
            starters: None,
            position: None,
            kind: None,
        };
        assert!(topic("Pflanzen", Some("sprout")).validate().is_ok());
        assert!(topic(" ", None).validate().is_err());
        assert!(topic("Pflanzen", Some("unicorn")).validate().is_err());
    }

    #[test]
    fn every_template_has_known_blocks() {
        for name in TEMPLATE_NAMES {
            let blocks = template(name).unwrap();
            assert!(blocks.iter().all(|(kind, _)| BLOCK_KINDS.contains(kind)));
        }
        assert_eq!(TEMPLATES.len(), TEMPLATE_NAMES.len());
    }

    #[test]
    fn reminders_come_back_within_a_year() {
        assert!(every(Some(7)).is_ok());
        assert!(every(None).is_ok());
        assert!(every(Some(0)).is_err());
        assert!(every(Some(400)).is_err());
    }
}
