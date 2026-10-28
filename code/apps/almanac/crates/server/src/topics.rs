//! Starting a topic: a name, and what it sticks to, which may be empty.
//! Almanac makes the topic at once; then Claude writes what it sticks to
//! (when you left it empty), a line about it, and picks its icon, its group,
//! its shape and the first questions its empty page offers. They reach the
//! page through sync.

use almanac_domain::{
    Actor, ICONS, ModelKind, Operation, SHAPES, StartTopicRequest, StartedTopic, Transaction,
};
use almanac_sync::Origin;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

const TOPIC: &str = "SELECT name, sticks_to FROM topics WHERE id = $1 AND workspace_id = $2";
const GROUPS: &str = "SELECT DISTINCT group_name FROM topics \
     WHERE workspace_id = $1 AND group_name <> '' ORDER BY group_name";

const SET_UP: &str = "You set up a new topic in Almanac, a notebook that keeps what someone \
learns, one topic at a time. Each topic has a page that keeps short points by theme, and talks \
beside it. From the topic's name and what it sticks to (which may be empty), return JSON in the \
schema's shape:

- title: one line saying what the topic is about, under 12 words.
- sticks_to: what talks in this topic stay with, in one or two sentences, so that a question \
that doesn't belong can be told apart. When they wrote it, return theirs as it is.
- icon: the one icon from the list that fits best.
- group: a short group for the sidebar, such as Home, Work, Health or Learning. Reuse one of \
the existing groups when it fits.
- shape: \"cards\" when the topic is about a set of things that each get a card (plants, pets, \
trips, recipes); \"document\" when what it keeps grows into one piece of writing with sections \
(a pitch, research, a plan).
- starters: three or four first questions they might ask to begin, each under 12 words.

Write in the language of the topic's name.";

#[derive(Deserialize)]
struct SetUp {
    title: String,
    sticks_to: String,
    icon: String,
    group: String,
    shape: String,
    starters: Vec<String>,
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
    let id = Uuid::now_v7();
    let mut data = Map::new();
    data.insert("name".into(), json!(request.name.trim()));
    if let Some(sticks_to) = request.sticks_to.as_deref().map(str::trim) {
        data.insert("sticks_to".into(), json!(sticks_to));
    }
    write(
        &state,
        member.actor,
        vec![Operation::Create {
            model: ModelKind::Topic,
            id,
            data,
        }],
    )
    .await?;
    let actor = member.actor;
    tokio::spawn(async move {
        if let Err(error) = set_up(&state, actor, id).await {
            tracing::warn!(%error, topic = %id, "couldn't set up the topic");
        }
    });
    Ok((StatusCode::CREATED, Json(StartedTopic { id })))
}

/// Has Claude write the rest of a new topic. Without a key the topic keeps
/// what it started with.
async fn set_up(state: &AppState, actor: Actor, id: Uuid) -> Result<(), AppError> {
    let Some(key) = state.claude.key(&state.pool).await? else {
        return Ok(());
    };
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
    let set_up: SetUp = state
        .claude
        .json(&key, SET_UP, &prompt, set_up_schema())
        .await
        .map_err(AppError::internal)?;
    let mut data = Map::new();
    data.insert("title".into(), json!(cut(&set_up.title, 200)));
    if sticks_to.is_empty() {
        data.insert("sticks_to".into(), json!(cut(&set_up.sticks_to, 1000)));
    }
    if ICONS.contains(&set_up.icon.as_str()) {
        data.insert("icon".into(), json!(set_up.icon));
    }
    data.insert("group_name".into(), json!(cut(&set_up.group, 40)));
    if SHAPES.contains(&set_up.shape.as_str()) {
        data.insert("shape".into(), json!(set_up.shape));
    }
    let starters: Vec<String> = set_up
        .starters
        .iter()
        .map(|s| cut(s, 200))
        .filter(|s| !s.is_empty())
        .take(4)
        .collect();
    data.insert("starters".into(), json!(starters));
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
