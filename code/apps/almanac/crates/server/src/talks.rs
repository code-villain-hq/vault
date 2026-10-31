//! A talk beside a topic's page. Each question sends Claude a fixed prompt,
//! what the topic sticks to, what its page has kept, and only this talk;
//! the answer streams back as it's written. What the page should keep
//! (points in the page's blocks, questions when the answer rested on a
//! guess, reminders it offers, the topic an off-topic question fits better)
//! Claude Code hands over with Almanac's tools as it answers (`crate::mcp`);
//! through the Messages API, Claude reads the exchange once more for it.
//!
//! In the workspace's own topic Claude reads the workspace instead of a
//! page, and proposes topics and changes them.
//!
//! A question may come with photos (`crate::photos`); Claude sees the
//! talk's photos with each question.

use std::convert::Infallible;

use almanac_domain::{
    Actor, AskRequest, FIELD_KINDS, ModelKind, Operation, SET_KINDS, Source, Transaction,
};
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
use crate::kept::{Answering, FieldArg, ItemArg, Kept, PointChange, RowArg, fields, reminder};
use crate::mcp::{Asking, Mode};
use crate::page::{Page, Workspace};
use crate::photos::{self, Image};
use crate::state::AppState;
use crate::topics::{cut, write};

const TALK_IN_TOPIC: &str =
    "SELECT EXISTS (SELECT 1 FROM talks WHERE id = $1 AND topic_id = $2 AND workspace_id = $3)";
const TALK: &str = "SELECT id, role, text FROM ( \
     SELECT id, role, text, created_at FROM messages \
     WHERE talk_id = $1 AND workspace_id = $2 AND problem IS NULL \
     ORDER BY created_at DESC LIMIT 40) m ORDER BY created_at";
/// The talk's title and its first question, which it keeps as its title
/// until an answer names it.
const TITLE_AND_QUESTION: &str = "SELECT t.title, coalesce((SELECT m.text FROM messages m \
     WHERE m.talk_id = t.id AND m.role = 'you' ORDER BY m.created_at LIMIT 1), '') \
     FROM talks t WHERE t.id = $1 AND t.workspace_id = $2";
const KIND: &str = "SELECT kind FROM topics WHERE id = $1 AND workspace_id = $2";
const WEB: &str = "SELECT web FROM claude_choices";

const ANSWER: &str = "You are Almanac, a notebook that keeps what someone learns, one topic at a \
time. You answer in a talk inside one topic; the page beside the talk keeps what's worth knowing \
later.

How to answer:
- What the topic sticks to is its brief: what it's about and how its talks answer. Follow it; \
where it says otherwise than the rules here (another language, longer answers), it wins.
- Stay inside this topic. Answer in its frame and build on what the page has kept, without \
reciting it back.
- Be short and concrete: a few sentences or a short list. No headings, no preamble, no closing \
offers.
- When the answer depends on something you don't know about their situation (which plant, which \
light, which audience), say what you assume and ask one question back.
- Say plainly when you're unsure.
- Name where something comes from when it matters: a book, a guide, an institution, a site, or \
what they told you, as a link where you use it, never as a list at the end. Never make up links.
- When they send photos, read them closely: names, labels, ingredients, what's in the picture. \
Say what you read when your answer rests on it.
- If the question has nothing to do with this topic, answer it in a sentence or two, then say \
it fits better in another topic, naming one of the other topics or a new one.
- Answer in the language they write in.";

/// With Claude Code: Almanac's tools keep the page as it answers.
const ANSWER_TOOLS: &str = "Keep the page with Almanac's tools while you answer. File what's \
worth knowing later where it belongs: add_items for paragraphs, callouts and what they told \
you about themselves; add_rows for things, terms, steps, events and measurements, with a cell \
per field; add_block, or a field with change_block, only when the page has nowhere for it. The page is \
read at a glance, as pictures of what it keeps: what a field can hold (a number, a day, a choice, \
yes or no) goes in a cell, not a sentence, and a paragraph is a sentence or two that only reads \
as prose. The page \
is kept up to date, not added to: change_points when something changed or was wrong, remove \
what's wrong or twice, move what reads better elsewhere. tick when they did a row to do, \
ask_to_check when your answer rested on a guess about their situation, offer_reminder when the \
talk settled something recurring or dated (for a row to do, naming it), fits_elsewhere \
when the question belongs in another topic, change_brief when they tell you how this topic \
should answer from now on or correct the same thing a second time, and name_talk while the talk \
has no name. When they ask to tidy up, turn sentences that hold what a collection's fields keep \
into its rows (\"Lisa wiegt etwa 3,4 kg\" into Gewicht): move the sentence to the collection, \
then change_points gives it its name as text and its cells, so it keeps its source; add the \
field with change_block first if it has none for it. A text field that holds yes or no, a number \
or a day with words around it is tidied too: change its kind with change_block, give each row \
its cell, and the words around it a text field of its own. Tidy in small steps: one collection's \
sentences an answer, which they see as its picture under your answer and can undo; then say in \
a sentence what's left to tidy, and do the next when they say so. Never pick a colour, a width \
or a picture: the app draws the page from what it keeps; only when they ask for another picture \
of a collection, pick one its fields allow. Write only the answer: they don't see the tools, so \
don't mention them or narrate using them. What you keep in a collection shows under your answer \
as its picture (bars, a board, a line), so the answer doesn't list those rows again: it says in \
a sentence or two what they mean for them, and what to do next.";

const WEB_ON: &str = "You can search the web and read pages. Do it when the answer depends on \
things that change (prices, laws, opening hours, news) or that you aren't sure of, and keep the \
page's url with what you keep from it.";
const WEB_OFF: &str = "You can't search the web: for things that change, such as prices, laws \
or news, say your knowledge may be out of date.";

const OWN: &str = "You are Almanac, a notebook that keeps what someone learns, one topic at a \
time. Each topic has a page that keeps what they learn in blocks, talks beside it, and reminders on the \
calendar. This talk is in the workspace's own topic: you know every topic in it, and help \
start new ones and change them.

How to answer:
- When it's unclear what a new topic should be about, ask one question back.
- When they send photos, read what's on them and use it.
- When a topic for it exists already, name it and say they can ask there.
- When they ask what's in the workspace, answer from the topics below.
- About Almanac itself: a topic keeps what you learn about one thing; its talks answer inside \
it, and its page keeps the points worth knowing later. Reminders a talk settles go on the \
calendar and come as notifications on the Mac.
- Be short and concrete: a few sentences. No headings, no preamble, no closing offers.
- Answer in the language they write in.";

/// With Claude Code: the workspace's own topic proposes and changes topics.
const OWN_TOOLS: &str = "- When they say what they want to keep track of, and no topic has it, \
propose one with propose_topic: a short name, what it sticks to (its brief), an icon, a group (reuse one \
that fits), the template it starts as and three or four starters. Its points are only what they told you, such \
as which plants they have and where; what you know about it, such as care advice, waits for the \
topic's talks, which keep it with its source. Add a reminder only when they asked for one or \
clearly need it. Then say in a sentence or two what it will keep, and which reminder it will \
offer. Never say it's started or set up: they start it with the card under your answer, or \
change it first.
- When they ask to change a topic (rename it, move it to another group, change what it sticks \
to or its icon), do it with change_topic, then say what you changed. They can undo it.
- Name the talk with name_talk while it has no name.
- Write only the answer. They don't see the tools, so don't mention them or narrate using them.";

/// A talk through the Messages API: the page keeps what it said
/// afterwards, but nothing moves blocks around.
const ANSWER_WITHOUT_TOOLS: &str = "You can't move the page's blocks from here: when they ask \
for that, say so, and how you'd lay it out.";

/// The workspace's own topic through the Messages API: no tools.
const OWN_WITHOUT_TOOLS: &str = "- Here you can't start or change topics yourself: suggest a \
name and what it sticks to, and say they start it with New topic in the sidebar.";

const KEEP: &str = "You keep the page of one topic in Almanac, a notebook that keeps what someone \
learns. After each exchange in a talk, you decide what the page keeps from it and what changes \
on it. Return JSON in the schema's shape; a list with nothing in it is [].

- blocks: only when the page has nowhere for what you keep, a block to start: its kind, its \
short title (what its points share), the row whose own page it goes on (\"p3\") or \"\" for \
the topic's, and for a collection its fields (name, kind, unit, goal \
as a plain number or \"\", options), each the narrowest kind that holds its fact: yes or no a \
check, a price a number with its unit, a date a day; a reason or a note a text field of its own. \
Items and rows name it by its title.
- items: sentences worth knowing later, in a block that isn't a collection: what explains, \
decisions, and what they told you about their situation. Nothing the page already has, and \
nothing a collection's field can hold (a number, a day, a choice): that is a row. \
Usually none to three. block names it as the page does (\"b2\") or by its title; look only in \
a callout (\"warn\" or \"info\"), \"\" otherwise; text as its block's kind asks; source where it comes from, in a few words (\"You \
said\", \"From the talk\", the book, guide or institution). Never a link.
- rows: things, terms, steps, events and measurements, in a collection: name, and cells, a field each \
(field, value) as the block names its fields, only for what they or a source said. A number \
plain (3.4), a day as 2026-09-24, a check yes or no.
- changes: the page is kept up to date, not added to. When the exchange changes or corrects a \
point the page has (a wash day moved, a weighing corrected), the point (\"p3\") with its new \
text (\"\" when only cells change) and the cells that change.
- remove: points or blocks that are wrong now, or say twice what another does (\"p4\"). When \
they asked to tidy up, a sentence that holds what a collection's fields keep (\"Lisa wiegt etwa \
3,4 kg\" and a collection Gewicht) becomes its row: the row in rows, with the sentence's source, \
and the sentence here. One collection's sentences an exchange; the answer says what's left.
- ticks: rows to do the exchange says are done (\"p3\"): rows of a collection with a check.
- questions: when the answer rested on a guess about their situation, ask to check it, with the \
point to keep if they say yes (text \"Does the Monstera stand by the north window?\", point \
\"Stands by the north window.\"). Point is \"\" when there's nothing to keep. Only for a guess \
the answer didn't ask about: a question the answer asked waits for their reply in the talk, so \
it's never asked here too. At most two, and none the page is already waiting on.
- reminders: only when the exchange settled something recurring or dated they'd want to be \
reminded of (watering every 7 days, repotting in March, sending the deck on Friday). A routine \
they told you they keep counts (washing their hair three times a week): one reminder for each \
day it comes round, so something a few times a week is a reminder per weekday, each every 7 \
days; when they didn't say which days, spread them evenly and the answer says which. title says \
what to do in two to four words (\"Water the Monstera\"). item is the row to do it rings \
for (\"p3\"), or \"\"; ticking the item ticks the reminder. due_on is the next day it's due \
(YYYY-MM-DD), at is a time (\"HH:MM\") or \"\", every_days is the rhythm in days, or 0 for once. \
replaces names the reminders it takes the place of, as the page names them ([\"r2\"]): when a \
reminder changes, offer the new one in place of the old. None the calendar already has.
- elsewhere: when the question had nothing to do with this topic, the topic it fits (one of the \
others, or a short new name); otherwise \"\".
- brief: only when they told you how this topic should answer from now on (\"always say when a \
source was written\", \"in English from now on\"), never for a wish about one answer: sticks_to \
is what the topic sticks to, its brief, whole, as it is from now on: all of it kept, with the rule \
added or changed where it fits. said is what changed, in a short sentence (\"Names when each \
source was written.\"). Otherwise both \"\".
- talk_title: a title for the talk, under six words.

Never pick a colour, a width or a picture: the app draws the page from what it keeps. Write in \
the language of the talk.";

#[derive(Deserialize)]
struct Keep {
    #[serde(default)]
    blocks: Vec<KeepBlock>,
    #[serde(default)]
    items: Vec<KeepItem>,
    #[serde(default)]
    rows: Vec<KeepRow>,
    #[serde(default)]
    changes: Vec<KeepChange>,
    #[serde(default)]
    remove: Vec<String>,
    #[serde(default)]
    ticks: Vec<String>,
    questions: Vec<KeepQuestion>,
    reminders: Vec<KeepReminder>,
    elsewhere: String,
    #[serde(default)]
    brief: KeepBrief,
    talk_title: String,
}

#[derive(Deserialize, Default)]
struct KeepBrief {
    sticks_to: String,
    said: String,
}

#[derive(Deserialize)]
struct KeepBlock {
    kind: String,
    title: String,
    /// The row whose page it goes on (p3), or "" for the topic's.
    #[serde(default)]
    row: String,
    #[serde(default)]
    fields: Vec<KeepField>,
}

#[derive(Deserialize)]
struct KeepField {
    name: String,
    kind: String,
    #[serde(default)]
    unit: String,
    /// A string, as every field the schema asks for is required.
    #[serde(default)]
    goal: String,
    #[serde(default)]
    options: Vec<String>,
}

#[derive(Deserialize)]
struct KeepItem {
    block: String,
    #[serde(default)]
    look: String,
    text: String,
    source: String,
}

#[derive(Deserialize)]
struct KeepRow {
    block: String,
    name: String,
    #[serde(default)]
    cells: Vec<KeepCell>,
    source: String,
}

#[derive(Deserialize)]
struct KeepCell {
    field: String,
    value: String,
}

#[derive(Deserialize)]
struct KeepChange {
    point: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    cells: Vec<KeepCell>,
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
    #[serde(default)]
    item: String,
    due_on: String,
    at: String,
    every_days: i64,
    #[serde(default)]
    replaces: Vec<String>,
}

/// Cells as the API gives them, a field and its value each.
fn cells_of(cells: &[KeepCell]) -> serde_json::Map<String, Value> {
    cells
        .iter()
        .map(|c| (c.field.clone(), json!(c.value)))
        .collect()
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
    let cells = list(object(json!({ "field": string, "value": string })));
    object(json!({
        "blocks": list(object(json!({
            "kind": { "type": "string", "enum": SET_KINDS },
            "title": string,
            "row": string,
            "fields": list(object(json!({
                "name": string,
                "kind": { "type": "string", "enum": FIELD_KINDS },
                "unit": string,
                "goal": string,
                "options": list(string.clone()),
            }))),
        }))),
        "items": list(object(json!({
            "block": string,
            "look": { "type": "string", "enum": ["", "warn", "info"] },
            "text": string,
            "source": string,
        }))),
        "rows": list(object(json!({
            "block": string,
            "name": string,
            "cells": cells,
            "source": string,
        }))),
        "changes": list(object(json!({ "point": string, "text": string, "cells": cells }))),
        "remove": list(string.clone()),
        "ticks": list(string.clone()),
        "questions": list(object(json!({ "theme": string, "text": string, "point": string }))),
        "reminders": list(object(json!({
            "theme": string,
            "title": string,
            "item": string,
            "due_on": string,
            "at": string,
            "every_days": { "type": "integer" },
            "replaces": list(string.clone()),
        }))),
        "elsewhere": string,
        "brief": object(json!({ "sticks_to": string, "said": string })),
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
    let about = request
        .about
        .as_deref()
        .map(|a| cut(a, 120))
        .filter(|a| !a.is_empty());
    let uploads = photos::check(request.photos.as_deref().unwrap_or_default())?;
    if text.is_empty() && uploads.is_empty() {
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
                    ("title", json!(title(&text, uploads.len()))),
                    ("last_at", now),
                ]),
            };
            (talk_id, vec![create])
        }
    };
    let question_id = Uuid::now_v7();
    let mut question = fields([
        ("talk_id", json!(talk_id)),
        ("topic_id", json!(topic_id)),
        ("role", json!("you")),
        ("text", json!(text)),
    ]);
    if !uploads.is_empty() {
        let photos: Vec<_> = uploads.iter().map(|u| &u.photo).collect();
        question.insert("photos".into(), json!(photos));
    }
    operations.push(Operation::Create {
        model: ModelKind::Message,
        id: question_id,
        data: question,
    });
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    photos::write_with(&state, actor, &transaction, question_id, &uploads).await?;

    let own = kind == "almanac";
    let (events, heard) = mpsc::unbounded_channel::<Event>();
    let _ = events.send(json_event("talk", json!({ "talk_id": talk_id })));
    tokio::spawn(async move {
        let row = request.row.and_then(|id| page.row(id));
        let about = (!own).then(|| about_line(about.as_deref(), row));
        let about = about.as_deref().filter(|a| !a.is_empty());
        answer(&state, actor, &page, own, talk_id, &text, about, events).await;
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

#[allow(clippy::too_many_arguments)]
async fn answer(
    state: &AppState,
    actor: Actor,
    page: &Page,
    own: bool,
    talk_id: Uuid,
    question: &str,
    about: Option<&str>,
    events: mpsc::UnboundedSender<Event>,
) {
    let answer_id = Uuid::now_v7();
    let unnamed = sqlx::query_as::<_, (String, String)>(TITLE_AND_QUESTION)
        .bind(talk_id)
        .bind(actor.workspace_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .is_some_and(|(title, question)| unnamed(&title, &question));
    let at = Answering {
        topic_id: page.id,
        topic_name: page.name.clone(),
        talk_id,
        answer_id,
        unnamed,
    };
    let said = match &state.claude_code {
        Some(claude_code) => {
            let asked = Asked {
                page,
                own,
                talk_id,
                about,
                unnamed,
            };
            say_with_claude_code(state, claude_code, actor, &asked, &events).await
        }
        None => say(state, actor, page, own, talk_id, about, &events)
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
                    let mut alone = alone;
                    if let Operation::Create { data, .. } = &mut alone {
                        data.insert(
                            "kept".into(),
                            json!("Almanac couldn't change the page this time. Ask again."),
                        );
                    }
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
        && let Err(error) = keep(state, actor, page, &at, question, about, &said.text).await
    {
        tracing::warn!(%error, "couldn't keep what the talk said");
    }
}

/// What a question is asked in: the topic's page, or the workspace's own
/// topic, the talk, and the section open in front of them.
struct Asked<'a> {
    page: &'a Page,
    own: bool,
    talk_id: Uuid,
    about: Option<&'a str>,
    /// The talk still has its first question for a title.
    unnamed: bool,
}

/// "They ask from the section Intifada…", after the page, when they ask
/// with a section of it open, or a row's page: its reference's number,
/// its collection and its name.
fn about_line(about: Option<&str>, row: Option<(usize, &str, &str)>) -> String {
    if let Some((n, collection, name)) = row {
        return format!(
            "\nThey ask from the page of the row [p{n}] {name}, in {collection}, which is open in \
             front of them: what they ask is about it. What fits one of {collection}'s fields \
             changes p{n}'s cells. The rest goes on its page, in blocks that name the row \
             (p{n}): what it has several of, such as offers or repottings, a collection there; \
             sentences about it a paragraph there.\n"
        );
    }
    match about {
        Some(about) => format!(
            "\nThey ask from the section {about} of the page, which is open in front of them: \
             what they ask is about it, and what's kept from it goes under {about}. Asked \
             there to lay the page out, such as to make a timeline of it, move and draw only \
             what's under {about}; the rest of the page stays as it is.\n"
        ),
        None => String::new(),
    }
}

/// Claude Code's answer to the talk's last question, streamed to `events`,
/// with what Almanac's tools kept meanwhile.
async fn say_with_claude_code(
    state: &AppState,
    claude_code: &ClaudeCode,
    actor: Actor,
    asked: &Asked<'_>,
    events: &mpsc::UnboundedSender<Event>,
) -> Result<Said, Problem> {
    let Asked {
        page,
        own,
        talk_id,
        about,
        unnamed,
    } = *asked;
    let trouble = |error: sqlx::Error| {
        tracing::warn!(%error, "couldn't read the talk");
        Problem("Almanac couldn't read this talk. Ask again.".into())
    };
    let said = the_talk(state, actor, talk_id).await.map_err(trouble)?;
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
            "{ANSWER}\n\n{ANSWER_TOOLS}\n\n{web_line}\n\nToday is {}.\n\n{}{}",
            today(),
            page.describe(),
            about.unwrap_or_default()
        )
    };
    let system = if unnamed {
        format!("{system}\n{UNNAMED}\n")
    } else {
        system
    };
    let mode = if own { Mode::Own } else { Mode::Topic };
    let mut kept = Kept::on(page.refs());
    kept.photos = said
        .iter()
        .flat_map(|line| line.images.iter().map(|i| i.id))
        .collect();
    let open = state
        .questions
        .open(Asking::new(actor, page.id, mode).on(kept));
    let tools = mode.tools();
    let (prompt, images) = talk_prompt(&said);
    let question = Question {
        system: &system,
        prompt: &prompt,
        images: &images,
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

/// One thing said in a talk, with the photos a question came with.
struct Line {
    role: String,
    text: String,
    images: Vec<Image>,
}

/// The talk so far, the last 40 things said, with their photos.
async fn the_talk(state: &AppState, actor: Actor, talk_id: Uuid) -> Result<Vec<Line>, sqlx::Error> {
    let said: Vec<(Uuid, String, String)> = sqlx::query_as(TALK)
        .bind(talk_id)
        .bind(actor.workspace_id)
        .fetch_all(&state.pool)
        .await?;
    let mut images = photos::of_talk(&state.pool, actor.workspace_id, talk_id).await?;
    Ok(said
        .into_iter()
        .map(|(id, role, text)| Line {
            images: images.remove(&id).unwrap_or_default(),
            role,
            text,
        })
        .collect())
}

/// What they ask, after the talk so far, and the talk's photos in the
/// order they came. The photos are numbered (`acp.rs` puts "Photo 2"
/// before each), and each question says which it came with, so an earlier
/// photo isn't taken for a new one. Never starting with a slash, which
/// Claude Code would take for one of its commands.
fn talk_prompt(said: &[Line]) -> (String, Vec<Image>) {
    let Some((last, before)) = said.split_last() else {
        return (String::new(), Vec::new());
    };
    let mut out = String::new();
    let mut first = 1;
    if before.is_empty() {
        out.push_str("They ask");
    } else {
        out.push_str("The talk so far:\n");
        for line in before {
            out.push_str(if line.role == "you" {
                "\nThey asked"
            } else {
                "\nYou answered"
            });
            out.push_str(&with_photos(first, line.images.len()));
            first += line.images.len();
            out.push_str(": ");
            out.push_str(&line.text);
            out.push('\n');
        }
        out.push_str("\nNow they ask");
    }
    let earlier = first > 1;
    if last.images.is_empty() && earlier {
        out.push_str(" (with no new photo)");
    }
    out.push_str(&with_photos(first, last.images.len()));
    out.push_str(": ");
    out.push_str(&last.text);
    if !last.images.is_empty() {
        out.push_str("\n\nTheir photos follow, numbered as above.");
    } else if earlier {
        out.push_str(
            "\n\nThe photos from earlier in the talk follow, numbered as above, to look back \
             at. This question came with none.",
        );
    }
    let images: Vec<Image> = said.iter().flat_map(|l| l.images.clone()).collect();
    (out, images)
}

/// " (with photos 2 and 3)" after "They asked", for a question that had
/// some, numbered from `first` in the talk.
fn with_photos(first: usize, count: usize) -> String {
    match count {
        0 => String::new(),
        1 => format!(" (with photo {first})"),
        2 => format!(" (with photos {first} and {})", first + 1),
        n => format!(" (with photos {first} to {})", first + n - 1),
    }
}

/// Said when the talk has no name yet: on its first answer, or one after
/// answers that didn't name it.
const UNNAMED: &str = "This talk has no name yet: name it with name_talk, for what it is about.";

/// Whether the talk still has the title it started with, its first question.
fn unnamed(title: &str, question: &str) -> bool {
    title.is_empty()
        || [1, 2]
            .iter()
            .any(|&photos| title == self::title(question, photos))
}

/// A new talk's title: its first question, short, or what it showed.
fn title(text: &str, photos: usize) -> String {
    match (text.is_empty(), photos) {
        (false, _) => cut(text, 60),
        (true, 1) => "A photo".into(),
        (true, _) => "Photos".into(),
    }
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
    for url in kept.urls().chain(read.iter().map(String::as_str)) {
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
#[allow(clippy::too_many_arguments)]
async fn say(
    state: &AppState,
    actor: Actor,
    page: &Page,
    own: bool,
    talk_id: Uuid,
    about: Option<&str>,
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
    let said = the_talk(state, actor, talk_id).await.map_err(trouble)?;
    let turns: Vec<Turn> = said
        .into_iter()
        .map(|line| Turn {
            role: if line.role == "you" {
                "user"
            } else {
                "assistant"
            },
            text: line.text,
            images: line.images,
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
            "{ANSWER}\n\n{ANSWER_WITHOUT_TOOLS}\n\n{WEB_OFF}\n\nToday is {}.\n\n{}{}",
            today(),
            page.describe(),
            about.unwrap_or_default()
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
    about: Option<&str>,
    answer: &str,
) -> Result<(), AppError> {
    let Some(key) = state.claude.key(&state.pool).await? else {
        return Ok(());
    };
    let prompt = format!(
        "Today is {}.\n\n{}{}\nThe exchange:\n\nThey asked: {question}\n\nAlmanac answered: {answer}",
        today(),
        page.describe(),
        about.unwrap_or_default(),
    );
    let from: Keep = state
        .claude
        .json(&key, KEEP, &prompt, keep_schema())
        .await
        .map_err(AppError::internal)?;
    let mut kept = Kept::on(page.refs());
    // Each on its own: what doesn't fit the page is left out, not the rest.
    for block in &from.blocks {
        let fields: Vec<FieldArg> = block
            .fields
            .iter()
            .map(|f| FieldArg {
                name: f.name.clone(),
                kind: f.kind.clone(),
                unit: f.unit.clone(),
                goal: f.goal.trim().replace(',', ".").parse().ok(),
                options: f.options.clone(),
                was: String::new(),
            })
            .collect();
        let _ = kept.add_block(
            &block.kind,
            &block.title,
            None,
            &fields,
            Some(block.row.as_str()),
        );
    }
    for item in &from.items {
        let _ = kept.add_items(
            &item.block,
            &[ItemArg {
                text: item.text.clone(),
                look: item.look.clone(),
                source: item.source.clone(),
                url: String::new(),
            }],
        );
    }
    for row in &from.rows {
        let _ = kept.add_rows(
            &row.block,
            &[RowArg {
                name: row.name.clone(),
                cells: cells_of(&row.cells),
                source: row.source.clone(),
                url: String::new(),
            }],
        );
    }
    for change in &from.changes {
        let _ = kept.change_points(&[PointChange {
            point: change.point.clone(),
            text: Some(change.text.clone()).filter(|t| !t.trim().is_empty()),
            cells: Some(cells_of(&change.cells)).filter(|c| !c.is_empty()),
            ..PointChange::default()
        }]);
    }
    for what in &from.remove {
        let _ = kept.remove(std::slice::from_ref(what));
    }
    for item in &from.ticks {
        let _ = kept.tick(std::slice::from_ref(item), true);
    }
    for question in &from.questions {
        let _ = kept.ask_to_check(&question.theme, &question.text, &question.point);
    }
    for r in &from.reminders {
        let title = match r.title.trim() {
            "" => kept.item_words(&r.item).unwrap_or_default(),
            title => title.to_string(),
        };
        if let Ok(reminder) = reminder(&r.theme, &title, &r.due_on, &r.at, Some(r.every_days)) {
            let item = Some(r.item.as_str()).filter(|i| !i.trim().is_empty());
            let _ = kept.offer_reminder(reminder, &r.replaces, item);
        }
    }
    if !from.brief.sticks_to.trim().is_empty() {
        let _ = kept.change_brief(
            page.id,
            &page.name,
            &page.sticks_to,
            &from.brief.sticks_to,
            &from.brief.said,
        );
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

    fn line(role: &str, text: &str, photos: usize) -> Line {
        let image = Image {
            id: Uuid::nil(),
            media_type: "image/jpeg".into(),
            data: "AAAA".into(),
        };
        Line {
            role: role.into(),
            text: text.into(),
            images: vec![image; photos],
        }
    }

    #[test]
    fn a_row_page_says_which_row_they_ask_about() {
        assert_eq!(about_line(None, None), "");
        let section = about_line(Some("Futter"), None);
        assert!(section.contains("goes under Futter"));
        let row = about_line(Some("Monstera"), Some((4, "Pflanzen", "Monstera")));
        assert!(row.contains("the row [p4] Monstera, in Pflanzen"));
        assert!(row.contains("changes p4's cells"));
        assert!(row.contains("blocks that name the row (p4)"));
    }

    #[test]
    fn the_talk_never_starts_with_a_slash() {
        let (prompt, images) = talk_prompt(&[line("you", "/clear", 0)]);
        assert_eq!(prompt, "They ask: /clear");
        assert!(images.is_empty());
        let said = [
            line("you", "Wie oft gießen?", 0),
            line("almanac", "Alle 7 Tage.", 0),
            line("you", "Und im Winter?", 0),
        ];
        let (prompt, _) = talk_prompt(&said);
        assert!(prompt.starts_with("The talk so far:"));
        assert!(prompt.ends_with("Now they ask: Und im Winter?"));
    }

    #[test]
    fn the_talk_says_which_question_had_photos() {
        let said = [
            line("you", "Meine Shampoos.", 2),
            line("almanac", "Ein mildes und eine Kur.", 0),
            line("you", "Und das hier?", 1),
        ];
        let (prompt, images) = talk_prompt(&said);
        assert!(prompt.contains("They asked (with photos 1 and 2): Meine Shampoos."));
        assert!(prompt.contains("Now they ask (with photo 3): Und das hier?"));
        assert!(prompt.ends_with("Their photos follow, numbered as above."));
        assert_eq!(images.len(), 3);
    }

    #[test]
    fn an_earlier_photo_is_not_taken_for_a_new_one() {
        let said = [
            line("you", "Mein Shampoo.", 1),
            line("almanac", "Head & Shoulders.", 0),
            line("you", "Trocken oder platt?", 0),
        ];
        let (prompt, images) = talk_prompt(&said);
        assert!(prompt.contains("They asked (with photo 1): Mein Shampoo."));
        assert!(prompt.contains("Now they ask (with no new photo): Trocken oder platt?"));
        assert!(prompt.ends_with("This question came with none."));
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn a_talk_of_only_photos_is_named_for_them() {
        assert_eq!(title("", 1), "A photo");
        assert_eq!(title("", 3), "Photos");
        assert_eq!(title("Meine Routine", 2), "Meine Routine");
        assert!(unnamed("Meine Routine", "Meine Routine"));
        assert!(unnamed("Photos", ""));
        assert!(!unnamed("Gießplan", "Wann gieße ich die Monstera?"));
    }

    #[test]
    fn a_source_is_named_by_its_site() {
        assert_eq!(host("https://www.rhs.org.uk/plants/monstera"), "rhs.org.uk");
        assert_eq!(host("http://example.org?q=1"), "example.org");
    }

    #[test]
    fn the_keep_schema_requires_every_field() {
        let schema = keep_schema();
        let required = |path: &[&str]| {
            let mut at = &schema;
            for step in path {
                at = &at[step];
            }
            at["required"].as_array().map(Vec::len)
        };
        assert_eq!(required(&[]), Some(11));
        assert_eq!(required(&["properties", "brief"]), Some(2));
        assert_eq!(required(&["properties", "reminders", "items"]), Some(7));
        assert_eq!(required(&["properties", "rows", "items"]), Some(4));
        assert_eq!(
            required(&[
                "properties",
                "blocks",
                "items",
                "properties",
                "fields",
                "items"
            ]),
            Some(5)
        );
    }
}
