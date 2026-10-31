//! A topic's page as Claude reads it: what the topic sticks to, its blocks
//! in order with the points each keeps (a collection's rows with their
//! cells), the questions it waits on, its reminders, and the names of the
//! other topics, for questions that belong there. A sub-topic follows its
//! parent's brief too, and a parent names its sub-topics. For the workspace's own topic, the workspace: its topics by
//! group, what each sticks to, its blocks and how much it has.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use almanac_domain::{Cell, Facet, Field};
use chrono::NaiveDate;
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

const TOPIC: &str = "SELECT t.name, t.sticks_to, p.name, p.sticks_to, t.cover IS NOT NULL FROM topics t \
     LEFT JOIN topics p ON p.id = t.parent_id AND p.workspace_id = t.workspace_id \
     WHERE t.id = $1 AND t.workspace_id = $2";
const CHILDREN: &str = "SELECT name FROM topics \
     WHERE parent_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const BLOCKS: &str = "SELECT id, kind, title, facets, fields, width, view, row_id, position \
     FROM blocks \
     WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const POINTS: &str = "SELECT id, block_id, theme, text, happened, facet, look, amount, note, \
       cells, done, position, source, source_url \
     FROM points WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const QUESTIONS: &str = "SELECT text FROM questions \
     WHERE topic_id = $1 AND workspace_id = $2 AND status = 'open' ORDER BY created_at";
const REMINDERS: &str = "SELECT id, title, due_on, every_days, status, point_id, last_done_on \
     FROM reminders \
     WHERE topic_id = $1 AND workspace_id = $2 AND status IN ('on', 'offered') ORDER BY due_on";
const OTHERS: &str = "SELECT name FROM topics \
     WHERE workspace_id = $2 AND id <> $1 AND kind = 'topic' ORDER BY position";
const WORKSPACE: &str = "SELECT name FROM workspaces WHERE id = $1";
const TOPICS: &str = "SELECT COALESCE(t.name || ', under ' || p.name, t.name), t.group_name, t.sticks_to, \
       COALESCE((SELECT string_agg(CASE WHEN b.title = '' THEN b.kind ELSE b.title END, ', ' \
           ORDER BY b.position, b.created_at) FROM blocks b \
           WHERE b.topic_id = t.id AND b.row_id IS NULL), ''), \
       (SELECT count(*) FROM points p WHERE p.topic_id = t.id), \
       (SELECT max(k.last_at)::date FROM talks k WHERE k.topic_id = t.id) \
     FROM topics t \
     LEFT JOIN topics p ON p.id = t.parent_id AND p.workspace_id = t.workspace_id \
     WHERE t.workspace_id = $1 AND t.kind = 'topic' \
     ORDER BY t.group_name = '', t.group_name, t.position, t.created_at";

/// A point as the page has it: an item, or a collection's row, its text
/// the row's name.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct PagePoint {
    pub id: Uuid,
    pub block_id: Uuid,
    pub theme: String,
    pub text: String,
    /// On a timeline.
    pub happened: String,
    pub facet: String,
    pub look: String,
    pub amount: Option<f64>,
    pub note: String,
    pub cells: Json<BTreeMap<String, Cell>>,
    pub done: bool,
    pub position: i32,
    pub source: Option<String>,
    pub source_url: Option<String>,
}

/// A reminder as the page has it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PageReminder {
    pub id: Uuid,
    pub title: String,
    pub due_on: NaiveDate,
    pub every_days: Option<i32>,
    /// `on`, or only `offered`.
    pub status: String,
    /// The checklist item it rings for.
    pub point_id: Option<Uuid>,
    pub last_done_on: Option<NaiveDate>,
}

impl PageReminder {
    /// Whether it's ticked for now: done, or done for a day it comes due
    /// after, as a checklist item shows it (`doneFor` in the web app).
    pub fn ticked(&self) -> bool {
        self.last_done_on
            .is_some_and(|day| self.status == "done" || (self.status == "on" && self.due_on > day))
    }
}

/// One block of the page, with its points in order.
pub struct PageBlock {
    pub block: RefBlock,
    pub points: Vec<PagePoint>,
}

/// A block as the page has it, by the reference Claude reads it with.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct RefBlock {
    pub id: Uuid,
    /// One of [`almanac_domain::SET_KINDS`], or `you`.
    pub kind: String,
    pub title: String,
    /// For cards from before the set: the facets its points may name.
    pub facets: Json<Vec<Facet>>,
    /// For a collection: its rows' fields.
    pub fields: Json<Vec<Field>>,
    /// Empty for its kind's own.
    pub width: String,
    /// The picture a person or a talk picked; empty for the app's own.
    pub view: String,
    /// The row whose own page it's on; none, the topic's.
    pub row_id: Option<Uuid>,
    pub position: i32,
}

/// The page's blocks, points and reminders by the references Claude reads
/// them with: `b2` is the second block, `p3` the third point as the page
/// lists them, `r2` the second reminder.
#[derive(Debug, Default, Clone)]
pub struct Refs {
    pub blocks: Vec<RefBlock>,
    pub points: Vec<PagePoint>,
    pub reminders: Vec<PageReminder>,
}

/// A topic's name and brief, its parent's, and whether it has a cover.
type TopicRow = (String, String, Option<String>, Option<String>, bool);

pub struct Page {
    pub id: Uuid,
    pub name: String,
    pub sticks_to: String,
    /// The topic it sits under: its name and brief.
    pub parent: Option<(String, String)>,
    /// The names of the topics under it.
    pub children: Vec<String>,
    /// Whether its page opens with a cover photo.
    pub covered: bool,
    pub blocks: Vec<PageBlock>,
    pub questions: Vec<String>,
    pub reminders: Vec<PageReminder>,
    pub others: Vec<String>,
}

impl Page {
    pub async fn load(
        pool: &PgPool,
        workspace_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Page>, sqlx::Error> {
        let topic: Option<TopicRow> = sqlx::query_as(TOPIC)
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
        let Some((name, sticks_to, parent, parent_sticks_to, covered)) = topic else {
            return Ok(None);
        };
        let blocks: Vec<RefBlock> = sqlx::query_as(BLOCKS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        let mut blocks: Vec<PageBlock> = blocks
            .into_iter()
            .map(|block| PageBlock {
                block,
                points: Vec::new(),
            })
            .collect();
        let points: Vec<PagePoint> = sqlx::query_as(POINTS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        for point in points {
            if let Some(block) = blocks.iter_mut().find(|b| b.block.id == point.block_id) {
                block.points.push(point);
            }
        }
        Ok(Some(Page {
            blocks: in_order(blocks),
            id,
            name,
            sticks_to,
            parent: parent.zip(parent_sticks_to),
            covered,
            children: sqlx::query_scalar(CHILDREN)
                .bind(id)
                .bind(workspace_id)
                .fetch_all(pool)
                .await?,
            questions: sqlx::query_scalar(QUESTIONS)
                .bind(id)
                .bind(workspace_id)
                .fetch_all(pool)
                .await?,
            reminders: sqlx::query_as(REMINDERS)
                .bind(id)
                .bind(workspace_id)
                .fetch_all(pool)
                .await?,
            others: sqlx::query_scalar(OTHERS)
                .bind(id)
                .bind(workspace_id)
                .fetch_all(pool)
                .await?,
        }))
    }

    /// The references `describe` gives the blocks, points and reminders.
    pub fn refs(&self) -> Refs {
        Refs {
            blocks: self.blocks.iter().map(|b| b.block.clone()).collect(),
            points: self
                .blocks
                .iter()
                .flat_map(|b| b.points.iter().cloned())
                .collect(),
            reminders: self.reminders.clone(),
        }
    }

    /// A collection's row by its id: its reference's number (p3), its
    /// collection's title and its name.
    pub fn row(&self, id: Uuid) -> Option<(usize, &str, &str)> {
        let mut n = 0;
        for PageBlock { block, points } in &self.blocks {
            for point in points {
                n += 1;
                if point.id == id && block.kind == "collection" {
                    return Some((n, &block.title, &point.text));
                }
            }
        }
        None
    }

    /// The topic and what it has kept, in plain text, each block, point
    /// and reminder with its reference.
    pub fn describe(&self) -> String {
        self.written(true)
    }

    /// The page for another app to read: what it holds, without how a
    /// talk makes one.
    pub fn as_read(&self) -> String {
        self.written(false)
    }

    fn written(&self, how: bool) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "The topic: {}", self.name);
        let sticks_to = match self.sticks_to.trim() {
            "" => "Not written yet. Stay with what the name means.",
            s => s,
        };
        let _ = writeln!(out, "What it sticks to, its brief: {sticks_to}");
        if let Some((parent, brief)) = &self.parent {
            let _ = writeln!(
                out,
                "It sits under {parent}, whose brief holds here too where this one doesn't say \
                 otherwise: {}",
                match brief.trim() {
                    "" => "not written yet.",
                    s => s,
                }
            );
        }
        if self.covered {
            let _ = writeln!(out, "Its page opens with a cover photo.");
        }
        if !self.children.is_empty() {
            let _ = writeln!(
                out,
                "Its sub-topics, each with its own page and talks: {}. What's about one of \
                 them alone belongs in that one's talks; here stays what they share.",
                self.children.join(", ")
            );
        }
        if how {
            let _ = writeln!(out, "\nHow a page is made: {SET_SAYS}");
        }
        out.push_str("\nThe page's blocks, in order:\n");
        if self.blocks.is_empty() {
            out.push_str("None yet.\n");
        }
        let numbers: Vec<&PagePoint> = self.blocks.iter().flat_map(|b| &b.points).collect();
        let mut n = 0;
        let mut on_rows = false;
        for (b, PageBlock { block, points }) in self.blocks.iter().enumerate() {
            let row = block
                .row_id
                .and_then(|id| numbers.iter().position(|p| p.id == id))
                .map(|i| (i + 1, &numbers[i].text));
            if row.is_some() && !on_rows {
                out.push_str("\nOn rows' own pages, a row's blocks after its row:\n");
                on_rows = true;
            }
            let title = match block.title.as_str() {
                "" => "Untitled",
                title => title,
            };
            let mut about = vec![block.kind.clone()];
            if let Some((n, name)) = &row {
                about.push(format!("on the page of [p{n}] {name}"));
            }
            if !block.fields.is_empty() {
                about.push(format!(
                    "fields: {}",
                    block
                        .fields
                        .iter()
                        .map(field_words)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !block.view.is_empty() {
                about.push(format!("shown as {}", block.view));
            }
            let _ = writeln!(out, "## [b{}] {title} ({})", b + 1, about.join("; "));
            if points.is_empty() && block.kind != "heading" {
                out.push_str("Nothing yet.\n");
            }
            let mut theme = None;
            for point in points {
                if !point.theme.is_empty() && theme != Some(&point.theme) {
                    let _ = writeln!(out, "### {}", point.theme);
                    theme = Some(&point.theme);
                }
                n += 1;
                let when = match point.happened.as_str() {
                    "" => String::new(),
                    happened => format!("{happened}: "),
                };
                let cells: String = block
                    .fields
                    .iter()
                    .filter_map(|f| {
                        point
                            .cells
                            .get(&f.name)
                            .map(|c| format!(" | {}: {}", f.name, cell_words(c)))
                    })
                    .collect();
                let source = match point.source.as_deref().filter(|s| !s.is_empty()) {
                    Some(source) => format!(" ({source})"),
                    None => String::new(),
                };
                let rings = self.reminders.iter().find(|r| r.point_id == Some(point.id));
                let _ = writeln!(
                    out,
                    "- [p{n}] {when}{}{cells}{source}{}",
                    point.text,
                    shape_words(point, rings)
                );
            }
        }
        if !self.questions.is_empty() {
            out.push_str("\nQuestions the page is waiting on:\n");
            for question in &self.questions {
                let _ = writeln!(out, "- {question}");
            }
        }
        if !self.reminders.is_empty() {
            let ids: Vec<Uuid> = self
                .blocks
                .iter()
                .flat_map(|b| b.points.iter().map(|p| p.id))
                .collect();
            out.push_str("\nReminders on the calendar:\n");
            for (n, reminder) in self.reminders.iter().enumerate() {
                let due_on = reminder.due_on;
                let when = match reminder.every_days {
                    Some(days) => format!("next on {due_on}, every {days} days"),
                    None => format!("on {due_on}"),
                };
                let offered = if reminder.status == "offered" {
                    " (offered, waiting for their yes)"
                } else {
                    ""
                };
                let item = reminder
                    .point_id
                    .and_then(|id| ids.iter().position(|p| *p == id))
                    .map(|i| format!(", rings for p{}", i + 1))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "- [r{}] {}: {when}{item}{offered}",
                    n + 1,
                    reminder.title
                );
            }
        }
        out.push_str("\nThe other topics: ");
        if self.others.is_empty() {
            out.push_str("none yet.\n");
        } else {
            out.push_str(&self.others.join(", "));
            out.push('\n');
        }
        out
    }
}

/// The topic's blocks in their order, then each row's own, after the
/// page that holds its row: the order Claude reads them in, which numbers
/// them. A block whose row is gone is on no page.
fn in_order(blocks: Vec<PageBlock>) -> Vec<PageBlock> {
    let (mut ordered, mut rest): (Vec<PageBlock>, Vec<PageBlock>) =
        blocks.into_iter().partition(|b| b.block.row_id.is_none());
    loop {
        let rows: Vec<Uuid> = ordered
            .iter()
            .flat_map(|b| b.points.iter().map(|p| p.id))
            .collect();
        let at = |b: &PageBlock| {
            b.block
                .row_id
                .and_then(|id| rows.iter().position(|p| *p == id))
        };
        let (mut found, left): (Vec<PageBlock>, Vec<PageBlock>) =
            rest.into_iter().partition(|b| at(b).is_some());
        rest = left;
        if found.is_empty() {
            return ordered;
        }
        found.sort_by_key(|b| (at(b), b.block.position));
        ordered.extend(found);
    }
}

/// A field as Claude reads it: "Gewicht (number, kg, goal 3.4)".
fn field_words(field: &Field) -> String {
    let mut said = vec![field.kind.clone()];
    if !field.unit.is_empty() {
        said.push(field.unit.clone());
    }
    if let Some(goal) = field.goal {
        said.push(format!("goal {goal}"));
    }
    if !field.options.is_empty() {
        said.push(field.options.join(" / "));
    }
    format!("{} ({})", field.name, said.join(", "))
}

/// A cell as Claude reads it.
fn cell_words(cell: &Cell) -> String {
    match cell {
        Cell::Check(yes) => if *yes { "yes" } else { "no" }.into(),
        Cell::Number(n) => n.to_string(),
        Cell::Text(text) => text.clone(),
    }
}

/// What a point carries besides its text and cells, as Claude reads it:
/// " {facet: Wasser, look: warn}", or a checklist item's reminder. Empty
/// when it carries nothing.
fn shape_words(point: &PagePoint, rings: Option<&PageReminder>) -> String {
    let mut said = Vec::new();
    if !point.facet.is_empty() {
        said.push(format!("facet: {}", point.facet));
    }
    if !point.look.is_empty() {
        said.push(format!("look: {}", point.look));
    }
    if let Some(amount) = point.amount {
        said.push(format!("amount: {amount}"));
    }
    if !point.note.is_empty() {
        said.push(format!("note: {}", point.note));
    }
    match rings {
        Some(reminder) if reminder.status == "on" => {
            if reminder.ticked() {
                said.push(format!("done, due again on {}", reminder.due_on));
            }
            said.push(match reminder.every_days {
                Some(days) => format!("every {days} days"),
                None => format!("on {}", reminder.due_on),
            });
        }
        _ if point.done => said.push("done".into()),
        _ => {}
    }
    match said.as_slice() {
        [] => String::new(),
        said => format!(" {{{}}}", said.join(", ")),
    }
}

/// The one set of blocks every page is made of, for Claude to file each
/// thing where it belongs.
const SET_SAYS: &str = "every page on every topic is made of the same few blocks, in order, \
each with a short title or none. The app draws them; you only say what goes where. The kinds: \
heading, a title that starts a part of the page, with no points; \
paragraph, a point a paragraph, for what reads as prose: two to four sentences that explain, \
never opening with a term in bold (a term with what it means is a collection's row); \
callout, a point set apart: look warn when it needs a look (a problem, a gap), info when \
something is ready; \
collection, rows that share the same fields, a row per thing, event or measurement (plants, \
offers, weighings, repairs): its text is the row's name, and it has a cell for each field it \
knows. A field is text (a few words, or a sentence or two for what a term means), number (plain, 3.4, with the field's unit, kg, and a goal \
when they aim at one), day (a day as 2026-09-24, or a year or a span, 1987–1993), choice (one of \
a few words), status (where a row stands, Offen, Bestellt, Erledigt), check (yes or no), photo \
(a photo from the talk, by its number) or link (a web page). The app draws a collection from \
its fields: days with numbers as a line, a check with at most a day as a checklist, a status as \
a board, photos as cards, a few names each with a short value (Gewicht: 3,4 kg) as figures, \
large in tiles (a colour written #a78bfa as its swatch), days past as a timeline and days to come as a week, a name with a number as \
bars, three fields or more as a table, the rest as a list. A status or a choice shows as a \
coloured pill, counted over the rows; steps in an order can be drawn as numbered steps. Prefer \
what draws to what reads: a number, a status or a choice says more at a glance than words. \
Lists are collections too: a list of things or terms is rows with only a name, a few words \
each; what to do or get is rows with a check (Done), ticked once done. A row to do that comes \
round or is due on a day is a reminder too (offer_reminder with its item): it rings on the Mac, \
and ticking the row ticks the reminder, until it's due again. Terms are rows too: the term its \
name and what it means a text field, a sentence or two, which draws as a list, a table or cards \
as they pick; a question or a step with more to say is a row, and its sentences a paragraph on \
its page. \
A row has a page of its own, made of the same blocks: what belongs to that one row and has \
parts of its own goes there, in a block that names the row, so the topic's page keeps the row \
and not its details. A shoe's offers are a collection on the shoe's page (Laden, Preis in kr, \
Tag), a plant's repottings one on the plant's, sentences about one thing a paragraph on its \
page. \
File each thing where it belongs, and keep the page's face short: a few words a line, whole \
sentences in paragraphs, on a row's page when they're about one row. What's measured, compared or tracked is rows with typed \
fields, never a sentence: a weighing is a row with its day and its number in kg, so the line \
draws it. A row's cells hold only what they or a source said: leave a cell out rather than \
guess. Add to the block it fits; add a field to a collection that lacks one; start a block \
only when none fits. The page is kept up to date, not added to: change what changed, remove \
what's wrong or twice, and keep one point where two would say the same. \
Besides the set, you is what they told you about themselves that your answers go by (\"Low \
porosity, by the float test\", \"The balcony faces north\"), a fact each in a few words: it \
isn't on the page but on the topic's About, so keep there what you learn about them, however \
it's worded.";

/// The workspace as its own topic reads it.
pub struct Workspace {
    pub name: String,
    /// Name, group, what it sticks to, its blocks' titles, how many points,
    /// the day of its last talk.
    pub topics: Vec<(String, String, String, String, i64, Option<NaiveDate>)>,
}

impl Workspace {
    pub async fn load(pool: &PgPool, workspace_id: Uuid) -> Result<Workspace, sqlx::Error> {
        Ok(Workspace {
            name: sqlx::query_scalar(WORKSPACE)
                .bind(workspace_id)
                .fetch_one(pool)
                .await?,
            topics: sqlx::query_as(TOPICS)
                .bind(workspace_id)
                .fetch_all(pool)
                .await?,
        })
    }

    pub fn groups(&self) -> Vec<&str> {
        let mut groups: Vec<&str> = Vec::new();
        for (_, group, ..) in &self.topics {
            if !group.is_empty() && !groups.contains(&group.as_str()) {
                groups.push(group);
            }
        }
        groups
    }

    pub fn describe(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "The workspace: {}", self.name);
        if self.topics.is_empty() {
            out.push_str("It has no topics yet besides this one.\n");
            return out;
        }
        out.push_str("\nIts topics, by group:\n");
        let mut group = None;
        for (name, in_group, sticks_to, blocks, points, last) in &self.topics {
            if group != Some(in_group) {
                let _ = writeln!(
                    out,
                    "## {}",
                    if in_group.is_empty() {
                        "No group"
                    } else {
                        in_group
                    }
                );
                group = Some(in_group);
            }
            let sticks_to = match sticks_to.trim() {
                "" => "not written yet",
                s => s,
            };
            let blocks = match blocks.as_str() {
                "" => "no blocks yet",
                blocks => blocks,
            };
            let last = match last {
                Some(day) => format!(", last talk on {day}"),
                None => ", no talks yet".into(),
            };
            let _ = writeln!(
                out,
                "- {name} ({blocks}; {points} points{last}). Sticks to: {sticks_to}"
            );
        }
        out
    }
}
