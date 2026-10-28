//! Starting a topic: a name, what it sticks to, which may be empty, and the
//! template its blocks start from, which Almanac may pick. Almanac makes
//! the topic at once; then Claude writes what it sticks to (when you left
//! it empty), and picks its icon, its group, its template and the first
//! questions its empty page offers. They reach the page through sync. A
//! topic the workspace's own topic proposed starts with all of that, and
//! keeps what you told it as its first points.

use almanac_domain::{
    Actor, ICONS, ModelKind, Operation, Proposal, StartTopicRequest, StartedTopic, TEMPLATE_NAMES,
    TopicChange, Transaction, template,
};
use almanac_sync::Origin;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::acp::Question;
use crate::auth::Member;
use crate::error::AppError;
use crate::kept::{fields, offer, patch_fields};
use crate::mcp::{Asking, Mode};
use crate::state::AppState;

const TOPIC: &str = "SELECT name, sticks_to FROM topics WHERE id = $1 AND workspace_id = $2";
const GROUPS: &str = "SELECT DISTINCT group_name FROM topics \
     WHERE workspace_id = $1 AND group_name <> '' ORDER BY group_name";
/// A proposal in an answer of the workspace's own topic.
const PROPOSAL: &str = "SELECT m.proposal FROM messages m JOIN topics t ON t.id = m.topic_id \
     WHERE m.id = $1 AND m.workspace_id = $2 AND t.kind = 'almanac' AND m.proposal IS NOT NULL";
const CHANGES: &str = "SELECT changes FROM messages WHERE id = $1 AND workspace_id = $2";
const STILL_THERE: &str = "SELECT id FROM topics WHERE workspace_id = $1 AND id = ANY($2)";

const SET_UP: &str = "You set up a new topic in Almanac, a notebook that keeps what someone \
learns, one topic at a time. Each topic has a page of blocks that keep points, and talks beside \
it. From the topic's name and what it sticks to (which may be empty), write:

- sticks_to: what talks in this topic stay with, in one or two sentences. The page shows it \
under the name, so say only what it's about (\"Houseplants and their care: watering, light, \
repotting.\"), not rules about what doesn't belong. When they wrote it, return theirs as it is.
- icon: the one icon from the list that fits best.
- group: a short group for the sidebar, such as Home, Work, Health or Learning. Reuse one of \
the existing groups when it fits.
- template: the blocks its page starts with. \"routine\" when it's a routine they keep, with \
steps on certain days and the things they use (a hair or skin routine, a training plan): \
Today, the week, You, Your shelf and Advice. \"collection\" when it's about a set of things \
that each get a card (plants, pets, trips, recipes): Today, the week and the cards. \"study\" \
when it's something to learn or write up (a field, a pitch, research): Reading, Terms and What \
it says. When they picked one, return theirs.
- cards: for a collection, what its cards are, in a word or two (\"Plants\", \"Recipes\"); \
otherwise \"\".
- starters: three or four first questions they might ask to begin, each under 12 words.

Write in the language of the topic's name.";

/// Claude Code hands the set-up over with a tool rather than as JSON.
const SET_UP_WITH_TOOL: &str = "Call set_up_topic once, with all of it. Nothing else is needed: \
no answer, no questions.";

/// A new topic as Claude sets it up.
#[derive(Debug, Deserialize)]
pub struct SetUp {
    pub sticks_to: String,
    pub icon: String,
    pub group: String,
    pub template: String,
    pub cards: String,
    pub starters: Vec<String>,
}

fn set_up_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["sticks_to", "icon", "group", "template", "cards", "starters"],
        "properties": {
            "sticks_to": { "type": "string" },
            "icon": { "type": "string", "enum": ICONS },
            "group": { "type": "string" },
            "template": { "type": "string", "enum": TEMPLATE_NAMES },
            "cards": { "type": "string" },
            "starters": { "type": "array", "items": { "type": "string" } },
        },
    })
}

/// `POST /api/w/{id}/topics`.
pub async fn start(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<StartTopicRequest>,
) -> Result<(StatusCode, Json<StartedTopic>), AppError> {
    member.require_not_guest()?;
    let actor = member.actor;
    let id = Uuid::now_v7();
    let picked = request
        .template
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if picked.is_some_and(|t| template(t).is_none()) {
        return Err(AppError::invalid(format!(
            "A topic starts as one of {TEMPLATE_NAMES:?}."
        )));
    }
    let proposed = match request.from_message {
        Some(message_id) => {
            let proposal: Option<Value> = sqlx::query_scalar(PROPOSAL)
                .bind(message_id)
                .bind(actor.workspace_id)
                .fetch_optional(&state.pool)
                .await?;
            let proposal: Proposal = proposal
                .and_then(|p| serde_json::from_value(p).ok())
                .ok_or_else(|| AppError::not_found("proposal not found"))?;
            if proposal.topic_id.is_some() {
                return Err(AppError::conflict("That topic is started already."));
            }
            Some((message_id, proposal))
        }
        None => None,
    };
    let proposal = proposed.as_ref().map(|(_, p)| p);
    let mine = |given: &Option<String>, proposed: Option<&str>| {
        given
            .as_deref()
            .map(str::trim)
            .or(proposed)
            .map(|v| v.to_string())
    };
    let mut data = Map::new();
    data.insert("name".into(), json!(request.name.trim()));
    // What you gave stays: a set-up after only fills in the rest.
    let mut given = Vec::new();
    for (field, value) in [
        (
            "sticks_to",
            mine(&request.sticks_to, proposal.map(|p| p.sticks_to.as_str())),
        ),
        (
            "icon",
            mine(&request.icon, proposal.map(|p| p.icon.as_str())),
        ),
        (
            "group_name",
            mine(&request.group_name, proposal.map(|p| p.group_name.as_str())),
        ),
    ] {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            data.insert(field.into(), json!(value));
            given.push(field);
        }
    }
    let mut operations = Vec::new();
    if let Some((message_id, proposal)) = &proposed {
        data.insert("starters".into(), json!(proposal.starters));
        operations.push(Operation::Create {
            model: ModelKind::Topic,
            id,
            data,
        });
        let template = picked
            .or(Some(proposal.template.as_str()))
            .filter(|t| template(t).is_some())
            .unwrap_or("collection");
        let (writes, blocks) = template_blocks(id, template, proposal.cards.as_deref());
        operations.extend(writes);
        for point in &proposal.points {
            let Some(block) = block_for(&blocks, point.block.as_deref()) else {
                continue;
            };
            let theme = match block.kind {
                "chips" | "list" => String::new(),
                _ if point.theme.trim().is_empty() => block.title.clone(),
                _ => cut(&point.theme, 120),
            };
            operations.push(Operation::Create {
                model: ModelKind::Point,
                id: Uuid::now_v7(),
                data: fields([
                    ("topic_id", json!(id)),
                    ("block_id", json!(block.id)),
                    ("theme", json!(theme)),
                    ("text", json!(point.text)),
                    ("source", json!("You said")),
                ]),
            });
        }
        // Offered under the answer that proposed them, to add with a click.
        for reminder in &proposal.reminders {
            operations.push(offer(id, *message_id, reminder, &[]));
        }
        let mut started = proposal.clone();
        started.topic_id = Some(id);
        operations.push(Operation::Update {
            model: ModelKind::Message,
            id: *message_id,
            data: fields([("proposal", json!(started))]),
        });
    } else {
        operations.push(Operation::Create {
            model: ModelKind::Topic,
            id,
            data,
        });
    }
    // A template you picked lays out the page at once; otherwise its
    // blocks come with the set-up.
    let picked = match (&proposed, picked) {
        (None, Some(name)) => {
            let (writes, blocks) = template_blocks(id, name, None);
            operations.extend(writes);
            let cards = blocks.iter().find(|b| b.kind == "cards").map(|b| b.id);
            Some((name.to_string(), cards))
        }
        _ => None,
    };
    write(&state, actor, operations).await?;
    if proposed.is_none() {
        tokio::spawn(async move {
            if let Err(error) = set_up(&state, actor, id, &given, picked).await {
                tracing::warn!(%error, topic = %id, "couldn't set up the topic");
            }
        });
    }
    Ok((StatusCode::CREATED, Json(StartedTopic { id })))
}

/// `POST /api/w/{id}/messages/{message_id}/undo`: puts back what an answer
/// changed in topics, for those still there.
pub async fn undo(
    State(state): State<AppState>,
    member: Member,
    Path((_, message_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    member.require_not_guest()?;
    let actor = member.actor;
    let changes: Option<Value> = sqlx::query_scalar(CHANGES)
        .bind(message_id)
        .bind(actor.workspace_id)
        .fetch_optional(&state.pool)
        .await?;
    let mut changes: Vec<TopicChange> = changes
        .and_then(|c| serde_json::from_value(c).ok())
        .ok_or_else(|| AppError::not_found("message not found"))?;
    let ids: Vec<Uuid> = changes.iter().map(|c| c.topic_id).collect();
    let there: Vec<Uuid> = sqlx::query_scalar(STILL_THERE)
        .bind(actor.workspace_id)
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?;
    let mut operations = Vec::new();
    for change in changes.iter_mut().filter(|c| !c.undone) {
        change.undone = true;
        if !there.contains(&change.topic_id) {
            continue;
        }
        if let Value::Object(data) = patch_fields(&change.before) {
            operations.push(Operation::Update {
                model: ModelKind::Topic,
                id: change.topic_id,
                data,
            });
        }
    }
    operations.push(Operation::Update {
        model: ModelKind::Message,
        id: message_id,
        data: fields([("changes", json!(changes))]),
    });
    write(&state, actor, operations).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Has Claude write the rest of a new topic, leaving what you `given` as
/// it is. A template you `picked` laid out its page already, its cards
/// block named once Claude says what the cards are; otherwise the page
/// starts as the template Claude picks, or a collection when Claude can't
/// answer. With no Claude the topic keeps what it started with.
async fn set_up(
    state: &AppState,
    actor: Actor,
    id: Uuid,
    given: &[&str],
    picked: Option<(String, Option<Uuid>)>,
) -> Result<(), AppError> {
    let (name, sticks_to): (String, String) = sqlx::query_as(TOPIC)
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_one(&state.pool)
        .await?;
    let set_up = match ask_set_up(state, actor, id, &name, &sticks_to, picked.as_ref()).await {
        Ok(set_up) => set_up,
        Err(error) => {
            tracing::warn!(%error, topic = %id, "Claude didn't set up the topic");
            None
        }
    };
    let cards = set_up
        .as_ref()
        .map(|s| cut(&s.cards, 60))
        .filter(|c| !c.is_empty());
    let mut operations = Vec::new();
    match &picked {
        Some((_, Some(block))) => {
            if let Some(cards) = &cards {
                operations.push(Operation::Update {
                    model: ModelKind::Block,
                    id: *block,
                    data: fields([("title", json!(cards))]),
                });
            }
        }
        Some((_, None)) => {}
        None => {
            let name = set_up
                .as_ref()
                .map(|s| s.template.as_str())
                .filter(|t| template(t).is_some())
                .unwrap_or("collection");
            operations.extend(template_blocks(id, name, cards.as_deref()).0);
        }
    }
    if let Some(set_up) = set_up {
        let mut data = Map::new();
        let mut set = |field: &str, value: Value| {
            if !given.contains(&field) {
                data.insert(field.into(), value);
            }
        };
        if sticks_to.is_empty() {
            set("sticks_to", json!(cut(&set_up.sticks_to, 1000)));
        }
        if ICONS.contains(&set_up.icon.as_str()) {
            set("icon", json!(set_up.icon));
        }
        set("group_name", json!(cut(&set_up.group, 40)));
        let starters: Vec<String> = set_up
            .starters
            .iter()
            .map(|s| cut(s, 200))
            .filter(|s| !s.is_empty())
            .take(4)
            .collect();
        set("starters", json!(starters));
        operations.push(Operation::Update {
            model: ModelKind::Topic,
            id,
            data,
        });
    }
    write(state, actor, operations).await
}

/// What Claude sets up for a new topic, or nothing when there's no Claude
/// or it didn't answer.
async fn ask_set_up(
    state: &AppState,
    actor: Actor,
    id: Uuid,
    name: &str,
    sticks_to: &str,
    picked: Option<&(String, Option<Uuid>)>,
) -> Result<Option<SetUp>, AppError> {
    let groups: Vec<String> = sqlx::query_scalar(GROUPS)
        .bind(actor.workspace_id)
        .fetch_all(&state.pool)
        .await?;
    let prompt = format!(
        "The topic's name: {name}\nWhat it sticks to: {}\nThe template they picked: {}\nThe existing groups: {}\nThe icons: {}",
        if sticks_to.is_empty() {
            "(empty)"
        } else {
            sticks_to
        },
        picked.map_or("none, pick one", |(t, _)| t.as_str()),
        if groups.is_empty() {
            "none yet".into()
        } else {
            groups.join(", ")
        },
        ICONS.join(", "),
    );
    match &state.claude_code {
        Some(claude_code) => {
            let open = state.questions.open(Asking::new(actor, id, Mode::SetUp));
            let system = format!("{SET_UP}\n\n{SET_UP_WITH_TOOL}");
            let tools = Mode::SetUp.tools();
            let question = Question {
                system: &system,
                prompt: &prompt,
                images: &[],
                web: false,
                tools,
                token: &open.token,
            };
            if let Err(failure) = claude_code.ask(&question, |_| {}).await {
                tracing::info!(?failure, topic = %id, "Claude Code didn't set up the topic");
                return Ok(None);
            }
            Ok(open.asking.take_set_up())
        }
        None => {
            let Some(key) = state.claude.key(&state.pool).await? else {
                return Ok(None);
            };
            state
                .claude
                .json(&key, SET_UP, &prompt, set_up_schema())
                .await
                .map(Some)
                .map_err(AppError::internal)
        }
    }
}

/// A block a template starts a topic with.
pub struct Started {
    pub id: Uuid,
    pub kind: &'static str,
    pub title: String,
}

/// The writes that give a topic the blocks of a template, its cards
/// titled `cards` when that's given, and the blocks they make.
pub fn template_blocks(
    topic_id: Uuid,
    name: &str,
    cards: Option<&str>,
) -> (Vec<Operation>, Vec<Started>) {
    let blocks: Vec<Started> = template(name)
        .unwrap_or_default()
        .iter()
        .map(|(kind, title)| Started {
            id: Uuid::now_v7(),
            kind,
            title: match (*kind, cards.map(|c| cut(c, 60))) {
                ("cards", Some(cards)) if !cards.is_empty() => cards,
                _ => title.to_string(),
            },
        })
        .collect();
    let writes = blocks
        .iter()
        .enumerate()
        .map(|(position, block)| Operation::Create {
            model: ModelKind::Block,
            id: block.id,
            data: fields([
                ("topic_id", json!(topic_id)),
                ("kind", json!(block.kind)),
                ("title", json!(block.title)),
                ("position", json!(position)),
            ]),
        })
        .collect();
    (writes, blocks)
}

/// The block a proposed point goes in: the one it names by title, or the
/// first that keeps points of its kind.
fn block_for<'a>(blocks: &'a [Started], named: Option<&str>) -> Option<&'a Started> {
    let named = named.map(|n| n.trim().to_lowercase()).unwrap_or_default();
    blocks
        .iter()
        .find(|b| b.kind != "week" && !named.is_empty() && b.title.to_lowercase() == named)
        .or_else(|| {
            ["cards", "text", "chips", "list", "shelf", "today"]
                .iter()
                .find_map(|kind| blocks.iter().find(|b| b.kind == *kind))
        })
}

/// The server's own writes to synced records, in one transaction.
pub async fn write(
    state: &AppState,
    actor: Actor,
    operations: Vec<Operation>,
) -> Result<(), AppError> {
    if operations.is_empty() {
        return Ok(());
    }
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    let mut db = state.pool.begin().await?;
    almanac_sync::apply_in(&mut db, actor, &transaction, Origin::Server).await?;
    db.commit().await?;
    Ok(())
}

/// Trimmed, and at most `max` characters.
pub fn cut(text: &str, max: usize) -> String {
    text.trim()
        .chars()
        .take(max)
        .collect::<String>()
        .trim_end()
        .to_string()
}
