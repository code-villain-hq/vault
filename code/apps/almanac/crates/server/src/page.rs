//! A topic's page as Claude reads it: what the topic sticks to, its blocks
//! in order with the points each keeps, the questions it waits on, its
//! reminders, and the names of the other topics, for questions that belong
//! there. For the workspace's own topic, the workspace: its topics by
//! group, what each sticks to, its blocks and how much it has.

use std::fmt::Write as _;

use almanac_domain::Facet;
use chrono::NaiveDate;
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

const TOPIC: &str = "SELECT name, sticks_to FROM topics WHERE id = $1 AND workspace_id = $2";
const BLOCKS: &str = "SELECT id, kind, title, facets, width FROM blocks \
     WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const POINTS: &str = "SELECT id, block_id, theme, text, happened, facet, look, amount, note, \
       done, source \
     FROM points WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const QUESTIONS: &str = "SELECT text FROM questions \
     WHERE topic_id = $1 AND workspace_id = $2 AND status = 'open' ORDER BY created_at";
const REMINDERS: &str = "SELECT id, title, due_on, every_days, status FROM reminders \
     WHERE topic_id = $1 AND workspace_id = $2 AND status IN ('on', 'offered') ORDER BY due_on";
const OTHERS: &str = "SELECT name FROM topics \
     WHERE workspace_id = $2 AND id <> $1 AND kind = 'topic' ORDER BY position";
const WORKSPACE: &str = "SELECT name FROM workspaces WHERE id = $1";
const TOPICS: &str = "SELECT t.name, t.group_name, t.sticks_to, \
       COALESCE((SELECT string_agg(CASE WHEN b.title = '' THEN b.kind ELSE b.title END, ', ' \
           ORDER BY b.position, b.created_at) FROM blocks b WHERE b.topic_id = t.id), ''), \
       (SELECT count(*) FROM points p WHERE p.topic_id = t.id), \
       (SELECT max(k.last_at)::date FROM talks k WHERE k.topic_id = t.id) \
     FROM topics t WHERE t.workspace_id = $1 AND t.kind = 'topic' \
     ORDER BY t.group_name = '', t.group_name, t.position, t.created_at";

/// A point as the page has it.
#[derive(sqlx::FromRow)]
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
    pub done: bool,
    pub source: Option<String>,
}

/// A reminder as the page has it: its id, title, next day, rhythm, and
/// whether it's `on` or only `offered`.
pub type PageReminder = (Uuid, String, NaiveDate, Option<i32>, String);

/// One block of the page, with its points in order.
pub struct PageBlock {
    pub id: Uuid,
    /// One of [`almanac_domain::BLOCK_KINDS`].
    pub kind: String,
    pub title: String,
    pub facets: Vec<Facet>,
    /// Empty for its kind's own.
    pub width: String,
    pub points: Vec<PagePoint>,
}

/// A block by the reference Claude reads it with.
#[derive(Debug, Clone)]
pub struct RefBlock {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    /// For cards: the facets its points may name.
    pub facets: Vec<Facet>,
}

/// The page's blocks, points and reminders by the references Claude reads
/// them with: `b2` is the second block, `p3` the third point as the page
/// lists them, `r2` the second reminder.
#[derive(Debug, Default, Clone)]
pub struct Refs {
    pub blocks: Vec<RefBlock>,
    /// Each with the block it's in.
    pub points: Vec<(Uuid, Uuid)>,
    /// Each with whether it's only offered yet.
    pub reminders: Vec<(Uuid, bool)>,
}

pub struct Page {
    pub id: Uuid,
    pub name: String,
    pub sticks_to: String,
    pub blocks: Vec<PageBlock>,
    pub questions: Vec<String>,
    pub reminders: Vec<PageReminder>,
    pub others: Vec<String>,
}

/// A block as `BLOCKS` reads it: id, kind, title, facets and width.
type BlockRow = (Uuid, String, String, Json<Vec<Facet>>, String);

impl Page {
    pub async fn load(
        pool: &PgPool,
        workspace_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Page>, sqlx::Error> {
        let topic: Option<(String, String)> = sqlx::query_as(TOPIC)
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
        let Some((name, sticks_to)) = topic else {
            return Ok(None);
        };
        let blocks: Vec<BlockRow> = sqlx::query_as(BLOCKS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        let mut blocks: Vec<PageBlock> = blocks
            .into_iter()
            .map(|(id, kind, title, Json(facets), width)| PageBlock {
                id,
                kind,
                title,
                facets,
                width,
                points: Vec::new(),
            })
            .collect();
        let points: Vec<PagePoint> = sqlx::query_as(POINTS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        for point in points {
            if let Some(block) = blocks.iter_mut().find(|b| b.id == point.block_id) {
                block.points.push(point);
            }
        }
        Ok(Some(Page {
            id,
            name,
            sticks_to,
            blocks,
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
            blocks: self
                .blocks
                .iter()
                .map(|b| RefBlock {
                    id: b.id,
                    kind: b.kind.clone(),
                    title: b.title.clone(),
                    facets: b.facets.clone(),
                })
                .collect(),
            points: self
                .blocks
                .iter()
                .flat_map(|b| b.points.iter().map(|p| (p.id, b.id)))
                .collect(),
            reminders: self
                .reminders
                .iter()
                .map(|(id, .., status)| (*id, status == "offered"))
                .collect(),
        }
    }

    /// The topic and what it has kept, in plain text, each block, point
    /// and reminder with its reference.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "The topic: {}", self.name);
        let sticks_to = match self.sticks_to.trim() {
            "" => "Not written yet. Stay with what the name means.",
            s => s,
        };
        let _ = writeln!(out, "What it sticks to, its brief: {sticks_to}");
        let _ = writeln!(out, "\nHow the page lays out its points: {BLOCKS_SAY}");
        out.push_str("\nThe page's blocks, in order:\n");
        if self.blocks.is_empty() {
            out.push_str("None yet.\n");
        }
        let mut n = 0;
        for (b, block) in self.blocks.iter().enumerate() {
            let title = match block.title.as_str() {
                "" => "The week",
                title => title,
            };
            let facets = match block.facets.as_slice() {
                [] => String::new(),
                facets => format!(
                    "; facets: {}",
                    facets
                        .iter()
                        .map(|f| format!("{} ({})", f.name, f.icon))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            let width = match block.width.as_str() {
                "" => String::new(),
                width => format!(", {width} wide"),
            };
            let _ = writeln!(
                out,
                "## [b{}] {title} ({}{width}{facets})",
                b + 1,
                block.kind
            );
            if block.points.is_empty() && block.kind != "week" {
                out.push_str("Nothing yet.\n");
            }
            let mut theme = None;
            for point in &block.points {
                if !point.theme.is_empty() && theme != Some(&point.theme) {
                    let _ = writeln!(out, "### {}", point.theme);
                    theme = Some(&point.theme);
                }
                n += 1;
                let when = match point.happened.as_str() {
                    "" => String::new(),
                    happened => format!("{happened}: "),
                };
                let source = match point.source.as_deref().filter(|s| !s.is_empty()) {
                    Some(source) => format!(" ({source})"),
                    None => String::new(),
                };
                let _ = writeln!(
                    out,
                    "- [p{n}] {when}{}{source}{}",
                    point.text,
                    shape_words(point)
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
            out.push_str("\nReminders on the calendar:\n");
            for (n, (_, title, due_on, every, status)) in self.reminders.iter().enumerate() {
                let when = match every {
                    Some(days) => format!("next on {due_on}, every {days} days"),
                    None => format!("on {due_on}"),
                };
                let offered = if status == "offered" {
                    " (offered, waiting for their yes)"
                } else {
                    ""
                };
                let _ = writeln!(out, "- [r{}] {title}: {when}{offered}", n + 1);
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

/// What a point carries besides its text, as Claude reads it: " {facet:
/// Wasser, look: warn}". Empty when it carries nothing.
fn shape_words(point: &PagePoint) -> String {
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
    if point.done {
        said.push("done".into());
    }
    match said.as_slice() {
        [] => String::new(),
        said => format!(" {{{}}}", said.join(", ")),
    }
}

/// What each kind of block shows, for Claude to keep each point where the
/// page shows it.
const BLOCKS_SAY: &str = "a page is its blocks, in order, each with a title of its own, and \
each lays out its points one way. A point goes in one block, named by its reference (b2) or \
its title; a theme groups points inside a block. The kinds: \
today, the steps of what comes round on the day: the theme is what comes round, named the way \
its reminders are (\"Elvital wash\"), and each point says what, a colon, then how (\"Elvital \
shampoo: on the scalp, lukewarm water\"), in the order they're done; \
week, the days of the topic's reminders, which keeps no points: offer a reminder instead; \
chips, facts about what the topic is about, in a few words (\"Epiphyte: grows on other \
plants\"), with no theme; \
you, what they told you about themselves that your answers go by (\"Low porosity, by the \
float test\", \"The balcony faces north\"), a fact each in a few words, with no theme: it \
isn't on the page but on the topic's About, so keep there what you learn about them, not \
on the page, however it's worded (their views, their CV, their hair), and a layout leaves it \
a you block; \
shelf, a tile for each thing they use, the thing's name as the theme (\"Elvital Hyaluron \
Plump\"): its first point says what it is in two or three words (\"Shampoo and \
conditioner\"), its second how they use it, any more are notes; \
cards, a card for each thing the topic is about (a plant, a pet, a place), its name as the \
theme, each point one short sentence, under 20 words. A block of cards names its facets, \
the facts every card has, each with an icon (Licht sun, Wasser droplet, Platz map-pin): a \
card's face shows a line for each, its first point under that facet, so that point says it \
in a few words (\"Hell, keine direkte Mittagssonne\"), and facet names it. The card's first \
point with no facet says what the thing is (\"Pfeilwurzgewächs\"); the other points with \
no facet show on its own page. A point about something that needs a look (brown edges, \
pests) has look warn; one ready for something (a cutting to pot) has look info; a card that \
is fine has neither. A block with one card shows it as a profile: its points with no facet \
as chips (\"3½ Jahre\", \"3,4 kg\"), and each facet as a tile of its points; \
text, sections of one piece of writing, the section's heading as the theme (Problem, \
Market, Team): each point keeps what the answer explained the way it explained it, two to \
four sentences, starting with the term it's about in **bold** when there is one, and what \
they told you stays one sentence. The page shows each section as a tile with the first \
sentence of its first point, so that sentence stands on its own; \
list, items to follow, read or do, one line each with its source, with no theme. They tick \
what's done; when they tell you they did one, tick it with tick_points; \
bars, numbers to compare, a bar each, on one scale per set: the theme is the set's \
caption (\"9/11 und die Kriege danach\"), each point's text the bar's label (\"In den \
Kriegen danach\") and its amount the number, plain (900000), with a few words beside it as \
its note (\"getötet\", \"im CV\"); look warn marks a gap. The numbers of a set count the \
same thing in the same unit, so their bars compare: hours of sleep and hours without food are \
two sets, or no bars; \
questions, questions they'd be asked, to rehearse (in an interview, an exam): the theme is \
the question as it would be asked, its points the answer in short lines, and the first \
point's note says who asks (\"Viaplay, Senior Fullstack AI Engineer\"). A question with no \
answer yet has one point, with look warn, saying what's missing. When they practise them, \
ask one at a time, and tick a question's points with tick_points once they answered it \
well; \
table, things compared across the same facts, a row each, the thing's name as the theme: the \
block's facets are its columns, as they are a card's lines (Preis tag, Wo map-pin, Vegan \
leaf), and a cell is the row's point with that facet, in a few words (\"709 kr\", \"Ja\", \
\"Nein, Nubukleder\"); a number cell has its amount too (709), so its column sorts, and look \
warn marks a cell that needs a look (over budget, not vegan). A row's points with no facet \
show on its own page. A table and cards keep the same points, so a block of cards becomes a \
table by its kind, when comparing them matters more than each one; \
trend, a number kept each time it's measured (a weight, a price, a score), a line per theme: \
the theme is what's measured (\"Lisas Gewicht\"), each point one measurement, its amount the \
number (3.4), happened the day it was measured (2026-09-24), its note the unit (\"kg\") and its \
text a few words on it (\"Beim Tierarzt gewogen\"). A point with no day is a mark they said \
they aim at (text \"Ziel\"), never a number told without its day. When they tell you a new \
measurement, keep it as a new point, never in place of the last; \
parts, a whole split into the parts it's made of, a ring per theme: the theme is the whole's \
caption (\"Wo man arbeitet\"), each point a part, its text the part's name (\"Remote\") and \
its amount how many (11), and the first point's note says what they count (\"Anzeigen\"). At \
most five parts, the largest first. The parts add up to the whole: numbers that don't (an ad \
asks for many skills) stay bars; \
timeline, what happened when, which the page lists oldest first: the theme is the section of \
the page it belongs to, named exactly as that section or card is (\"Intifada\"), each point \
one line on what happened (\"Die Erste Intifada\"), and it says when it happened: a year \
(1959), a span (1987–1993) or a day (11. September 2001), with \"v. Chr.\" or \"BC\" before \
the year 1, in the talk's language. When the page has a timeline and a talk explains \
something that happened at a time, it keeps both: the explanation in its section, and one \
line with its date on the timeline. \
The page is a grid of twelve columns, and each block takes a width of it: 1/4, 1/3, 1/2, 2/3, \
3/4 or whole. One that names none has its kind's: 1/3 for chips, trends and parts, 1/2 for \
bars, questions, lists and timelines, the whole for the rest. Blocks fill the rows in their \
order, and a row's blocks grow to fill it, so a small block sits beside its neighbour when \
their widths add up (a trend at 1/3 before text at 2/3). \
When no block fits a point, and only then, it may start a new block after the others, with \
a kind and a short title.";

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
