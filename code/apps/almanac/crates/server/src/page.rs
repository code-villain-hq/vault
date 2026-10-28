//! A topic's page as Claude reads it: what the topic sticks to, the points
//! it kept by theme, the questions it waits on, its reminders, and the
//! names of the other topics, for questions that belong there.

use std::fmt::Write as _;

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

const TOPIC: &str = "SELECT name, sticks_to, title, shape FROM topics \
     WHERE id = $1 AND workspace_id = $2";
const POINTS: &str = "SELECT theme, text, source FROM points \
     WHERE topic_id = $1 AND workspace_id = $2 ORDER BY position, created_at";
const QUESTIONS: &str = "SELECT text FROM questions \
     WHERE topic_id = $1 AND workspace_id = $2 AND status = 'open' ORDER BY created_at";
const REMINDERS: &str = "SELECT title, due_on, every_days FROM reminders \
     WHERE topic_id = $1 AND workspace_id = $2 AND status IN ('on', 'offered') ORDER BY due_on";
const OTHERS: &str =
    "SELECT name FROM topics WHERE workspace_id = $2 AND id <> $1 ORDER BY position";

/// A theme and its points: each a text and where it comes from.
pub type Theme = (String, Vec<(String, Option<String>)>);

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
    pub reminders: Vec<(String, NaiveDate, Option<i32>)>,
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
        let points: Vec<(String, String, Option<String>)> = sqlx::query_as(POINTS)
            .bind(id)
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;
        let mut themes: Vec<Theme> = Vec::new();
        for (theme, text, source) in points {
            match themes.iter_mut().find(|(t, _)| *t == theme) {
                Some((_, kept)) => kept.push((text, source)),
                None => themes.push((theme, vec![(text, source)])),
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

    /// The topic and what it has kept, in plain text.
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
        for (theme, points) in &self.themes {
            let _ = writeln!(out, "## {theme}");
            for (text, source) in points {
                match source.as_deref().filter(|s| !s.is_empty()) {
                    Some(source) => {
                        let _ = writeln!(out, "- {text} ({source})");
                    }
                    None => {
                        let _ = writeln!(out, "- {text}");
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
            for (title, due_on, every) in &self.reminders {
                match every {
                    Some(days) => {
                        let _ = writeln!(out, "- {title}: next on {due_on}, every {days} days");
                    }
                    None => {
                        let _ = writeln!(out, "- {title}: on {due_on}");
                    }
                }
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
