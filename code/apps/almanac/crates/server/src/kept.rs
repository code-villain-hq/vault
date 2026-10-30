//! What one answer keeps: points in the page's blocks, questions to check,
//! reminders it offers, the topic an off-topic question fits, the talk's
//! title, how the topic answers from now on (its brief), and, in the
//! workspace's own topic, a topic it proposes and the topics it changed. A point or reminder may take the place of ones the
//! page has, by the references the page gave them (`crate::page::Refs`):
//! the page is kept up to date, not added to. Claude Code hands them over with Almanac's tools
//! while it answers (`crate::mcp`); through the Messages API one more
//! request reads them from the exchange (`crate::talks`). Either way they
//! are written with the answer, through the sync engine.

use almanac_domain::{
    BLOCK_KINDS, FACET_ICONS, Facet, LOOKS, MOST_FACETS, ModelKind, Operation, Proposal,
    ProposedReminder, STICKS_TO_MAX, TopicChange, TopicPatch,
};
use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::layout::Laid;
use crate::page::{RefBlock, Refs};
use crate::topics::cut;

#[derive(Debug, Default)]
pub struct Kept {
    /// The page's blocks, points and reminders, for where points go and
    /// what replaces them.
    pub refs: Refs,
    /// Blocks the answer starts, after the page's.
    pub blocks: Vec<RefBlock>,
    pub points: Vec<KeptPoint>,
    pub questions: Vec<KeptQuestion>,
    pub reminders: Vec<KeptReminder>,
    /// Points ticked done, or not, in a list or questions.
    pub ticks: Vec<(Uuid, bool)>,
    /// The topic an off-topic question fits better, by name.
    pub elsewhere: Option<String>,
    pub talk_title: Option<String>,
    pub proposal: Option<Proposal>,
    pub changes: Vec<TopicChange>,
    /// The page laid out anew, when they asked for that (`crate::layout`).
    pub layout: Option<Laid>,
}

#[derive(Debug)]
pub struct KeptPoint {
    pub block_id: Uuid,
    /// Empty in chips and lists.
    pub theme: String,
    /// What the answer says it kept it under: its theme, or its block.
    pub under: String,
    pub text: String,
    /// When it happened, on a timeline; empty elsewhere.
    pub happened: String,
    pub shape: Shape,
    pub source: Option<String>,
    pub url: Option<String>,
    /// The page's points it takes the place of.
    pub replaces: Vec<Uuid>,
}

/// A point as Claude hands it over (`crate::mcp`, `crate::talks`): the
/// `block` it goes in (`b2`, or its title), the `kind` that starts one the
/// page lacks, what it `replaces` (`p3`), and the point itself.
#[derive(Debug, Default, Deserialize)]
pub struct Handed {
    #[serde(default)]
    pub block: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub theme: String,
    pub text: String,
    #[serde(default)]
    pub happened: String,
    #[serde(default)]
    pub facet: String,
    #[serde(default)]
    pub look: String,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub replaces: Vec<String>,
}

/// What a point carries besides its text, as its block's kind shows it:
/// the facet of a card it's about, whether it needs a look, a bar's
/// number and the few words beside a bar or a question.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Shape {
    pub facet: String,
    pub look: String,
    pub amount: Option<f64>,
    pub note: String,
}

impl Shape {
    /// The shape as a block of `kind` with these `facets` takes it, called
    /// `title` in what's wrong. A facet the block doesn't name is wrong when
    /// Claude gave it (`named`), and goes quietly when a point only moved.
    pub fn fit(
        self,
        kind: &str,
        facets: &[Facet],
        title: &str,
        named: bool,
    ) -> Result<Shape, String> {
        let facet = match (kind, self.facet.trim()) {
            (_, "") => String::new(),
            ("cards" | "table", facet) => match facets
                .iter()
                .find(|f| f.name.to_lowercase() == facet.to_lowercase())
            {
                Some(f) => f.name.clone(),
                None if !named => String::new(),
                None if facets.is_empty() => {
                    return Err(format!(
                        "{title} names no facets: leave facet out, or lay out the page to give the block some."
                    ));
                }
                None => {
                    let names: Vec<&str> = facets.iter().map(|f| f.name.as_str()).collect();
                    return Err(format!(
                        "{title} has no facet {facet}: it's one of {}, or none.",
                        names.join(", ")
                    ));
                }
            },
            _ => String::new(),
        };
        let look = match kind {
            "cards" | "table" | "bars" | "questions" => {
                let look = self.look.trim().to_lowercase();
                if !LOOKS.contains(&look.as_str()) {
                    return Err(format!(
                        "A point's look is warn (it needs a look), info (it's ready for something) or none, not {look}."
                    ));
                }
                look
            }
            _ => String::new(),
        };
        let amount = match kind {
            "bars" | "trend" | "parts" => match self.amount {
                Some(amount) if amount.is_finite() => Some(amount),
                _ => {
                    return Err(format!(
                        "{} in {title} needs its amount: {}.",
                        match kind {
                            "bars" => "A bar",
                            "trend" => "A measurement",
                            _ => "A part",
                        },
                        match kind {
                            "parts" => "how many, plain (12)",
                            "trend" => "the number it measured, plain (3.4)",
                            _ => "the number, plain (900000)",
                        }
                    ));
                }
            },
            "table" => self.amount.filter(|a| a.is_finite()),
            _ => None,
        };
        let note = match kind {
            "bars" | "questions" | "trend" | "parts" => cut(&self.note, 80),
            _ => String::new(),
        };
        Ok(Shape {
            facet,
            look,
            amount,
            note,
        })
    }
}

/// When a point in a block of `kind`, called `title` in what's wrong,
/// happened: on a timeline a year, a span or a day; in a trend the day it
/// was measured, or none for the mark it aims at; nothing elsewhere.
pub fn when(kind: &str, happened: &str, title: &str) -> Result<String, String> {
    let happened = cut(happened, 60);
    match kind {
        "timeline" if !happened.chars().any(|c| c.is_ascii_digit()) => Err(format!(
            "A point on {title} says when it happened: a year (1959), a span (1987–1993) or a day (11. September 2001)."
        )),
        "timeline" => Ok(happened),
        "trend" if happened.is_empty() || happened.parse::<NaiveDate>().is_ok() => Ok(happened),
        "trend" => Err(format!(
            "A measurement in {title} says the day it was taken, as 2026-09-24, or none for the mark it aims at."
        )),
        _ => Ok(String::new()),
    }
}

/// The facets a block of cards names, tidied: a name each, an icon Almanac
/// has, none twice.
pub fn facets(given: &[Facet]) -> Result<Vec<Facet>, String> {
    if given.len() > MOST_FACETS {
        return Err(format!(
            "A block names at most {MOST_FACETS} facets: the facts every card has."
        ));
    }
    let mut facets: Vec<Facet> = Vec::new();
    for facet in given {
        let name = cut(&facet.name, 40);
        if name.is_empty() {
            return Err("A facet needs a name: a word or two (Licht, Wasser).".into());
        }
        let icon = facet.icon.trim().to_lowercase();
        if !FACET_ICONS.contains(&icon.as_str()) {
            return Err(format!(
                "There's no icon {icon} for {name}: it's one of {FACET_ICONS:?}."
            ));
        }
        if facets
            .iter()
            .any(|f| f.name.to_lowercase() == name.to_lowercase())
        {
            return Err(format!("{name} is in facets twice."));
        }
        facets.push(Facet { name, icon });
    }
    Ok(facets)
}

#[derive(Debug)]
pub struct KeptReminder {
    pub reminder: ProposedReminder,
    /// The reminders it takes the place of.
    pub replaces: Vec<Uuid>,
}

#[derive(Debug)]
pub struct KeptQuestion {
    pub theme: Option<String>,
    pub text: String,
    pub point: Option<String>,
}

/// Where the answer is, for what it keeps.
pub struct Answering {
    pub topic_id: Uuid,
    pub topic_name: String,
    pub talk_id: Uuid,
    pub answer_id: Uuid,
    /// Only the first answer names the talk.
    pub first: bool,
}

/// How much one answer may keep. A page keeps points; more than this
/// from one exchange is a list copied in. Points that take the place of
/// others count only towards the most it may change at all, which sorting
/// a page needs.
const MOST_POINTS: usize = 8;
const MOST_CHANGED: usize = 40;
const MOST_QUESTIONS: usize = 2;
const MOST_REMINDERS: usize = 3;
const MOST_BLOCKS: usize = 2;
const MOST_TICKS: usize = 40;

impl Kept {
    /// Keeping for a page with these points and reminders.
    pub fn on(refs: Refs) -> Self {
        Kept {
            refs,
            ..Kept::default()
        }
    }

    /// Keeps a point in a `block` of the page (`b2`, or its title), in
    /// place of the points it `replaces` (`p3`). A block the page doesn't
    /// have is started, after the others, when the point gives its `kind`.
    /// On a timeline it says when it `happened`; in cards, bars and
    /// questions it has a [`Shape`]. What's wrong with it comes back in
    /// words, for Claude to fix.
    pub fn keep_point(&mut self, point: Handed) -> Result<(), String> {
        let Handed {
            block,
            kind,
            theme,
            text,
            happened,
            facet,
            look,
            amount,
            note,
            source,
            url,
            replaces,
        } = point;
        if self.layout.is_some() {
            return Err(
                "You laid out the page in this answer: keep new points in the next one, or draw them with lay_out_page's new_points."
                    .into(),
            );
        }
        let text = cut(&text, 2000);
        if text.is_empty() {
            return Err("A point needs a text.".into());
        }
        let new = self.points.iter().filter(|p| p.replaces.is_empty()).count();
        if replaces.is_empty() && new >= MOST_POINTS {
            return Err(format!(
                "This answer already keeps {MOST_POINTS} points: keep only the ones worth knowing later."
            ));
        }
        if self.points.len() >= MOST_CHANGED {
            return Err(format!(
                "This answer already changes {MOST_CHANGED} points: the rest can wait for the next."
            ));
        }
        let replaces = replaces
            .iter()
            .map(|r| {
                found(r, 'p', self.refs.points.len())
                    .map(|n| self.refs.points[n])
                    .ok_or_else(|| format!("The page has no point {r}: name one like p3."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (placed, starts) = self.place(&block, &kind, replaces.first().map(|(_, b)| *b))?;
        let theme = match placed.kind.as_str() {
            "week" => {
                return Err(format!(
                    "{} keeps no points: it shows the days of the reminders, so offer one instead.",
                    title_of(&placed)
                ));
            }
            "chips" | "list" | "you" => String::new(),
            _ => {
                let theme = cut(&theme, 120);
                if theme.is_empty() {
                    return Err(format!(
                        "A point in {} needs a theme: {}.",
                        title_of(&placed),
                        match placed.kind.as_str() {
                            "today" => "what comes round, named as its reminders are",
                            "shelf" => "the thing's name",
                            "text" => "the section's heading",
                            "timeline" => "the section it belongs to, named as the page names it",
                            "bars" => "the set's caption",
                            "questions" => "the question, as it would be asked",
                            "table" => "the thing its row is about",
                            "trend" => "what's measured (Lisas Gewicht)",
                            "parts" => "the whole's caption",
                            _ => "the thing the card is about",
                        }
                    ));
                }
                theme
            }
        };
        let happened = when(&placed.kind, &happened, title_of(&placed))?;
        let shape = Shape {
            facet,
            look,
            amount,
            note,
        }
        .fit(&placed.kind, &placed.facets, title_of(&placed), true)?;
        if self
            .points
            .iter()
            .any(|p| p.block_id == placed.id && p.text == text && p.theme == theme)
        {
            return Ok(());
        }
        let under = if theme.is_empty() {
            title_of(&placed).to_string()
        } else {
            theme.clone()
        };
        let block_id = placed.id;
        if starts {
            self.blocks.push(placed);
        }
        let url = cut(&url, 2000);
        self.points.push(KeptPoint {
            block_id,
            theme,
            under,
            text,
            happened,
            shape,
            source: some(cut(&source, 200)),
            url: (url.starts_with("https://") || url.starts_with("http://")).then_some(url),
            replaces: replaces.into_iter().map(|(id, _)| id).collect(),
        });
        Ok(())
    }

    /// The block a point names, and whether the answer starts it. With no
    /// block named, a point that replaces others stays in their block.
    fn place(
        &self,
        block: &str,
        kind: &str,
        replacing: Option<Uuid>,
    ) -> Result<(RefBlock, bool), String> {
        let named = block.trim();
        let all = || self.refs.blocks.iter().chain(&self.blocks);
        if let Some(n) = found(named, 'b', self.refs.blocks.len()) {
            return Ok((self.refs.blocks[n].clone(), false));
        }
        if !named.is_empty()
            && let Some(b) = all().find(|b| b.title.to_lowercase() == named.to_lowercase())
        {
            return Ok((b.clone(), false));
        }
        if named.is_empty()
            && let Some(b) = replacing.and_then(|id| all().find(|b| b.id == id))
        {
            return Ok((b.clone(), false));
        }
        let kind = kind.trim();
        if named.is_empty() {
            return Err("Name the block the point goes in, like b2.".into());
        }
        if kind.is_empty() {
            return Err(format!(
                "The page has no block {named}: name one like b2, or give a kind to start it."
            ));
        }
        if !BLOCK_KINDS.contains(&kind) {
            return Err(format!(
                "There's no kind of block {kind}: it's one of {BLOCK_KINDS:?}."
            ));
        }
        if self.blocks.len() >= MOST_BLOCKS {
            return Err(format!(
                "An answer starts at most {MOST_BLOCKS} blocks: keep the rest in the page's own."
            ));
        }
        let title = cut(named, 60);
        Ok((
            RefBlock {
                id: Uuid::now_v7(),
                kind: kind.into(),
                title,
                facets: Vec::new(),
            },
            true,
        ))
    }

    /// Ticks the page's `points` (`p3`) done, or not: a list's items, a
    /// question they answered well.
    pub fn tick_points(&mut self, points: &[String], done: bool) -> Result<(), String> {
        if points.is_empty() {
            return Err("Name the points to tick, like p3.".into());
        }
        for reference in points {
            let (id, block) = found(reference, 'p', self.refs.points.len())
                .map(|n| self.refs.points[n])
                .ok_or_else(|| format!("The page has no point {reference}: name one like p3."))?;
            let kind = self
                .refs
                .blocks
                .iter()
                .find(|b| b.id == block)
                .map_or("", |b| b.kind.as_str());
            if kind != "list" && kind != "questions" {
                return Err(format!(
                    "{} is in a {kind} block: only a list's items and questions are ticked.",
                    reference.trim()
                ));
            }
            self.ticks.retain(|(t, _)| *t != id);
            if self.ticks.len() >= MOST_TICKS {
                return Err(format!("An answer ticks at most {MOST_TICKS} points."));
            }
            self.ticks.push((id, done));
        }
        Ok(())
    }

    /// A question to check a guess the answer rested on, with the point to
    /// keep when they say yes.
    pub fn ask_to_check(&mut self, theme: &str, text: &str, point: &str) -> Result<(), String> {
        let text = cut(text, 2000);
        if text.is_empty() {
            return Err("Say what to check.".into());
        }
        if self.questions.len() >= MOST_QUESTIONS {
            return Err(format!(
                "An answer asks to check at most {MOST_QUESTIONS} things."
            ));
        }
        self.questions.push(KeptQuestion {
            theme: some(cut(theme, 120)),
            text,
            point: some(cut(point, 2000)),
        });
        Ok(())
    }

    /// Offers a reminder, in place of the reminders it `replaces` (`r2`).
    pub fn offer_reminder(
        &mut self,
        reminder: ProposedReminder,
        replaces: &[String],
    ) -> Result<(), String> {
        if self.reminders.len() >= MOST_REMINDERS {
            return Err(format!(
                "An answer offers at most {MOST_REMINDERS} reminders."
            ));
        }
        let replaces = replaces
            .iter()
            .map(|r| {
                found(r, 'r', self.refs.reminders.len())
                    .map(|n| self.refs.reminders[n].0)
                    .ok_or_else(|| format!("The calendar has no reminder {r}: name one like r2."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.reminders.push(KeptReminder { reminder, replaces });
        Ok(())
    }

    /// Changes how the topic answers from now on: what it sticks to, its
    /// brief, given whole, in place of the brief it has `now`. A second
    /// change in the same answer counts the first as how it is now. They
    /// see what it `said` under the answer, and can undo it.
    pub fn change_brief(
        &mut self,
        topic_id: Uuid,
        name: &str,
        now: &str,
        brief: &str,
        said: &str,
    ) -> Result<(), String> {
        let brief = cut(brief, STICKS_TO_MAX);
        let earlier = self.changes.iter().position(|c| c.topic_id == topic_id);
        let now = earlier
            .and_then(|n| self.changes[n].after.sticks_to.clone())
            .unwrap_or_else(|| now.trim().to_string());
        // A rule on its own would lose the rest of the brief.
        if brief.chars().count() < now.chars().count() / 2 {
            return Err(
                "Give the whole brief as it is from now on: all of it, with the rule added or changed where it fits."
                    .into(),
            );
        }
        if brief == now {
            return Err("The brief says that already.".into());
        }
        let said = match cut(said, 200) {
            said if said.is_empty() => "Changed how it answers.".to_string(),
            said => said,
        };
        match earlier {
            Some(n) => {
                let change = &mut self.changes[n];
                change.after.sticks_to = Some(brief);
                change.before.sticks_to.get_or_insert(now);
                if change.said != said {
                    change.said = format!("{} {said}", change.said);
                }
            }
            None => self.changes.push(TopicChange {
                topic_id,
                name: name.into(),
                said,
                before: TopicPatch {
                    sticks_to: Some(now),
                    ..TopicPatch::default()
                },
                after: TopicPatch {
                    sticks_to: Some(brief),
                    ..TopicPatch::default()
                },
                layout: None,
                undone: false,
            }),
        }
        Ok(())
    }

    /// The answer's own fields (`kept`, `elsewhere`, `proposal`,
    /// `changes`), and the writes that come after it.
    pub fn writes(&self, at: &Answering) -> (Map<String, Value>, Vec<Operation>) {
        let mut operations = Vec::new();
        // The layout first: it moves what the page has, and its new points
        // come from this talk.
        if let Some(laid) = &self.layout {
            for operation in &laid.operations {
                let mut operation = operation.clone();
                if let Operation::Create {
                    model: ModelKind::Point,
                    data,
                    ..
                } = &mut operation
                {
                    data.insert("talk_id".into(), json!(at.talk_id));
                }
                operations.push(operation);
            }
        }
        for block in &self.blocks {
            operations.push(Operation::Create {
                model: ModelKind::Block,
                id: block.id,
                data: fields([
                    ("topic_id", json!(at.topic_id)),
                    ("kind", json!(block.kind)),
                    ("title", json!(block.title)),
                    ("facets", json!(block.facets)),
                ]),
            });
        }
        let mut themes: Vec<String> = Vec::new();
        let mut replaced: Vec<Uuid> = Vec::new();
        let mut updated = 0;
        for point in &self.points {
            if !themes.contains(&point.under) {
                themes.push(point.under.clone());
            }
            let data = fields([
                ("block_id", json!(point.block_id)),
                ("theme", json!(point.theme)),
                ("text", json!(point.text)),
                ("happened", json!(point.happened)),
                ("facet", json!(point.shape.facet)),
                ("look", json!(point.shape.look)),
                ("amount", json!(point.shape.amount)),
                ("note", json!(point.shape.note)),
                ("source", json!(point.source)),
                ("source_url", json!(point.url)),
                ("talk_id", json!(at.talk_id)),
            ]);
            // A point that replaces others takes the first one's place,
            // with the unread dot, and the rest go.
            let replaces: Vec<Uuid> = point
                .replaces
                .iter()
                .filter(|id| !replaced.contains(id))
                .copied()
                .collect();
            match replaces.first() {
                Some(&id) => {
                    let mut data = data;
                    data.insert("seen".into(), json!(false));
                    operations.push(Operation::Update {
                        model: ModelKind::Point,
                        id,
                        data,
                    });
                    updated += 1;
                    replaced.push(id);
                }
                None => {
                    let mut data = data;
                    data.insert("topic_id".into(), json!(at.topic_id));
                    operations.push(Operation::Create {
                        model: ModelKind::Point,
                        id: Uuid::now_v7(),
                        data,
                    });
                }
            }
            for &id in replaces.iter().skip(1) {
                operations.push(Operation::Delete {
                    model: ModelKind::Point,
                    id,
                });
                replaced.push(id);
            }
        }
        for &(id, done) in &self.ticks {
            if !replaced.contains(&id) {
                operations.push(Operation::Update {
                    model: ModelKind::Point,
                    id,
                    data: fields([("done", json!(done))]),
                });
            }
        }
        for question in self.questions.iter().take(2) {
            operations.push(Operation::Create {
                model: ModelKind::Question,
                id: Uuid::now_v7(),
                data: fields([
                    ("topic_id", json!(at.topic_id)),
                    ("theme", json!(question.theme)),
                    ("text", json!(question.text)),
                    ("point", json!(question.point)),
                    ("talk_id", json!(at.talk_id)),
                ]),
            });
        }
        for kept in &self.reminders {
            // One only offered goes at once; one on the calendar stays
            // until the offer is taken.
            let (offered, on): (Vec<Uuid>, Vec<Uuid>) = kept.replaces.iter().partition(|id| {
                self.refs
                    .reminders
                    .iter()
                    .any(|(r, offered)| r == *id && *offered)
            });
            for id in offered {
                operations.push(Operation::Delete {
                    model: ModelKind::Reminder,
                    id,
                });
            }
            operations.push(offer(at.topic_id, at.answer_id, &kept.reminder, &on));
        }
        // The week shows the days of what the topic offers to remind.
        let has_week = self
            .refs
            .blocks
            .iter()
            .chain(&self.blocks)
            .any(|b| b.kind == "week");
        if !self.reminders.is_empty() && !has_week {
            operations.push(Operation::Create {
                model: ModelKind::Block,
                id: Uuid::now_v7(),
                data: fields([("topic_id", json!(at.topic_id)), ("kind", json!("week"))]),
            });
        }
        for change in &self.changes {
            let Value::Object(data) = patch_fields(&change.after) else {
                continue;
            };
            operations.push(Operation::Update {
                model: ModelKind::Topic,
                id: change.topic_id,
                data,
            });
        }
        if at.first
            && let Some(title) = self.talk_title.as_deref().map(|t| cut(t, 80))
            && !title.is_empty()
        {
            operations.push(Operation::Update {
                model: ModelKind::Talk,
                id: at.talk_id,
                data: fields([("title", json!(title))]),
            });
        }

        let mut message = Map::new();
        let kept: Vec<String> = [
            (!self.points.is_empty()).then(|| kept_words(self.points.len(), updated, &themes)),
            ticked_words(&self.ticks),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !kept.is_empty() {
            message.insert("kept".into(), json!(kept.join(" ")));
        }
        if let Some(elsewhere) = self.elsewhere.as_deref().map(|e| cut(e, 80))
            && !elsewhere.is_empty()
            && elsewhere != at.topic_name
        {
            message.insert("elsewhere".into(), json!(elsewhere));
        }
        if let Some(proposal) = &self.proposal {
            message.insert("proposal".into(), json!(proposal));
        }
        let changes: Vec<&TopicChange> = self
            .changes
            .iter()
            .chain(self.layout.as_ref().map(|laid| &laid.change))
            .collect();
        if !changes.is_empty() {
            message.insert("changes".into(), json!(changes));
        }
        (message, operations)
    }
}

/// A reminder from what Claude wrote: a title, the next day it's due
/// (`YYYY-MM-DD`), a time (`HH:MM`) or none, and a rhythm in days or none.
pub fn reminder(
    theme: &str,
    title: &str,
    due_on: &str,
    at: &str,
    every_days: Option<i64>,
) -> Result<ProposedReminder, String> {
    let title = cut(title, 200);
    if title.is_empty() {
        return Err("A reminder needs a title that says what to do.".into());
    }
    let due_on = NaiveDate::parse_from_str(due_on.trim(), "%Y-%m-%d")
        .map_err(|_| "due_on is a day, as YYYY-MM-DD.".to_string())?;
    let at = match at.trim() {
        "" => None,
        at => Some(
            NaiveTime::parse_from_str(at, "%H:%M")
                .map_err(|_| "at is a time, as HH:MM, or left out.".to_string())?,
        ),
    };
    let every_days = match every_days {
        None | Some(0) => None,
        Some(days @ 1..=366) => Some(days as i32),
        Some(_) => return Err("every_days is between 1 and 366, or left out for once.".into()),
    };
    Ok(ProposedReminder {
        theme: some(cut(theme, 120)),
        title,
        due_on,
        at,
        every_days,
    })
}

fn some(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

/// Which of `count` a reference such as `p3` (or `[p3]`) names, from 0.
pub fn found(reference: &str, kind: char, count: usize) -> Option<usize> {
    let n: usize = reference
        .trim()
        .trim_matches(['[', ']'])
        .strip_prefix(kind)?
        .parse()
        .ok()?;
    n.checked_sub(1).filter(|n| *n < count)
}

/// A block as a sentence names it.
fn title_of(block: &RefBlock) -> &str {
    match block.title.as_str() {
        "" => "The week",
        title => title,
    }
}

/// A reminder an answer offers, waiting for a yes under it, in place of the
/// reminders it `replaces` once it's taken.
pub fn offer(
    topic_id: Uuid,
    message_id: Uuid,
    reminder: &ProposedReminder,
    replaces: &[Uuid],
) -> Operation {
    Operation::Create {
        model: ModelKind::Reminder,
        id: Uuid::now_v7(),
        data: fields([
            ("topic_id", json!(topic_id)),
            ("theme", json!(reminder.theme)),
            ("title", json!(reminder.title)),
            ("due_on", json!(reminder.due_on)),
            ("at", json!(reminder.at)),
            ("every_days", json!(reminder.every_days)),
            ("status", json!("offered")),
            ("message_id", json!(message_id)),
            ("replaces", json!(replaces)),
        ]),
    }
}

/// A topic patch as sync fields: only the ones it sets.
pub fn patch_fields(patch: &almanac_domain::TopicPatch) -> Value {
    let mut value = json!(patch);
    if let Some(fields) = value.as_object_mut() {
        fields.retain(|_, v| !v.is_null());
    }
    value
}

/// "Kept 2 points under Monstera and Basilikum.", or "Updated a point
/// under Monstera." when they all replaced points the page had.
pub fn kept_words(points: usize, updated: usize, themes: &[String]) -> String {
    let what = if points == 1 {
        "a point".to_string()
    } else {
        format!("{points} points")
    };
    let verb = match updated {
        0 => "Kept",
        n if n == points => "Updated",
        _ => "Kept and updated",
    };
    let under = match themes {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("{verb} {what} under {under}.")
}

/// "Ticked 2 points off.", "Ticked a point on again."
fn ticked_words(ticks: &[(Uuid, bool)]) -> Option<String> {
    let words = |n: usize, how: &str| match n {
        0 => None,
        1 => Some(format!("Ticked a point {how}.")),
        n => Some(format!("Ticked {n} points {how}.")),
    };
    let done = ticks.iter().filter(|(_, d)| *d).count();
    let said: Vec<String> = [words(done, "off"), words(ticks.len() - done, "on again")]
        .into_iter()
        .flatten()
        .collect();
    (!said.is_empty()).then(|| said.join(" "))
}

pub fn fields<const N: usize>(pairs: [(&str, Value); N]) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_words_name_the_themes() {
        assert_eq!(
            kept_words(1, 0, &["Monstera".into()]),
            "Kept a point under Monstera."
        );
        assert_eq!(
            kept_words(
                3,
                0,
                &["Monstera".into(), "Basilikum".into(), "Efeu".into()]
            ),
            "Kept 3 points under Monstera, Basilikum and Efeu."
        );
        assert_eq!(
            kept_words(1, 1, &["Monstera".into()]),
            "Updated a point under Monstera."
        );
    }

    fn answering() -> Answering {
        Answering {
            topic_id: Uuid::now_v7(),
            topic_name: "Haare".into(),
            talk_id: Uuid::now_v7(),
            answer_id: Uuid::now_v7(),
            first: false,
        }
    }

    fn block(kind: &str, title: &str) -> RefBlock {
        RefBlock {
            id: Uuid::now_v7(),
            kind: kind.into(),
            title: title.into(),
            facets: Vec::new(),
        }
    }

    /// A point for `block`, starting one of `kind` when there's none.
    fn point(block: &str, kind: &str, theme: &str, text: &str) -> Handed {
        Handed {
            block: block.into(),
            kind: kind.into(),
            theme: theme.into(),
            text: text.into(),
            ..Handed::default()
        }
    }

    #[test]
    fn a_timeline_point_says_when_it_happened() {
        let timeline = block("timeline", "Zeitleiste");
        let mut kept = Kept::on(Refs {
            blocks: vec![block("text", "Geschichte"), timeline.clone()],
            ..Refs::default()
        });
        let undated = kept
            .keep_point(point("b2", "", "Intifada", "Sie beginnt"))
            .unwrap_err();
        assert!(undated.contains("says when it happened"), "{undated}");
        kept.keep_point(Handed {
            happened: "1987–1993".into(),
            source: "From the talk".into(),
            ..point("Zeitleiste", "", "Intifada", "Die erste Intifada beginnt")
        })
        .unwrap();
        // Elsewhere a point has no date.
        kept.keep_point(Handed {
            happened: "1987".into(),
            source: "From the talk".into(),
            ..point("b1", "", "Intifada", "Ein Aufstand gegen die Besatzung.")
        })
        .unwrap();
        assert_eq!(kept.points[0].block_id, timeline.id);
        assert_eq!(kept.points[0].happened, "1987–1993");
        assert_eq!(kept.points[1].happened, "");
    }

    #[test]
    fn a_layout_keeps_no_other_points() {
        let mut kept = Kept {
            layout: Some(Laid {
                change: TopicChange {
                    topic_id: Uuid::now_v7(),
                    name: "Katzen".into(),
                    said: "Sorted it.".into(),
                    before: TopicPatch::default(),
                    after: TopicPatch::default(),
                    layout: None,
                    undone: false,
                },
                operations: Vec::new(),
            }),
            ..Kept::default()
        };
        assert!(
            kept.keep_point(point("b1", "", "", "x"))
                .unwrap_err()
                .contains("laid out the page")
        );
    }

    #[test]
    fn a_point_takes_the_place_of_the_ones_it_replaces() {
        let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
        let notes = block("cards", "Notes");
        let mut kept = Kept::on(Refs {
            points: vec![(first, notes.id), (second, notes.id)],
            blocks: vec![notes.clone()],
            reminders: vec![],
        });
        assert!(
            kept.keep_point(Handed {
                replaces: vec!["p3".into()],
                ..point("b1", "", "Wäsche", "x")
            })
            .unwrap_err()
            .contains("no point p3")
        );
        // Named no block, it stays in the one it replaces.
        kept.keep_point(Handed {
            source: "You said".into(),
            replaces: vec!["p1".into(), "[p2]".into()],
            ..point("", "", "Wäsche", "Washes Wednesday and Friday.")
        })
        .unwrap();
        let (said, operations) = kept.writes(&answering());
        assert!(matches!(
            &operations[..],
            [
                Operation::Update { id: a, data, .. },
                Operation::Delete { id: b, .. },
            ] if *a == first && *b == second && data["seen"] == json!(false)
                && data["block_id"] == json!(notes.id)
        ));
        assert_eq!(said["kept"], json!("Updated a point under Wäsche."));
    }

    #[test]
    fn sorting_a_page_keeps_each_point_in_its_block() {
        let (today, you) = (block("today", "Today"), block("you", "You"));
        let notes = block("cards", "Notes");
        let ids: Vec<Uuid> = (0..12).map(|_| Uuid::now_v7()).collect();
        let mut kept = Kept::on(Refs {
            points: ids.iter().map(|id| (*id, notes.id)).collect(),
            blocks: vec![today.clone(), block("week", ""), you.clone(), notes],
            reminders: vec![],
        });
        assert!(
            kept.keep_point(point("b2", "", "", "Wash on Friday"))
                .unwrap_err()
                .contains("keeps no points")
        );
        assert!(
            kept.keep_point(point("Today", "", "", "Shampoo"))
                .unwrap_err()
                .contains("needs a theme")
        );
        // More than an answer may add, each in a point's place.
        for n in 1..=12 {
            kept.keep_point(Handed {
                replaces: vec![format!("p{n}")],
                ..point("you", "", "You", &format!("Fact {n}"))
            })
            .unwrap();
        }
        kept.keep_point(point(
            "b1",
            "",
            "Elvital wash",
            "Elvital shampoo: on the scalp",
        ))
        .unwrap();
        let (said, operations) = kept.writes(&answering());
        assert_eq!(operations.len(), 13);
        assert!(matches!(
            &operations[12],
            Operation::Create { data, .. }
                if data["block_id"] == json!(today.id) && data["theme"] == json!("Elvital wash")
        ));
        // A fact about them has no theme.
        assert!(matches!(
            &operations[0],
            Operation::Update { id, data, .. }
                if *id == ids[0] && data["block_id"] == json!(you.id) && data["theme"] == json!("")
        ));
        assert_eq!(
            said["kept"],
            json!("Kept and updated 13 points under You and Elvital wash.")
        );
    }

    #[test]
    fn a_point_may_start_a_block_the_page_lacks() {
        let mut kept = Kept::on(Refs::default());
        assert!(
            kept.keep_point(point("Reading", "", "", "Ward, Plant care"))
                .unwrap_err()
                .contains("give a kind")
        );
        kept.keep_point(point("Reading", "list", "", "Ward, Plant care"))
            .unwrap();
        // The next one finds it by its title.
        kept.keep_point(point("reading", "", "", "Kew, Ferns"))
            .unwrap();
        kept.keep_point(point("Terms", "chips", "", "Epiphyte"))
            .unwrap();
        assert!(
            kept.keep_point(point("Places", "cards", "Kew", "In London"))
                .unwrap_err()
                .contains("at most 2 blocks")
        );
        let (said, operations) = kept.writes(&answering());
        let [
            Operation::Create {
                model: ModelKind::Block,
                id: reading,
                data,
            },
            Operation::Create {
                model: ModelKind::Block,
                ..
            },
            Operation::Create { data: first, .. },
            Operation::Create { data: second, .. },
            Operation::Create { .. },
        ] = &operations[..]
        else {
            panic!("{operations:?}");
        };
        assert_eq!(data["kind"], json!("list"));
        assert_eq!(first["block_id"], json!(reading));
        assert_eq!(second["block_id"], json!(reading));
        assert_eq!(
            said["kept"],
            json!("Kept 3 points under Reading and Terms.")
        );
    }

    #[test]
    fn a_card_point_names_one_of_its_blocks_facets() {
        let mut plants = block("cards", "Pflanzen");
        plants.facets = vec![
            Facet {
                name: "Licht".into(),
                icon: "sun".into(),
            },
            Facet {
                name: "Wasser".into(),
                icon: "droplet".into(),
            },
        ];
        let mut kept = Kept::on(Refs {
            blocks: vec![plants, block("list", "Lesen")],
            ..Refs::default()
        });
        let unknown = kept
            .keep_point(Handed {
                facet: "Erde".into(),
                ..point("b1", "", "Monstera", "Torffrei")
            })
            .unwrap_err();
        assert!(unknown.contains("one of Licht, Wasser"), "{unknown}");
        kept.keep_point(Handed {
            facet: "wasser".into(),
            look: "Warn".into(),
            ..point("b1", "", "Monstera", "Braune Ränder: zu wenig")
        })
        .unwrap();
        // A list has no facets or looks: they go quietly.
        kept.keep_point(Handed {
            facet: "Wasser".into(),
            look: "warn".into(),
            ..point("b2", "", "", "Ward, Plant care")
        })
        .unwrap();
        assert_eq!(kept.points[0].shape.facet, "Wasser");
        assert_eq!(kept.points[0].shape.look, "warn");
        assert_eq!(kept.points[1].shape, Shape::default());
    }

    #[test]
    fn a_bar_has_an_amount() {
        let mut kept = Kept::on(Refs {
            blocks: vec![block("bars", "In Zahlen")],
            ..Refs::default()
        });
        assert!(
            kept.keep_point(point("b1", "", "9/11", "Am 11. September"))
                .unwrap_err()
                .contains("needs its amount")
        );
        kept.keep_point(Handed {
            amount: Some(2977.0),
            note: "getötet".into(),
            ..point("b1", "", "9/11", "Am 11. September")
        })
        .unwrap();
        let (_, operations) = kept.writes(&answering());
        assert!(matches!(
            &operations[..],
            [Operation::Create { data, .. }]
                if data["amount"] == json!(2977.0) && data["note"] == json!("getötet")
        ));
    }

    #[test]
    fn a_trend_keeps_a_number_on_a_day() {
        let mut kept = Kept::on(Refs {
            blocks: vec![block("trend", "Gewicht")],
            ..Refs::default()
        });
        let weighed = |happened: &str, amount: Option<f64>| Handed {
            happened: happened.into(),
            amount,
            note: "kg".into(),
            source: "You said".into(),
            ..point("b1", "", "Lisas Gewicht", "Gewogen")
        };
        assert!(
            kept.keep_point(weighed("2026-09-24", None))
                .unwrap_err()
                .contains("needs its amount")
        );
        assert!(
            kept.keep_point(weighed("24. September", Some(3.5)))
                .unwrap_err()
                .contains("as 2026-09-24")
        );
        kept.keep_point(weighed("2026-09-24", Some(3.5))).unwrap();
        // With no day it's the mark it aims at.
        kept.keep_point(Handed {
            text: "Ziel".into(),
            ..weighed("", Some(3.4))
        })
        .unwrap();
        assert_eq!(kept.points[0].happened, "2026-09-24");
        assert_eq!(kept.points[0].shape.note, "kg");
        assert_eq!(kept.points[1].happened, "");
    }

    #[test]
    fn a_table_cell_is_a_point_with_its_column() {
        let mut shoes = block("table", "Schuhe");
        shoes.facets = vec![Facet {
            name: "Preis".into(),
            icon: "tag".into(),
        }];
        let mut kept = Kept::on(Refs {
            blocks: vec![shoes, block("parts", "Wo man arbeitet")],
            ..Refs::default()
        });
        assert!(
            kept.keep_point(Handed {
                facet: "Größe".into(),
                ..point("b1", "", "Exani Axio M", "43, 44")
            })
            .unwrap_err()
            .contains("no facet Größe")
        );
        kept.keep_point(Handed {
            facet: "preis".into(),
            amount: Some(709.0),
            look: "warn".into(),
            ..point("b1", "", "Exani Axio M", "709 kr")
        })
        .unwrap();
        assert_eq!(
            kept.points[0].shape,
            Shape {
                facet: "Preis".into(),
                look: "warn".into(),
                amount: Some(709.0),
                note: String::new(),
            }
        );
        // A part needs how many.
        assert!(
            kept.keep_point(point("b2", "", "28 Anzeigen", "Remote"))
                .unwrap_err()
                .contains("A part in Wo man arbeitet needs its amount")
        );
    }

    #[test]
    fn ticks_are_for_lists_and_questions() {
        let (read, cards) = (block("list", "Lesen"), block("cards", "Pflanzen"));
        let (book, plant) = (Uuid::now_v7(), Uuid::now_v7());
        let mut kept = Kept::on(Refs {
            points: vec![(book, read.id), (plant, cards.id)],
            blocks: vec![read, cards],
            reminders: vec![],
        });
        assert!(
            kept.tick_points(&["p2".into()], true)
                .unwrap_err()
                .contains("only a list's items and questions")
        );
        kept.tick_points(&["p1".into()], true).unwrap();
        let (said, operations) = kept.writes(&answering());
        assert!(matches!(
            &operations[..],
            [Operation::Update { id, data, .. }] if *id == book && data["done"] == json!(true)
        ));
        assert_eq!(said["kept"], json!("Ticked a point off."));
    }

    #[test]
    fn facets_have_an_icon_almanac_has() {
        let facet = |name: &str, icon: &str| Facet {
            name: name.into(),
            icon: icon.into(),
        };
        assert!(
            facets(&[facet("Licht", "lamp")])
                .unwrap_err()
                .contains("no icon lamp")
        );
        assert!(
            facets(&[facet("Licht", "sun"), facet("licht", "sun")])
                .unwrap_err()
                .contains("twice")
        );
        assert_eq!(
            facets(&[facet(" Licht ", "Sun")]).unwrap(),
            [facet("Licht", "sun")]
        );
    }

    #[test]
    fn a_reminder_brings_the_week_when_the_page_has_none() {
        let mut kept = Kept::on(Refs::default());
        let friday = reminder("", "Water the Monstera", "2026-10-30", "", Some(7)).unwrap();
        kept.offer_reminder(friday, &[]).unwrap();
        let (_, operations) = kept.writes(&answering());
        assert!(matches!(
            &operations[..],
            [
                Operation::Create { model: ModelKind::Reminder, .. },
                Operation::Create { model: ModelKind::Block, data, .. },
            ] if data["kind"] == json!("week")
        ));
    }

    #[test]
    fn a_reminder_replaces_one_on_the_calendar_once_its_taken() {
        let (on, offered) = (Uuid::now_v7(), Uuid::now_v7());
        let mut kept = Kept::on(Refs {
            blocks: vec![block("week", "")],
            points: vec![],
            reminders: vec![(on, false), (offered, true)],
        });
        let friday = reminder("Wäsche", "Elvital wash", "2026-10-30", "", Some(7)).unwrap();
        kept.offer_reminder(friday, &["r1".into(), "r2".into()])
            .unwrap();
        let (_, operations) = kept.writes(&answering());
        let [
            Operation::Delete { id: gone, .. },
            Operation::Create { data, .. },
        ] = &operations[..]
        else {
            panic!("{operations:?}");
        };
        assert_eq!(*gone, offered);
        assert_eq!(data["replaces"], json!([on]));
    }

    #[test]
    fn a_brief_changes_whole_and_says_so() {
        let topic = Uuid::now_v7();
        let now = "Geschichte global erklärt.\nImmer Deutsch, klar und knapp.";
        let mut kept = Kept::default();
        assert!(
            kept.change_brief(topic, "Geschichte", now, "Daten nennen.", "")
                .unwrap_err()
                .contains("whole brief")
        );
        let dated = format!("{now}\nBei Quellen sagen, wann sie geschrieben wurden.");
        kept.change_brief(
            topic,
            "Geschichte",
            now,
            &dated,
            "Nennt, wann eine Quelle geschrieben wurde.",
        )
        .unwrap();
        let shorter = format!("{dated}\nKürzer: drei Sätze.");
        kept.change_brief(topic, "Geschichte", now, &shorter, "Antwortet kürzer.")
            .unwrap();
        let [change] = &kept.changes[..] else {
            panic!("{:?}", kept.changes);
        };
        assert_eq!(change.before.sticks_to.as_deref(), Some(now));
        assert_eq!(change.after.sticks_to.as_deref(), Some(shorter.as_str()));
        assert_eq!(
            change.said,
            "Nennt, wann eine Quelle geschrieben wurde. Antwortet kürzer."
        );
        assert!(change.after.name.is_none(), "only the brief");
        let (said, operations) = kept.writes(&answering());
        assert_eq!(said["changes"][0]["name"], json!("Geschichte"));
        assert!(matches!(
            &operations[..],
            [Operation::Update { model: ModelKind::Topic, data, .. }]
                if data.keys().collect::<Vec<_>>() == ["sticks_to"]
        ));
    }

    #[test]
    fn a_change_writes_only_what_it_changes() {
        let patch = TopicPatch {
            group_name: Some("Hobby".into()),
            ..TopicPatch::default()
        };
        assert_eq!(patch_fields(&patch), json!({ "group_name": "Hobby" }));
    }

    #[test]
    fn only_the_first_answer_names_the_talk() {
        let kept = Kept {
            talk_title: Some("Gießen".into()),
            ..Kept::default()
        };
        let mut at = Answering {
            topic_id: Uuid::now_v7(),
            topic_name: "Pflanzen".into(),
            talk_id: Uuid::now_v7(),
            answer_id: Uuid::now_v7(),
            first: true,
        };
        assert_eq!(kept.writes(&at).1.len(), 1);
        at.first = false;
        assert!(kept.writes(&at).1.is_empty());
    }
}
