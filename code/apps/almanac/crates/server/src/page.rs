//! A topic's page as Claude reads it: what the topic sticks to, its blocks
//! in order with the points each keeps, the questions it waits on, its
//! reminders, and the names of the other topics, for questions that belong
//! there. For the workspace's own topic, the workspace: its topics by
//! group, what each sticks to, its blocks and how much it has.

use std::fmt::Write as _;

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

const TOPIC: &str = "SELECT name, sticks_to FROM topics WHERE id = $1 AND workspace_id = $2";
const BLOCKS: &str = "SELECT id, kind, title FROM blocks \
     WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const POINTS: &str = "SELECT id, block_id, theme, text, source FROM points \
     WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
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

/// A point as the page has it: its id, theme, text and where it comes from.
pub type PagePoint = (Uuid, String, String, Option<String>);

/// A reminder as the page has it: its id, title, next day, rhythm, and
/// whether it's `on` or only `offered`.
pub type PageReminder = (Uuid, String, NaiveDate, Option<i32>, String);

/// One block of the page, with its points in order.
pub struct PageBlock {
    pub id: Uuid,
    /// One of [`almanac_domain::BLOCK_KINDS`].
    pub kind: String,
    pub title: String,
    pub points: Vec<PagePoint>,
}

/// A block by the reference Claude reads it with.
#[derive(Debug, Clone)]
pub struct RefBlock {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
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
        let blocks: Vec<(Uuid, String, String)> = sqlx::query_as(BLOCKS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        let mut blocks: Vec<PageBlock> = blocks
            .into_iter()
            .map(|(id, kind, title)| PageBlock {
                id,
                kind,
                title,
                points: Vec::new(),
            })
            .collect();
        let points: Vec<(Uuid, Uuid, String, String, Option<String>)> = sqlx::query_as(POINTS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        for (id, block_id, theme, text, source) in points {
            if let Some(block) = blocks.iter_mut().find(|b| b.id == block_id) {
                block.points.push((id, theme, text, source));
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
                })
                .collect(),
            points: self
                .blocks
                .iter()
                .flat_map(|b| b.points.iter().map(|(id, ..)| (*id, b.id)))
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
            let _ = writeln!(out, "## [b{}] {title} ({})", b + 1, block.kind);
            if block.points.is_empty() && block.kind != "week" {
                out.push_str("Nothing yet.\n");
            }
            let mut theme = None;
            for (_, in_theme, text, source) in &block.points {
                if !in_theme.is_empty() && theme != Some(in_theme) {
                    let _ = writeln!(out, "### {in_theme}");
                    theme = Some(in_theme);
                }
                n += 1;
                match source.as_deref().filter(|s| !s.is_empty()) {
                    Some(source) => {
                        let _ = writeln!(out, "- [p{n}] {text} ({source})");
                    }
                    None => {
                        let _ = writeln!(out, "- [p{n}] {text}");
                    }
                }
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

/// What each kind of block shows, for Claude to keep each point where the
/// page shows it.
const BLOCKS_SAY: &str = "a page is its blocks, in order, each with a title of its own, and \
each lays out its points one way. A point goes in one block, named by its reference (b2) or \
its title; a theme groups points inside a block. The kinds: \
today, the steps of what comes round on the day: the theme is what comes round, named the way \
its reminders are (\"Elvital wash\"), and each point says what, a colon, then how (\"Elvital \
shampoo: on the scalp, lukewarm water\"), in the order they're done; \
week, the days of the topic's reminders, which keeps no points: offer a reminder instead; \
chips, facts in a few words (\"Low porosity, by the float test\"), with no theme; \
shelf, a tile for each thing they use, the thing's name as the theme (\"Elvital Hyaluron \
Plump\"): its first point says what it is in two or three words (\"Shampoo and \
conditioner\"), its second how they use it, any more are notes; \
cards, a card for each thing the topic is about (a plant, a pet, a place), its name as the \
theme, each point one short sentence, under 20 words; \
text, sections of one piece of writing, the section's heading as the theme (Problem, \
Market, Team): each point keeps what the answer explained the way it explained it, two to \
four sentences, starting with the term it's about in **bold** when there is one, and what \
they told you stays one sentence; \
list, items to follow or read, one line each with its source, with no theme. \
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
