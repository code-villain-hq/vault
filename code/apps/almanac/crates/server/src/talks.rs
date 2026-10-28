//! A talk beside a topic's page. Each question sends Claude a fixed prompt,
//! what the topic sticks to, what its page has kept, and only this talk;
//! the answer streams back as it's written. Then Claude reads the exchange
//! once more for what the page should keep: points under their themes,
//! questions when the answer rested on a guess, reminders it offers, and
//! the topic an off-topic question fits better.

use std::convert::Infallible;

use almanac_domain::{Actor, AskRequest, ModelKind, Operation};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{Local, NaiveDate, NaiveTime, Utc};
use futures::Stream;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::Member;
use crate::claude::{Problem, Turn};
use crate::error::AppError;
use crate::page::Page;
use crate::state::AppState;
use crate::topics::{cut, write};

const TALK_IN_TOPIC: &str =
    "SELECT EXISTS (SELECT 1 FROM talks WHERE id = $1 AND topic_id = $2 AND workspace_id = $3)";
const TALK: &str = "SELECT role, text FROM ( \
     SELECT role, text, created_at FROM messages \
     WHERE talk_id = $1 AND workspace_id = $2 AND problem IS NULL \
     ORDER BY created_at DESC LIMIT 40) m ORDER BY created_at";
const ANSWERS_SO_FAR: &str =
    "SELECT count(*) FROM messages WHERE talk_id = $1 AND workspace_id = $2 AND role = 'almanac'";

const ANSWER: &str = "You are Almanac, a notebook that keeps what someone learns, one topic at a \
time. You answer in a talk inside one topic; the page beside the talk keeps what's worth knowing \
later.

How to answer:
- Stay inside this topic. Answer in its frame and build on what the page has kept, without \
reciting it back.
- Be short and concrete: a few sentences or a short list. No headings, no preamble, no closing \
offers.
- When the answer depends on something you don't know about their situation (which plant, which \
light, which audience), say what you assume and ask one question back.
- Say plainly when you're unsure. You can't search the web: for things that change, such as \
prices, laws or news, say your knowledge may be out of date.
- Name where something comes from when it matters: a book, a guide, an institution, or what they \
told you. Never make up links.
- If the question has nothing to do with this topic, answer it in a sentence or two, then say \
it fits better in another topic, naming one of the other topics or a new one.
- Answer in the language they write in.";

const KEEP: &str = "You keep the page of one topic in Almanac, a notebook that keeps what someone \
learns. After each exchange in a talk, you decide what the page keeps from it. Return JSON in the \
schema's shape.

- points: what's worth knowing later from this exchange: facts, decisions, and what they told you \
about their situation. One short sentence each, under 25 words. Nothing the page already has. \
Usually none to four. On a page of cards each theme is one thing the topic is about, named \
plainly (a plant, a pet, a place); in a document each theme is a section of one piece of writing \
(such as Problem, Market, Team). Use an existing theme when one fits.
- source: where the point comes from, in a few words: \"You said\", \"From the talk\", or the \
book, guide or institution the answer relied on. Never a link.
- questions: when the answer rested on a guess about their situation, ask to check it, with the \
point to keep if they say yes (text \"Does the Monstera stand by the north window?\", point \
\"Stands by the north window.\"). Point is \"\" when there's nothing to keep. At most two, and \
none the page is already waiting on.
- reminders: only when the exchange settled something recurring or dated they'd want to be \
reminded of (watering every 7 days, repotting in March, sending the deck on Friday). title says \
what to do (\"Water the Monstera\"), due_on is the next day it's due (YYYY-MM-DD), at is a time \
(\"HH:MM\") or \"\", every_days is the rhythm in days, or 0 for once. None the calendar already \
has.
- elsewhere: when the question had nothing to do with this topic, the topic it fits (one of the \
others, or a short new name); otherwise \"\".
- talk_title: a title for the talk, under six words.

Write in the language of the talk.";

#[derive(Deserialize)]
struct Keep {
    points: Vec<KeepPoint>,
    questions: Vec<KeepQuestion>,
    reminders: Vec<KeepReminder>,
    elsewhere: String,
    talk_title: String,
}

#[derive(Deserialize)]
struct KeepPoint {
    theme: String,
    text: String,
    source: String,
}

#[derive(Deserialize)]
struct KeepQuestion {
    theme: String,
    text: String,
    point: String,
}

#[derive(Deserialize)]
struct KeepReminder {
    theme: String,
    title: String,
    due_on: String,
    at: String,
    every_days: i64,
}

fn keep_schema() -> Value {
    let object = |fields: Value| {
        let required: Vec<&String> = fields
            .as_object()
            .map(|f| f.keys().collect())
            .unwrap_or_default();
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": required,
            "properties": fields,
        })
    };
    let string = json!({ "type": "string" });
    let list = |item: Value| json!({ "type": "array", "items": item });
    object(json!({
        "points": list(object(json!({ "theme": string, "text": string, "source": string }))),
        "questions": list(object(json!({ "theme": string, "text": string, "point": string }))),
        "reminders": list(object(json!({
            "theme": string,
            "title": string,
            "due_on": string,
            "at": string,
            "every_days": { "type": "integer" },
        }))),
        "elsewhere": string,
        "talk_title": string,
    }))
}

/// `POST /api/w/{id}/topics/{topic_id}/ask`: the answer, as server-sent
/// events. The answer is written and kept even when nobody's listening
/// any more.
pub async fn ask(
    State(state): State<AppState>,
    member: Member,
    Path((_, topic_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<AskRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    member.require_not_guest()?;
    let actor = member.actor;
    let text = request.text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::invalid("Ask something first."));
    }
    let page = Page::load(&state.pool, actor.workspace_id, topic_id)
        .await?
        .ok_or_else(|| AppError::not_found("topic not found"))?;
    let now = json!(Utc::now());
    let (talk_id, mut operations) = match request.talk_id {
        Some(talk_id) => {
            let known: bool = sqlx::query_scalar(TALK_IN_TOPIC)
                .bind(talk_id)
                .bind(topic_id)
                .bind(actor.workspace_id)
                .fetch_one(&state.pool)
                .await?;
            if !known {
                return Err(AppError::not_found("talk not found"));
            }
            let update = Operation::Update {
                model: ModelKind::Talk,
                id: talk_id,
                data: fields([("last_at", now)]),
            };
            (talk_id, vec![update])
        }
        None => {
            let talk_id = Uuid::now_v7();
            let create = Operation::Create {
                model: ModelKind::Talk,
                id: talk_id,
                data: fields([
                    ("topic_id", json!(topic_id)),
                    ("title", json!(cut(&text, 60))),
                    ("last_at", now),
                ]),
            };
            (talk_id, vec![create])
        }
    };
    operations.push(Operation::Create {
        model: ModelKind::Message,
        id: Uuid::now_v7(),
        data: fields([
            ("talk_id", json!(talk_id)),
            ("topic_id", json!(topic_id)),
            ("role", json!("you")),
            ("text", json!(text)),
        ]),
    });
    write(&state, actor, operations).await?;

    let (events, heard) = mpsc::unbounded_channel::<Event>();
    let _ = events.send(json_event("talk", json!({ "talk_id": talk_id })));
    tokio::spawn(async move {
        answer(&state, actor, &page, talk_id, &text, events).await;
    });
    let stream = futures::stream::unfold(heard, |mut heard| async move {
        heard.recv().await.map(|event| (Ok(event), heard))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn answer(
    state: &AppState,
    actor: Actor,
    page: &Page,
    talk_id: Uuid,
    question: &str,
    events: mpsc::UnboundedSender<Event>,
) {
    let answer_id = Uuid::now_v7();
    let answered = match say(state, actor, page, talk_id, &events).await {
        Ok(text) => text,
        Err(problem) => {
            let saved = write(
                state,
                actor,
                vec![Operation::Create {
                    model: ModelKind::Message,
                    id: answer_id,
                    data: fields([
                        ("talk_id", json!(talk_id)),
                        ("topic_id", json!(page.id)),
                        ("role", json!("almanac")),
                        ("text", json!("")),
                        ("problem", json!(problem.0)),
                    ]),
                }],
            )
            .await;
            if let Err(error) = saved {
                tracing::warn!(%error, "couldn't save why there's no answer");
            }
            let _ = events.send(json_event("problem", json!({ "text": problem.0 })));
            return;
        }
    };
    let saved = write(
        state,
        actor,
        vec![Operation::Create {
            model: ModelKind::Message,
            id: answer_id,
            data: fields([
                ("talk_id", json!(talk_id)),
                ("topic_id", json!(page.id)),
                ("role", json!("almanac")),
                ("text", json!(answered)),
            ]),
        }],
    )
    .await;
    if let Err(error) = saved {
        tracing::warn!(%error, "couldn't save the answer");
        let _ = events.send(json_event(
            "problem",
            json!({ "text": "Almanac couldn't save the answer. Ask again." }),
        ));
        return;
    }
    let _ = events.send(json_event("done", json!({ "message_id": answer_id })));
    drop(events);
    if let Err(error) = keep(state, actor, page, talk_id, answer_id, question, &answered).await {
        tracing::warn!(%error, "couldn't keep what the talk said");
    }
}

/// Claude's answer to the talk's last question, streamed to `events`.
async fn say(
    state: &AppState,
    actor: Actor,
    page: &Page,
    talk_id: Uuid,
    events: &mpsc::UnboundedSender<Event>,
) -> Result<String, Problem> {
    let trouble = |error: sqlx::Error| {
        tracing::warn!(%error, "couldn't read the talk");
        Problem("Almanac couldn't read this talk. Ask again.".into())
    };
    let key = state
        .claude
        .key(&state.pool)
        .await
        .map_err(trouble)?
        .ok_or_else(|| {
            Problem("Almanac needs a Claude key to answer. Add one in Settings → Claude.".into())
        })?;
    let said: Vec<(String, String)> = sqlx::query_as(TALK)
        .bind(talk_id)
        .bind(actor.workspace_id)
        .fetch_all(&state.pool)
        .await
        .map_err(trouble)?;
    let turns: Vec<Turn> = said
        .into_iter()
        .map(|(role, text)| Turn {
            role: if role == "you" { "user" } else { "assistant" },
            text,
        })
        .collect();
    let system = format!("{ANSWER}\n\nToday is {}.\n\n{}", today(), page.describe());
    state
        .claude
        .answer(&key, &system, &turns, |piece| {
            let _ = events.send(json_event("text", json!({ "text": piece })));
        })
        .await
}

/// What the page keeps from one exchange, and the words for it under the
/// answer.
async fn keep(
    state: &AppState,
    actor: Actor,
    page: &Page,
    talk_id: Uuid,
    answer_id: Uuid,
    question: &str,
    answer: &str,
) -> Result<(), AppError> {
    let Some(key) = state.claude.key(&state.pool).await? else {
        return Ok(());
    };
    let prompt = format!(
        "Today is {}.\n\n{}\nThe exchange:\n\nThey asked: {question}\n\nAlmanac answered: {answer}",
        today(),
        page.describe(),
    );
    let kept: Keep = state
        .claude
        .json(&key, KEEP, &prompt, keep_schema())
        .await
        .map_err(AppError::internal)?;

    let mut operations = Vec::new();
    let mut themes: Vec<String> = Vec::new();
    let mut points = 0;
    for point in &kept.points {
        let (theme, text) = (cut(&point.theme, 120), cut(&point.text, 2000));
        if theme.is_empty() || text.is_empty() {
            continue;
        }
        points += 1;
        if !themes.contains(&theme) {
            themes.push(theme.clone());
        }
        operations.push(Operation::Create {
            model: ModelKind::Point,
            id: Uuid::now_v7(),
            data: fields([
                ("topic_id", json!(page.id)),
                ("theme", json!(theme)),
                ("text", json!(text)),
                ("source", optional(&point.source, 200)),
                ("talk_id", json!(talk_id)),
            ]),
        });
    }
    for question in kept.questions.iter().take(2) {
        let text = cut(&question.text, 2000);
        if text.is_empty() {
            continue;
        }
        operations.push(Operation::Create {
            model: ModelKind::Question,
            id: Uuid::now_v7(),
            data: fields([
                ("topic_id", json!(page.id)),
                ("theme", optional(&question.theme, 120)),
                ("text", json!(text)),
                ("point", optional(&question.point, 2000)),
                ("talk_id", json!(talk_id)),
            ]),
        });
    }
    for reminder in &kept.reminders {
        let title = cut(&reminder.title, 200);
        let Ok(due_on) = NaiveDate::parse_from_str(reminder.due_on.trim(), "%Y-%m-%d") else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let at = NaiveTime::parse_from_str(reminder.at.trim(), "%H:%M").ok();
        let every = (1..=366)
            .contains(&reminder.every_days)
            .then_some(reminder.every_days);
        operations.push(Operation::Create {
            model: ModelKind::Reminder,
            id: Uuid::now_v7(),
            data: fields([
                ("topic_id", json!(page.id)),
                ("theme", optional(&reminder.theme, 120)),
                ("title", json!(title)),
                ("due_on", json!(due_on)),
                ("at", json!(at)),
                ("every_days", json!(every)),
                ("status", json!("offered")),
                ("message_id", json!(answer_id)),
            ]),
        });
    }
    let mut said = Map::new();
    if points > 0 {
        said.insert("kept".into(), json!(kept_words(points, &themes)));
    }
    let elsewhere = cut(&kept.elsewhere, 80);
    if !elsewhere.is_empty() && elsewhere != page.name {
        said.insert("elsewhere".into(), json!(elsewhere));
    }
    if !said.is_empty() {
        operations.push(Operation::Update {
            model: ModelKind::Message,
            id: answer_id,
            data: said,
        });
    }
    let answers: i64 = sqlx::query_scalar(ANSWERS_SO_FAR)
        .bind(talk_id)
        .bind(actor.workspace_id)
        .fetch_one(&state.pool)
        .await?;
    let title = cut(&kept.talk_title, 80);
    if answers <= 1 && !title.is_empty() {
        operations.push(Operation::Update {
            model: ModelKind::Talk,
            id: talk_id,
            data: fields([("title", json!(title))]),
        });
    }
    write(state, actor, operations).await
}

/// "Kept 2 points under Monstera and Basilikum."
fn kept_words(points: usize, themes: &[String]) -> String {
    let what = if points == 1 {
        "a point".to_string()
    } else {
        format!("{points} points")
    };
    let under = match themes {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("Kept {what} under {under}.")
}

fn today() -> String {
    Local::now().format("%A, %Y-%m-%d").to_string()
}

fn optional(text: &str, max: usize) -> Value {
    let text = cut(text, max);
    if text.is_empty() {
        Value::Null
    } else {
        json!(text)
    }
}

fn fields<const N: usize>(pairs: [(&str, Value); N]) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn json_event(name: &str, data: Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_words_name_the_themes() {
        assert_eq!(
            kept_words(1, &["Monstera".into()]),
            "Kept a point under Monstera."
        );
        assert_eq!(
            kept_words(3, &["Monstera".into(), "Basilikum".into(), "Efeu".into()]),
            "Kept 3 points under Monstera, Basilikum and Efeu."
        );
    }

    #[test]
    fn the_keep_schema_requires_every_field() {
        let schema = keep_schema();
        assert_eq!(schema["required"].as_array().map(Vec::len), Some(5));
        assert_eq!(
            schema["properties"]["reminders"]["items"]["required"]
                .as_array()
                .map(Vec::len),
            Some(5)
        );
    }
}
