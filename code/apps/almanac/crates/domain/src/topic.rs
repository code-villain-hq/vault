use std::collections::BTreeMap;

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
    /// The topic it sits under, one level deep: its page shows this one as
    /// a tile, and this one's talks keep to its brief too. A parent that's
    /// gone counts as none.
    pub parent_id: Option<Uuid>,
    /// The photo its page opens with and its tile shows, by its id.
    pub cover: Option<Uuid>,
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
    pub parent_id: Option<Uuid>,
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
    /// Null takes it out from under its parent.
    #[ts(optional = nullable)]
    pub parent_id: Option<Uuid>,
    /// Null takes the cover off.
    #[ts(optional = nullable)]
    pub cover: Option<Uuid>,
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
    "shirt",
    "footprints",
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

/// The one set of blocks every page is made of. A `heading` is its title; a
/// `paragraph` has a point a paragraph; a `callout` a point set apart, with
/// its look; a `collection` rows with its [`Field`]s, a point a row: its
/// text the row's name, its cells the rest, and a page of its own. The app
/// picks a collection's picture from its fields: a list is rows with only
/// a name, a checklist rows with a check. Beside them a topic has `you`,
/// what it knows about them, which its About shows and its page doesn't.
pub const SET_KINDS: &[&str] = &["heading", "paragraph", "callout", "collection"];

/// The kinds the set had before lists, checklists and toggles became
/// collections (`proposals/facts-as-data.md`, C).
pub const LIST_KINDS: &[&str] = &["bullets", "checklist", "toggle"];

/// How wide a block is, of the page's twelve columns: a quarter, a third,
/// a half, two thirds, three quarters or the whole. Empty is its kind's own.
pub const BLOCK_WIDTHS: &[&str] = &["", "1/4", "1/3", "1/2", "2/3", "3/4", "whole"];

/// How a person asked a collection to be drawn, with View as: a table,
/// cards, a list, a checklist, a timeline, a line, a board, a week, bars,
/// figures or steps. Empty is the
/// app's own pick from its fields. Only a person sets it; a talk's layout
/// never does.
pub const BLOCK_VIEWS: &[&str] = &[
    "",
    "table",
    "cards",
    "list",
    "checklist",
    "timeline",
    "line",
    "board",
    "week",
    "bars",
    "figures",
    "steps",
];

/// What a collection's field holds: a few words, a number (with its unit
/// and a goal when there is one), a day or a span of days or years, one of
/// a few words the collection uses (a choice), where a row stands (a
/// status), yes or no (a check), a photo from a talk (its id) or a web page.
pub const FIELD_KINDS: &[&str] = &[
    "text", "number", "day", "choice", "status", "check", "photo", "link",
];

/// How many fields a collection has, besides each row's name.
pub const MOST_FIELDS: usize = 8;

/// A field every row of a collection has, such as Gewicht or Status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Field {
    pub name: String,
    /// One of [`FIELD_KINDS`].
    pub kind: String,
    /// A number's unit: "kg", "€". Empty for none.
    pub unit: String,
    /// The mark a number aims at.
    pub goal: Option<f64>,
    /// A choice's or a status's words, in order: a board's lanes, the last
    /// one done. Empty for those its rows use.
    pub options: Vec<String>,
}

/// What a row has in a field: a number, yes or no, or words (a day as
/// `YYYY-MM-DD` or a span, a photo's id, a link).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum Cell {
    Check(bool),
    Number(f64),
    Text(String),
}

/// How a point asks for a look: `warn` for a problem or a gap, `info` for
/// something ready (a cutting to pot); empty when it needs none.
pub const LOOKS: &[&str] = &["", "warn", "info"];

/// The icons a facet can have: Lucide's names, which the web app draws.
pub const FACET_ICONS: &[&str] = &[
    "sun",
    "droplet",
    "map-pin",
    "scissors",
    "thermometer",
    "sprout",
    "leaf",
    "flower",
    "bug",
    "wind",
    "utensils",
    "heart-pulse",
    "pill",
    "paw-print",
    "moon",
    "home",
    "users",
    "calendar",
    "clock",
    "ruler",
    "scale",
    "tag",
    "star",
    "sparkles",
    "shield",
    "info",
    "book",
    "briefcase",
    "wallet",
    "map",
    "car",
    "dumbbell",
];

/// How many facets a block of cards names: a card's face shows a line each.
pub const MOST_FACETS: usize = 6;

/// A fact every card in a block has, such as Licht or Wasser: a line on
/// each card's face with its icon, one of [`FACET_ICONS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Facet {
    pub name: String,
    pub icon: String,
}

/// A block a topic starts with: its kind (of [`SET_KINDS`]), its title,
/// and for a checklist, a collection, the name of its check field.
pub type TemplateBlock = (&'static str, &'static str, &'static str);

/// The blocks a topic starts with. The first collection with no check is
/// the one its rows go in, named for what they are ("Plants") when Almanac
/// knows. Each ends with You, for what it learns about them, which only
/// About shows.
pub const TEMPLATES: &[(&str, &[TemplateBlock])] = &[
    (
        "routine",
        &[
            ("collection", "Routine", "Done"),
            ("collection", "Your shelf", ""),
            ("paragraph", "Advice", ""),
            ("you", "You", ""),
        ],
    ),
    (
        "collection",
        &[
            ("collection", "To do", "Done"),
            ("collection", "Notes", ""),
            ("you", "You", ""),
        ],
    ),
    (
        "study",
        &[
            ("collection", "Reading", "Done"),
            ("collection", "Terms", ""),
            ("paragraph", "What it says", ""),
            ("you", "You", ""),
        ],
    ),
];

/// The names of [`TEMPLATES`].
pub const TEMPLATE_NAMES: &[&str] = &["routine", "collection", "study"];

/// The blocks of a template, by its name.
pub fn template(name: &str) -> Option<&'static [TemplateBlock]> {
    TEMPLATES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, blocks)| *blocks)
}

/// A part of a topic's page: its kind, one of [`SET_KINDS`] or `you`, says
/// how it lays out the points kept in it; its title is the topic's own
/// ("You", "Terms"). A topic shows its blocks by position, and a row's page
/// the blocks that name it (`row_id`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Block {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub topic_id: Uuid,
    pub kind: String,
    /// Empty for none.
    pub title: String,
    /// For cards from before the set, which the server reads once to move
    /// them into a collection's fields.
    pub facets: Vec<Facet>,
    /// For a collection: the fields every row has, besides its name.
    pub fields: Vec<Field>,
    /// One of [`BLOCK_WIDTHS`].
    pub width: String,
    /// One of [`BLOCK_VIEWS`].
    pub view: String,
    /// The collection row whose own page it sits on; none, the topic's.
    pub row_id: Option<Uuid>,
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
    pub facets: Option<Vec<Facet>>,
    pub fields: Option<Vec<Field>>,
    pub width: Option<String>,
    pub view: Option<String>,
    pub row_id: Option<Uuid>,
    /// After the topic's last block when left out.
    pub position: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct BlockPatch {
    pub kind: Option<String>,
    pub title: Option<String>,
    pub facets: Option<Vec<Facet>>,
    pub fields: Option<Vec<Field>>,
    pub width: Option<String>,
    pub view: Option<String>,
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
    /// Empty in chips, lists and you.
    pub theme: String,
    pub text: String,
    /// When it happened, on a timeline: a year ("1959"), a span
    /// ("1987–1993") or a day ("11. September 2001"). Empty elsewhere.
    pub happened: String,
    /// On a card: the facet it's about, one of its block's ("Wasser").
    pub facet: String,
    /// Whether it needs a look, one of [`LOOKS`]: a card's problem, a gap.
    pub look: String,
    /// In bars: the number its bar shows.
    pub amount: Option<f64>,
    /// A few words beside it: by a bar ("getötet"), or who asked a
    /// question, on its first point.
    pub note: String,
    /// Ticked: a list's item done, a question practised.
    pub done: bool,
    /// In a collection: what the row has in each of its block's fields, by
    /// the field's name. Its text is its name.
    pub cells: BTreeMap<String, Cell>,
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
    pub facet: Option<String>,
    pub look: Option<String>,
    pub amount: Option<f64>,
    pub note: Option<String>,
    pub done: Option<bool>,
    pub cells: Option<BTreeMap<String, Cell>>,
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
    pub facet: Option<String>,
    pub look: Option<String>,
    /// Null takes the number away.
    pub amount: Option<f64>,
    pub note: Option<String>,
    pub done: Option<bool>,
    pub cells: Option<BTreeMap<String, Cell>>,
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
    /// The checklist item it rings for, which ticks it.
    pub point_id: Option<Uuid>,
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
    pub point_id: Option<Uuid>,
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
    /// The topic it sits under.
    pub parent_id: Option<Uuid>,
    /// One of [`TEMPLATE_NAMES`]; Almanac picks one when it's left out.
    pub template: Option<String>,
    /// The answer whose proposal this starts.
    pub from_message: Option<Uuid>,
    /// Which of its sub-topics, counting from 0; its proposal without.
    pub sub_topic: Option<u32>,
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
        block_kind(Some(&self.kind))?;
        one_of(self.width.as_deref(), BLOCK_WIDTHS, "width")?;
        one_of(self.view.as_deref(), BLOCK_VIEWS, "view")?;
        self.title.as_deref().map(block_title).transpose()?;
        self.facets.as_deref().map(facets).transpose()?;
        self.fields.as_deref().map(fields).transpose()?;
        Ok(())
    }
}

impl Validate for BlockPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        block_kind(self.kind.as_deref())?;
        one_of(self.width.as_deref(), BLOCK_WIDTHS, "width")?;
        one_of(self.view.as_deref(), BLOCK_VIEWS, "view")?;
        self.title.as_deref().map(block_title).transpose()?;
        self.facets.as_deref().map(facets).transpose()?;
        self.fields.as_deref().map(fields).transpose()?;
        Ok(())
    }
}

impl Validate for PointCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        self.theme.as_deref().map(theme).transpose()?;
        self.happened.as_deref().map(happened).transpose()?;
        shape(
            self.facet.as_deref(),
            self.look.as_deref(),
            self.amount,
            self.note.as_deref(),
        )?;
        self.cells.as_ref().map(cells).transpose()?;
        words(&self.text, "A point needs some words.", 2000)
    }
}

impl Validate for PointPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        self.theme.as_deref().map(theme).transpose()?;
        self.happened.as_deref().map(happened).transpose()?;
        shape(
            self.facet.as_deref(),
            self.look.as_deref(),
            self.amount,
            self.note.as_deref(),
        )?;
        self.cells.as_ref().map(cells).transpose()?;
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

fn facets(facets: &[Facet]) -> Result<(), ValidationError> {
    if facets.len() > MOST_FACETS {
        return invalid(format!("A block names at most {MOST_FACETS} facets."));
    }
    for facet in facets {
        if facet.name.trim().is_empty() || facet.name.chars().count() > 40 {
            return invalid("A facet's name has a word or two, at most 40 characters.");
        }
        if !FACET_ICONS.contains(&facet.icon.as_str()) {
            return invalid("Almanac doesn't know that facet's icon.");
        }
    }
    Ok(())
}

fn block_kind(kind: Option<&str>) -> Result<(), ValidationError> {
    match kind {
        Some(k) if k != "you" && !SET_KINDS.contains(&k) => invalid(format!("Unknown kind: {k}.")),
        _ => Ok(()),
    }
}

fn fields(fields: &[Field]) -> Result<(), ValidationError> {
    if fields.len() > MOST_FIELDS {
        return invalid(format!(
            "A collection has at most {MOST_FIELDS} fields besides each row's name."
        ));
    }
    for (n, field) in fields.iter().enumerate() {
        if field.name.trim().is_empty() || field.name.chars().count() > 40 {
            return invalid("A field's name has a word or two, at most 40 characters.");
        }
        if fields[..n].iter().any(|f| f.name == field.name) {
            return invalid(format!("A collection has one field named {}.", field.name));
        }
        one_of(Some(&field.kind), FIELD_KINDS, "kind of field")?;
        if field.unit.chars().count() > 12 {
            return invalid("A unit is at most 12 characters.");
        }
        if field.goal.is_some_and(|g| !g.is_finite()) {
            return invalid("A goal is a number.");
        }
        if field.options.len() > 12 || field.options.iter().any(|o| o.chars().count() > 40) {
            return invalid("A field has at most 12 words to choose from, each short.");
        }
    }
    Ok(())
}

fn cells(cells: &BTreeMap<String, Cell>) -> Result<(), ValidationError> {
    if cells.len() > MOST_FIELDS {
        return invalid(format!("A row has at most {MOST_FIELDS} cells."));
    }
    for (name, cell) in cells {
        if name.trim().is_empty() || name.chars().count() > 40 {
            return invalid("A cell is named by its field, at most 40 characters.");
        }
        match cell {
            Cell::Number(n) if !n.is_finite() => return invalid("A number cell is a number."),
            Cell::Text(t) if t.chars().count() > 500 => {
                return invalid("A cell is at most 500 characters.");
            }
            _ => {}
        }
    }
    Ok(())
}

/// What a card's face, a bar or a question shows besides a point's text.
fn shape(
    facet: Option<&str>,
    look: Option<&str>,
    amount: Option<f64>,
    note: Option<&str>,
) -> Result<(), ValidationError> {
    if facet.is_some_and(|f| f.chars().count() > 40) {
        return invalid("A facet is at most 40 characters.");
    }
    one_of(look, LOOKS, "look")?;
    if amount.is_some_and(|a| !a.is_finite()) {
        return invalid("A bar's number is a number.");
    }
    if note.is_some_and(|n| n.chars().count() > 80) {
        return invalid("What's beside a point is at most 80 characters.");
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
            parent_id: None,
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
            assert!(
                blocks
                    .iter()
                    .all(|(kind, _, _)| SET_KINDS.contains(kind) || *kind == "you")
            );
        }
        assert_eq!(TEMPLATES.len(), TEMPLATE_NAMES.len());
    }

    #[test]
    fn a_row_keeps_numbers_checks_and_words_by_field() {
        let row: BTreeMap<String, Cell> =
            serde_json::from_str(r#"{"Gewicht": 3.4, "Gechipt": true, "Tag": "2026-10-30"}"#)
                .unwrap();
        assert_eq!(row["Gewicht"], Cell::Number(3.4));
        assert_eq!(row["Gechipt"], Cell::Check(true));
        assert_eq!(row["Tag"], Cell::Text("2026-10-30".into()));
        assert!(cells(&row).is_ok());
        let field = |name: &str, kind: &str| Field {
            name: name.into(),
            kind: kind.into(),
            unit: String::new(),
            goal: None,
            options: vec![],
        };
        assert!(fields(&[field("Gewicht", "number"), field("Tag", "day")]).is_ok());
        assert!(fields(&[field("Tag", "day"), field("Tag", "day")]).is_err());
        assert!(fields(&[field("Farbe", "colour")]).is_err());
        assert!(block_kind(Some("collection")).is_ok());
        assert!(block_kind(Some("database")).is_err());
    }

    #[test]
    fn reminders_come_back_within_a_year() {
        assert!(every(Some(7)).is_ok());
        assert!(every(None).is_ok());
        assert!(every(Some(0)).is_err());
        assert!(every(Some(400)).is_err());
    }
}
