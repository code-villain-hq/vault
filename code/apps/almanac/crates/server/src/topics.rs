//! Starting a topic: a name, and what it sticks to, which may be empty.
//! Almanac makes the topic at once; then Claude writes what it sticks to
//! (when you left it empty), a line about it, and picks its icon, its group,
//! its shape and the first questions its empty page offers. They reach the
//! page through sync. A topic the workspace's own topic proposed starts
//! with all of that, and keeps what you told it as its first points.

use almanac_domain::{
    Actor, ICONS, ModelKind, Operation, Proposal, SHAPES, StartTopicRequest, StartedTopic,
    TopicChange, Transaction,
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
learns, one topic at a time. Each topic has a page that keeps points by theme, and talks beside \
it. From the topic's name and what it sticks to (which may be empty), write:

- title: one line saying what the topic is about, under 12 words.
- sticks_to: what talks in this topic stay with, in one or two sentences. The page shows it \
under the name, so say only what it's about (\"Houseplants and their care: watering, light, \
repotting.\"), not rules about what doesn't belong. When they wrote it, return theirs as it is.
- icon: the one icon from the list that fits best.
- group: a short group for the sidebar, such as Home, Work, Health or Learning. Reuse one of \
the existing groups when it fits.
- shape: \"cards\" when the topic is about a set of things that each get a card (plants, pets, \
trips, recipes); \"document\" when what it keeps grows into one piece of writing with sections \
(a pitch, research, a plan).
- starters: three or four first questions they might ask to begin, each under 12 words.

Write in the language of the topic's name.";

/// Claude Code hands the set-up over with a tool rather than as JSON.
const SET_UP_WITH_TOOL: &str = "Call set_up_topic once, with all of it. Nothing else is needed: \
no answer, no questions.";

/// A new topic as Claude sets it up.
#[derive(Debug, Deserialize)]
pub struct SetUp {
    pub title: String,
    pub sticks_to: String,
    pub icon: String,
    pub group: String,
    pub shape: String,
    pub starters: Vec<String>,
}

fn set_up_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "sticks_to", "icon", "group", "shape", "starters"],
        "properties": {
            "title": { "type": "string" },
            "sticks_to": { "type": "string" },
            "icon": { "type": "string", "enum": ICONS },
            "group": { "type": "string" },
            "shape": { "type": "string", "enum": SHAPES },
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
        (
            "shape",
            mine(&request.shape, proposal.map(|p| p.shape.as_str())),
        ),
        (
            "title",
            mine(&request.title, proposal.and_then(|p| p.title.as_deref())),
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
        for point in &proposal.points {
            operations.push(Operation::Create {
                model: ModelKind::Point,
                id: Uuid::now_v7(),
                data: fields([
                    ("topic_id", json!(id)),
                    ("theme", json!(point.theme)),
                    ("text", json!(point.text)),
                    ("source", json!("You said")),
                ]),
            });
        }
        // Offered under the answer that proposed them, to add with a click.
        for reminder in &proposal.reminders {
            operations.push(offer(id, *message_id, reminder));
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
    write(&state, actor, operations).await?;
    if proposed.is_none() {
        tokio::spawn(async move {
            if let Err(error) = set_up(&state, actor, id, &given).await {
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
/// it is. With no Claude the topic keeps what it started with.
async fn set_up(state: &AppState, actor: Actor, id: Uuid, given: &[&str]) -> Result<(), AppError> {
    let (name, sticks_to): (String, String) = sqlx::query_as(TOPIC)
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_one(&state.pool)
        .await?;
    let groups: Vec<String> = sqlx::query_scalar(GROUPS)
        .bind(actor.workspace_id)
        .fetch_all(&state.pool)
        .await?;
    let prompt = format!(
        "The topic's name: {name}\nWhat it sticks to: {}\nThe existing groups: {}\nThe icons: {}",
        if sticks_to.is_empty() {
            "(empty)"
        } else {
            &sticks_to
        },
        if groups.is_empty() {
            "none yet".into()
        } else {
            groups.join(", ")
        },
        ICONS.join(", "),
    );
    let set_up = match &state.claude_code {
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
                return Ok(());
            }
            match open.asking.take_set_up() {
                Some(set_up) => set_up,
                None => return Ok(()),
            }
        }
        None => {
            let Some(key) = state.claude.key(&state.pool).await? else {
                return Ok(());
            };
            state
                .claude
                .json(&key, SET_UP, &prompt, set_up_schema())
                .await
                .map_err(AppError::internal)?
        }
    };
    let mut data = Map::new();
    let mut set = |field: &str, value: Value| {
        if !given.contains(&field) {
            data.insert(field.into(), value);
        }
    };
    set("title", json!(cut(&set_up.title, 200)));
    if sticks_to.is_empty() {
        set("sticks_to", json!(cut(&set_up.sticks_to, 1000)));
    }
    if ICONS.contains(&set_up.icon.as_str()) {
        set("icon", json!(set_up.icon));
    }
    set("group_name", json!(cut(&set_up.group, 40)));
    if SHAPES.contains(&set_up.shape.as_str()) {
        set("shape", json!(set_up.shape));
    }
    let starters: Vec<String> = set_up
        .starters
        .iter()
        .map(|s| cut(s, 200))
        .filter(|s| !s.is_empty())
        .take(4)
        .collect();
    set("starters", json!(starters));
    write(
        state,
        actor,
        vec![Operation::Update {
            model: ModelKind::Topic,
            id,
            data,
        }],
    )
    .await
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
