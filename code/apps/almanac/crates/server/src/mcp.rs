//! Almanac's tools for Claude Code, over MCP on this Mac's server
//! (`POST /api/mcp`), as Layer gives its conversations theirs. Each
//! question opens a token ([`Questions::open`]) that Claude Code sends as
//! its bearer, so a tool call lands on the question it belongs to and on
//! nothing else; the token goes when the answer is done. A tool doesn't
//! write: it collects what the answer keeps ([`Kept`]), and the talk
//! writes it with the answer, in one transaction. On a server, Claude Code
//! on the asker's Mac calls it the same way, for a question the server
//! handed it ([`crate::handed`]).

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard};

use almanac_domain::{
    Actor, Answered, FIELD_KINDS, ICONS, PageLayout, Proposal, ProposedPoint, SET_KINDS,
    STICKS_TO_MAX, TEMPLATE_NAMES, TopicChange, TopicPatch,
};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::kept::{self, FieldArg, ItemArg, Kept, PointChange, RowArg, reminder};
use crate::state::AppState;
use crate::topics::{SetUp, cut};

pub(crate) const PROTOCOL_VERSION: &str = "2025-06-18";

const TOPIC_NAMED: &str = "SELECT id, name, sticks_to, icon, group_name, kind \
     FROM topics WHERE workspace_id = $1 AND lower(name) = lower($2) \
     ORDER BY created_at LIMIT 1";
const SUB_TOPIC_NAMED: &str = "SELECT id, name FROM topics \
     WHERE workspace_id = $1 AND parent_id = $2 AND lower(name) = lower($3)";
const NEXT_POSITION: &str = "SELECT COALESCE(max(position) + 1, 0) FROM blocks \
     WHERE workspace_id = $1 AND topic_id = $2 AND row_id IS NULL";
const TOPIC_BY_ID: &str = "SELECT id, name, sticks_to, icon, group_name, kind \
     FROM topics WHERE workspace_id = $1 AND id = $2";
const THIS_OR_SUB_NAMED: &str = "SELECT id FROM topics \
     WHERE workspace_id = $1 AND (id = $2 OR parent_id = $2) AND lower(name) = lower($3)";
/// The sub-topics one answer proposes, at most.
const MOST_SUB_TOPICS: usize = 6;
const NAMES: &str = "SELECT name FROM topics WHERE workspace_id = $1 ORDER BY position";

/// What a question is for, and so which tools it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A talk in a topic: it keeps points, asks to check, offers reminders,
    /// and changes how the topic answers, its name, icon and group.
    Topic,
    /// A talk in the workspace's own topic: it proposes and changes topics.
    Own,
    /// Setting up a topic that was just started.
    SetUp,
}

impl Mode {
    /// The tools' names, for Claude Code's allowed tools.
    pub fn tools(self) -> &'static [&'static str] {
        match self {
            Mode::Topic => &[
                "add_block",
                "change_block",
                "add_items",
                "add_rows",
                "change_points",
                "tick",
                "move",
                "remove",
                "ask_to_check",
                "offer_reminder",
                "change_brief",
                "change_topic",
                "read_topic",
                "propose_sub_topic",
                "move_to_sub_topic",
                "fetch_picture",
                "set_cover",
                "name_talk",
                "fits_elsewhere",
            ],
            Mode::Own => &[
                "propose_topic",
                "change_topic",
                "name_talk",
                "fits_elsewhere",
            ],
            Mode::SetUp => &["set_up_topic"],
        }
    }
}

/// One question Claude Code is answering, and what its tools collected.
pub struct Asking {
    pub actor: Actor,
    pub topic_id: Uuid,
    pub mode: Mode,
    pub kept: Mutex<Kept>,
    pub set_up: Mutex<Option<SetUp>>,
    /// Where the asker's Mac says what it made of a question handed to it.
    pub handed: Mutex<Option<oneshot::Sender<Answered>>>,
}

impl Asking {
    pub fn new(actor: Actor, topic_id: Uuid, mode: Mode) -> Self {
        Asking {
            actor,
            topic_id,
            mode,
            kept: Mutex::new(Kept::default()),
            set_up: Mutex::new(None),
            handed: Mutex::new(None),
        }
    }

    /// Handed to the asker's Mac: where it'll say what it made of it.
    pub fn handed(self) -> (Self, oneshot::Receiver<Answered>) {
        let (said, answered) = oneshot::channel();
        let asking = Asking {
            handed: Mutex::new(Some(said)),
            ..self
        };
        (asking, answered)
    }

    /// Keeping on a page: its copy, with the talk's photos.
    pub fn on(self, kept: Kept) -> Self {
        Asking {
            kept: Mutex::new(kept),
            ..self
        }
    }

    /// What the tools collected, taken out.
    pub fn take_kept(&self) -> Kept {
        self.kept
            .lock()
            .map(|mut kept| std::mem::take(&mut *kept))
            .unwrap_or_default()
    }

    pub fn take_set_up(&self) -> Option<SetUp> {
        self.set_up.lock().ok().and_then(|mut s| s.take())
    }
}

/// The questions being answered, by their tokens.
#[derive(Clone, Default)]
pub struct Questions(Arc<Mutex<HashMap<String, Arc<Asking>>>>);

impl Questions {
    /// Opens a token for `asking`, which lasts as long as the returned
    /// [`Open`].
    pub fn open(&self, asking: Asking) -> Open {
        let token = crate::auth::new_token();
        let asking = Arc::new(asking);
        if let Ok(mut open) = self.0.lock() {
            open.insert(token.clone(), asking.clone());
        }
        Open {
            token,
            asking,
            questions: self.clone(),
        }
    }

    pub(crate) fn get(&self, token: &str) -> Option<Arc<Asking>> {
        self.0.lock().ok()?.get(token).cloned()
    }
}

/// A question's token, closed when this is dropped.
pub struct Open {
    pub token: String,
    pub asking: Arc<Asking>,
    questions: Questions,
}

impl Drop for Open {
    fn drop(&mut self) {
        if let Ok(mut open) = self.questions.0.lock() {
            open.remove(&self.token);
        }
    }
}

/// `POST /api/mcp`: one JSON-RPC message from Claude Code.
pub async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let asking = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| state.questions.get(token));
    let Some(asking) = asking else {
        return (StatusCode::UNAUTHORIZED, "This question is over.").into_response();
    };
    let id = message["id"].clone();
    if id.is_null() {
        // A notification, such as `notifications/initialized`.
        return StatusCode::ACCEPTED.into_response();
    }
    let result = match message["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "almanac", "version": env!("CARGO_PKG_VERSION") },
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools(asking.mode) }),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or("");
            let arguments = message["params"]["arguments"].clone();
            let (text, failed) = match call(&state, &asking, name, arguments).await {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            };
            tracing::info!(tool = name, failed, said = %text, "Claude Code called a tool");
            json!({ "content": [{ "type": "text", "text": text }], "isError": failed })
        }
        _ => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "Almanac doesn't offer that." },
            }))
            .into_response();
        }
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

/// Runs one tool. What's wrong comes back as its result, in words, so
/// Claude can fix it and call again.
async fn call(
    state: &AppState,
    asking: &Asking,
    name: &str,
    arguments: Value,
) -> Result<String, String> {
    if !asking.mode.tools().contains(&name) {
        return Err(format!("There's no {name} here."));
    }
    fn read<T: DeserializeOwned>(arguments: Value) -> Result<T, String> {
        serde_json::from_value(arguments).map_err(|e| format!("The arguments don't fit: {e}."))
    }
    let sub = match PAGE_TOOLS.contains(&name) {
        true => sub_page(state, asking, &arguments).await?,
        false => None,
    };
    let kept = || {
        asking
            .kept
            .lock()
            .map(|guard| On { guard, sub })
            .map_err(|_| "Almanac stumbled.".to_string())
    };
    match name {
        "add_block" => {
            let args: BlockArgs = read(arguments)?;
            let fields = args.fields.unwrap_or_default();
            let b = kept()?.add_block(
                &args.kind,
                &args.title,
                args.after.as_deref(),
                &fields,
                args.row.as_deref(),
            )?;
            Ok(format!("Added {b}."))
        }
        "change_block" => {
            let args: BlockChangeArgs = read(arguments)?;
            let unread = kept()?.change_block(
                &args.block,
                args.title.as_deref(),
                args.fields.as_deref(),
                args.view.as_deref(),
            )?;
            Ok(format!("Changed, once your answer is done. {unread}")
                .trim_end()
                .into())
        }
        "add_items" => {
            let args: ItemsArgs = read(arguments)?;
            let refs = kept()?.add_items(&args.block, &args.items)?;
            Ok(format!("Added {}.", refs.join(", ")))
        }
        "add_rows" => {
            let args: RowsArgs = read(arguments)?;
            let refs = kept()?.add_rows(&args.block, &args.rows)?;
            Ok(format!("Added {}.", refs.join(", ")))
        }
        "change_points" => {
            let args: PointsArgs = read(arguments)?;
            kept()?.change_points(&args.points)?;
            Ok("Changed, once your answer is done.".into())
        }
        "tick" => {
            let ticks: TickArgs = read(arguments)?;
            kept()?.tick(&ticks.items, ticks.done)?;
            Ok(if ticks.done {
                "Ticked off."
            } else {
                "Ticked on again."
            }
            .into())
        }
        "move" => {
            let args: MoveArgs = read(arguments)?;
            let mut guard = kept()?;
            match (args.block, args.points.is_empty()) {
                (Some(block), true) => {
                    guard.move_block(&block, args.after.as_deref().unwrap_or(""))?
                }
                (None, false) => {
                    let to = args.to.ok_or("Say which block they go to.")?;
                    guard.move_points(&args.points, &to, args.after.as_deref())?
                }
                _ => return Err("Move either a block or points, not both.".into()),
            }
            Ok("Moved, once your answer is done.".into())
        }
        "remove" => {
            let args: RemoveArgs = read(arguments)?;
            kept()?.remove(&args.what)?;
            Ok("Removed, once your answer is done. They can undo it.".into())
        }
        "ask_to_check" => {
            let question: QuestionArgs = read(arguments)?;
            kept()?.ask_to_check(&question.theme, &question.text, &question.point)?;
            Ok("They'll see it under the answer.".into())
        }
        "offer_reminder" => {
            let r: ReminderArgs = read(arguments)?;
            let mut guard = kept()?;
            let title = match (r.title.trim(), &r.item) {
                ("", Some(item)) => guard.item_words(item).unwrap_or_default(),
                (title, _) => title.to_string(),
            };
            let reminder = reminder(&r.theme, &title, &r.due_on, &r.at, r.every_days)?;
            guard.offer_reminder(reminder, &r.replaces, r.item.as_deref())?;
            Ok("Offered: they add it to the calendar with a click.".into())
        }
        "change_brief" => {
            let args: BriefArgs = read(arguments)?;
            let (id, name, now, ..) = this_or_sub(state, asking, &args.topic).await?;
            kept()?.change_brief(id, &name, &now, &args.sticks_to, &args.said)?;
            Ok(
                "Changed, once your answer is done. They see what you said, and can undo it."
                    .into(),
            )
        }
        "read_topic" => {
            let args: ElsewhereArgs = read(arguments)?;
            let named = cut(&args.topic, 80);
            // Its own sub-topic first, when another topic has the same name.
            let sub = this_or_sub(state, asking, &named)
                .await
                .ok()
                .map(|row| row.0);
            let row: Option<TopicRow> = match sub {
                Some(id) => sqlx::query_as(TOPIC_BY_ID)
                    .bind(asking.actor.workspace_id)
                    .bind(id),
                None => sqlx::query_as(TOPIC_NAMED)
                    .bind(asking.actor.workspace_id)
                    .bind(&named),
            }
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
            let Some((id, ..)) = row else {
                return Err(format!(
                    "No topic is called {named}. The topics: {}.",
                    names(state, asking).await
                ));
            };
            if id == asking.topic_id {
                return Ok("That's this topic: its page is the one you have.".into());
            }
            let page = crate::page::Page::load(&state.pool, asking.actor.workspace_id, id)
                .await
                .map_err(|_| "Almanac couldn't read that topic.".to_string())?
                .ok_or("That topic is gone.")?;
            let sub = sub.is_some();
            Ok(format!(
                "{}\n\n{}",
                match sub {
                    true =>
                        "A sub-topic of this one: its page's references (b2, p5) go to your \
                             page tools with its name as their topic. You also rename, re-brief \
                             and move into it.",
                    false =>
                        "Its page only to read: its references (b2, p5) aren't this page's, \
                              so none go to your tools.",
                },
                page.describe()
            ))
        }
        "fetch_picture" => {
            let args: PictureArgs = read(arguments)?;
            let id = crate::photos::from_web(&state.pool, asking.actor.workspace_id, &args.address)
                .await?;
            let mut guard = kept()?;
            guard.photos.push(id);
            Ok(format!(
                "Kept it as photo {}: put it in a photo cell as \"photo {}\", or make it the cover.",
                guard.photos.len(),
                guard.photos.len()
            ))
        }
        "set_cover" => {
            let args: CoverArgs = read(arguments)?;
            kept()?.set_cover(&args.photo)?;
            Ok("The page opens with it, once your answer is done.".into())
        }
        "name_talk" => {
            let args: NameArgs = read(arguments)?;
            kept()?.talk_title = Some(cut(&args.title, 80));
            Ok("Named.".into())
        }
        "fits_elsewhere" => {
            let args: ElsewhereArgs = read(arguments)?;
            kept()?.elsewhere = Some(cut(&args.topic, 80));
            Ok("They'll see where it fits.".into())
        }
        "propose_sub_topic" => {
            let mut args: ProposeArgs = read(arguments)?;
            let here: Option<(String, String, Option<Uuid>)> = sqlx::query_as(
                "SELECT name, group_name, parent_id FROM topics WHERE workspace_id = $1 AND id = $2",
            )
            .bind(asking.actor.workspace_id)
            .bind(asking.topic_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| "Almanac couldn't look at the topic.".to_string())?;
            let (name, group, parent) = here.ok_or("This topic is gone.")?;
            if parent.is_some() {
                return Err(format!(
                    "{name} sits under another topic already: sub-topics go one level deep. \
                     Say so, and that its parent's talk can start one."
                ));
            }
            args.group = group;
            let taken = kept()?.taken(&args.takes)?;
            let mut proposal = propose(state, asking, args).await?;
            {
                let guard = kept()?;
                let other = guard
                    .sub_topics
                    .iter()
                    .filter(|p| !p.name.eq_ignore_ascii_case(&proposal.name));
                for other in other {
                    if let Some((_, what)) = taken.iter().find(|(id, _)| other.takes.contains(id)) {
                        return Err(format!(
                            "{what} goes to {} already: each thing moves to one sub-topic.",
                            other.name
                        ));
                    }
                }
            }
            proposal.parent_id = Some(asking.topic_id);
            proposal.takes = taken.into_iter().map(|(id, _)| id).collect();
            let mut guard = kept()?;
            let same = guard
                .sub_topics
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&proposal.name));
            Ok(match same {
                Some(n) => {
                    guard.sub_topics[n] = proposal;
                    "Proposed, in place of the one before by that name."
                }
                None if guard.sub_topics.len() >= MOST_SUB_TOPICS => {
                    return Err(format!(
                        "An answer proposes at most {MOST_SUB_TOPICS} sub-topics: the rest can come in the next."
                    ));
                }
                None => {
                    guard.sub_topics.push(proposal);
                    "Proposed. They see it as a card under your answer, with Start and Change; \
                     it isn't started until they click Start. Propose each other sub-topic they \
                     asked for too, in this answer: each gets its own card."
                }
            }
            .into())
        }
        "propose_topic" => {
            let proposal = propose(state, asking, read(arguments)?).await?;
            let replaced = kept()?.proposal.replace(proposal).is_some();
            Ok(if replaced {
                "Proposed, in place of the one before."
            } else {
                "Proposed. They see it as a card under your answer, with Start and Change. \
                 It isn't started until they click Start."
            }
            .into())
        }
        "move_to_sub_topic" => {
            let args: MoveToArgs = read(arguments)?;
            let child: Option<(Uuid, String)> = sqlx::query_as(SUB_TOPIC_NAMED)
                .bind(asking.actor.workspace_id)
                .bind(asking.topic_id)
                .bind(args.to.trim())
                .fetch_optional(&state.pool)
                .await
                .map_err(|_| "Almanac couldn't look at the sub-topics.".to_string())?;
            let (child, child_name) = child.ok_or_else(|| {
                format!(
                    "{} isn't a sub-topic of this one: the page names its sub-topics.",
                    args.to
                )
            })?;
            let taken = kept()?.taken(&args.takes)?;
            if kept()?.sub_pages.iter().any(|p| p.topic_id == child) {
                return Err(format!(
                    "You change {child_name}'s page in this answer already: move into it in the next."
                ));
            }
            if taken.is_empty() {
                return Err("Name what moves, like b2 or p3.".into());
            }
            let first: i32 = sqlx::query_scalar(NEXT_POSITION)
                .bind(asking.actor.workspace_id)
                .bind(child)
                .fetch_one(&state.pool)
                .await
                .map_err(|_| "Almanac couldn't look at the sub-topic.".to_string())?;
            let ids: Vec<Uuid> = taken.iter().map(|(id, _)| *id).collect();
            let moved = crate::topics::take(
                state,
                asking.actor.workspace_id,
                asking.topic_id,
                &ids,
                child,
                first,
            )
            .await
            .map_err(|_| "Almanac couldn't read what moves.".to_string())?;
            let (made_blocks, made_points) = moved.on_the_page();
            let names: Vec<&str> = taken.iter().map(|(_, name)| name.as_str()).collect();
            let mut guard = kept()?;
            // Off this page as a removal, which Undo puts back; on the
            // sub-topic's as what it made, which Undo takes off.
            guard.move_off(&ids)?;
            guard.in_sub_topics.extend(moved.made);
            guard.changes.push(TopicChange {
                topic_id: child,
                name: child_name.clone(),
                said: format!("Moved {} here.", names.join(", ")),
                before: TopicPatch::default(),
                after: TopicPatch::default(),
                layout: Some(PageLayout {
                    moves: Vec::new(),
                    blocks: Vec::new(),
                    points: Vec::new(),
                    made_blocks,
                    made_points,
                    collections: Vec::new(),
                    rows: Vec::new(),
                }),
                undone: false,
            });
            Ok(format!(
                "Moved to {child_name}, once your answer is done: it's off this page then, with its pages. They can undo it."
            ))
        }
        "change_topic" => {
            let mut args: ChangeArgs = read(arguments)?;
            // A topic's own talk changes its topic and its sub-topics.
            if asking.mode == Mode::Topic {
                let (id, name, ..) = this_or_sub(state, asking, &args.topic).await?;
                (args.id, args.topic) = (Some(id), name);
                args.sticks_to = None;
            }
            let change = change(state, asking, args).await?;
            let mut guard = kept()?;
            match guard
                .changes
                .iter_mut()
                .find(|c| c.topic_id == change.topic_id)
            {
                Some(earlier) => merge(earlier, change),
                None => guard.changes.push(change),
            }
            Ok(
                "Changed, once your answer is done. They see what you said, and can undo it."
                    .into(),
            )
        }
        "set_up_topic" => {
            let set_up: SetUp = read(arguments)?;
            if !ICONS.contains(&set_up.icon.as_str())
                || !TEMPLATE_NAMES.contains(&set_up.template.as_str())
            {
                return Err("Pick the icon and the template from their lists.".into());
            }
            if let Ok(mut slot) = asking.set_up.lock() {
                *slot = Some(set_up);
            }
            Ok("Set up.".into())
        }
        _ => Err(format!("There's no {name} here.")),
    }
}

#[derive(Deserialize)]
struct BlockArgs {
    kind: String,
    #[serde(default)]
    title: String,
    after: Option<String>,
    fields: Option<Vec<FieldArg>>,
    row: Option<String>,
}

#[derive(Deserialize)]
struct BlockChangeArgs {
    block: String,
    title: Option<String>,
    fields: Option<Vec<FieldArg>>,
    view: Option<String>,
}

#[derive(Deserialize)]
struct ItemsArgs {
    block: String,
    items: Vec<ItemArg>,
}

#[derive(Deserialize)]
struct RowsArgs {
    block: String,
    rows: Vec<RowArg>,
}

#[derive(Deserialize)]
struct PointsArgs {
    points: Vec<PointChange>,
}

#[derive(Deserialize)]
struct MoveArgs {
    block: Option<String>,
    #[serde(default)]
    points: Vec<String>,
    to: Option<String>,
    after: Option<String>,
}

#[derive(Deserialize)]
struct RemoveArgs {
    what: Vec<String>,
}

#[derive(Deserialize)]
struct TickArgs {
    items: Vec<String>,
    #[serde(default = "yes")]
    done: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct QuestionArgs {
    #[serde(default)]
    theme: String,
    text: String,
    #[serde(default)]
    point: String,
}

#[derive(Deserialize)]
struct ReminderArgs {
    #[serde(default)]
    theme: String,
    #[serde(default)]
    title: String,
    due_on: String,
    #[serde(default)]
    at: String,
    every_days: Option<i64>,
    #[serde(default)]
    replaces: Vec<String>,
    item: Option<String>,
}

#[derive(Deserialize)]
struct BriefArgs {
    #[serde(default)]
    topic: String,
    sticks_to: String,
    #[serde(default)]
    said: String,
}

#[derive(Deserialize)]
struct PictureArgs {
    address: String,
}

#[derive(Deserialize)]
struct CoverArgs {
    photo: String,
}

#[derive(Deserialize)]
struct NameArgs {
    title: String,
}

#[derive(Deserialize)]
struct ElsewhereArgs {
    topic: String,
}

#[derive(Deserialize)]
struct ProposeArgs {
    name: String,
    sticks_to: String,
    icon: String,
    #[serde(default)]
    group: String,
    template: String,
    #[serde(default)]
    cards: String,
    #[serde(default)]
    fields: Vec<FieldArg>,
    #[serde(default)]
    starters: Vec<String>,
    #[serde(default)]
    points: Vec<ProposedPointArgs>,
    #[serde(default)]
    reminders: Vec<ReminderArgs>,
    #[serde(default)]
    takes: Vec<String>,
}

#[derive(Deserialize)]
struct MoveToArgs {
    to: String,
    takes: Vec<String>,
}

#[derive(Deserialize)]
struct ProposedPointArgs {
    #[serde(default)]
    block: String,
    text: String,
    #[serde(default)]
    cells: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
struct ChangeArgs {
    #[serde(default)]
    topic: String,
    /// The topic itself, once a topic's talk has found it: another may
    /// have the same name.
    #[serde(skip)]
    id: Option<Uuid>,
    name: Option<String>,
    sticks_to: Option<String>,
    icon: Option<String>,
    group: Option<String>,
    said: String,
}

type TopicRow = (Uuid, String, String, String, String, String);

/// This topic, or one of its sub-topics by name, as this answer may have
/// renamed it: what a topic's talk changes. None named, this one.
async fn this_or_sub(state: &AppState, asking: &Asking, named: &str) -> Result<TopicRow, String> {
    let named = cut(named, 80);
    let renamed = asking.kept.lock().ok().and_then(|kept| {
        kept.changes
            .iter()
            .find(|c| {
                c.after
                    .name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(&named))
            })
            .map(|c| c.topic_id)
    });
    let id: Option<Uuid> = match renamed {
        Some(id) => Some(id),
        None if named.is_empty() => Some(asking.topic_id),
        None => sqlx::query_scalar(THIS_OR_SUB_NAMED)
            .bind(asking.actor.workspace_id)
            .bind(asking.topic_id)
            .bind(&named)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| "Almanac couldn't look at the topics.".to_string())?,
    };
    let id = id.ok_or_else(|| {
        format!(
            "{named} is neither this topic nor one of its sub-topics: another topic you only read."
        )
    })?;
    let row: Option<TopicRow> = sqlx::query_as(TOPIC_BY_ID)
        .bind(asking.actor.workspace_id)
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| "Almanac couldn't look at the topic.".to_string())?;
    row.ok_or_else(|| "That topic is gone.".into())
}

/// The tools that change a page: this topic's, or a sub-topic's they name.
const PAGE_TOOLS: &[&str] = &[
    "add_block",
    "change_block",
    "add_items",
    "add_rows",
    "change_points",
    "tick",
    "move",
    "remove",
    "set_cover",
];

/// The page a tool works on, held while it does: this topic's, or the copy
/// of one of its sub-topics'.
struct On<'a> {
    guard: MutexGuard<'a, Kept>,
    sub: Option<usize>,
}

impl Deref for On<'_> {
    type Target = Kept;

    fn deref(&self) -> &Kept {
        match self.sub {
            Some(n) => &self.guard.sub_pages[n].kept,
            None => &self.guard,
        }
    }
}

impl DerefMut for On<'_> {
    fn deref_mut(&mut self) -> &mut Kept {
        match self.sub {
            Some(n) => &mut self.guard.sub_pages[n].kept,
            None => &mut self.guard,
        }
    }
}

/// The sub-topic whose page a page tool's `topic` names, copied the first
/// time this answer changes it; none for this topic's own page.
async fn sub_page(
    state: &AppState,
    asking: &Asking,
    arguments: &Value,
) -> Result<Option<usize>, String> {
    let named = arguments["topic"].as_str().unwrap_or("").trim();
    if named.is_empty() {
        return Ok(None);
    }
    let (id, name, ..) = this_or_sub(state, asking, named).await?;
    if id == asking.topic_id {
        return Ok(None);
    }
    let moved_into = |kept: &Kept| {
        kept.changes
            .iter()
            .any(|c| c.topic_id == id && c.layout.is_some())
    };
    let at = |kept: &mut Kept| {
        let n = kept.sub_pages.iter().position(|p| p.topic_id == id)?;
        // A photo of the talk, or one it fetched, goes on any page.
        kept.sub_pages[n].kept.photos = kept.photos.clone();
        Some(n)
    };
    {
        let mut kept = asking.kept.lock().map_err(|_| "Almanac stumbled.")?;
        if moved_into(&kept) {
            return Err(format!(
                "You moved into {name} in this answer: change its page in the next."
            ));
        }
        if let Some(n) = at(&mut kept) {
            return Ok(Some(n));
        }
    }
    let page = crate::page::Page::load(&state.pool, asking.actor.workspace_id, id)
        .await
        .map_err(|_| "Almanac couldn't read that topic.".to_string())?
        .ok_or("That topic is gone.")?;
    let mut kept = asking.kept.lock().map_err(|_| "Almanac stumbled.")?;
    if at(&mut kept).is_none() {
        let mut copy = Kept::on(page.refs());
        copy.today = kept.today;
        kept.sub_pages.push(kept::SubPage {
            topic_id: id,
            name,
            kept: copy,
        });
    }
    Ok(at(&mut kept))
}

async fn names(state: &AppState, asking: &Asking) -> String {
    let names: Vec<String> = sqlx::query_scalar(NAMES)
        .bind(asking.actor.workspace_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
    names.join(", ")
}

async fn propose(state: &AppState, asking: &Asking, args: ProposeArgs) -> Result<Proposal, String> {
    let name = cut(&args.name, 80);
    if name.is_empty() {
        return Err("A topic needs a name.".into());
    }
    let existing: Option<TopicRow> = sqlx::query_as(TOPIC_NAMED)
        .bind(asking.actor.workspace_id)
        .bind(&name)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
    if let Some(existing) = existing {
        return Err(format!(
            "There's a topic called {} already. Say so, or change it with change_topic.",
            existing.1
        ));
    }
    if !ICONS.contains(&args.icon.as_str()) {
        return Err("Pick the icon from its list.".into());
    }
    if !TEMPLATE_NAMES.contains(&args.template.as_str()) {
        return Err(format!("The template is one of {TEMPLATE_NAMES:?}."));
    }
    let (fields, _) = kept::fields_of(&args.fields, &[])?;
    let mut points = Vec::new();
    for p in args
        .points
        .iter()
        .filter(|p| !p.text.trim().is_empty())
        .take(8)
    {
        let mut cells = std::collections::BTreeMap::new();
        for (name, value) in &p.cells {
            let field = fields
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("The collection has no field {name}: name it in fields."))?;
            if let Some(cell) = kept::cell(field, value, &[])? {
                cells.insert(field.name.clone(), cell);
            }
        }
        points.push(ProposedPoint {
            block: Some(cut(&p.block, 60)).filter(|b| !b.is_empty()),
            text: cut(&p.text, 2000),
            cells,
        });
    }
    let reminders = args
        .reminders
        .iter()
        .take(3)
        .map(|r| reminder(&r.theme, &r.title, &r.due_on, &r.at, r.every_days))
        .collect::<Result<Vec<_>, _>>()?;
    let cards = cut(&args.cards, 60);
    Ok(Proposal {
        name,
        sticks_to: cut(&args.sticks_to, STICKS_TO_MAX),
        icon: args.icon,
        group_name: cut(&args.group, 40),
        template: args.template,
        cards: (!cards.is_empty()).then_some(cards),
        fields,
        starters: args
            .starters
            .iter()
            .map(|s| cut(s, 200))
            .filter(|s| !s.is_empty())
            .take(4)
            .collect(),
        points,
        reminders,
        topic_id: None,
        parent_id: None,
        takes: Vec::new(),
    })
}

async fn change(
    state: &AppState,
    asking: &Asking,
    args: ChangeArgs,
) -> Result<TopicChange, String> {
    let named = cut(&args.topic, 80);
    let earlier = |find: &dyn Fn(&TopicChange) -> bool| {
        asking
            .kept
            .lock()
            .ok()
            .and_then(|kept| kept.changes.iter().find(|c| find(c)).cloned())
    };
    // A topic this answer renamed already goes by its new name.
    let renamed = earlier(&|c| {
        c.after
            .name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(&named))
    });
    let row: Option<TopicRow> = match args.id.or(renamed.as_ref().map(|c| c.topic_id)) {
        Some(id) => sqlx::query_as(TOPIC_BY_ID)
            .bind(asking.actor.workspace_id)
            .bind(id),
        None => sqlx::query_as(TOPIC_NAMED)
            .bind(asking.actor.workspace_id)
            .bind(&named),
    }
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
    let Some((id, mut name, mut sticks_to, mut icon, mut group_name, kind)) = row else {
        return Err(format!(
            "No topic is called {named}. The topics: {}.",
            names(state, asking).await
        ));
    };
    // What this answer changed in it already counts as how it is now.
    if let Some(change) = earlier(&|c| c.topic_id == id) {
        let now = change.after;
        for (field, value) in [
            (&mut name, now.name),
            (&mut sticks_to, now.sticks_to),
            (&mut icon, now.icon),
            (&mut group_name, now.group_name),
        ] {
            if let Some(value) = value {
                *field = value;
            }
        }
    }
    let own = kind == "almanac";
    if own && (args.icon.is_some() || args.group.is_some()) {
        return Err("This topic can only get another name or say what it sticks to.".into());
    }
    let mut before = TopicPatch::default();
    let mut after = TopicPatch::default();
    let set = |given: Option<String>,
               max: usize,
               now: String,
               b: &mut Option<String>,
               a: &mut Option<String>| {
        if let Some(given) = given.map(|g| cut(&g, max))
            && given != now
        {
            *b = Some(now);
            *a = Some(given);
        }
    };
    set(
        args.name,
        80,
        name.clone(),
        &mut before.name,
        &mut after.name,
    );
    set(
        args.sticks_to,
        STICKS_TO_MAX,
        sticks_to,
        &mut before.sticks_to,
        &mut after.sticks_to,
    );
    set(args.icon, 40, icon, &mut before.icon, &mut after.icon);
    set(
        args.group,
        40,
        group_name,
        &mut before.group_name,
        &mut after.group_name,
    );
    if after == TopicPatch::default() {
        return Err(format!("{name} is like that already."));
    }
    if after.name.as_deref() == Some("") {
        return Err("A topic needs a name.".into());
    }
    if after.icon.as_deref().is_some_and(|i| !ICONS.contains(&i)) {
        return Err("Pick the icon from its list.".into());
    }
    if let Some(new_name) = &after.name
        && !new_name.eq_ignore_ascii_case(&name)
    {
        let taken: Option<TopicRow> = sqlx::query_as(TOPIC_NAMED)
            .bind(asking.actor.workspace_id)
            .bind(new_name)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
        if taken.is_some() {
            return Err(format!("There's a topic called {new_name} already."));
        }
    }
    let said = cut(&args.said, 200);
    Ok(TopicChange {
        topic_id: id,
        name: after.name.clone().unwrap_or(name),
        said: if said.is_empty() {
            "Changed it.".into()
        } else {
            said
        },
        before,
        after,
        layout: None,
        undone: false,
    })
}

/// A second change to the same topic in one answer: the fields it sets
/// over the first's, and what was there before the first.
fn merge(earlier: &mut TopicChange, later: TopicChange) {
    fn over(
        b: &mut Option<String>,
        a: &mut Option<String>,
        lb: Option<String>,
        la: Option<String>,
    ) {
        if la.is_some() {
            if b.is_none() {
                *b = lb;
            }
            *a = la;
        }
    }
    let (
        TopicChange {
            before,
            after,
            said,
            ..
        },
        e,
    ) = (later, earlier);
    if said != e.said {
        e.said = format!("{} {said}", e.said);
    }
    over(
        &mut e.before.name,
        &mut e.after.name,
        before.name,
        after.name,
    );
    over(
        &mut e.before.sticks_to,
        &mut e.after.sticks_to,
        before.sticks_to,
        after.sticks_to,
    );
    over(
        &mut e.before.icon,
        &mut e.after.icon,
        before.icon,
        after.icon,
    );
    over(
        &mut e.before.group_name,
        &mut e.after.group_name,
        before.group_name,
        after.group_name,
    );
    if let Some(name) = &e.after.name {
        e.name = name.clone();
    }
}

/// What a topic sticks to, for the tools that write it.
const BRIEF_SAYS: &str = "The topic's brief, which every talk in it follows: first what it's \
about, in a sentence or two (\"Houseplants and their care: watering, light, repotting.\"), then, \
a line each, how it answers: in which language, the tone and the length, what it brings in, what \
it checks, and what it leaves to other topics. Fit it to the topic, with nothing every topic \
does anyway: usually 400 to 1,200 characters. The page doesn't show it, only the topic's \
About.";

/// A collection's fields, for the tools that name them.
const FIELDS_SAY: &str = "A collection's fields, besides each row's name, eight at most, in \
order: the facts every row has (Gewicht, Tag, Licht, Status). A field holds one kind of fact: \
text a few words (or a sentence or two for what a term means), number a plain number with its unit (kg) and a goal when they aim at one, \
day a day or a year or a span, choice one of a few words, status where a row stands (its \
words in order, the last one done), check yes or no, photo a photo from the talk, link a web \
page. Pick the narrowest kind that holds the fact: yes or no is a check, an amount or a price a \
number with its unit, a date a day, a web page a link, one of a few words that repeat a choice; \
text only for words. A cell holds the fact alone: a reason or a note goes in a text field of its \
own (Vegan a check, Material \"Nubuk und Wildleder\"). A field changed to another kind reads its \
cells anew. The app picks the picture from them: never name a colour, a width or a picture here.";

/// One field, for the tools that name fields.
fn field_schema(renames: bool) -> Value {
    let mut properties = json!({
        "name": { "type": "string", "description": "A word or two, in the page's language." },
        "kind": { "type": "string", "enum": FIELD_KINDS },
        "unit": { "type": "string", "description": "A number's unit (kg, €, cm), or left out." },
        "goal": { "type": "number", "description": "The number they aim at, only when they said one." },
        "options": { "type": "array", "items": { "type": "string" }, "description": "A choice's or a status's words, in order; left out for those its rows use." },
    });
    if renames {
        properties["was"] =
            json!({ "type": "string", "description": "Only when it's renamed: the name it had." });
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": ["name", "kind"],
        "additionalProperties": false,
    })
}

/// The templates a topic starts as, for the tools that pick one.
const TEMPLATES_SAY: &str = "The blocks its page starts with. routine when it's a routine they \
keep, with steps on certain days and the things they use (a hair or skin routine, a training \
plan): Routine, its steps to tick; Your shelf, a collection of the things they use, named by \
cards, with its fields; and Advice. collection when it's about a set of things, events or \
measurements that each get a row (plants, pets, trips, weighings): To do, and the collection, \
named by cards, with its fields. study when it's something to learn or write up (a field, a \
pitch, research): Reading, to tick; Terms, a collection, named by cards; and What it says. Each also has You, for what it learns \
about them, which only its About shows. When they picked one, theirs.";

/// The tools a question gets, with what each is for.
fn tools(mode: Mode) -> Vec<Value> {
    let string = |description: &str| json!({ "type": "string", "description": description });
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
        })
    };
    let reminder_fields = json!({
        "theme": string("The theme it belongs to, or left out."),
        "title": string("What to do in two to four words, as the calendar shows it: \"Water the Monstera\", \"Elvital wash\". The steps go in the page's points, not here."),
        "due_on": string("The next day it's due, as YYYY-MM-DD."),
        "at": string("A time, as HH:MM, or left out for nine in the morning."),
        "every_days": { "type": "integer", "description": "The rhythm in days, or left out for once." },
    });
    let replaces = |what: &str, like: &str| {
        json!({
            "type": "array",
            "items": { "type": "string" },
            "description": format!("The page's {what} this one takes the place of, as the page names them ({like}), or left out."),
        })
    };
    let mut offer_fields = reminder_fields.clone();
    offer_fields["replaces"] = replaces("reminders", "r2");
    offer_fields["item"] = string(
        "The row to do it rings for, in a collection with a check, as the page names it (p3). Its title is the row's name when you give none.",
    );
    offer_fields["title"] = string(
        "What to do in two to four words, as the calendar shows it: \"Water the Monstera\". Left out for an item's reminder, which takes the item's words.",
    );
    let fetch_picture = tool(
        "fetch_picture",
        "Keeps a picture from the web for the page: a product's, a plant's, a place's. Give the \
         picture's address, or the address of a page about that one thing (a shop's product \
         page), whose own picture it takes. It answers with the photo's number, which a photo \
         cell or set_cover takes.",
        json!({ "address": string("The picture's https address, or its page's.") }),
        &["address"],
    );
    let set_cover = tool(
        "set_cover",
        "Makes a photo the picture the topic's page opens with and its tile shows: one that \
         shows what the topic is about. Only when they ask for one, or the page has none and a \
         photo they sent shows it.",
        json!({ "photo": string("The photo, as \"photo 2\".") }),
        &["photo"],
    );
    let name_talk = tool(
        "name_talk",
        "Names the talk while it has no name: under six words, in the talk's language.",
        json!({ "title": string("The talk's title.") }),
        &["title"],
    );
    let fits_elsewhere = tool(
        "fits_elsewhere",
        "When the question has nothing to do with this topic: the topic it fits, one of the \
         others by name, or a short name for a new one. They see a link to it under the answer.",
        json!({ "topic": string("The topic's name.") }),
        &["topic"],
    );
    // What a proposed topic is, for the workspace's own talk and, under
    // the topic, for a topic's.
    let proposal = json!({
        "name": string("A short name, one to three words, in their language."),
        "sticks_to": string(BRIEF_SAYS),
        "icon": { "type": "string", "enum": ICONS, "description": "The icon that fits best." },
        "group": string("A short group for the sidebar, such as Home, Work, Health or Learning. Reuse an existing group when one fits."),
        "template": { "type": "string", "enum": TEMPLATE_NAMES, "description": TEMPLATES_SAY },
        "cards": string("For a collection, what its rows are, in a word or two (\"Plants\", \"Weighings\"): the collection's title. Otherwise left out."),
        "fields": { "type": "array", "items": field_schema(false), "description": format!("For a collection: {FIELDS_SAY}") },
        "starters": { "type": "array", "items": { "type": "string" }, "description": "Three or four first questions they might ask, each under 12 words." },
        "points": {
            "type": "array",
            "description": "Only what they told you, as its first points: never advice or what you know, which the topic's talks keep with its source.",
            "items": {
                "type": "object",
                "properties": {
                    "block": string("The template's block it goes in, by title: in any of them You (a fact about them, which the topic's About shows); in a routine Routine (a step), Your shelf (a thing they use) or Advice (a sentence); in a collection To do or the collection; in a study Reading, Terms (a term) or What it says (a sentence)."),
                    "text": string("The point, one short sentence; in a collection, the row's name."),
                    "cells": { "type": "object", "description": "In a collection, the row's cells by field name, as its fields say: only what they told you.", "additionalProperties": true },
                },
                "required": ["block", "text"],
                "additionalProperties": false,
            },
        },
        "reminders": {
            "type": "array",
            "description": "Reminders to offer once it's started, only when they asked for one or clearly need it.",
            "items": {
                "type": "object",
                "properties": reminder_fields,
                "required": ["title", "due_on"],
                "additionalProperties": false,
            },
        },
    });
    let mut sub_topic = proposal.clone();
    if let Some(props) = sub_topic.as_object_mut() {
        props.remove("group");
        props.insert(
            "takes".into(),
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": "What moves from this page to the sub-topic when they start it: blocks (b2), and rows with a page of their own (p3), whose page becomes the sub-topic's. They leave this page then. Leave points out for what's taken.",
            }),
        );
    }
    let mut tools = match mode {
        Mode::Topic => vec![
            tool(
                "add_block",
                "Starts a block of the set, only when none on the page fits what you keep (a \
                 reading list, the terms, the weighings). It answers with its reference (b5), \
                 which the other tools take. A block like it the page has already is the one it \
                 answers with.",
                json!({
                    "kind": { "type": "string", "enum": SET_KINDS },
                    "title": string("Its short title, in the page's language: what its points share, never one of them. A heading is only its title."),
                    "after": string("The block it goes after, on the same page (b2); \"\" for the top. Left out, at the end."),
                    "fields": { "type": "array", "items": field_schema(false), "description": FIELDS_SAY },
                    "row": string("The row whose own page it goes on (p3): what belongs to that one row and has parts of its own, such as a shoe's offers or a plant's repottings. Left out, the topic's page."),
                }),
                &["kind", "title"],
            ),
            tool(
                "change_block",
                "Changes a block: its title; a collection's fields, given whole as they are from \
                 now on (one renamed says the name it was, so its cells go with it; one left out \
                 goes, with its cells); or, only when they ask for another picture, one of the \
                 pictures its fields allow. Only what changes.",
                json!({
                    "block": string("The block, as the page names it (b2)."),
                    "title": string("Its new title."),
                    "fields": { "type": "array", "items": field_schema(true), "description": FIELDS_SAY },
                    "view": string("Only when they ask for another picture: one the page lists as its fields allow; \"\" for the app's own pick."),
                }),
                &["block"],
            ),
            tool(
                "add_items",
                "Adds sentences to a paragraph, a callout or You: what explains, a decision, or \
                 something they told you about themselves worth knowing later; nothing the page \
                 has already. Things, terms and steps are rows (add_rows). Each as its block's kind asks, as the page says. \
                 To a picture it adds photos, each with a caption or none. It \
                 answers with their references (p12). Usually none to three per answer.",
                json!({
                    "block": string("The block, as the page names it (b2), or its title."),
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "text": string("The point: two to four sentences in a paragraph, never opening with a term in bold; a few words in a callout or You; a picture's caption, a few words, or \"\" for none."),
                                "photo": string("Only in a picture: its photo, \"photo 2\" for the talk's second, or the number fetch_picture answered with."),
                                "look": { "type": "string", "enum": ["warn", "info"], "description": "Only in a callout: warn when it needs a look, info when something's ready." },
                                "source": string("Where it comes from, in a few words: \"You said\", \"From the talk\", or the book, guide, institution or site the answer relied on."),
                                "url": string("The page it comes from, when you read one. Never a made-up link."),
                            },
                            "required": ["text", "source"],
                            "additionalProperties": false,
                        },
                    },
                }),
                &["block", "items"],
            ),
            tool(
                "add_rows",
                "Adds rows to a collection: a thing, a term, a step, an event or a measurement \
                 each, its name (a few words) and \
                 a cell for each field it knows. What's measured, compared or tracked is a row, \
                 never a sentence: an event with its year a row of events, an amount to \
                 compare a row with its number. A cell holds only what they or a source said: leave it out \
                 rather than guess. A field the rows need and the collection lacks comes first, \
                 with change_block. It answers with their references (p12).",
                json!({
                    "block": string("The collection, as the page names it (b2), or its title."),
                    "rows": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": string("The row's name: the thing, or what happened, in a few words (Monstera, Wiegen)."),
                                "cells": { "type": "object", "description": "Its cells by field name: a number plain (3.4, without its unit), a day as 2026-09-24 (or a year, a span), a choice's or a status's word, true or false for a check, \"photo 2\" for the talk's second photo, a link's address, a few words for text, or a sentence or two for what a term means.", "additionalProperties": true },
                                "source": string("Where it comes from, in a few words."),
                                "url": string("The page it comes from, when you read one. Never a made-up link."),
                            },
                            "required": ["name", "source"],
                            "additionalProperties": false,
                        },
                    },
                }),
                &["block", "rows"],
            ),
            tool(
                "change_points",
                "Changes points the page has, when what they say changed or was wrong (a wash day \
                 moved, a product swapped, a weighing corrected): their words (a row's name), \
                 a callout's look, or a row's cells, which merge into the \
                 row's (null takes one out). The page is kept up to date, not added to. They see \
                 what changed under your answer, and can undo it.",
                json!({
                    "points": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "point": string("The point, as the page names it (p3)."),
                                "text": string("Its new words."),
                                "look": { "type": "string", "enum": ["", "warn", "info"], "description": "Its new look, in a callout." },
                                "cells": { "type": "object", "description": "The cells that change, by field name, as add_rows takes them; null takes one out.", "additionalProperties": true },
                            },
                            "required": ["point"],
                            "additionalProperties": false,
                        },
                    },
                }),
                &["points"],
            ),
            tool(
                "tick",
                "Ticks rows to do done, in a collection with a check: what they did, got or \
                 read, and a question they answered well when they practised it. Only when \
                 they told you, or did it in this talk. A row a reminder rings for ticks the \
                 reminder too, until it's due again. done false ticks them on again.",
                json!({
                    "items": { "type": "array", "items": { "type": "string" }, "description": "The rows, as the page names them (p3)." },
                    "done": { "type": "boolean", "description": "Whether they're done; left out, they are." },
                }),
                &["items"],
            ),
            tool(
                "move",
                "Moves a block (block and after), or points to a block (points, to and after), \
                 when they'd read better there: a sentence into the paragraph it belongs \
                 with, a sentence into a collection as its row (then \
                 change_points gives it its name and cells). They see what moved under your \
                 answer, and can undo it.",
                json!({
                    "block": string("A block to move, as the page names it (b2)."),
                    "points": { "type": "array", "items": { "type": "string" }, "description": "Points to move, in order, as the page names them (p3)." },
                    "to": string("The block the points go to (b2)."),
                    "after": string("What it goes after: a block (b2), or for points one of the block's points (p3); \"\" for the top. Left out, points go at the end."),
                }),
                &[],
            ),
            tool(
                "remove",
                "Removes blocks, with their points, and points: what's wrong, said twice, or \
                 they asked to drop. They see what went under your answer, and can undo it.",
                json!({
                    "what": { "type": "array", "items": { "type": "string" }, "description": "The blocks and points, as the page names them (b2, p3)." },
                }),
                &["what"],
            ),
            tool(
                "ask_to_check",
                "When your answer rested on a guess about their situation that you didn't ask \
                 about in the answer, asks them to check it on the page, with the point to keep \
                 if they say yes (text \"Does the Monstera stand by the north window?\", point \
                 \"Stands by the north window.\"). A question you ask in your answer waits for \
                 their reply in the talk: never ask it here too. At most two per answer, and \
                 none the page is already waiting on.",
                json!({
                    "theme": string("The theme the point would go under."),
                    "text": string("The question to check."),
                    "point": string("The point to keep when they say yes, or left out."),
                }),
                &["text"],
            ),
            tool(
                "offer_reminder",
                "Offers a reminder, only when the talk settled something recurring or dated \
                 they'd want to be reminded of (watering every 7 days, repotting in March, \
                 sending the deck on Friday). When it's a row to do, name the row: the \
                 reminder rings for it, and ticking one ticks the other; add the row first \
                 when the page lacks it. A routine they told you they keep counts (washing \
                 their hair three times a week): one reminder for each day it comes round, so \
                 something a few times a week is a reminder per weekday, each every 7 days; \
                 when they didn't say which days, spread them evenly and say which in your \
                 answer. They add it to the calendar with a click. None the calendar has \
                 already: when a reminder changes (a wash day moved, another rhythm), offer the \
                 new one in place of the old with replaces, naming it as the page does (r2).",
                offer_fields,
                &["due_on"],
            ),
            tool(
                "change_brief",
                "Changes how this topic answers from now on, in what it sticks to: its brief, \
                 which the page you read starts with. Only when they tell you how it should \
                 answer from now on (\"always say when a source was written\", \"in English \
                 from now on\"), or correct the same thing a second time in this talk (\"too \
                 long again\"): never for a wish about one answer. Give the brief whole, as it \
                 is from now on: all of it kept, with the rule added or changed where it fits. \
                 Its name, group or icon go with change_topic. A sub-topic's brief too, when \
                 they ask, named in topic. They see what you said under the answer, and can \
                 undo it.",
                json!({
                    "topic": string("One of this topic's sub-topics, by name; left out, this topic."),
                    "sticks_to": string("The whole brief as it is from now on."),
                    "said": string("What changed, in a short sentence for under the answer, in the talk's language: \"Names when each source was written.\""),
                }),
                &["sticks_to", "said"],
            ),
            tool(
                "change_topic",
                "Changes this topic's name, its icon or its group in the sidebar, or one of \
                 its sub-topics', when they ask. Only the fields to change. They see what you \
                 said under the answer, and can undo it.",
                json!({
                    "topic": string("One of this topic's sub-topics, by name; left out, this topic."),
                    "name": string("Its new name."),
                    "icon": { "type": "string", "enum": ICONS },
                    "group": string("Its group in the sidebar."),
                    "said": string("What you changed, in a short sentence for under the answer, in the talk's language: \"Renamed it to Backend.\""),
                }),
                &["said"],
            ),
            tool(
                "read_topic",
                "Reads another topic's page, by its name, when the question needs what it \
                 keeps. Only to read: this talk changes nothing there.",
                json!({ "topic": string("The topic's name.") }),
                &["topic"],
            ),
            tool(
                "propose_sub_topic",
                "Proposes a sub-topic under this one, shown under your answer as a card they \
                 start with a click or change first. Only when they ask for one. What this page \
                 keeps about it goes in takes and moves there on Start, with its pages, so \
                 nothing is kept twice; with takes, the page is what's taken, and the template \
                 lays out nothing. What the move empties here goes with it: a block no row \
                 stays in, a lane none does. Call it once for each sub-topic they ask for, in \
                 the same answer: each is its own card.",
                sub_topic,
                &["name", "sticks_to", "icon", "template", "starters"],
            ),
            tool(
                "move_to_sub_topic",
                "Moves what this page keeps about one of its sub-topics there, when they ask: \
                 blocks, and rows with a page of their own, whose page goes whole. It leaves this \
                 page, so nothing is kept twice, and what it empties here goes with it: a block \
                 no row stays in, a lane none does. The only change you make outside this topic.",
                json!({
                    "to": string("The sub-topic's name, as the page names it."),
                    "takes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "What moves: blocks (b2), and rows with a page of their own (p3).",
                    },
                }),
                &["to", "takes"],
            ),
            fetch_picture,
            set_cover,
            name_talk,
            fits_elsewhere,
        ],
        Mode::Own => vec![
            tool(
                "propose_topic",
                "Proposes a new topic, shown under your answer as a card they start with a \
                 click or change first. Only when they said what they want to keep track of \
                 and no topic has it already. One per answer.",
                proposal.clone(),
                &["name", "sticks_to", "icon", "group", "template", "starters"],
            ),
            tool(
                "change_topic",
                "Changes a topic they named: its name, what it sticks to, its icon or its \
                 group. Only the fields to change. They see what you said and \
                 can undo it. This topic itself can only be renamed or say what it sticks to.",
                json!({
                    "topic": string("The topic's name now."),
                    "name": string("Its new name."),
                    "sticks_to": string("What it sticks to now, its brief, whole: all of it kept, with what changes in its place."),
                    "icon": { "type": "string", "enum": ICONS },
                    "group": string("Its group in the sidebar."),
                    "said": string("What you changed, in a short sentence for under the answer: \"Moved it to Hobby.\""),
                }),
                &["topic", "said"],
            ),
            name_talk,
            fits_elsewhere,
        ],
        Mode::SetUp => vec![tool(
            "set_up_topic",
            "Sets up the new topic. Call it once, with every field.",
            json!({
                "sticks_to": string(&format!("{BRIEF_SAYS} When they wrote some, all of theirs, word for word, at the start.")),
                "icon": { "type": "string", "enum": ICONS },
                "group": string("A short group for the sidebar, such as Home, Work, Health or Learning. Reuse an existing group when one fits."),
                "template": { "type": "string", "enum": TEMPLATE_NAMES, "description": TEMPLATES_SAY },
                "cards": string("For a collection, what its rows are, in a word or two (\"Plants\", \"Weighings\"); otherwise \"\"."),
                "starters": { "type": "array", "items": { "type": "string" }, "description": "Three or four first questions, each under 12 words." },
            }),
            &[
                "sticks_to",
                "icon",
                "group",
                "template",
                "cards",
                "starters",
            ],
        )],
    };
    // Each page tool works on a sub-topic's page too.
    for tool in &mut tools {
        if PAGE_TOOLS.contains(&tool["name"].as_str().unwrap_or("")) {
            tool["inputSchema"]["properties"]["topic"] = string(
                "One of this topic's sub-topics, by name, to change its page instead, with the \
                 references read_topic gives it. Left out, this page.",
            );
        }
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_question_gets_its_own_tools() {
        assert!(Mode::Topic.tools().contains(&"add_rows"));
        assert!(!Mode::Topic.tools().contains(&"propose_topic"));
        assert!(Mode::Own.tools().contains(&"change_topic"));
        assert!(Mode::Topic.tools().contains(&"change_topic"));
        assert!(Mode::Topic.tools().contains(&"read_topic"));
        assert!(Mode::Topic.tools().contains(&"propose_sub_topic"));
        assert!(Mode::Topic.tools().contains(&"move_to_sub_topic"));
        for mode in [Mode::Topic, Mode::Own, Mode::SetUp] {
            let tools = tools(mode);
            let described: Vec<&str> = tools
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect();
            assert_eq!(described, mode.tools());
        }
    }

    #[test]
    fn a_token_closes_with_its_answer() {
        let questions = Questions::default();
        let actor = Actor {
            user_id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
        };
        let open = questions.open(Asking::new(actor, Uuid::now_v7(), Mode::Topic));
        let token = open.token.clone();
        assert!(questions.get(&token).is_some());
        drop(open);
        assert!(questions.get(&token).is_none());
    }

    #[test]
    fn a_second_change_keeps_what_was_there_first() {
        let id = Uuid::now_v7();
        let mut first = TopicChange {
            topic_id: id,
            name: "Plants".into(),
            said: "Moved it to Hobby.".into(),
            before: TopicPatch {
                group_name: Some("Home".into()),
                ..TopicPatch::default()
            },
            after: TopicPatch {
                group_name: Some("Hobby".into()),
                ..TopicPatch::default()
            },
            layout: None,
            undone: false,
        };
        let second = TopicChange {
            topic_id: id,
            name: "Garden".into(),
            said: "Renamed it.".into(),
            before: TopicPatch {
                name: Some("Plants".into()),
                group_name: Some("Hobby".into()),
                ..TopicPatch::default()
            },
            after: TopicPatch {
                name: Some("Garden".into()),
                group_name: Some("Outside".into()),
                ..TopicPatch::default()
            },
            layout: None,
            undone: false,
        };
        merge(&mut first, second);
        assert_eq!(first.before.group_name.as_deref(), Some("Home"));
        assert_eq!(first.after.group_name.as_deref(), Some("Outside"));
        assert_eq!(first.before.name.as_deref(), Some("Plants"));
        assert_eq!(first.name, "Garden");
    }
}
