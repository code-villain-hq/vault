//! What one answer keeps: points under their themes, questions to check,
//! reminders it offers, the topic an off-topic question fits, the talk's
//! title, and, in the workspace's own topic, a topic it proposes and the
//! topics it changed. A point or reminder may take the place of ones the
//! page has, by the references the page gave them (`crate::page::Refs`):
//! the page is kept up to date, not added to. Claude Code hands them over with Almanac's tools
//! while it answers (`crate::mcp`); through the Messages API one more
//! request reads them from the exchange (`crate::talks`). Either way they
//! are written with the answer, through the sync engine.

use almanac_domain::{ModelKind, Operation, Proposal, ProposedReminder, TopicChange};
use chrono::{NaiveDate, NaiveTime};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::page::Refs;
use crate::topics::cut;

#[derive(Debug, Default)]
pub struct Kept {
    /// The page's points and reminders, for what replaces them.
    pub refs: Refs,
    pub points: Vec<KeptPoint>,
    pub questions: Vec<KeptQuestion>,
    pub reminders: Vec<KeptReminder>,
    /// The topic an off-topic question fits better, by name.
    pub elsewhere: Option<String>,
    pub talk_title: Option<String>,
    pub proposal: Option<Proposal>,
    pub changes: Vec<TopicChange>,
}

#[derive(Debug)]
pub struct KeptPoint {
    pub theme: String,
    pub text: String,
    pub source: Option<String>,
    pub url: Option<String>,
    /// The page's points it takes the place of.
    pub replaces: Vec<Uuid>,
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
/// from one exchange is a list copied in.
const MOST_POINTS: usize = 8;
const MOST_QUESTIONS: usize = 2;
const MOST_REMINDERS: usize = 3;

impl Kept {
    /// Keeping for a page with these points and reminders.
    pub fn on(refs: Refs) -> Self {
        Kept {
            refs,
            ..Kept::default()
        }
    }

    /// Keeps a point for the page, in place of the points it `replaces`
    /// (`p3`). What's wrong with it comes back in words, for Claude to fix.
    pub fn keep_point(
        &mut self,
        theme: &str,
        text: &str,
        source: &str,
        url: &str,
        replaces: &[String],
    ) -> Result<(), String> {
        let (theme, text) = (cut(theme, 120), cut(text, 2000));
        if theme.is_empty() || text.is_empty() {
            return Err("A point needs a theme and a text.".into());
        }
        if self.points.len() >= MOST_POINTS {
            return Err(format!(
                "This answer already keeps {MOST_POINTS} points: keep only the ones worth knowing later."
            ));
        }
        let replaces = replaces
            .iter()
            .map(|r| {
                found(r, 'p', &self.refs.points)
                    .ok_or_else(|| format!("The page has no point {r}: name one like p3."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if self.points.iter().any(|p| p.text == text) {
            return Ok(());
        }
        let url = cut(url, 2000);
        self.points.push(KeptPoint {
            theme,
            text,
            source: some(cut(source, 200)),
            url: (url.starts_with("https://") || url.starts_with("http://")).then_some(url),
            replaces,
        });
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
        let ids: Vec<Uuid> = self.refs.reminders.iter().map(|(id, _)| *id).collect();
        let replaces = replaces
            .iter()
            .map(|r| {
                found(r, 'r', &ids)
                    .ok_or_else(|| format!("The calendar has no reminder {r}: name one like r2."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.reminders.push(KeptReminder { reminder, replaces });
        Ok(())
    }

    /// The answer's own fields (`kept`, `elsewhere`, `proposal`,
    /// `changes`), and the writes that come after it.
    pub fn writes(&self, at: &Answering) -> (Map<String, Value>, Vec<Operation>) {
        let mut operations = Vec::new();
        let mut themes: Vec<String> = Vec::new();
        let mut replaced: Vec<Uuid> = Vec::new();
        let mut updated = 0;
        for point in &self.points {
            if !themes.contains(&point.theme) {
                themes.push(point.theme.clone());
            }
            let data = fields([
                ("theme", json!(point.theme)),
                ("text", json!(point.text)),
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
        if !self.points.is_empty() {
            message.insert(
                "kept".into(),
                json!(kept_words(self.points.len(), updated, &themes)),
            );
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
        if !self.changes.is_empty() {
            message.insert("changes".into(), json!(self.changes));
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

/// What a reference such as `p3` (or `[p3]`) names among `ids`.
fn found(reference: &str, kind: char, ids: &[Uuid]) -> Option<Uuid> {
    let n: usize = reference
        .trim()
        .trim_matches(['[', ']'])
        .strip_prefix(kind)?
        .parse()
        .ok()?;
    ids.get(n.checked_sub(1)?).copied()
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

pub fn fields<const N: usize>(pairs: [(&str, Value); N]) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use almanac_domain::TopicPatch;

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

    #[test]
    fn a_point_takes_the_place_of_the_ones_it_replaces() {
        let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
        let mut kept = Kept::on(Refs {
            points: vec![first, second],
            reminders: vec![],
        });
        assert!(
            kept.keep_point("Wäsche", "x", "", "", &["p3".into()])
                .unwrap_err()
                .contains("no point p3")
        );
        kept.keep_point(
            "Wäsche",
            "Washes Wednesday and Friday.",
            "You said",
            "",
            &["p1".into(), "[p2]".into()],
        )
        .unwrap();
        let (said, operations) = kept.writes(&answering());
        assert!(matches!(
            &operations[..],
            [
                Operation::Update { id: a, data, .. },
                Operation::Delete { id: b, .. },
            ] if *a == first && *b == second && data["seen"] == json!(false)
        ));
        assert_eq!(said["kept"], json!("Updated a point under Wäsche."));
    }

    #[test]
    fn a_reminder_replaces_one_on_the_calendar_once_its_taken() {
        let (on, offered) = (Uuid::now_v7(), Uuid::now_v7());
        let mut kept = Kept::on(Refs {
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
