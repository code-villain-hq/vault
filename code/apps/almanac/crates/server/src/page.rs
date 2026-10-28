//! A topic's page as Claude reads it: what the topic sticks to, the points
//! it kept by theme, the questions it waits on, its reminders, and the
//! names of the other topics, for questions that belong there. For the
//! workspace's own topic, the workspace: its topics by group, what each
//! sticks to, and how much it has.

use std::fmt::Write as _;

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

const TOPIC: &str = "SELECT name, sticks_to, title, shape FROM topics \
     WHERE id = $1 AND workspace_id = $2";
const POINTS: &str = "SELECT id, theme, text, source FROM points \
     WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const QUESTIONS: &str = "SELECT text FROM questions \
     WHERE topic_id = $1 AND workspace_id = $2 AND status = 'open' ORDER BY created_at";
const REMINDERS: &str = "SELECT id, title, due_on, every_days, status FROM reminders \
     WHERE topic_id = $1 AND workspace_id = $2 AND status IN ('on', 'offered') ORDER BY due_on";
const OTHERS: &str = "SELECT name FROM topics \
     WHERE workspace_id = $2 AND id <> $1 AND kind = 'topic' ORDER BY position";
const WORKSPACE: &str = "SELECT name FROM workspaces WHERE id = $1";
const TOPICS: &str = "SELECT t.name, t.group_name, t.sticks_to, t.shape, \
       (SELECT count(*) FROM points p WHERE p.topic_id = t.id), \
       (SELECT max(k.last_at)::date FROM talks k WHERE k.topic_id = t.id) \
     FROM topics t WHERE t.workspace_id = $1 AND t.kind = 'topic' \
     ORDER BY t.group_name = '', t.group_name, t.position, t.created_at";

/// A theme and its points: each its id, a text and where it comes from.
pub type Theme = (String, Vec<(Uuid, String, Option<String>)>);

/// A reminder as the page has it: its id, title, next day, rhythm, and
/// whether it's `on` or only `offered`.
pub type PageReminder = (Uuid, String, NaiveDate, Option<i32>, String);

/// The page's points and reminders by the references Claude reads them
/// with: `p3` is the third point as the page lists them, `r2` the second
/// reminder.
#[derive(Debug, Default, Clone)]
pub struct Refs {
    pub points: Vec<Uuid>,
    /// Each with whether it's only offered yet.
    pub reminders: Vec<(Uuid, bool)>,
}

pub struct Page {
    pub id: Uuid,
    pub name: String,
    pub sticks_to: String,
    pub title: Option<String>,
    /// `cards` or `document`.
    pub shape: String,
    /// Each theme with its points, in page order.
    pub themes: Vec<Theme>,
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
        let topic: Option<(String, String, Option<String>, String)> = sqlx::query_as(TOPIC)
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
        let Some((name, sticks_to, title, shape)) = topic else {
            return Ok(None);
        };
        let points: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(POINTS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        let mut themes: Vec<Theme> = Vec::new();
        for (id, theme, text, source) in points {
            match themes.iter_mut().find(|(t, _)| *t == theme) {
                Some((_, kept)) => kept.push((id, text, source)),
                None => themes.push((theme, vec![(id, text, source)])),
            }
        }
        Ok(Some(Page {
            id,
            name,
            sticks_to,
            title,
            shape,
            themes,
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

    /// The references `describe` gives the points and reminders.
    pub fn refs(&self) -> Refs {
        Refs {
            points: self
                .themes
                .iter()
                .flat_map(|(_, points)| points.iter().map(|(id, ..)| *id))
                .collect(),
            reminders: self
                .reminders
                .iter()
                .map(|(id, .., status)| (*id, status == "offered"))
                .collect(),
        }
    }

    /// The topic and what it has kept, in plain text, each point and
    /// reminder with its reference.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "The topic: {}", self.name);
        if let Some(title) = self.title.as_deref().filter(|t| !t.is_empty()) {
            let _ = writeln!(out, "About: {title}");
        }
        let sticks_to = match self.sticks_to.trim() {
            "" => "Not written yet. Stay with what the name means.",
            s => s,
        };
        let _ = writeln!(out, "What it sticks to: {sticks_to}");
        let _ = writeln!(
            out,
            "The page's shape: {}",
            if self.shape == "document" {
                "document (each theme is a section of one piece of writing)"
            } else {
                "cards (each theme is one thing the topic is about)"
            }
        );
        out.push_str("\nWhat the page has kept, by theme:\n");
        if self.themes.is_empty() {
            out.push_str("Nothing yet.\n");
        }
        let mut n = 0;
        for (theme, points) in &self.themes {
            let _ = writeln!(out, "## {theme}");
            for (_, text, source) in points {
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

/// The workspace as its own topic reads it.
pub struct Workspace {
    pub name: String,
    /// Name, group, what it sticks to, shape, how many points, the day of
    /// its last talk.
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
        for (name, in_group, sticks_to, shape, points, last) in &self.topics {
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
            let last = match last {
                Some(day) => format!(", last talk on {day}"),
                None => ", no talks yet".into(),
            };
            let _ = writeln!(
                out,
                "- {name} ({shape}, {points} points{last}). Sticks to: {sticks_to}"
            );
        }
        out
    }
}
