//! What one answer keeps and changes on its topic's page, and what else it
//! leaves: questions to check, reminders it offers, the topic an off-topic
//! question fits, the talk's title, how the topic answers from now on (its
//! brief), and, in the workspace's own topic, a topic it proposes and the
//! topics it changed. Claude files into the one set of blocks
//! (`almanac_domain::SET_KINDS`) with Almanac's tools while it answers
//! (`crate::mcp`); through the Messages API one more request reads the same
//! from the exchange (`crate::talks`). Each tool works on a copy of the
//! page, by the references the page gave its blocks and points
//! (`crate::page::Refs`), and the copy is written with the answer through
//! the sync engine: what's new is kept, and what changed, moved or went is
//! one change in the answer's Changes, which Undo puts back
//! (`crate::layout::undo`).

use std::collections::BTreeMap;

use almanac_domain::{
    BlockWas, Cell, FIELD_KINDS, Field, LOOKS, MOST_FIELDS, ModelKind, Operation, PageLayout,
    PointWas, Proposal, ProposedReminder, SET_KINDS, STICKS_TO_MAX, TopicChange, TopicPatch,
};
use chrono::{Days, Local, NaiveDate, NaiveTime};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::types::Json;
use uuid::Uuid;

use crate::page::{PagePoint, RefBlock, Refs};
use crate::topics::cut;

#[derive(Debug)]
pub struct Kept {
    /// The page as it was, which the answer's copy is written against.
    pub refs: Refs,
    /// The answer's copy of the page: its blocks and points, then the ones
    /// the answer starts, in the order of their references.
    blocks: Vec<RefBlock>,
    points: Vec<PagePoint>,
    /// The blocks and points it removed.
    gone: Vec<Uuid>,
    /// The blocks and points it moved on purpose, not only along.
    moved: Vec<Uuid>,
    /// The page's blocks and points it changed, moved, removed or ticked.
    touched: Vec<Uuid>,
    /// Items ticked done, or not.
    ticks: Vec<(Uuid, bool)>,
    /// Reminders an item's tick ticks, with what changes on them.
    rung: Vec<(Uuid, Map<String, Value>)>,
    /// The photos of the talk, in order, which a photo cell names by number.
    pub photos: Vec<Uuid>,
    /// For when a ticked reminder comes round again.
    pub today: NaiveDate,
    pub questions: Vec<KeptQuestion>,
    pub reminders: Vec<KeptReminder>,
    /// The topic an off-topic question fits better, by name.
    pub elsewhere: Option<String>,
    pub talk_title: Option<String>,
    pub proposal: Option<Proposal>,
    pub changes: Vec<TopicChange>,
}

impl Default for Kept {
    fn default() -> Self {
        Kept::on(Refs::default())
    }
}

/// A field of a collection as Claude gives it: its name, its kind (one of
/// [`FIELD_KINDS`]), a number's unit and goal, the words of a choice or a
/// status, and the name it `was` when it's renamed.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct FieldArg {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub goal: Option<f64>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub was: String,
}

/// An item as Claude adds it: a line, a paragraph, a toggle's point under
/// its line (the theme), a callout with its look.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct ItemArg {
    pub text: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub look: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub url: String,
}

/// A collection's row as Claude adds it: its name and its cells by field.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RowArg {
    pub name: String,
    #[serde(default)]
    pub cells: Map<String, Value>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub url: String,
}

/// What changes on a point the page has: its words (a row's name), its
/// theme, its look, and cells, merged into the row's, `null` removing one.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct PointChange {
    pub point: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub look: Option<String>,
    #[serde(default)]
    pub cells: Option<Map<String, Value>>,
}

#[derive(Debug)]
pub struct KeptReminder {
    pub reminder: ProposedReminder,
    /// The reminders it takes the place of.
    pub replaces: Vec<Uuid>,
    /// The checklist item it rings for.
    pub point_id: Option<Uuid>,
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

/// How much one answer may do. A page keeps what's worth knowing later;
/// more new points than this from one exchange is a list copied in. Tidying
/// a page changes more, up to the most it may change at all.
const MOST_NEW: usize = 12;
const MOST_CHANGED: usize = 40;
const MOST_BLOCKS: usize = 4;
const MOST_QUESTIONS: usize = 2;
const MOST_REMINDERS: usize = 3;

/// Where a block or point goes among the others.
#[derive(Clone, Copy)]
enum At {
    End,
    Top,
    After(usize),
}

impl Kept {
    /// Keeping for a page with these blocks, points and reminders.
    pub fn on(refs: Refs) -> Self {
        Kept {
            blocks: refs.blocks.clone(),
            points: refs.points.clone(),
            refs,
            gone: Vec::new(),
            moved: Vec::new(),
            touched: Vec::new(),
            ticks: Vec::new(),
            rung: Vec::new(),
            photos: Vec::new(),
            today: Local::now().date_naive(),
            questions: Vec::new(),
            reminders: Vec::new(),
            elsewhere: None,
            talk_title: None,
            proposal: None,
            changes: Vec::new(),
        }
    }

    /// Starts a block of the set, `after` the one it names ("" for the
    /// top; the end when left out), with a collection's fields. It answers
    /// with the block's reference. A block like it the page has already is
    /// the one it answers with.
    pub fn add_block(
        &mut self,
        kind: &str,
        title: &str,
        after: Option<&str>,
        fields: &[FieldArg],
    ) -> Result<String, String> {
        let kind = kind.trim().to_lowercase();
        if !SET_KINDS.contains(&kind.as_str()) {
            return Err(format!(
                "There's no kind of block {kind}: it's one of {}.",
                SET_KINDS.join(", ")
            ));
        }
        let title = cut(title, 60);
        if kind == "heading" && title.is_empty() {
            return Err("A heading is its title: give one.".into());
        }
        if kind != "collection" && !fields.is_empty() {
            return Err("Only a collection has fields.".into());
        }
        let (fields, _) = fields_of(fields, &[])?;
        if let Some(n) = (0..self.blocks.len()).find(|&n| {
            let b = &self.blocks[n];
            !self.is_gone(b.id) && b.kind == kind && b.title.to_lowercase() == title.to_lowercase()
        }) {
            return Ok(format!("b{}", n + 1));
        }
        if self.blocks.len() - self.refs.blocks.len() >= MOST_BLOCKS {
            return Err(format!(
                "An answer starts at most {MOST_BLOCKS} blocks: keep the rest in the page's own."
            ));
        }
        let at = self.block_at_place(after)?;
        self.blocks.push(RefBlock {
            id: Uuid::now_v7(),
            kind,
            title,
            facets: Json(Vec::new()),
            fields: Json(fields),
            width: String::new(),
            view: String::new(),
            position: 0,
        });
        let n = self.blocks.len() - 1;
        self.put_block(n, at);
        Ok(format!("b{}", n + 1))
    }

    /// Renames a block, gives a collection its fields as they are from now
    /// on (one renamed says the name it `was`; one left out goes, with its
    /// cells), or picks another of the pictures its fields can draw.
    pub fn change_block(
        &mut self,
        block: &str,
        title: Option<&str>,
        fields: Option<&[FieldArg]>,
        view: Option<&str>,
    ) -> Result<(), String> {
        let n = self.block_at(block)?;
        let kind = self.blocks[n].kind.clone();
        if title.is_none() && fields.is_none() && view.is_none() {
            return Err("Say what changes: its title, its fields or its picture.".into());
        }
        self.touch(self.blocks[n].id)?;
        if let Some(title) = title {
            let title = cut(title, 60);
            if kind == "week" {
                return Err("The week has no title.".into());
            }
            if kind == "heading" && title.is_empty() {
                return Err("A heading is its title: give one.".into());
            }
            self.blocks[n].title = title;
        }
        if let Some(given) = fields {
            if kind != "collection" {
                return Err(format!(
                    "{} isn't a collection: only those have fields.",
                    self.title(n)
                ));
            }
            let (fields, renamed) = fields_of(given, &self.blocks[n].fields)?;
            let id = self.blocks[n].id;
            for point in self.points.iter_mut().filter(|p| p.block_id == id) {
                let mut cells = BTreeMap::new();
                for (name, cell) in std::mem::take(&mut point.cells.0) {
                    let name = renamed
                        .iter()
                        .find(|(was, _)| *was == name)
                        .map_or(name, |(_, now)| now.clone());
                    if fields.iter().any(|f| f.name == name) {
                        cells.insert(name, cell);
                    }
                }
                point.cells = Json(cells);
            }
            self.blocks[n].fields = Json(fields);
        }
        if let Some(view) = view {
            let view = view.trim().to_lowercase();
            if kind != "collection" {
                return Err(format!(
                    "{} is drawn as its kind says: only a collection has pictures to pick from.",
                    self.title(n)
                ));
            }
            let id = self.blocks[n].id;
            let rows: Vec<&PagePoint> = self.points_of(id).map(|i| &self.points[i]).collect();
            let fits = pictures(&self.blocks[n].fields, &rows, self.today);
            if !view.is_empty() && !fits.contains(&view.as_str()) {
                return Err(match fits.as_slice() {
                    [] => format!("{} has no rows yet to draw.", self.title(n)),
                    fits => format!(
                        "{} can be drawn as {}: its fields allow no other.",
                        self.title(n),
                        fits.join(", ")
                    ),
                });
            }
            self.blocks[n].view = view;
        }
        Ok(())
    }

    /// Adds items to a block: lines to bullets or a checklist, paragraphs,
    /// a toggle's points under their line, a callout. It answers with their
    /// references. One the block has already is the one it answers with.
    pub fn add_items(&mut self, block: &str, items: &[ItemArg]) -> Result<Vec<String>, String> {
        let n = self.block_at(block)?;
        let (kind, title) = (self.blocks[n].kind.clone(), self.title(n).to_string());
        let themed = takes_items(&kind, &title)?;
        if items.is_empty() {
            return Err("Give the items to add.".into());
        }
        let mut refs = Vec::new();
        for item in items {
            let text = cut(&item.text, 2000);
            if text.is_empty() {
                return Err("An item needs its words.".into());
            }
            let theme = match themed {
                Some(says) => {
                    let theme = cut(&item.theme, 120);
                    if theme.is_empty() {
                        return Err(format!("An item in {title} needs a theme: {says}."));
                    }
                    theme
                }
                None => String::new(),
            };
            let look = self.look(&kind, &title, &item.look)?;
            refs.push(self.add_point(
                n,
                theme,
                text,
                look,
                BTreeMap::new(),
                &item.source,
                &item.url,
            )?);
        }
        Ok(refs)
    }

    /// Adds rows to a collection, each its name and its cells by field. It
    /// answers with their references.
    pub fn add_rows(&mut self, block: &str, rows: &[RowArg]) -> Result<Vec<String>, String> {
        let n = self.block_at(block)?;
        let title = self.title(n).to_string();
        if self.blocks[n].kind != "collection" {
            return Err(format!("{title} isn't a collection: add items to it."));
        }
        if rows.is_empty() {
            return Err("Give the rows to add.".into());
        }
        let mut refs = Vec::new();
        for row in rows {
            let name = cut(&row.name, 200);
            if name.is_empty() {
                return Err("A row needs its name: the thing, event or measurement it is.".into());
            }
            let mut cells = BTreeMap::new();
            fill(
                &mut cells,
                &title,
                &self.blocks[n].fields,
                &row.cells,
                &self.photos,
            )?;
            refs.push(self.add_point(
                n,
                String::new(),
                name,
                String::new(),
                cells,
                &row.source,
                &row.url,
            )?);
        }
        Ok(refs)
    }

    /// Changes points the page has: their words, theme, look or cells.
    pub fn change_points(&mut self, changes: &[PointChange]) -> Result<(), String> {
        if changes.is_empty() {
            return Err("Give the points to change.".into());
        }
        for change in changes {
            let i = self.point_at(&change.point)?;
            let b = self.block_of(i);
            let (kind, title) = (self.blocks[b].kind.clone(), self.title(b).to_string());
            self.touch(self.points[i].id)?;
            if let Some(text) = &change.text {
                let text = cut(text, 2000);
                if text.is_empty() {
                    return Err(format!(
                        "{} needs its words: remove it instead.",
                        change.point.trim()
                    ));
                }
                self.points[i].text = text;
            }
            if let Some(theme) = &change.theme {
                if !has_themes(&kind) {
                    return Err(format!("{title} has no themes."));
                }
                let theme = cut(theme, 120);
                if theme.is_empty() {
                    return Err(format!("A point in {title} needs a theme."));
                }
                self.points[i].theme = theme;
            }
            if let Some(look) = &change.look {
                self.points[i].look = self.look(&kind, &title, look)?;
            }
            if let Some(given) = &change.cells {
                if kind != "collection" {
                    return Err(format!(
                        "{title} isn't a collection: its points have no cells."
                    ));
                }
                let mut cells = self.points[i].cells.0.clone();
                fill(
                    &mut cells,
                    &title,
                    &self.blocks[b].fields,
                    given,
                    &self.photos,
                )?;
                self.points[i].cells = Json(cells);
            }
        }
        Ok(())
    }

    /// Ticks items done, or not: a checklist's, and a list's or questions'
    /// from before the set. An item a reminder rings for ticks the
    /// reminder: done for now, it's due again on its next day.
    pub fn tick(&mut self, items: &[String], done: bool) -> Result<(), String> {
        if items.is_empty() {
            return Err("Name the items to tick, like p3.".into());
        }
        for reference in items {
            let i = self.point_at(reference)?;
            let b = self.block_of(i);
            let kind = self.blocks[b].kind.as_str();
            if !matches!(kind, "checklist" | "list" | "questions") {
                return Err(format!(
                    "{} is in {}, a {kind}: only a checklist's items are ticked.",
                    reference.trim(),
                    self.title(b)
                ));
            }
            let id = self.points[i].id;
            self.touch(id)?;
            let rings = self
                .refs
                .reminders
                .iter()
                .find(|r| r.point_id == Some(id) && r.status == "on");
            match rings {
                Some(reminder) => {
                    self.rung.retain(|(r, _)| *r != reminder.id);
                    if reminder.ticked() != done {
                        let data = if done {
                            done_for(reminder.due_on, reminder.every_days, self.today)
                        } else {
                            fields([
                                ("status", json!("on")),
                                ("due_on", json!(reminder.last_done_on)),
                            ])
                        };
                        self.rung.push((reminder.id, data));
                    }
                }
                None => self.points[i].done = done,
            }
            self.ticks.retain(|(t, _)| *t != id);
            self.ticks.push((id, done));
        }
        Ok(())
    }

    /// Moves a block `after` another ("" for the top).
    pub fn move_block(&mut self, block: &str, after: &str) -> Result<(), String> {
        let n = self.block_at(block)?;
        let at = self.block_at_place(Some(after))?;
        if matches!(at, At::After(a) if a == n) {
            return Err("A block can't go after itself.".into());
        }
        let id = self.blocks[n].id;
        self.touch(id)?;
        self.moved.push(id);
        self.put_block(n, at);
        Ok(())
    }

    /// Moves points `to` a block, `after` one of its points ("" for the
    /// top; the end when left out), in the order given. A point keeps its
    /// theme where the block has themes.
    pub fn move_points(
        &mut self,
        points: &[String],
        to: &str,
        after: Option<&str>,
    ) -> Result<(), String> {
        if points.is_empty() {
            return Err("Name the points to move, like p3.".into());
        }
        let b = self.block_at(to)?;
        let (kind, title) = (self.blocks[b].kind.clone(), self.title(b).to_string());
        let themed = match kind.as_str() {
            "collection" => None,
            _ => takes_items(&kind, &title)?,
        };
        let mut at = match after.map(str::trim) {
            None => At::End,
            Some("") => At::Top,
            Some(reference) => {
                let p = self.point_at(reference)?;
                if self.points[p].block_id != self.blocks[b].id {
                    return Err(format!("{} isn't in {title}.", reference.trim()));
                }
                At::After(p)
            }
        };
        for reference in points {
            let i = self.point_at(reference)?;
            if matches!(at, At::After(a) if a == i) {
                return Err("A point can't go after itself.".into());
            }
            if themed.is_some() && self.points[i].theme.is_empty() {
                return Err(format!(
                    "{} needs a theme in {title}: give it one with change_points first.",
                    reference.trim()
                ));
            }
            if themed.is_none() {
                self.points[i].theme = String::new();
            }
            let id = self.points[i].id;
            self.touch(id)?;
            self.moved.push(id);
            self.points[i].block_id = self.blocks[b].id;
            self.put_point(i, at);
            at = At::After(i);
        }
        Ok(())
    }

    /// Removes blocks (`b2`), with their points, and points (`p3`).
    pub fn remove(&mut self, references: &[String]) -> Result<(), String> {
        if references.is_empty() {
            return Err("Name what to remove, like b2 or p3.".into());
        }
        for reference in references {
            let named = reference.trim().trim_matches(['[', ']']);
            let id = if named.starts_with('b') {
                let n = self.block_at(named)?;
                if self.blocks[n].kind == "you" {
                    return Err(
                        "You stays: it keeps what they told you. Remove its points instead.".into(),
                    );
                }
                self.blocks[n].id
            } else {
                self.points[self.point_at(named)?].id
            };
            self.touch(id)?;
            self.gone.push(id);
        }
        Ok(())
    }

    /// The words of the checklist item (`p3`) a reminder rings for, for the
    /// title it has when Claude gives none.
    pub fn item_words(&self, reference: &str) -> Option<String> {
        self.point_at(reference)
            .ok()
            .map(|i| self.points[i].text.clone())
    }

    /// The pages the points this answer adds come from.
    pub fn urls(&self) -> impl Iterator<Item = &str> {
        self.points[self.refs.points.len()..]
            .iter()
            .filter(|p| !self.is_gone(p.id))
            .filter_map(|p| p.source_url.as_deref())
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

    /// Offers a reminder, in place of the reminders it `replaces` (`r2`),
    /// ringing for a checklist `item` (`p3`) when it names one.
    pub fn offer_reminder(
        &mut self,
        reminder: ProposedReminder,
        replaces: &[String],
        item: Option<&str>,
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
                    .map(|n| self.refs.reminders[n].id)
                    .ok_or_else(|| format!("The calendar has no reminder {r}: name one like r2."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let point_id = match item.map(str::trim).filter(|i| !i.is_empty()) {
            None => None,
            Some(item) => {
                let i = self.point_at(item)?;
                let b = self.block_of(i);
                if self.blocks[b].kind != "checklist" {
                    return Err(format!(
                        "{item} is in {}: a reminder rings for a checklist's item.",
                        self.title(b)
                    ));
                }
                Some(self.points[i].id)
            }
        };
        self.reminders.push(KeptReminder {
            reminder,
            replaces,
            point_id,
        });
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
        let mut layout = PageLayout {
            moves: Vec::new(),
            blocks: Vec::new(),
            points: Vec::new(),
            made_blocks: Vec::new(),
            made_points: Vec::new(),
        };
        let (mut changed_blocks, mut moved_blocks, mut removed_blocks) = (0, 0, Vec::new());
        for (n, block) in self.blocks.iter().enumerate() {
            let Some(was) = self.refs.blocks.get(n) else {
                if !self.is_gone(block.id) {
                    operations.push(Operation::Create {
                        model: ModelKind::Block,
                        id: block.id,
                        data: fields([
                            ("topic_id", json!(at.topic_id)),
                            ("kind", json!(block.kind)),
                            ("title", json!(block.title)),
                            ("fields", json!(block.fields.0)),
                            ("view", json!(block.view)),
                            ("position", json!(block.position)),
                        ]),
                    });
                    layout.made_blocks.push(block.id);
                }
                continue;
            };
            if self.is_gone(block.id) {
                continue;
            }
            let mut data = Map::new();
            if block.title != was.title {
                data.insert("title".into(), json!(block.title));
            }
            if block.fields != was.fields {
                data.insert("fields".into(), json!(block.fields.0));
            }
            if block.view != was.view {
                data.insert("view".into(), json!(block.view));
            }
            if !data.is_empty() {
                changed_blocks += 1;
            }
            if block.position != was.position {
                data.insert("position".into(), json!(block.position));
                if self.moved.contains(&block.id) {
                    moved_blocks += 1;
                }
            }
            if !data.is_empty() {
                operations.push(Operation::Update {
                    model: ModelKind::Block,
                    id: block.id,
                    data,
                });
                layout.blocks.push(block_was(was, false));
            }
        }
        let mut deletes = Vec::new();
        let (mut changed_points, mut moved_points, mut removed_points) = (0, 0, 0);
        let mut added = 0;
        let mut unders: Vec<String> = Vec::new();
        for (i, point) in self.points.iter().enumerate() {
            let in_gone = self.is_gone(point.block_id);
            let gone = in_gone || self.is_gone(point.id);
            let Some(was) = self.refs.points.get(i) else {
                if !gone {
                    operations.push(Operation::Create {
                        model: ModelKind::Point,
                        id: point.id,
                        data: fields([
                            ("topic_id", json!(at.topic_id)),
                            ("block_id", json!(point.block_id)),
                            ("theme", json!(point.theme)),
                            ("text", json!(point.text)),
                            ("look", json!(point.look)),
                            ("cells", json!(point.cells.0)),
                            ("done", json!(point.done)),
                            ("position", json!(point.position)),
                            ("source", json!(point.source)),
                            ("source_url", json!(point.source_url)),
                            ("talk_id", json!(at.talk_id)),
                        ]),
                    });
                    layout.made_points.push(point.id);
                    added += 1;
                    let under = match point.theme.as_str() {
                        "" => self.title_of(point.block_id).to_string(),
                        theme => theme.to_string(),
                    };
                    if !unders.contains(&under) {
                        unders.push(under);
                    }
                }
                continue;
            };
            if gone {
                // Deleted one by one: a block's cascade wouldn't sync.
                deletes.push(Operation::Delete {
                    model: ModelKind::Point,
                    id: point.id,
                });
                layout.points.push(point_was(was, true));
                if !in_gone {
                    removed_points += 1;
                }
                continue;
            }
            let mut data = Map::new();
            if point.text != was.text {
                data.insert("text".into(), json!(point.text));
            }
            if point.theme != was.theme {
                data.insert("theme".into(), json!(point.theme));
            }
            if point.look != was.look {
                data.insert("look".into(), json!(point.look));
            }
            if point.cells != was.cells {
                data.insert("cells".into(), json!(point.cells.0));
            }
            let reworded = !data.is_empty();
            if reworded {
                data.insert("seen".into(), json!(false));
                changed_points += 1;
            }
            if point.block_id != was.block_id {
                data.insert("block_id".into(), json!(point.block_id));
            }
            if point.position != was.position {
                data.insert("position".into(), json!(point.position));
            }
            if self.moved.contains(&point.id) && data.contains_key("block_id")
                || self.moved.contains(&point.id) && data.contains_key("position")
            {
                moved_points += 1;
            }
            // A tick isn't a change to undo: it shows with what's kept.
            if !data.is_empty() {
                layout.points.push(point_was(was, false));
            }
            if point.done != was.done {
                data.insert("done".into(), json!(point.done));
            }
            if !data.is_empty() {
                operations.push(Operation::Update {
                    model: ModelKind::Point,
                    id: point.id,
                    data,
                });
            }
        }
        operations.extend(deletes);
        for (block, was) in self.blocks.iter().zip(&self.refs.blocks) {
            if self.is_gone(block.id) {
                operations.push(Operation::Delete {
                    model: ModelKind::Block,
                    id: block.id,
                });
                layout.blocks.push(block_was(was, true));
                removed_blocks.push(match was.title.as_str() {
                    "" => format!("the {}", was.kind),
                    title => title.to_string(),
                });
            }
        }
        for (id, data) in &self.rung {
            operations.push(Operation::Update {
                model: ModelKind::Reminder,
                id: *id,
                data: data.clone(),
            });
        }
        for question in &self.questions {
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
                    .any(|r| r.id == **id && r.status == "offered")
            });
            for id in offered {
                operations.push(Operation::Delete {
                    model: ModelKind::Reminder,
                    id,
                });
            }
            operations.push(offer(
                at.topic_id,
                at.answer_id,
                &kept.reminder,
                &on,
                kept.point_id,
            ));
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
            (added > 0).then(|| kept_words(added, &unders)),
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
        let mut changes = self.changes.clone();
        let said = change_words(
            (changed_blocks, changed_points),
            (moved_blocks, moved_points),
            &removed_blocks,
            removed_points,
        );
        if let Some(said) = said {
            changes.push(TopicChange {
                topic_id: at.topic_id,
                name: at.topic_name.clone(),
                said,
                before: TopicPatch::default(),
                after: TopicPatch::default(),
                layout: Some(layout),
                undone: false,
            });
        }
        if !changes.is_empty() {
            message.insert("changes".into(), json!(changes));
        }
        (message, operations)
    }

    // The copy of the page.

    fn is_gone(&self, id: Uuid) -> bool {
        self.gone.contains(&id)
    }

    /// Counts a block or point of the page's towards what one answer may
    /// change.
    fn touch(&mut self, id: Uuid) -> Result<(), String> {
        let had = self.refs.blocks.iter().any(|b| b.id == id)
            || self.refs.points.iter().any(|p| p.id == id);
        if had && !self.touched.contains(&id) {
            if self.touched.len() >= MOST_CHANGED {
                return Err(format!(
                    "This answer already changes {MOST_CHANGED} things: the rest can wait for the next."
                ));
            }
            self.touched.push(id);
        }
        Ok(())
    }

    /// The block a reference (`b2`) or a title names.
    fn block_at(&self, reference: &str) -> Result<usize, String> {
        let named = reference.trim();
        let n = found(named, 'b', self.blocks.len()).or_else(|| {
            (!named.is_empty())
                .then(|| {
                    self.blocks.iter().position(|b| {
                        !self.is_gone(b.id) && b.title.to_lowercase() == named.to_lowercase()
                    })
                })
                .flatten()
        });
        match n {
            Some(n) if !self.is_gone(self.blocks[n].id) => Ok(n),
            Some(_) => Err(format!("{named} was removed in this answer.")),
            None if named.is_empty() => Err("Name the block, like b2.".into()),
            None => Err(format!("The page has no block {named}: name one like b2.")),
        }
    }

    /// Where `after` puts a block: after the one it names, at the top for
    /// "", at the end when left out.
    fn block_at_place(&self, after: Option<&str>) -> Result<At, String> {
        Ok(match after.map(str::trim) {
            None => At::End,
            Some("") => At::Top,
            Some(after) => At::After(self.block_at(after)?),
        })
    }

    fn point_at(&self, reference: &str) -> Result<usize, String> {
        let named = reference.trim();
        match found(named, 'p', self.points.len()) {
            Some(i) if self.is_gone(self.points[i].id) || self.is_gone(self.points[i].block_id) => {
                Err(format!("{named} was removed in this answer."))
            }
            Some(i) => Ok(i),
            None => Err(format!("The page has no point {named}: name one like p3.")),
        }
    }

    fn block_of(&self, point: usize) -> usize {
        let id = self.points[point].block_id;
        self.blocks.iter().position(|b| b.id == id).unwrap_or(0)
    }

    /// A block as a sentence names it.
    fn title(&self, n: usize) -> &str {
        match (self.blocks[n].title.as_str(), self.blocks[n].kind.as_str()) {
            ("", "week") => "The week",
            ("", _) => "the untitled block",
            (title, _) => title,
        }
    }

    fn title_of(&self, block: Uuid) -> &str {
        match self.blocks.iter().position(|b| b.id == block) {
            Some(n) => self.title(n),
            None => "",
        }
    }

    /// A block's points, in order.
    fn points_of(&self, block: Uuid) -> impl Iterator<Item = usize> + '_ {
        let mut order: Vec<usize> = (0..self.points.len())
            .filter(|&i| {
                let p = &self.points[i];
                p.block_id == block && !self.is_gone(p.id)
            })
            .collect();
        order.sort_by_key(|&i| (self.points[i].position, i));
        order.into_iter()
    }

    /// A point's look, as a block of `kind` takes it.
    fn look(&self, kind: &str, title: &str, look: &str) -> Result<String, String> {
        let look = look.trim().to_lowercase();
        if look.is_empty() {
            return Ok(look);
        }
        if !matches!(kind, "callout" | "cards" | "table" | "bars" | "questions") {
            return Err(format!(
                "{title} has no looks: a callout does, warn for what needs a look, info for what's ready."
            ));
        }
        if !LOOKS.contains(&look.as_str()) {
            return Err(format!(
                "A look is warn (it needs a look), info (it's ready) or none, not {look}."
            ));
        }
        Ok(look)
    }

    /// Adds a point to the block `n` of the copy, after the last with its
    /// theme, or at the end, and answers with its reference.
    #[allow(clippy::too_many_arguments)]
    fn add_point(
        &mut self,
        n: usize,
        theme: String,
        text: String,
        look: String,
        cells: BTreeMap<String, Cell>,
        source: &str,
        url: &str,
    ) -> Result<String, String> {
        let block_id = self.blocks[n].id;
        if let Some(i) = self.points_of(block_id).find(|&i| {
            let p = &self.points[i];
            p.text == text && p.theme == theme && p.cells.0 == cells
        }) {
            return Ok(format!("p{}", i + 1));
        }
        let new = (self.refs.points.len()..self.points.len())
            .filter(|&i| !self.is_gone(self.points[i].id))
            .count();
        if new >= MOST_NEW {
            return Err(format!(
                "This answer already adds {MOST_NEW} points: keep only what's worth knowing later."
            ));
        }
        let at = match theme.as_str() {
            "" => At::End,
            theme => self
                .points_of(block_id)
                .filter(|&i| self.points[i].theme == theme)
                .last()
                .map_or(At::End, At::After),
        };
        let url = cut(url, 2000);
        self.points.push(PagePoint {
            id: Uuid::now_v7(),
            block_id,
            theme,
            text,
            happened: String::new(),
            facet: String::new(),
            look,
            amount: None,
            note: String::new(),
            cells: Json(cells),
            done: false,
            position: 0,
            source: some(cut(source, 200)),
            source_url: (url.starts_with("https://") || url.starts_with("http://")).then_some(url),
        });
        let i = self.points.len() - 1;
        self.put_point(i, at);
        Ok(format!("p{}", i + 1))
    }

    /// Puts block `n` where `at` says: at the end past the others, or
    /// between them, which numbers them all anew.
    fn put_block(&mut self, n: usize, at: At) {
        let mut order: Vec<usize> = (0..self.blocks.len())
            .filter(|&b| b != n && !self.is_gone(self.blocks[b].id))
            .collect();
        order.sort_by_key(|&b| (self.blocks[b].position, b));
        match at {
            At::End => {
                self.blocks[n].position = order
                    .iter()
                    .map(|&b| self.blocks[b].position + 1)
                    .max()
                    .unwrap_or(0);
                return;
            }
            At::Top => order.insert(0, n),
            At::After(after) => {
                let k = order
                    .iter()
                    .position(|&b| b == after)
                    .map_or(order.len(), |k| k + 1);
                order.insert(k, n);
            }
        }
        for (position, b) in order.into_iter().enumerate() {
            self.blocks[b].position = position as i32;
        }
    }

    /// Puts point `i` where `at` says in its block, as [`Kept::put_block`].
    fn put_point(&mut self, i: usize, at: At) {
        let block = self.points[i].block_id;
        let mut order: Vec<usize> = self.points_of(block).filter(|&p| p != i).collect();
        match at {
            At::End => {
                self.points[i].position = order
                    .iter()
                    .map(|&p| self.points[p].position + 1)
                    .max()
                    .unwrap_or(0);
                return;
            }
            At::Top => order.insert(0, i),
            At::After(after) => {
                let k = order
                    .iter()
                    .position(|&p| p == after)
                    .map_or(order.len(), |k| k + 1);
                order.insert(k, i);
            }
        }
        for (position, p) in order.into_iter().enumerate() {
            self.points[p].position = position as i32;
        }
    }
}

/// Whether a block of `kind`, called `title`, takes items, and what its
/// theme says when its items have one.
fn takes_items(kind: &str, title: &str) -> Result<Option<&'static str>, String> {
    match kind {
        "bullets" | "checklist" | "paragraph" | "callout" | "you" | "list" | "chips" => Ok(None),
        "toggle" => Ok(Some("the line it opens from")),
        "text" => Ok(Some("the section's heading")),
        "today" => Ok(Some("what comes round, named as its reminders are")),
        "shelf" | "cards" => Ok(Some("the thing it's about")),
        "heading" => Err(format!("{title} is a heading: it has no points.")),
        "collection" => Err(format!("{title} is a collection: add rows to it.")),
        "week" => Err(format!(
            "{title} shows the days of the reminders: offer one instead."
        )),
        _ => Err(format!(
            "{title} is from before the set: put what's new in a block of the set."
        )),
    }
}

/// Whether the points of a block of `kind` go under themes.
fn has_themes(kind: &str) -> bool {
    !matches!(
        kind,
        "bullets" | "checklist" | "paragraph" | "callout" | "you" | "list" | "chips" | "collection"
    )
}

/// Fields renamed, each (was, now).
pub type Renamed = Vec<(String, String)>;

/// A collection's fields as Claude gives them, tidied, and the ones renamed
/// from what they `was`.
pub fn fields_of(given: &[FieldArg], had: &[Field]) -> Result<(Vec<Field>, Renamed), String> {
    if given.len() > MOST_FIELDS {
        return Err(format!(
            "A collection has at most {MOST_FIELDS} fields besides each row's name."
        ));
    }
    let mut fields: Vec<Field> = Vec::new();
    let mut renamed = Vec::new();
    for field in given {
        let name = cut(&field.name, 40);
        if name.is_empty() {
            return Err("A field needs a name: a word or two (Gewicht, Status).".into());
        }
        if fields
            .iter()
            .any(|f| f.name.to_lowercase() == name.to_lowercase())
        {
            return Err(format!("{name} is in the fields twice."));
        }
        let was = field.was.trim();
        let before = had.iter().find(|f| {
            let named = if was.is_empty() { &name } else { was };
            f.name.to_lowercase() == named.to_lowercase()
        });
        if !was.is_empty() && before.is_none() {
            return Err(format!("There's no field {was} to rename."));
        }
        let kind = match field.kind.trim().to_lowercase() {
            kind if kind.is_empty() => before.map_or("text".into(), |f| f.kind.clone()),
            kind => kind,
        };
        if !FIELD_KINDS.contains(&kind.as_str()) {
            return Err(format!(
                "There's no kind of field {kind}: it's one of {}.",
                FIELD_KINDS.join(", ")
            ));
        }
        if field.goal.is_some_and(|g| !g.is_finite()) {
            return Err(format!("{name}'s goal is a number."));
        }
        let options = match kind.as_str() {
            "choice" | "status" => {
                let mut options: Vec<String> = Vec::new();
                for option in &field.options {
                    let option = cut(option, 40);
                    if !option.is_empty() && !options.contains(&option) {
                        options.push(option);
                    }
                }
                if options.len() > 12 {
                    return Err(format!("{name} has at most 12 words to choose from."));
                }
                options
            }
            _ => Vec::new(),
        };
        if let Some(before) = before
            && before.name != name
        {
            renamed.push((before.name.clone(), name.clone()));
        }
        fields.push(Field {
            unit: if kind == "number" {
                cut(&field.unit, 12)
            } else {
                String::new()
            },
            goal: if kind == "number" { field.goal } else { None },
            name,
            kind,
            options,
        });
    }
    Ok((fields, renamed))
}

/// Fills a row's `cells` from what Claude gave, by the collection's
/// `fields`: an empty or `null` one goes.
fn fill(
    cells: &mut BTreeMap<String, Cell>,
    title: &str,
    fields: &[Field],
    given: &Map<String, Value>,
    photos: &[Uuid],
) -> Result<(), String> {
    for (name, value) in given {
        let Some(field) = fields
            .iter()
            .find(|f| f.name.to_lowercase() == name.trim().to_lowercase())
        else {
            let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
            return Err(match names.as_slice() {
                [] => format!("{title} has no fields yet: give it some with change_block."),
                names => format!(
                    "{title} has no field {name}: it has {}. Add one with change_block.",
                    names.join(", ")
                ),
            });
        };
        match cell(field, value, photos)? {
            Some(cell) => cells.insert(field.name.clone(), cell),
            None => cells.remove(&field.name),
        };
    }
    Ok(())
}

/// A row's cell in `field`, from what Claude wrote; none for an empty one.
pub fn cell(field: &Field, value: &Value, photos: &[Uuid]) -> Result<Option<Cell>, String> {
    let words = match value {
        Value::Null => return Ok(None),
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(yes) => yes.to_string(),
        _ => return Err(format!("{} is one value, not a list.", field.name)),
    };
    if words.is_empty() {
        return Ok(None);
    }
    let name = &field.name;
    Ok(Some(match field.kind.as_str() {
        "number" => {
            let plain = match value {
                Value::Number(n) => n.as_f64(),
                _ => {
                    let unit = field.unit.to_lowercase();
                    let lower = words.to_lowercase();
                    let bare = lower
                        .strip_suffix(unit.as_str())
                        .filter(|_| !unit.is_empty());
                    bare.unwrap_or(&lower)
                        .trim()
                        .replace(' ', "")
                        .replace(',', ".")
                        .parse()
                        .ok()
                }
            };
            match plain.filter(|n: &f64| n.is_finite()) {
                Some(n) => Cell::Number(n),
                None => return Err(format!("{name} is a number, plain (3.4), not {words}.")),
            }
        }
        "check" => match words.to_lowercase().as_str() {
            "yes" | "true" | "ja" | "y" | "x" => Cell::Check(true),
            "no" | "false" | "nein" | "n" => Cell::Check(false),
            _ => return Err(format!("{name} is yes or no, not {words}.")),
        },
        "day" => {
            if words.parse::<NaiveDate>().is_err() && !words.chars().any(|c| c.is_ascii_digit()) {
                return Err(format!(
                    "{name} is a day, as 2026-09-24, or a year or a span (1987–1993)."
                ));
            }
            Cell::Text(cut(&words, 60))
        }
        "choice" | "status" => {
            let words = cut(&words, 40);
            match field
                .options
                .iter()
                .find(|o| o.to_lowercase() == words.to_lowercase())
            {
                Some(option) => Cell::Text(option.clone()),
                None if field.options.is_empty() => Cell::Text(words),
                None => {
                    return Err(format!(
                        "{name} is one of {}: change the collection's fields to add {words}.",
                        field.options.join(", ")
                    ));
                }
            }
        }
        "photo" => {
            let number = words
                .to_lowercase()
                .trim_start_matches("photo")
                .trim()
                .trim_start_matches('#')
                .parse::<usize>()
                .ok();
            match number
                .and_then(|n| n.checked_sub(1))
                .and_then(|n| photos.get(n))
            {
                Some(id) => Cell::Text(id.to_string()),
                None if photos.iter().any(|id| id.to_string() == words) => Cell::Text(words),
                None if photos.is_empty() => {
                    return Err(format!("The talk has no photos for {name}."));
                }
                None => {
                    return Err(format!(
                        "{name} is a photo of the talk's, by its number: photo 1 to photo {}.",
                        photos.len()
                    ));
                }
            }
        }
        "link" if words.starts_with("https://") || words.starts_with("http://") => {
            Cell::Text(cut(&words, 2000))
        }
        "link" => return Err(format!("{name} is a web page's address, from https://.")),
        _ => Cell::Text(cut(&words, 200)),
    }))
}

/// The pictures a collection's rows can be drawn as, as View as offers them
/// (`apps/web/src/collection.ts`): none without rows.
pub fn pictures(fields: &[Field], rows: &[&PagePoint], today: NaiveDate) -> Vec<&'static str> {
    if rows.is_empty() {
        return Vec::new();
    }
    let filled: Vec<&Field> = fields
        .iter()
        .filter(|f| rows.iter().any(|r| r.cells.contains_key(&f.name)))
        .collect();
    let of = |kind: &str| filled.iter().find(|f| f.kind == kind).copied();
    let day_of = |row: &PagePoint, field: &Field| match row.cells.get(&field.name) {
        Some(Cell::Text(t)) => t.parse::<NaiveDate>().ok(),
        _ => None,
    };
    let number_of = |row: &PagePoint, field: &Field| {
        matches!(row.cells.get(&field.name), Some(Cell::Number(_)))
    };
    let (day, number) = (of("day"), of("number"));
    let line = day.zip(number).is_some_and(|(d, n)| {
        rows.iter()
            .any(|r| day_of(r, d).is_some() && number_of(r, n))
    });
    let pick = if line {
        "line"
    } else if of("status").is_some() {
        "board"
    } else if of("photo").is_some() {
        "cards"
    } else if let Some(day) = day {
        if rows
            .iter()
            .any(|r| day_of(r, day).is_some_and(|d| d >= today))
        {
            "week"
        } else {
            "timeline"
        }
    } else if number.is_some() {
        "bars"
    } else if filled.len() + 1 >= 3 {
        "table"
    } else {
        "list"
    };
    let mut fits = vec![pick, "cards", "table", "list"];
    if line {
        fits.push("line");
    }
    if of("status").is_some() || of("choice").is_some() {
        fits.push("board");
    }
    if let Some(day) = day {
        fits.push("timeline");
        if rows.iter().any(|r| day_of(r, day).is_some()) {
            fits.push("week");
        }
    }
    if number.is_some() {
        fits.push("bars");
    }
    [
        "line", "board", "cards", "timeline", "week", "bars", "table", "list",
    ]
    .into_iter()
    .filter(|p| fits.contains(p))
    .collect()
}

/// What changes on a reminder ticked for its day `due_on`: one that comes
/// round is due again a rhythm on, from today when it was overdue; one
/// that doesn't is done.
fn done_for(due_on: NaiveDate, every_days: Option<i32>, today: NaiveDate) -> Map<String, Value> {
    match every_days.filter(|d| *d > 0) {
        Some(days) => {
            let from = if due_on > today { due_on } else { today };
            fields([
                ("due_on", json!(from + Days::new(days as u64))),
                ("last_done_on", json!(due_on)),
            ])
        }
        None => fields([("status", json!("done")), ("last_done_on", json!(due_on))]),
    }
}

fn block_was(block: &RefBlock, removed: bool) -> BlockWas {
    BlockWas {
        id: block.id,
        kind: block.kind.clone(),
        title: block.title.clone(),
        facets: block.facets.0.clone(),
        width: block.width.clone(),
        view: block.view.clone(),
        fields: block.fields.0.clone(),
        position: block.position,
        removed,
    }
}

fn point_was(point: &PagePoint, removed: bool) -> PointWas {
    PointWas {
        id: point.id,
        block_id: point.block_id,
        theme: point.theme.clone(),
        text: point.text.clone(),
        happened: point.happened.clone(),
        facet: point.facet.clone(),
        look: point.look.clone(),
        amount: point.amount,
        note: point.note.clone(),
        cells: point.cells.0.clone(),
        done: point.done,
        position: point.position,
        removed,
        source: point.source.clone(),
        source_url: point.source_url.clone(),
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

/// A reminder an answer offers, waiting for a yes under it, in place of the
/// reminders it `replaces` once it's taken, ringing for a checklist item
/// when it has one.
pub fn offer(
    topic_id: Uuid,
    message_id: Uuid,
    reminder: &ProposedReminder,
    replaces: &[Uuid],
    point_id: Option<Uuid>,
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
            ("point_id", json!(point_id)),
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

/// "Kept 2 points under Monstera and Basilikum."
pub fn kept_words(points: usize, unders: &[String]) -> String {
    let under = match unders {
        [] => String::new(),
        [one] => format!(" under {one}"),
        [rest @ .., last] => format!(" under {} and {last}", rest.join(", ")),
    };
    format!("Kept {}{under}.", count(points, "point"))
}

/// "Ticked 2 points off.", "Ticked a point on again."
fn ticked_words(ticks: &[(Uuid, bool)]) -> Option<String> {
    let words =
        |n: usize, how: &str| (n > 0).then(|| format!("Ticked {} {how}.", count(n, "point")));
    let done = ticks.iter().filter(|(_, d)| *d).count();
    let said: Vec<String> = [words(done, "off"), words(ticks.len() - done, "on again")]
        .into_iter()
        .flatten()
        .collect();
    (!said.is_empty()).then(|| said.join(" "))
}

/// "Changed 2 points, moved a block and removed Gewicht.", from how many
/// blocks and points changed and moved, and what went; none when nothing
/// did.
fn change_words(
    changed: (usize, usize),
    moved: (usize, usize),
    removed_blocks: &[String],
    removed_points: usize,
) -> Option<String> {
    let things = |(blocks, points): (usize, usize)| {
        let said: Vec<String> = [(blocks, "block"), (points, "point")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, noun)| count(n, noun))
            .collect();
        (!said.is_empty()).then(|| said.join(" and "))
    };
    let mut gone: Vec<String> = removed_blocks.to_vec();
    if removed_points > 0 {
        gone.push(count(removed_points, "point"));
    }
    let parts: Vec<String> = [
        things(changed).map(|t| format!("changed {t}")),
        things(moved).map(|t| format!("moved {t}")),
        (!gone.is_empty()).then(|| format!("removed {}", and(&gone))),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        return None;
    }
    let said = and(&parts);
    let mut chars = said.chars();
    let first = chars
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    Some(format!("{first}{}.", chars.as_str()))
}

/// "a point", "3 points".
fn count(n: usize, noun: &str) -> String {
    match n {
        1 => format!("a {noun}"),
        n => format!("{n} {noun}s"),
    }
}

/// "A, B and C".
fn and(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

pub fn fields<const N: usize>(pairs: [(&str, Value); N]) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::PageReminder;

    fn answering() -> Answering {
        Answering {
            topic_id: Uuid::now_v7(),
            topic_name: "Katzen".into(),
            talk_id: Uuid::now_v7(),
            answer_id: Uuid::now_v7(),
            first: false,
        }
    }

    fn block(kind: &str, title: &str, position: i32) -> RefBlock {
        RefBlock {
            id: Uuid::now_v7(),
            kind: kind.into(),
            title: title.into(),
            facets: Json(Vec::new()),
            fields: Json(Vec::new()),
            width: String::new(),
            view: String::new(),
            position,
        }
    }

    fn point(block: &RefBlock, theme: &str, text: &str, position: i32) -> PagePoint {
        PagePoint {
            id: Uuid::now_v7(),
            block_id: block.id,
            theme: theme.into(),
            text: text.into(),
            happened: String::new(),
            facet: String::new(),
            look: String::new(),
            amount: None,
            note: String::new(),
            cells: Json(BTreeMap::new()),
            done: false,
            position,
            source: None,
            source_url: None,
        }
    }

    fn item(text: &str) -> ItemArg {
        ItemArg {
            text: text.into(),
            ..ItemArg::default()
        }
    }

    fn field(name: &str, kind: &str) -> FieldArg {
        FieldArg {
            name: name.into(),
            kind: kind.into(),
            ..FieldArg::default()
        }
    }

    fn row(name: &str, cells: Value) -> RowArg {
        RowArg {
            name: name.into(),
            cells: cells.as_object().cloned().unwrap_or_default(),
            ..RowArg::default()
        }
    }

    #[test]
    fn a_weighing_is_a_row_with_typed_cells() {
        let mut kept = Kept::on(Refs {
            blocks: vec![block("bullets", "Notes", 0)],
            ..Refs::default()
        });
        let weight = kept
            .add_block(
                "collection",
                "Gewicht",
                None,
                &[
                    field("Tag", "day"),
                    FieldArg {
                        unit: "kg".into(),
                        goal: Some(3.4),
                        ..field("Gewicht", "number")
                    },
                ],
            )
            .unwrap();
        assert_eq!(weight, "b2");
        assert!(
            kept.add_items("b2", &[item("Lisa wiegt 3,5 kg")])
                .unwrap_err()
                .contains("add rows")
        );
        assert!(
            kept.add_rows("b2", &[row("Lisa", json!({"Größe": "30 cm"}))])
                .unwrap_err()
                .contains("has Tag, Gewicht")
        );
        assert!(
            kept.add_rows("b2", &[row("Lisa", json!({"Gewicht": "schwer"}))])
                .unwrap_err()
                .contains("a number, plain")
        );
        let rows = kept
            .add_rows(
                "gewicht",
                &[
                    row("Lisa", json!({"tag": "2026-10-20", "Gewicht": "3,5 kg"})),
                    row("Lisa", json!({"Tag": "2026-10-27", "Gewicht": 3.45})),
                ],
            )
            .unwrap();
        assert_eq!(rows, ["p1", "p2"]);
        let (said, operations) = kept.writes(&answering());
        let [
            Operation::Create {
                model: ModelKind::Block,
                id,
                data: made,
            },
            Operation::Create { data: first, .. },
            Operation::Create { data: second, .. },
        ] = &operations[..]
        else {
            panic!("{operations:?}");
        };
        assert_eq!(made["position"], json!(1));
        assert_eq!(made["fields"][1]["unit"], json!("kg"));
        assert_eq!(first["block_id"], json!(id));
        assert_eq!(first["cells"], json!({"Tag": "2026-10-20", "Gewicht": 3.5}));
        assert_eq!(second["position"], json!(1));
        assert_eq!(said["kept"], json!("Kept 2 points under Gewicht."));
        assert!(said.get("changes").is_none(), "only new things");
    }

    #[test]
    fn a_toggle_keeps_points_under_their_line() {
        let terms = block("toggle", "Begriffe", 0);
        let first = point(&terms, "Intifada", "Ein Aufstand.", 0);
        let other = point(&terms, "Oslo", "Ein Abkommen.", 1);
        let mut kept = Kept::on(Refs {
            blocks: vec![terms],
            points: vec![first.clone(), other.clone()],
            ..Refs::default()
        });
        assert!(
            kept.add_items("b1", &[item("1987 begonnen")])
                .unwrap_err()
                .contains("needs a theme: the line it opens from")
        );
        kept.add_items(
            "b1",
            &[ItemArg {
                theme: "Intifada".into(),
                ..item("Sie begann 1987.")
            }],
        )
        .unwrap();
        // It goes after its theme's point, and the next moves down.
        let (_, operations) = kept.writes(&answering());
        let [
            Operation::Update {
                id, data: moved, ..
            },
            Operation::Create { data, .. },
        ] = &operations[..]
        else {
            panic!("{operations:?}");
        };
        assert_eq!(data["position"], json!(1));
        assert_eq!(*id, other.id);
        assert_eq!(moved["position"], json!(2));
    }

    #[test]
    fn changes_and_removals_are_one_change_to_undo() {
        let (notes, old) = (block("bullets", "Notes", 0), block("cards", "Pflanzen", 1));
        let fact = point(&notes, "", "Lisa wiegt 3,5 kg", 0);
        let monstera = point(&old, "Monstera", "Hell", 0);
        let mut kept = Kept::on(Refs {
            blocks: vec![notes.clone(), old.clone()],
            points: vec![fact.clone(), monstera.clone()],
            ..Refs::default()
        });
        kept.change_points(&[PointChange {
            point: "p1".into(),
            text: Some("Lisa wiegt 3,4 kg".into()),
            ..PointChange::default()
        }])
        .unwrap();
        kept.remove(&["b2".into()]).unwrap();
        assert!(
            kept.change_points(&[PointChange {
                point: "p2".into(),
                text: Some("x".into()),
                ..PointChange::default()
            }])
            .unwrap_err()
            .contains("was removed")
        );
        let (said, operations) = kept.writes(&answering());
        assert!(
            matches!(
                &operations[..],
                [
                    Operation::Update { id: a, data, .. },
                    Operation::Delete { id: b, model: ModelKind::Point },
                    Operation::Delete { id: c, model: ModelKind::Block },
                ] if *a == fact.id && data["seen"] == json!(false) && *b == monstera.id && *c == old.id
            ),
            "{operations:?}"
        );
        let change = &said["changes"][0];
        assert_eq!(
            change["said"],
            json!("Changed a point and removed Pflanzen.")
        );
        let layout: PageLayout = serde_json::from_value(change["layout"].clone()).unwrap();
        assert!(
            layout
                .points
                .iter()
                .any(|p| p.id == monstera.id && p.removed)
        );
        assert!(layout.blocks.iter().any(|b| b.id == old.id && b.removed));
    }

    #[test]
    fn moving_points_renumbers_their_block() {
        let (a, b) = (block("bullets", "A", 0), block("checklist", "B", 1));
        let points = vec![
            point(&a, "", "eins", 0),
            point(&a, "", "zwei", 1),
            point(&b, "", "drei", 0),
        ];
        let mut kept = Kept::on(Refs {
            blocks: vec![a, b.clone()],
            points: points.clone(),
            ..Refs::default()
        });
        kept.move_points(&["p1".into()], "b2", Some("")).unwrap();
        let (said, operations) = kept.writes(&answering());
        assert!(
            matches!(
                &operations[..],
                [
                    Operation::Update { id: one, data, .. },
                    Operation::Update { id: three, data: down, .. },
                ] if *one == points[0].id && data["block_id"] == json!(b.id)
                    && data.get("seen").is_none()
                    && *three == points[2].id && down["position"] == json!(1)
            ),
            "{operations:?}"
        );
        assert_eq!(said["changes"][0]["said"], json!("Moved a point."));
    }

    #[test]
    fn a_ticked_item_ticks_its_reminder() {
        let list = block("checklist", "Routine", 0);
        let (wash, bin) = (
            point(&list, "", "Haare waschen", 0),
            point(&list, "", "Müll", 1),
        );
        let reminder = PageReminder {
            id: Uuid::now_v7(),
            title: "Haare waschen".into(),
            due_on: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
            every_days: Some(3),
            status: "on".into(),
            point_id: Some(wash.id),
            last_done_on: None,
        };
        let mut kept = Kept::on(Refs {
            blocks: vec![list],
            points: vec![wash, bin.clone()],
            reminders: vec![reminder.clone()],
        });
        kept.today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        kept.tick(&["p1".into(), "p2".into()], true).unwrap();
        let (said, operations) = kept.writes(&answering());
        assert!(
            matches!(
                &operations[..],
                [
                    Operation::Update { id: item, data: done, .. },
                    Operation::Update { model: ModelKind::Reminder, id, data },
                ] if *item == bin.id && done["done"] == json!(true)
                    && *id == reminder.id
                    && data["due_on"] == json!("2026-11-03")
                    && data["last_done_on"] == json!("2026-10-30")
            ),
            "{operations:?}"
        );
        assert_eq!(said["kept"], json!("Ticked 2 points off."));
        assert!(said.get("changes").is_none());
    }

    #[test]
    fn a_reminder_rings_for_a_new_item() {
        let mut kept = Kept::on(Refs {
            blocks: vec![
                block("checklist", "Routine", 0),
                block("bullets", "Notes", 1),
            ],
            ..Refs::default()
        });
        let [wash] = &kept.add_items("b1", &[item("Haare waschen")]).unwrap()[..] else {
            panic!();
        };
        let wash = wash.clone();
        kept.add_items("b2", &[item("Niedrige Porosität")]).unwrap();
        let friday = reminder("", "Haare waschen", "2026-10-30", "", Some(3)).unwrap();
        assert!(
            kept.offer_reminder(friday.clone(), &[], Some("p2"))
                .unwrap_err()
                .contains("a checklist's item")
        );
        kept.offer_reminder(friday, &[], Some(&wash)).unwrap();
        assert_eq!(kept.item_words(&wash).as_deref(), Some("Haare waschen"));
        let (_, operations) = kept.writes(&answering());
        let Operation::Create { id, .. } = &operations[0] else {
            panic!();
        };
        assert!(matches!(
            &operations[2],
            Operation::Create { model: ModelKind::Reminder, data, .. } if data["point_id"] == json!(id)
        ));
    }

    #[test]
    fn a_collection_picks_among_the_pictures_its_fields_allow() {
        let mut kept = Kept::on(Refs::default());
        kept.add_block(
            "collection",
            "Angebote",
            None,
            &[field("Status", "status"), field("Preis", "number")],
        )
        .unwrap();
        assert!(
            kept.change_block("b1", None, None, Some("board"))
                .unwrap_err()
                .contains("no rows yet")
        );
        kept.add_rows(
            "b1",
            &[row("Axio", json!({"Status": "Offen", "Preis": 709}))],
        )
        .unwrap();
        assert!(
            kept.change_block("b1", None, None, Some("line"))
                .unwrap_err()
                .contains("board, cards, bars, table, list")
        );
        kept.change_block("b1", None, None, Some("bars")).unwrap();
        // A renamed field keeps its cells; one left out takes them with it.
        kept.change_block(
            "b1",
            None,
            Some(&[FieldArg {
                was: "Preis".into(),
                unit: "kr".into(),
                ..field("Kosten", "number")
            }]),
            None,
        )
        .unwrap();
        let (_, operations) = kept.writes(&answering());
        assert!(
            matches!(
                &operations[..],
                [Operation::Create { data: b, .. }, Operation::Create { data: p, .. }]
                    if b["view"] == json!("bars") && p["cells"] == json!({"Kosten": 709.0})
            ),
            "{operations:?}"
        );
    }

    #[test]
    fn old_blocks_take_only_what_fits_them() {
        let mut kept = Kept::on(Refs {
            blocks: vec![
                block("timeline", "Zeit", 0),
                block("list", "Lesen", 1),
                block("heading", "Teil", 2),
            ],
            ..Refs::default()
        });
        assert!(
            kept.add_items("b1", &[item("1987")])
                .unwrap_err()
                .contains("from before the set")
        );
        assert!(
            kept.add_items("b3", &[item("x")])
                .unwrap_err()
                .contains("a heading")
        );
        kept.add_items("Lesen", &[item("Ward, Plant care")])
            .unwrap();
        assert!(
            kept.add_block("table", "Schuhe", None, &[])
                .unwrap_err()
                .contains("no kind of block table")
        );
    }

    #[test]
    fn cells_take_what_their_field_holds() {
        let photos = [Uuid::now_v7(), Uuid::now_v7()];
        let check = |kind: &str, value: Value| {
            let field = Field {
                name: "F".into(),
                kind: kind.into(),
                unit: "kg".into(),
                goal: None,
                options: if kind == "status" {
                    vec!["Offen".into(), "Erledigt".into()]
                } else {
                    vec![]
                },
            };
            cell(&field, &value, &photos)
        };
        assert_eq!(
            check("number", json!("3,4 kg")),
            Ok(Some(Cell::Number(3.4)))
        );
        assert_eq!(check("check", json!("ja")), Ok(Some(Cell::Check(true))));
        assert_eq!(
            check("status", json!("erledigt")),
            Ok(Some(Cell::Text("Erledigt".into())))
        );
        assert!(
            check("status", json!("Bestellt"))
                .unwrap_err()
                .contains("Offen, Erledigt")
        );
        assert_eq!(
            check("photo", json!("photo 2")),
            Ok(Some(Cell::Text(photos[1].to_string())))
        );
        assert!(check("photo", json!("photo 3")).is_err());
        assert!(check("day", json!("gestern")).is_err());
        assert_eq!(
            check("day", json!("1987–1993")),
            Ok(Some(Cell::Text("1987–1993".into())))
        );
        assert!(check("link", json!("example.com")).is_err());
        assert_eq!(check("text", json!("")), Ok(None));
    }

    #[test]
    fn kept_words_name_where() {
        assert_eq!(
            kept_words(1, &["Monstera".into()]),
            "Kept a point under Monstera."
        );
        assert_eq!(
            kept_words(3, &["Monstera".into(), "Basilikum".into(), "Efeu".into()]),
            "Kept 3 points under Monstera, Basilikum and Efeu."
        );
        assert_eq!(
            change_words((1, 2), (0, 0), &["Gewicht".into()], 1).as_deref(),
            Some("Changed a block and 2 points and removed Gewicht and a point.")
        );
    }

    #[test]
    fn a_reminder_replaces_one_on_the_calendar_once_its_taken() {
        let reminder_of = |status: &str| PageReminder {
            id: Uuid::now_v7(),
            title: "Wash".into(),
            due_on: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
            every_days: None,
            status: status.into(),
            point_id: None,
            last_done_on: None,
        };
        let (on, offered) = (reminder_of("on"), reminder_of("offered"));
        let mut kept = Kept::on(Refs {
            reminders: vec![on.clone(), offered.clone()],
            ..Refs::default()
        });
        let friday = reminder("Wäsche", "Elvital wash", "2026-10-30", "", Some(7)).unwrap();
        kept.offer_reminder(friday, &["r1".into(), "r2".into()], None)
            .unwrap();
        let (_, operations) = kept.writes(&answering());
        let [
            Operation::Delete { id: gone, .. },
            Operation::Create { data, .. },
        ] = &operations[..]
        else {
            panic!("{operations:?}");
        };
        assert_eq!(*gone, offered.id);
        assert_eq!(data["replaces"], json!([on.id]));
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
