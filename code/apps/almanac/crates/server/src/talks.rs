//! A talk beside a topic's page. Each question sends Claude a fixed prompt,
//! what the topic sticks to, what its page has kept, and only this talk;
//! the answer streams back as it's written. What the page should keep
//! (points under their themes, questions when the answer rested on a
//! guess, reminders it offers, the topic an off-topic question fits better)
//! Claude Code hands over with Almanac's tools as it answers (`crate::mcp`);
//! through the Messages API, Claude reads the exchange once more for it.
//!
//! In the workspace's own topic Claude reads the workspace instead of a
//! page, and proposes topics and changes them.

use std::convert::Infallible;

use almanac_domain::{Actor, AskRequest, ModelKind, Operation, Source};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{Local, Utc};
use futures::Stream;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::acp::{ClaudeCode, Failure, Question};
use crate::auth::Member;
use crate::claude::{Problem, Turn};
use crate::error::AppError;
use crate::kept::{Answering, Kept, fields, reminder};
use crate::mcp::{Asking, Mode};
use crate::page::{Page, Workspace};
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
const KIND: &str = "SELECT kind FROM topics WHERE id = $1 AND workspace_id = $2";
const WEB: &str = "SELECT web FROM claude_choices";

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
- Say plainly when you're unsure.
- Name where something comes from when it matters: a book, a guide, an institution, a site, or \
what they told you. Never make up links.
- If the question has nothing to do with this topic, answer it in a sentence or two, then say \
it fits better in another topic, naming one of the other topics or a new one.
- Answer in the language they write in.";

/// With Claude Code: Almanac's tools keep the page as it answers.
const ANSWER_TOOLS: &str = "Keep the page with Almanac's tools while you answer: keep_point for \
what's worth knowing later, ask_to_check when your answer rested on a guess about their \
situation, offer_reminder when the talk settled something recurring or dated, fits_elsewhere \
when the question belongs in another topic, and name_talk on a talk's first answer. Write only \
the answer: they don't see the tools, so don't mention them or narrate using them.";

const WEB_ON: &str = "You can search the web and read pages. Do it when the answer depends on \
things that change (prices, laws, opening hours, news) or that you aren't sure of, and keep the \
page's url with what you keep from it.";
const WEB_OFF: &str = "You can't search the web: for things that change, such as prices, laws \
or news, say your knowledge may be out of date.";

const OWN: &str = "You are Almanac, a notebook that keeps what someone learns, one topic at a \
time. Each topic has a page that keeps points by theme, talks beside it, and reminders on the \
calendar. This talk is in the workspace's own topic: you know every topic in it, and help \
start new ones and change them.

How to answer:
- When it's unclear what a new topic should be about, ask one question back.
- When a topic for it exists already, name it and say they can ask there.
- When they ask what's in the workspace, answer from the topics below.
- About Almanac itself: a topic keeps what you learn about one thing; its talks answer inside \
it, and its page keeps the points worth knowing later. Reminders a talk settles go on the \
calendar and come as notifications on the Mac.
- Be short and concrete: a few sentences. No headings, no preamble, no closing offers.
- Answer in the language they write in.";

/// With Claude Code: the workspace's own topic proposes and changes topics.
const OWN_TOOLS: &str = "- When they say what they want to keep track of, and no topic has it, \
propose one with propose_topic: a short name, what it sticks to, an icon, a group (reuse one \
that fits), the shape and three or four starters. Its points are only what they told you, such \
as which plants they have and where; what you know about it, such as care advice, waits for the \
topic's talks, which keep it with its source. Add a reminder only when they asked for one or \
clearly need it. Then say in a sentence or two what it will keep, and which reminder it will \
offer. Never say it's started or set up: they start it with the card under your answer, or \
change it first.
- When they ask to change a topic (rename it, move it to another group, change what it sticks \
to, its icon or its shape), do it with change_topic, then say what you changed. They can undo it.
- Name the talk with name_talk on its first answer.
- Write only the answer. They don't see the tools, so don't mention them or narrate using them.";

/// The workspace's own topic through the Messages API: no tools.
const OWN_WITHOUT_TOOLS: &str = "- Here you can't start or change topics yourself: suggest a \
name and what it sticks to, and say they start it with New topic in the sidebar.";

const KEEP: &str = "You keep the page of one topic in Almanac, a notebook that keeps what someone \
learns. After each exchange in a talk, you decide what the page keeps from it. Return JSON in the \
schema's shape.

- points: what's worth knowing later from this exchange: facts, decisions, and what they told you \
about their situation. Nothing the page already has. Usually none to four. On a page of cards \
each theme is one thing the topic is about, named plainly (a plant, a pet, a place), and a point \
is one short sentence, under 25 words. In a document each theme is a section of one piece of \
writing (such as Problem, Market, Team), and it's read later as writing, so a point keeps what \
the answer explained the way it explained it: two to four sentences on what it is and why it \
matters, starting with the term it's about in **bold** when there is one. What they told you \
stays one sentence. Use an existing theme when one fits.
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
    let kind: String = sqlx::query_scalar(KIND)
        .bind(topic_id)
        .bind(actor.workspace_id)
        .fetch_one(&state.pool)
        .await?;
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

    let own = kind == "almanac";
    let (events, heard) = mpsc::unbounded_channel::<Event>();
    let _ = events.send(json_event("talk", json!({ "talk_id": talk_id })));
    tokio::spawn(async move {
        answer(&state, actor, &page, own, talk_id, &text, events).await;
    });
    let stream = futures::stream::unfold(heard, |mut heard| async move {
        heard.recv().await.map(|event| (Ok(event), heard))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// An answer, the pages it came from, and, from Claude Code, what its
/// tools kept.
struct Said {
    text: String,
    sources: Vec<Source>,
    kept: Option<Kept>,
}

async fn answer(
    state: &AppState,
    actor: Actor,
    page: &Page,
    own: bool,
    talk_id: Uuid,
    question: &str,
    events: mpsc::UnboundedSender<Event>,
) {
    let answer_id = Uuid::now_v7();
    let answers: i64 = sqlx::query_scalar(ANSWERS_SO_FAR)
        .bind(talk_id)
        .bind(actor.workspace_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(1);
    let at = Answering {
        topic_id: page.id,
        topic_name: page.name.clone(),
        talk_id,
        answer_id,
        first: answers == 0,
    };
    let said = match &state.claude_code {
        Some(claude_code) => {
            say_with_claude_code(state, claude_code, actor, page, own, talk_id, &events).await
        }
        None => say(state, actor, page, own, talk_id, &events)
            .await
            .map(|text| Said {
                text,
                sources: Vec::new(),
                kept: None,
            }),
    };
    let said = match said {
        Ok(said) => said,
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
    let mut message = fields([
        ("talk_id", json!(talk_id)),
        ("topic_id", json!(page.id)),
        ("role", json!("almanac")),
        ("text", json!(said.text)),
        ("sources", json!(said.sources)),
    ]);
    let alone = Operation::Create {
        model: ModelKind::Message,
        id: answer_id,
        data: message.clone(),
    };
    let saved = match &said.kept {
        Some(kept) => {
            let (fields, mut operations) = kept.writes(&at);
            message.extend(fields);
            operations.insert(
                0,
                Operation::Create {
                    model: ModelKind::Message,
                    id: answer_id,
                    data: message,
                },
            );
            match write(state, actor, operations).await {
                Ok(()) => Ok(()),
                // What it kept doesn't fit: the answer still stands.
                Err(error) => {
                    tracing::warn!(%error, "couldn't keep what the answer kept");
                    write(state, actor, vec![alone]).await
                }
            }
        }
        None => write(state, actor, vec![alone]).await,
    };
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
    if said.kept.is_none()
        && !own
        && let Err(error) = keep(state, actor, page, &at, question, &said.text).await
    {
        tracing::warn!(%error, "couldn't keep what the talk said");
    }
}

/// Claude Code's answer to the talk's last question, streamed to `events`,
/// with what Almanac's tools kept meanwhile.
async fn say_with_claude_code(
    state: &AppState,
    claude_code: &ClaudeCode,
    actor: Actor,
    page: &Page,
    own: bool,
    talk_id: Uuid,
    events: &mpsc::UnboundedSender<Event>,
) -> Result<Said, Problem> {
    let trouble = |error: sqlx::Error| {
        tracing::warn!(%error, "couldn't read the talk");
        Problem("Almanac couldn't read this talk. Ask again.".into())
    };
    let said: Vec<(String, String)> = sqlx::query_as(TALK)
        .bind(talk_id)
        .bind(actor.workspace_id)
        .fetch_all(&state.pool)
        .await
        .map_err(trouble)?;
    let web: bool = sqlx::query_scalar(WEB)
        .fetch_optional(&state.pool)
        .await
        .map_err(trouble)?
        .unwrap_or(true);
    let web_line = if web { WEB_ON } else { WEB_OFF };
    let system = if own {
        let workspace = Workspace::load(&state.pool, actor.workspace_id)
            .await
            .map_err(trouble)?;
        format!(
            "{OWN}\n{OWN_TOOLS}\n\n{web_line}\n\nToday is {}.\n\n{}\nThe groups so far: {}.\n",
            today(),
            workspace.describe(),
            match workspace.groups() {
                groups if groups.is_empty() => "none yet".into(),
                groups => groups.join(", "),
            }
        )
    } else {
        format!(
            "{ANSWER}\n\n{ANSWER_TOOLS}\n\n{web_line}\n\nToday is {}.\n\n{}",
            today(),
            page.describe()
        )
    };
    let mode = if own { Mode::Own } else { Mode::Topic };
    let open = state.questions.open(Asking::new(actor, page.id, mode));
    let tools = mode.tools();
    let prompt = talk_prompt(&said);
    let question = Question {
        system: &system,
        prompt: &prompt,
        web,
        tools,
        token: &open.token,
    };
    let answer = claude_code
        .ask(&question, |piece| {
            let _ = events.send(json_event("text", json!({ "text": piece })));
        })
        .await
        .map_err(problem_of)?;
    match answer.stop.as_str() {
        "refusal" => {
            return Err(Problem(
                "Claude declined to answer this. Ask it another way.".into(),
            ));
        }
        "cancelled" => {
            return Err(Problem(
                "The answer stopped before it finished. Ask again.".into(),
            ));
        }
        _ => {}
    }
    let text = answer.text.trim().to_string();
    if text.is_empty() {
        return Err(Problem("Claude Code gave no answer. Ask again.".into()));
    }
    let kept = open.asking.take_kept();
    Ok(Said {
        sources: sources(&answer.read, &kept),
        text,
        kept: Some(kept),
    })
}

/// What they ask, after the talk so far. Never starting with a slash,
/// which Claude Code would take for one of its commands.
fn talk_prompt(said: &[(String, String)]) -> String {
    let Some(((_, last), before)) = said.split_last() else {
        return String::new();
    };
    if before.is_empty() {
        return format!("They ask: {last}");
    }
    let mut out = String::from("The talk so far:\n");
    for (role, text) in before {
        out.push_str(if role == "you" {
            "\nThey asked: "
        } else {
            "\nYou answered: "
        });
        out.push_str(text);
        out.push('\n');
    }
    out.push_str("\nNow they ask: ");
    out.push_str(last);
    out
}

/// Why Claude Code gave no answer, in words for the talk.
fn problem_of(failure: Failure) -> Problem {
    Problem(match failure {
        Failure::NotInstalled => "Almanac answers with Claude Code on this Mac, and its ACP \
             adapter isn't installed. Settings → Claude has the command that installs it; \
             then ask again."
            .into(),
        Failure::SignedOut => "Claude Code isn't signed in on this Mac. Run claude in Terminal \
             and sign in there, then ask again."
            .into(),
        Failure::Trouble(said) => said,
    })
}

/// The pages an answer came from: those its points name first, then the
/// others it read.
fn sources(read: &[String], kept: &Kept) -> Vec<Source> {
    let mut urls: Vec<&str> = Vec::new();
    for url in kept
        .points
        .iter()
        .filter_map(|p| p.url.as_deref())
        .chain(read.iter().map(String::as_str))
    {
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls.into_iter()
        .take(8)
        .map(|url| Source {
            title: host(url),
            url: Some(url.to_string()),
        })
        .collect()
}

/// `rhs.org.uk` for `https://www.rhs.org.uk/plants/monstera`.
fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

/// Claude's answer to the talk's last question, through the Messages API,
/// streamed to `events`.
async fn say(
    state: &AppState,
    actor: Actor,
    page: &Page,
    own: bool,
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
    let system = if own {
        let workspace = Workspace::load(&state.pool, actor.workspace_id)
            .await
            .map_err(trouble)?;
        format!(
            "{OWN}\n{OWN_WITHOUT_TOOLS}\n\n{WEB_OFF}\n\nToday is {}.\n\n{}",
            today(),
            workspace.describe()
        )
    } else {
        format!(
            "{ANSWER}\n\n{WEB_OFF}\n\nToday is {}.\n\n{}",
            today(),
            page.describe()
        )
    };
    state
        .claude
        .answer(&key, &system, &turns, |piece| {
            let _ = events.send(json_event("text", json!({ "text": piece })));
        })
        .await
}

/// What the page keeps from one exchange, read by Claude afterwards, and
/// the words for it under the answer.
async fn keep(
    state: &AppState,
    actor: Actor,
    page: &Page,
    at: &Answering,
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
    let from: Keep = state
        .claude
        .json(&key, KEEP, &prompt, keep_schema())
        .await
        .map_err(AppError::internal)?;
    let mut kept = Kept::default();
    for point in &from.points {
        let _ = kept.keep_point(&point.theme, &point.text, &point.source, "");
    }
    for question in &from.questions {
        let _ = kept.ask_to_check(&question.theme, &question.text, &question.point);
    }
    for r in &from.reminders {
        if let Ok(reminder) = reminder(&r.theme, &r.title, &r.due_on, &r.at, Some(r.every_days)) {
            let _ = kept.offer_reminder(reminder);
        }
    }
    kept.elsewhere = Some(from.elsewhere);
    kept.talk_title = Some(from.talk_title);
    let (said, mut operations) = kept.writes(at);
    if !said.is_empty() {
        operations.insert(
            0,
            Operation::Update {
                model: ModelKind::Message,
                id: at.answer_id,
                data: said,
            },
        );
    }
    write(state, actor, operations).await
}

fn today() -> String {
    Local::now().format("%A, %Y-%m-%d").to_string()
}

fn json_event(name: &str, data: Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_talk_never_starts_with_a_slash() {
        let said = vec![("you".to_string(), "/clear".to_string())];
        assert_eq!(talk_prompt(&said), "They ask: /clear");
        let said = vec![
            ("you".to_string(), "Wie oft gießen?".to_string()),
            ("almanac".to_string(), "Alle 7 Tage.".to_string()),
            ("you".to_string(), "Und im Winter?".to_string()),
        ];
        let prompt = talk_prompt(&said);
        assert!(prompt.starts_with("The talk so far:"));
        assert!(prompt.ends_with("Now they ask: Und im Winter?"));
    }

    #[test]
    fn a_source_is_named_by_its_site() {
        assert_eq!(host("https://www.rhs.org.uk/plants/monstera"), "rhs.org.uk");
        assert_eq!(host("http://example.org?q=1"), "example.org");
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
