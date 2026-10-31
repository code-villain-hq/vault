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

use std::collections::{BTreeMap, HashSet};

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
use crate::topics::{Left, cut, left_behind};

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
    /// Rows ticked done, or not.
    ticks: Vec<(Uuid, bool)>,
    /// Rows whose check a tick sets, with what it sets: a tick isn't a
    /// change to undo.
    checked: Vec<(Uuid, bool)>,
    /// Reminders an item's tick ticks, with what changes on them.
    rung: Vec<(Uuid, Map<String, Value>)>,
    /// The photos of the talk, in order, which a photo cell names by number,
    /// then the pictures it fetched from the web.
    pub photos: Vec<Uuid>,
    /// The photo the topic's page opens with from now on.
    cover: Option<Uuid>,
    /// For when a ticked reminder comes round again.
    pub today: NaiveDate,
    pub questions: Vec<KeptQuestion>,
    pub reminders: Vec<KeptReminder>,
    /// The topic an off-topic question fits better, by name.
    pub elsewhere: Option<String>,
    pub talk_title: Option<String>,
    pub proposal: Option<Proposal>,
    /// The sub-topics it proposes, each started on its own.
    pub sub_topics: Vec<Proposal>,
    pub changes: Vec<TopicChange>,
    /// What it made in this topic's sub-topics, moved there from its page.
    pub in_sub_topics: Vec<Operation>,
    /// What it moved there, which isn't removed in words.
    moved_off: Vec<Uuid>,
    /// Copies of its sub-topics' pages it changes, each written with this
    /// one and undone with it.
    pub sub_pages: Vec<SubPage>,
}

/// A sub-topic's page, as one answer of its parent's talk changes it.
#[derive(Debug)]
pub struct SubPage {
    pub topic_id: Uuid,
    pub name: String,
    pub kept: Kept,
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

/// An item as Claude adds it: a paragraph, a callout with its look.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct ItemArg {
    pub text: String,
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
    /// Only an answer in a talk without a name names it: its first, or a
    /// later one when the first didn't.
    pub unnamed: bool,
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
            checked: Vec::new(),
            rung: Vec::new(),
            photos: Vec::new(),
            cover: None,
            today: Local::now().date_naive(),
            questions: Vec::new(),
            reminders: Vec::new(),
            elsewhere: None,
            talk_title: None,
            proposal: None,
            sub_topics: Vec::new(),
            changes: Vec::new(),
            in_sub_topics: Vec::new(),
            moved_off: Vec::new(),
            sub_pages: Vec::new(),
        }
    }

    /// Starts a block of the set, on the topic's page or on a `row`'s own
    /// (`p3`), `after` the one it names ("" for the top; the end when left
    /// out), with a collection's fields. It answers with the block's
    /// reference. A block like it the page has already is the one it
    /// answers with.
    pub fn add_block(
        &mut self,
        kind: &str,
        title: &str,
        after: Option<&str>,
        fields: &[FieldArg],
        row: Option<&str>,
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
        let row_id = match row.map(str::trim).filter(|r| !r.is_empty()) {
            Some(reference) => {
                let i = self.point_at(reference)?;
                let b = self.block_of(i);
                if self.blocks[b].kind != "collection" {
                    return Err(format!(
                        "{} isn't a row: only a collection's rows have pages of their own.",
                        reference.trim()
                    ));
                }
                Some(self.points[i].id)
            }
            None => None,
        };
        if let Some(n) = (0..self.blocks.len()).find(|&n| {
            let b = &self.blocks[n];
            !self.is_gone(b.id)
                && b.row_id == row_id
                && b.kind == kind
                && b.title.to_lowercase() == title.to_lowercase()
        }) {
            return Ok(format!("b{}", n + 1));
        }
        if self.blocks.len() - self.refs.blocks.len() >= MOST_BLOCKS {
            return Err(format!(
                "An answer starts at most {MOST_BLOCKS} blocks: keep the rest in the page's own."
            ));
        }
        let at = self.block_at_place(after)?;
        if let At::After(a) = at
            && self.blocks[a].row_id != row_id
        {
            return Err(format!(
                "{} is on another page: name a block on the same page, or leave after out.",
                after.unwrap_or_default().trim()
            ));
        }
        self.blocks.push(RefBlock {
            id: Uuid::now_v7(),
            kind,
            title,
            facets: Json(Vec::new()),
            fields: Json(fields),
            width: String::new(),
            view: String::new(),
            row_id,
            position: 0,
        });
        let n = self.blocks.len() - 1;
        self.put_block(n, at);
        Ok(format!("b{}", n + 1))
    }

    /// Renames a block, gives a collection its fields as they are from now
    /// on (one renamed says the name it `was`; one left out goes, with its
    /// cells), or picks another of the pictures its fields can draw. A field
    /// of another kind reads its cells anew; it answers with the rows whose
    /// words it couldn't read, which it left empty. A new kind of field
    /// leaves the picture to the app, unless one is picked with it.
    pub fn change_block(
        &mut self,
        block: &str,
        title: Option<&str>,
        fields: Option<&[FieldArg]>,
        view: Option<&str>,
    ) -> Result<String, String> {
        let n = self.block_at(block)?;
        let kind = self.blocks[n].kind.clone();
        if title.is_none() && fields.is_none() && view.is_none() {
            return Err("Say what changes: its title, its fields or its picture.".into());
        }
        self.touch(self.blocks[n].id)?;
        let mut unread: Vec<String> = Vec::new();
        if let Some(title) = title {
            let title = cut(title, 60);
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
            // The fields as they were, to tell a cell whose kind changed.
            let kinds: Vec<(String, String)> = self.blocks[n]
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.kind.clone()))
                .collect();
            for point in self.points.iter_mut().filter(|p| p.block_id == id) {
                let mut cells = BTreeMap::new();
                for (was, cell) in std::mem::take(&mut point.cells.0) {
                    let name = renamed
                        .iter()
                        .find(|(old, _)| *old == was)
                        .map_or(was.clone(), |(_, now)| now.clone());
                    let Some(field) = fields.iter().find(|f| f.name == name) else {
                        continue;
                    };
                    let same = kinds.iter().any(|(n, k)| *n == was && *k == field.kind);
                    if same {
                        cells.insert(name, cell);
                        continue;
                    }
                    let value = match &cell {
                        Cell::Check(yes) => json!(yes),
                        Cell::Number(n) => json!(n),
                        Cell::Text(words) => json!(words),
                    };
                    match self::cell(field, &value, &self.photos) {
                        Ok(Some(read)) => {
                            cells.insert(name, read);
                        }
                        Ok(None) => {}
                        Err(_) => unread.push(format!("{}: {name} was {value}", point.text)),
                    }
                }
                point.cells = Json(cells);
            }
            // A picture picked before a new kind of field came gives way to
            // the app's pick, unless one comes with it.
            let new_kind = fields
                .iter()
                .any(|f| !kinds.iter().any(|(_, kind)| *kind == f.kind));
            if new_kind && view.is_none() {
                self.blocks[n].view = String::new();
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
        Ok(match unread.as_slice() {
            [] => String::new(),
            rows => format!(
                "These cells don't read as their field's kind now, so they're empty: {}. Give \
                 each its cell with change_points, and what else it said a text field of its own.",
                rows.join("; ")
            ),
        })
    }

    /// Adds items to a block: paragraphs, or a callout's. It answers with
    /// their references. One the block has already is the one it answers
    /// with.
    pub fn add_items(&mut self, block: &str, items: &[ItemArg]) -> Result<Vec<String>, String> {
        let n = self.block_at(block)?;
        let (kind, title) = (self.blocks[n].kind.clone(), self.title(n).to_string());
        takes_items(&kind, &title)?;
        if items.is_empty() {
            return Err("Give the items to add.".into());
        }
        let mut refs = Vec::new();
        for item in items {
            let text = cut(&item.text, 2000);
            if text.is_empty() {
                return Err("An item needs its words.".into());
            }
            as_prose(&kind, &text)?;
            let look = self.look(&kind, &title, &item.look)?;
            refs.push(self.add_point(
                n,
                String::new(),
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
                as_prose(&kind, &text)?;
                self.points[i].text = text;
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

    /// Ticks rows done, or not: their collection's check. A row a reminder
    /// rings for ticks the reminder: done for now, it's due again on its
    /// next day.
    pub fn tick(&mut self, items: &[String], done: bool) -> Result<(), String> {
        if items.is_empty() {
            return Err("Name the items to tick, like p3.".into());
        }
        for reference in items {
            let i = self.point_at(reference)?;
            let b = self.block_of(i);
            if check_of(&self.blocks[b]).is_none() {
                return Err(format!(
                    "{} is in {}, which has no check: only rows with a check are ticked.",
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
                None => {
                    self.checked.retain(|(c, _)| *c != id);
                    self.checked.push((id, done));
                }
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
        if let At::After(a) = at
            && self.blocks[a].row_id != self.blocks[n].row_id
        {
            return Err(format!(
                "{} is on another page: a block moves among those on its own page.",
                after.trim()
            ));
        }
        let id = self.blocks[n].id;
        self.touch(id)?;
        self.moved.push(id);
        self.put_block(n, at);
        Ok(())
    }

    /// Moves points `to` a block, `after` one of its points ("" for the
    /// top; the end when left out), in the order given.
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
        if kind != "collection" {
            takes_items(&kind, &title)?;
        }
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
            self.points[i].theme = String::new();
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
            self.take_off(id);
        }
        Ok(())
    }

    /// Removes blocks and rows by id, as `remove` does by reference.
    pub fn remove_ids(&mut self, ids: &[Uuid]) -> Result<(), String> {
        for &id in ids {
            self.touch(id)?;
            self.take_off(id);
        }
        Ok(())
    }

    /// Takes what moves to a sub-topic off the copy, and what the rows that
    /// move leave behind with it (`topics::left_behind`): a block no row
    /// stays in, a choice's lanes none does.
    pub fn move_off(&mut self, ids: &[Uuid]) -> Result<(), String> {
        let went: Vec<(Uuid, Value)> = self
            .points
            .iter()
            .filter(|p| ids.contains(&p.id))
            .map(|p| (p.block_id, json!(p.cells.0)))
            .collect();
        self.remove_ids(ids)?;
        self.moved_off.extend(ids);
        let mut from: Vec<Uuid> = went.iter().map(|(block, _)| *block).collect();
        from.dedup();
        for block in from {
            let Some(n) = self.blocks.iter().position(|b| b.id == block) else {
                continue;
            };
            if self.is_gone(block) {
                continue;
            }
            let taken: Vec<Value> = went
                .iter()
                .filter(|(b, _)| *b == block)
                .map(|(_, cells)| cells.clone())
                .collect();
            let stay: Vec<Value> = self
                .points
                .iter()
                .filter(|p| p.block_id == block && !self.is_gone(p.id))
                .map(|p| json!(p.cells.0))
                .collect();
            match left_behind(&self.blocks[n].kind, &self.blocks[n].fields, &taken, &stay) {
                Left::As => {}
                Left::Empty => self.remove_ids(&[block])?,
                Left::Lanes(lanes) => {
                    self.touch(block)?;
                    self.blocks[n].fields = Json(lanes);
                }
            }
        }
        Ok(())
    }

    /// Takes a block or point off the copy, and with a row the blocks on
    /// its page, and theirs.
    fn take_off(&mut self, id: Uuid) {
        if self.is_gone(id) {
            return;
        }
        self.gone.push(id);
        let on_page: Vec<Uuid> = self
            .blocks
            .iter()
            .filter(|b| b.row_id == Some(id))
            .map(|b| b.id)
            .collect();
        let held: Vec<Uuid> = self
            .points
            .iter()
            .filter(|p| p.block_id == id)
            .map(|p| p.id)
            .collect();
        for id in on_page.into_iter().chain(held) {
            self.take_off(id);
        }
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
                if check_of(&self.blocks[b]).is_none() {
                    return Err(format!(
                        "{item} is in {}: a reminder rings for a row with a check.",
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

    /// Makes one of the talk's photos, "photo 2", the topic's cover.
    pub fn set_cover(&mut self, photo: &str) -> Result<(), String> {
        let n: usize = photo
            .trim()
            .trim_start_matches("photo")
            .trim()
            .parse()
            .map_err(|_| "Name the photo by its number: photo 1.".to_string())?;
        let id = n
            .checked_sub(1)
            .and_then(|n| self.photos.get(n))
            .ok_or_else(|| format!("The talk has no photo {n}."))?;
        self.cover = Some(*id);
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
            collections: Vec::new(),
            rows: Vec::new(),
        };
        let mut shown: Vec<Uuid> = Vec::new();
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
                            ("row_id", json!(block.row_id)),
                            ("position", json!(block.position)),
                        ]),
                    });
                    layout.made_blocks.push(block.id);
                    shown.push(block.id);
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
                shown.push(block.id);
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
        // Sentences tidied into a collection's rows, by the collection.
        let mut rows_made: Vec<(String, usize)> = Vec::new();
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
                    shown.push(point.block_id);
                    if self.kind_of(point.block_id) == "collection" {
                        layout.rows.push(point.id);
                    }
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
                if !in_gone && !self.moved_off.contains(&point.id) {
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
            let into_rows = point.block_id != was.block_id
                && self.kind_of(point.block_id) == "collection"
                && self.kind_of(was.block_id) != "collection";
            if reworded {
                data.insert("seen".into(), json!(false));
                if !into_rows {
                    changed_points += 1;
                }
                shown.push(point.block_id);
            }
            if point.block_id != was.block_id {
                data.insert("block_id".into(), json!(point.block_id));
                shown.push(point.block_id);
            }
            if point.position != was.position {
                data.insert("position".into(), json!(point.position));
            }
            if (reworded || data.contains_key("block_id"))
                && self.kind_of(point.block_id) == "collection"
            {
                layout.rows.push(point.id);
            }
            if into_rows {
                let title = self.title_of(point.block_id).to_string();
                match rows_made.iter_mut().find(|(t, _)| *t == title) {
                    Some((_, n)) => *n += 1,
                    None => rows_made.push((title, 1)),
                }
            } else if self.moved.contains(&point.id) && data.contains_key("block_id")
                || self.moved.contains(&point.id) && data.contains_key("position")
            {
                moved_points += 1;
            }
            // A tick isn't a change to undo: it shows with what's kept.
            if !data.is_empty() {
                layout.points.push(point_was(was, false));
            }
            if let Some((_, done)) = self.checked.iter().find(|(c, _)| *c == point.id) {
                let block = self.blocks.iter().find(|b| b.id == point.block_id);
                if let Some(check) = block.and_then(check_of) {
                    let mut cells = point.cells.0.clone();
                    cells.insert(check.to_string(), Cell::Check(*done));
                    if cells != was.cells.0 {
                        data.insert("cells".into(), json!(cells));
                    }
                }
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
                // What moved to a sub-topic says so there, and a row's page
                // goes with its row.
                let along = was.row_id.is_some_and(|row| self.is_gone(row));
                if !along && !self.moved_off.contains(&block.id) {
                    removed_blocks.push(match was.title.as_str() {
                        "" => format!("the {}", was.kind),
                        title => title.to_string(),
                    });
                }
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
        if let Some(cover) = self.cover {
            operations.push(Operation::Update {
                model: ModelKind::Topic,
                id: at.topic_id,
                data: fields([("cover", json!(cover))]),
            });
        }
        operations.extend(self.in_sub_topics.iter().cloned());
        for change in &self.changes {
            let Value::Object(data) = patch_fields(&change.after) else {
                continue;
            };
            if data.is_empty() {
                continue;
            }
            operations.push(Operation::Update {
                model: ModelKind::Topic,
                id: change.topic_id,
                data,
            });
        }
        if at.unnamed
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
        if let Some(ticked) = ticked_words(&self.ticks) {
            message.insert("kept".into(), json!(ticked));
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
        if !self.sub_topics.is_empty() {
            message.insert("sub_topics".into(), json!(self.sub_topics));
        }
        let mut changes = self.changes.clone();
        for sub in &self.sub_pages {
            let at = Answering {
                topic_id: sub.topic_id,
                topic_name: sub.name.clone(),
                unnamed: false,
                ..*at
            };
            let (said, written) = sub.kept.writes(&at);
            operations.extend(written);
            if let Some(Ok(theirs)) = said
                .get("changes")
                .map(|c| serde_json::from_value::<Vec<TopicChange>>(c.clone()))
            {
                changes.extend(theirs);
            }
        }
        let said: Vec<String> = [
            self.cover.map(|_| "Gave the page a cover.".to_string()),
            (added > 0).then(|| kept_words(added, &unders)),
            rows_words(&rows_made),
            change_words(
                (changed_blocks, changed_points),
                (moved_blocks, moved_points),
                &removed_blocks,
                removed_points,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !said.is_empty() {
            let said = said.join(" ");
            let mut collections: Vec<_> = self
                .blocks
                .iter()
                .filter(|b| b.kind == "collection" && shown.contains(&b.id) && !self.is_gone(b.id))
                .collect();
            collections.sort_by_key(|b| b.position);
            layout.collections = collections.into_iter().map(|b| b.id).collect();
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
    /// What a sub-topic would take from this page, by reference: blocks
    /// on the topic's own page (`b2`) and rows with a page of their own
    /// (`p3`), as the page has them before this answer, with their names.
    pub fn taken(&self, references: &[String]) -> Result<Vec<(Uuid, String)>, String> {
        let mut ids = Vec::new();
        for reference in references {
            let named = reference.trim().trim_matches(['[', ']']);
            let id = if !named.starts_with('b') {
                let point = &self.points[self.point_at(named)?];
                if !self.refs.points.iter().any(|p| p.id == point.id) {
                    return Err(format!(
                        "{named} came in this answer: a sub-topic takes what the page had."
                    ));
                }
                let has_page = self.blocks.iter().any(|b| b.row_id == Some(point.id));
                if !has_page {
                    return Err(format!(
                        "{named} has no page of its own: take its block, or copy it as points."
                    ));
                }
                (point.id, point.text.clone())
            } else {
                let block = &self.blocks[self.block_at(named)?];
                if !self.refs.blocks.iter().any(|b| b.id == block.id) {
                    return Err(format!(
                        "{named} came in this answer: a sub-topic takes what the page had."
                    ));
                }
                if block.row_id.is_some() || block.kind == "you" {
                    return Err(format!(
                        "{named} isn't a block of the topic's page: take the row whose page it's on."
                    ));
                }
                (block.id, self.title(self.block_at(named)?).to_string())
            };
            if !ids.iter().any(|(had, _)| *had == id.0) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

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

    /// A block as a sentence names it: an untitled one on a row's own
    /// page by the row's name.
    fn title(&self, n: usize) -> &str {
        let block = &self.blocks[n];
        let row = || {
            let id = block.row_id?;
            self.points
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.text.as_str())
        };
        match block.title.as_str() {
            "" => row().unwrap_or("the untitled block"),
            title => title,
        }
    }

    fn kind_of(&self, block: Uuid) -> &str {
        match self.blocks.iter().find(|b| b.id == block) {
            Some(b) => &b.kind,
            None => "",
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
        if kind != "callout" {
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

    /// Puts block `n` where `at` says among the blocks of its page: at the
    /// end past the others, or between them, which numbers them all anew.
    fn put_block(&mut self, n: usize, at: At) {
        let row = self.blocks[n].row_id;
        let mut order: Vec<usize> = (0..self.blocks.len())
            .filter(|&b| b != n && !self.is_gone(self.blocks[b].id) && self.blocks[b].row_id == row)
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

/// A text's bold lead and what follows it: "**Zwei Sichten**: Für …".
pub fn led(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim().strip_prefix("**")?;
    let (lead, after) = rest.split_once("**")?;
    let after = after.strip_prefix([':', '.', ',']).unwrap_or(after);
    let words = after.trim_start();
    let lead = lead.trim();
    (!lead.is_empty() && !lead.contains('*') && words.len() < after.len() && !words.is_empty())
        .then_some((lead, words))
}

/// A paragraph's words as they may go in: prose, not a term with what it
/// means, which is a collection's row.
fn as_prose(kind: &str, text: &str) -> Result<(), String> {
    if kind == "paragraph" && led(text).is_some() {
        return Err(
            "A paragraph doesn't open with a term in bold: a term with what it means is \
             a row of a collection, the term its name and what it means a text field. Add them \
             with add_rows, to a collection with that field (add_block)."
                .into(),
        );
    }
    Ok(())
}

/// Whether a block of `kind`, called `title`, takes items: sentences.
fn takes_items(kind: &str, title: &str) -> Result<(), String> {
    match kind {
        "paragraph" | "callout" | "you" => Ok(()),
        "heading" => Err(format!("{title} is a heading: it has no points.")),
        "collection" => Err(format!("{title} is a collection: add rows to it.")),
        _ => Err(format!("{title} is a {kind}, which Almanac doesn't know.")),
    }
}

/// The check a collection's rows are ticked by: its check field, when it
/// has one and only a day besides, as a checklist has.
pub fn check_of(block: &RefBlock) -> Option<&str> {
    let fields = &block.fields.0;
    let checks: Vec<&Field> = fields.iter().filter(|f| f.kind == "check").collect();
    match checks[..] {
        [check]
            if block.kind == "collection"
                && fields.iter().all(|f| f.kind == "check" || f.kind == "day") =>
        {
            Some(&check.name)
        }
        _ => None,
    }
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

/// The most rows figures draw, a tile each: past it they're a table.
const FIGURES_MOST: usize = 8;
/// The longest value a figure's tile holds, "12–16 Stunden".
const FIGURE_LONGEST: usize = 16;
/// The most rows drawn as steps, a numbered line each.
const STEPS_MOST: usize = 12;

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
    // One measurement a day, or one a day for each name measured again.
    let line = day.zip(number).is_some_and(|(d, n)| {
        let measured: Vec<(&str, NaiveDate)> = rows
            .iter()
            .filter(|r| number_of(r, n))
            .filter_map(|r| day_of(r, d).map(|day| (r.text.as_str(), day)))
            .collect();
        let days: HashSet<NaiveDate> = measured.iter().map(|(_, d)| *d).collect();
        let named: HashSet<&(&str, NaiveDate)> = measured.iter().collect();
        let names: HashSet<&str> = measured.iter().map(|(n, _)| *n).collect();
        !measured.is_empty()
            && (days.len() == measured.len()
                || named.len() == measured.len() && names.len() < measured.len())
    });
    // A day every row shares only says when they were all seen.
    let apart = |day: &Field| {
        let days: HashSet<String> = rows
            .iter()
            .filter_map(|r| r.cells.get(&day.name).map(cell_key))
            .collect();
        rows.len() < 2 || days.len() > 1 || rows.iter().any(|r| !r.cells.contains_key(&day.name))
    };
    // One check, and at most a day besides: rows to tick, as `check_of`.
    let checklist = fields.iter().filter(|f| f.kind == "check").count() == 1
        && fields.iter().all(|f| f.kind == "check" || f.kind == "day");
    // A few names each with a short value of its own, "Gewicht 3,4 kg", as
    // `figuresOf`.
    let figures = (2..=FIGURES_MOST).contains(&rows.len())
        && matches!(filled.as_slice(), [f] if f.kind == "text")
        && rows.iter().all(|r| match r.cells.get(&filled[0].name) {
            Some(Cell::Text(t)) => t.chars().count() <= FIGURE_LONGEST,
            _ => false,
        });
    let pick = if line {
        "line"
    } else if checklist {
        "checklist"
    } else if of("status").is_some() {
        "board"
    } else if of("photo").is_some() {
        "cards"
    } else if figures {
        "figures"
    } else if let Some(day) = day.filter(|d| apart(d)) {
        if rows
            .iter()
            .any(|r| day_of(r, day).is_some_and(|d| d >= today))
        {
            "week"
        } else {
            "timeline"
        }
    } else if number.is_some() && filled.len() <= 2 {
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
    if checklist {
        fits.push("checklist");
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
    if (2..=FIGURES_MOST).contains(&rows.len()) && !filled.is_empty() {
        fits.push("figures");
    }
    if (2..=STEPS_MOST).contains(&rows.len()) {
        fits.push("steps");
    }
    [
        "line",
        "checklist",
        "steps",
        "board",
        "figures",
        "cards",
        "timeline",
        "week",
        "bars",
        "table",
        "list",
    ]
    .into_iter()
    .filter(|p| fits.contains(p))
    .collect()
}

/// A cell as a key to tell cells apart by.
fn cell_key(cell: &Cell) -> String {
    match cell {
        Cell::Check(yes) => yes.to_string(),
        Cell::Number(n) => n.to_string(),
        Cell::Text(text) => text.clone(),
    }
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
        row_id: block.row_id,
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

/// "Made a sentence a row in Gewicht.", for sentences tidied into rows.
fn rows_words(made: &[(String, usize)]) -> Option<String> {
    let parts: Vec<String> = made
        .iter()
        .map(|(title, n)| match n {
            1 => format!("a sentence a row in {title}"),
            n => format!("{n} sentences rows in {title}"),
        })
        .collect();
    (!parts.is_empty()).then(|| format!("Made {}.", and(&parts)))
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
            unnamed: false,
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
            row_id: None,
            position,
        }
    }

    fn checklist(title: &str, position: i32) -> RefBlock {
        RefBlock {
            fields: Json(vec![Field {
                name: "Done".into(),
                kind: "check".into(),
                unit: String::new(),
                goal: None,
                options: Vec::new(),
            }]),
            ..block("collection", title, position)
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
    fn offers_draw_as_a_line_only_once_a_shop_has_two_days() {
        let fields: Vec<Field> = ["Preis:number", "Tag:day", "Link:link"]
            .iter()
            .map(|f| {
                let (name, kind) = f.split_once(':').unwrap();
                Field {
                    name: name.into(),
                    kind: kind.into(),
                    unit: String::new(),
                    goal: None,
                    options: Vec::new(),
                }
            })
            .collect();
        let offers = block("collection", "Angebote", 0);
        let offer = |shop: &str, cells: Value| PagePoint {
            cells: Json(serde_json::from_value(cells).unwrap()),
            ..point(&offers, "", shop, 0)
        };
        let today = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let first = [
            offer(
                "Scorett",
                json!({"Preis": 1599, "Tag": "2026-10-30", "Link": "https://example.com/a"}),
            ),
            offer(
                "Skocenter",
                json!({"Preis": 1799, "Tag": "2026-10-30", "Link": "https://example.com/b"}),
            ),
        ];
        let rows: Vec<&PagePoint> = first.iter().collect();
        assert!(!pictures(&fields, &rows, today).contains(&"line"));
        let again = offer("Scorett", json!({"Preis": 1499, "Tag": "2026-11-02"}));
        let rows: Vec<&PagePoint> = first.iter().chain([&again]).collect();
        assert!(pictures(&fields, &rows, today).contains(&"line"));
    }

    #[test]
    fn a_few_names_with_short_values_are_figures() {
        let fields = vec![Field {
            name: "Wert".into(),
            kind: "text".into(),
            unit: String::new(),
            goal: None,
            options: Vec::new(),
        }];
        let facts = block("collection", "Lisa", 0);
        let fact = |name: &str, value: &str| PagePoint {
            cells: Json(serde_json::from_value(json!({ "Wert": value })).unwrap()),
            ..point(&facts, "", name, 0)
        };
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let short = [fact("Alter", "3½ Jahre"), fact("Gewicht", "3,4 kg")];
        let rows: Vec<&PagePoint> = short.iter().collect();
        assert_eq!(
            pictures(&fields, &rows, today),
            ["steps", "figures", "cards", "table", "list"]
        );
        let long = fact("Schlaf", "zwölf bis sechzehn Stunden am Tag");
        let rows: Vec<&PagePoint> = short.iter().chain([&long]).collect();
        assert!(pictures(&fields, &rows, today).contains(&"list"));
    }

    #[test]
    fn a_weighing_is_a_row_with_typed_cells() {
        let mut kept = Kept::on(Refs {
            blocks: vec![block("paragraph", "Notes", 0)],
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
                None,
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
        assert!(said.get("kept").is_none(), "only ticks");
        let change = &said["changes"][0];
        assert_eq!(change["said"], json!("Kept 2 points under Gewicht."));
        assert_eq!(change["layout"]["collections"], json!([id]));
        assert_eq!(change["layout"]["made_points"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn changes_and_removals_are_one_change_to_undo() {
        let (notes, old) = (
            block("paragraph", "Notes", 0),
            block("paragraph", "Pflanzen", 1),
        );
        let fact = point(&notes, "", "Lisa wiegt 3,5 kg", 0);
        let monstera = point(&old, "", "Hell", 0);
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
    fn a_row_has_a_page_of_blocks_that_goes_with_it() {
        let notes = block("paragraph", "Notes", 0);
        let shoes = block("collection", "Schuhe", 1);
        let (fact, saola) = (
            point(&notes, "", "Vegan heißt ohne Leder.", 0),
            point(&shoes, "", "Saola", 0),
        );
        let mut kept = Kept::on(Refs {
            blocks: vec![notes.clone(), shoes.clone()],
            points: vec![fact, saola.clone()],
            ..Refs::default()
        });
        assert!(
            kept.add_block("collection", "Angebote", None, &[], Some("p1"))
                .unwrap_err()
                .contains("isn't a row")
        );
        assert!(
            kept.add_block("paragraph", "Pflege", Some("b1"), &[], Some("p2"))
                .unwrap_err()
                .contains("on another page")
        );
        let offers = kept
            .add_block(
                "collection",
                "Angebote",
                None,
                &[field("Preis", "number"), field("Tag", "day")],
                Some("p2"),
            )
            .unwrap();
        assert_eq!(offers, "b3");
        // The same title on the topic's page is another block.
        assert_eq!(
            kept.add_block("collection", "Angebote", None, &[], None),
            Ok("b4".into())
        );
        kept.add_rows(
            &offers,
            &[row(
                "Scorett",
                json!({ "Preis": "1599", "Tag": "2026-10-30" }),
            )],
        )
        .unwrap();
        let (_, operations) = kept.writes(&answering());
        let made = operations
            .iter()
            .find_map(|o| match o {
                Operation::Create {
                    model: ModelKind::Block,
                    data,
                    ..
                } if data["title"] == json!("Angebote") && data["row_id"] != Value::Null => {
                    Some(data)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(made["row_id"], json!(saola.id));
        assert_eq!(made["position"], json!(0));

        // Removing the row takes its page.
        let mut page = block("collection", "Angebote", 0);
        page.row_id = Some(saola.id);
        let offer = point(&page, "", "Scorett", 0);
        let mut kept = Kept::on(Refs {
            blocks: vec![notes.clone(), shoes.clone(), page.clone()],
            points: vec![saola.clone(), offer.clone()],
            ..Refs::default()
        });
        kept.remove(&["p1".into()]).unwrap();
        assert!(kept.add_rows("b3", &[row("Byway", json!({}))]).is_err());
        let (said, operations) = kept.writes(&answering());
        for id in [saola.id, offer.id, page.id] {
            assert!(
                operations
                    .iter()
                    .any(|o| matches!(o, Operation::Delete { id: d, .. } if *d == id)),
                "{operations:?}"
            );
        }
        let layout: PageLayout =
            serde_json::from_value(said["changes"][0]["layout"].clone()).unwrap();
        assert!(
            layout
                .blocks
                .iter()
                .any(|b| b.id == page.id && b.row_id == Some(saola.id) && b.removed)
        );
    }

    #[test]
    fn a_sentence_tidied_into_a_row_says_so() {
        let notes = block("paragraph", "Notes", 0);
        let mut weight = block("collection", "Gewicht", 1);
        weight.fields = Json(vec![Field {
            name: "Gewicht".into(),
            kind: "number".into(),
            unit: "kg".into(),
            goal: None,
            options: Vec::new(),
        }]);
        let fact = point(&notes, "", "Lisa wiegt etwa 3,4 kg.", 0);
        let mut kept = Kept::on(Refs {
            blocks: vec![notes, weight.clone()],
            points: vec![fact.clone()],
            ..Refs::default()
        });
        kept.move_points(&["p1".into()], "b2", None).unwrap();
        kept.change_points(&[PointChange {
            point: "p1".into(),
            text: Some("Lisa".into()),
            cells: Some(json!({"Gewicht": 3.4}).as_object().cloned().unwrap()),
            ..PointChange::default()
        }])
        .unwrap();
        let (said, operations) = kept.writes(&answering());
        assert!(
            matches!(
                &operations[..],
                [Operation::Update { id, data, .. }]
                    if *id == fact.id && data["block_id"] == json!(weight.id)
                        && data["text"] == json!("Lisa")
                        && data["cells"] == json!({"Gewicht": 3.4})
            ),
            "{operations:?}"
        );
        let change = &said["changes"][0];
        assert_eq!(change["said"], json!("Made a sentence a row in Gewicht."));
        assert_eq!(change["layout"]["collections"], json!([weight.id]));
        assert_eq!(change["layout"]["rows"], json!([fact.id]));
        let layout: PageLayout = serde_json::from_value(change["layout"].clone()).unwrap();
        assert!(layout.points.iter().any(|p| p.id == fact.id && !p.removed));
    }

    #[test]
    fn moving_points_renumbers_their_block() {
        let (a, b) = (block("paragraph", "A", 0), block("paragraph", "B", 1));
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
        let list = checklist("Routine", 0);
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
                ] if *item == bin.id && done["cells"] == json!({"Done": true})
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
            blocks: vec![checklist("Routine", 0), block("paragraph", "Notes", 1)],
            ..Refs::default()
        });
        let [wash] = &kept
            .add_rows("b1", &[row("Haare waschen", json!({}))])
            .unwrap()[..]
        else {
            panic!();
        };
        let wash = wash.clone();
        kept.add_items("b2", &[item("Niedrige Porosität")]).unwrap();
        let friday = reminder("", "Haare waschen", "2026-10-30", "", Some(3)).unwrap();
        assert!(
            kept.offer_reminder(friday.clone(), &[], Some("p2"))
                .unwrap_err()
                .contains("a row with a check")
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
    fn a_field_of_another_kind_reads_its_cells_anew() {
        let mut kept = Kept::on(Refs::default());
        kept.add_block(
            "collection",
            "Schuhe",
            None,
            &[field("Vegan", "text"), field("Preis", "text")],
            None,
        )
        .unwrap();
        kept.add_rows(
            "b1",
            &[
                row("Saola", json!({"Vegan": "Ja", "Preis": "1 790"})),
                row(
                    "Byway",
                    json!({"Vegan": "Nein, Nubuk und Wildleder", "Preis": "1599 kr"}),
                ),
            ],
        )
        .unwrap();
        let unread = kept
            .change_block(
                "b1",
                None,
                Some(&[
                    field("Vegan", "check"),
                    FieldArg {
                        unit: "kr".into(),
                        ..field("Preis", "number")
                    },
                ]),
                None,
            )
            .unwrap();
        assert!(
            unread.contains("Byway: Vegan was \"Nein, Nubuk und Wildleder\""),
            "{unread}"
        );
        assert!(!unread.contains("Saola"), "{unread}");
        let (_, operations) = kept.writes(&answering());
        let cells: Vec<&Value> = operations
            .iter()
            .filter_map(|o| match o {
                Operation::Create { data, .. } => data.get("cells"),
                _ => None,
            })
            .collect();
        assert_eq!(
            cells,
            [
                &json!({"Vegan": true, "Preis": 1790.0}),
                &json!({"Preis": 1599.0})
            ]
        );
    }

    #[test]
    fn a_collection_picks_among_the_pictures_its_fields_allow() {
        let mut kept = Kept::on(Refs::default());
        kept.add_block(
            "collection",
            "Angebote",
            None,
            &[field("Status", "status"), field("Preis", "number")],
            None,
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
        // A new kind of field leaves the picture to the app again.
        kept.change_block("b1", None, Some(&[field("Stand", "status")]), None)
            .unwrap();
        let (_, operations) = kept.writes(&answering());
        assert!(
            matches!(&operations[..], [Operation::Create { data: b, .. }, ..] if b["view"] == json!("")),
            "{operations:?}"
        );
    }

    #[test]
    fn blocks_take_only_what_fits_them() {
        let mut kept = Kept::on(Refs {
            blocks: vec![
                block("collection", "Zeit", 0),
                block("paragraph", "Lesen", 1),
                block("heading", "Teil", 2),
            ],
            ..Refs::default()
        });
        assert!(
            kept.add_items("b1", &[item("1987")])
                .unwrap_err()
                .contains("add rows")
        );
        assert!(
            kept.add_items("b3", &[item("x")])
                .unwrap_err()
                .contains("a heading")
        );
        kept.add_items("Lesen", &[item("Ward, Plant care")])
            .unwrap();
        assert!(
            kept.add_block("table", "Schuhe", None, &[], None)
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
    fn only_an_answer_in_an_unnamed_talk_names_it() {
        let kept = Kept {
            talk_title: Some("Gießen".into()),
            ..Kept::default()
        };
        let mut at = Answering {
            topic_id: Uuid::now_v7(),
            topic_name: "Pflanzen".into(),
            talk_id: Uuid::now_v7(),
            answer_id: Uuid::now_v7(),
            unnamed: true,
        };
        assert_eq!(kept.writes(&at).1.len(), 1);
        at.unnamed = false;
        assert!(kept.writes(&at).1.is_empty());
    }

    #[test]
    fn a_paragraph_is_prose_not_terms() {
        assert_eq!(
            led("**Managing Up** heißt, die Beziehung zu gestalten."),
            Some(("Managing Up", "heißt, die Beziehung zu gestalten."))
        );
        assert_eq!(
            led("**Nakam**, eine Gruppe"),
            Some(("Nakam", "eine Gruppe"))
        );
        assert_eq!(led("**Achtung**"), None);
        assert_eq!(led("Das ist **wichtig** hier."), None);
        assert!(as_prose("paragraph", "**Issue Selling**: Themen nach oben.").is_err());
        assert!(as_prose("paragraph", "Themen nach oben, mit **Nachdruck**.").is_ok());
        assert!(as_prose("callout", "**Achtung**: Lilien sind giftig.").is_ok());
    }
}
