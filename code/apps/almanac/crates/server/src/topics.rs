//! Starting a topic: a name, what it sticks to, which may be empty, and the
//! template its blocks start from, which Almanac may pick. Almanac makes
//! the topic at once; then Claude writes what it sticks to, its brief,
//! around what you wrote, and picks its icon, its group, its template and
//! the first questions its empty page offers. They reach the page through sync. A
//! topic the workspace's own topic proposed starts with all of that, and
//! keeps what you told it as its first points. A sub-topic a topic's talk
//! proposed takes what it names from the parent's page: it moves there.

use almanac_domain::{
    Actor, Answered, Field, ICONS, ModelKind, Operation, Proposal, STICKS_TO_MAX,
    StartTopicRequest, StartedTopic, TEMPLATE_NAMES, TopicChange, Transaction, template,
};
use almanac_sync::Origin;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::acp::{Failure, Question};
use crate::auth::Member;
use crate::error::AppError;
use crate::handed;
use crate::kept::{fields, offer, patch_fields};
use crate::mcp::{Asking, Mode, Open};
use crate::page::Page;
use crate::state::AppState;

const TOPIC: &str = "SELECT name, sticks_to FROM topics WHERE id = $1 AND workspace_id = $2";
/// What a sub-topic takes, as the parent's page has it: a block on the
/// page, or a row with a page of its own.
const TAKEN_BLOCK: &str = "SELECT to_jsonb(b) FROM blocks b \
     WHERE b.workspace_id = $1 AND b.topic_id = $2 AND b.id = $3 AND b.row_id IS NULL";
const TAKEN_ROW: &str = "SELECT b.id FROM points p JOIN blocks b ON b.row_id = p.id \
     WHERE p.workspace_id = $1 AND p.topic_id = $2 AND p.id = $3 ORDER BY b.position";
const BLOCK: &str = "SELECT to_jsonb(b) FROM blocks b WHERE b.workspace_id = $1 AND b.id = $2";
const BLOCK_POINTS: &str = "SELECT to_jsonb(p) FROM points p \
     WHERE p.workspace_id = $1 AND p.block_id = $2 ORDER BY p.position";
const ROW: &str = "SELECT to_jsonb(p) FROM points p WHERE p.workspace_id = $1 AND p.id = $2";
const ROW_BLOCKS: &str = "SELECT b.id FROM blocks b \
     WHERE b.workspace_id = $1 AND b.row_id = $2 ORDER BY b.position";
const GROUPS: &str = "SELECT DISTINCT group_name FROM topics \
     WHERE workspace_id = $1 AND group_name <> '' ORDER BY group_name";
/// A proposal in an answer of the workspace's own topic.
const PROPOSAL: &str = "SELECT m.proposal FROM messages m JOIN topics t ON t.id = m.topic_id \
     WHERE m.id = $1 AND m.workspace_id = $2 AND m.proposal IS NOT NULL";
/// The sub-topics an answer of a topic's talk proposes.
const SUB_TOPICS: &str =
    "SELECT m.sub_topics FROM messages m WHERE m.id = $1 AND m.workspace_id = $2";
/// One start from a proposal at a time: two sub-topics of an answer
/// started at once would each write the answer's list as they read it.
static STARTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const CHANGES: &str = "SELECT changes FROM messages WHERE id = $1 AND workspace_id = $2";
const STILL_THERE: &str = "SELECT id FROM topics WHERE workspace_id = $1 AND id = ANY($2)";

const SET_UP: &str = "You set up a new topic in Almanac, a notebook that keeps what someone \
learns, one topic at a time. Each topic has a page of blocks that keep points, and talks beside \
it. From the topic's name and what it sticks to (which may be empty), write:

- sticks_to: the topic's brief, which every talk in it follows. First what it's about, in a \
sentence or two (\"Houseplants and their care: watering, light, repotting.\"). Then, a line \
each, how it answers: in which language, the tone and the length, what it brings in (sources, \
perspectives, what photos show), what it checks before it answers, and what it leaves to other \
topics. Fit it to this topic, with nothing every topic does anyway. Usually 400 to 1,200 \
characters, never more than 4,000. It's for the answers: the page doesn't show it, only the \
topic's About. When they wrote some, keep all of theirs, word for word, at the start, and \
write the rest after it.
- icon: the one icon from the list that fits best.
- group: a short group for the sidebar, such as Home, Work, Health or Learning. Reuse one of \
the existing groups when it fits.
- template: the blocks its page starts with. \"routine\" when it's a routine they keep, with \
steps on certain days and the things they use (a hair or skin routine, a training plan): a \
checklist of its steps, Your shelf (a collection of what they use) and Advice. \"collection\" \
when it's about a set of things that each get a row (plants, pets, trips, recipes): a To do \
checklist and a collection of the things. \"study\" when it's something to learn or write up \
(a field, a pitch, research): a Reading checklist, Terms and What it says. Each also has You, \
for what it learns about them, which only its About shows. When they picked one, return theirs.
- cards: for a collection, what its rows are, in a word or two (\"Plants\", \"Recipes\"); \
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
    let _starting = match request.from_message {
        Some(_) => Some(STARTING.lock().await),
        None => None,
    };
    // The answer's sub-topics, when it's one of them.
    let mut sub_topics: Vec<Proposal> = Vec::new();
    let proposed = match request.from_message {
        Some(message_id) => {
            let proposal: Option<Proposal> = match request.sub_topic {
                Some(n) => {
                    let all: Option<Value> = sqlx::query_scalar(SUB_TOPICS)
                        .bind(message_id)
                        .bind(actor.workspace_id)
                        .fetch_optional(&state.pool)
                        .await?;
                    sub_topics = all
                        .and_then(|all| serde_json::from_value(all).ok())
                        .unwrap_or_default();
                    sub_topics.get(n as usize).cloned()
                }
                None => sqlx::query_scalar::<_, Value>(PROPOSAL)
                    .bind(message_id)
                    .bind(actor.workspace_id)
                    .fetch_optional(&state.pool)
                    .await?
                    .and_then(|p| serde_json::from_value(p).ok()),
            };
            let proposal = proposal.ok_or_else(|| AppError::not_found("proposal not found"))?;
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
    if let Some(parent) = request.parent_id.or(proposal.and_then(|p| p.parent_id)) {
        let mut db = state.pool.acquire().await?;
        almanac_sync::may_sit_under(&mut db, actor.workspace_id, id, parent).await?;
        data.insert("parent_id".into(), json!(parent));
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
        let (mut writes, mut blocks) =
            template_blocks(id, template, proposal.cards.as_deref(), &proposal.fields);
        let taken = match proposal.parent_id {
            Some(parent) if !proposal.takes.is_empty() => {
                let taken =
                    take(&state, actor.workspace_id, parent, &proposal.takes, id, 0).await?;
                // Made there before they go here.
                [taken.made, taken.gone].concat()
            }
            _ => Vec::new(),
        };
        if !taken.is_empty() {
            // What it takes is its page: of the template, only You.
            let you = blocks.iter().position(|b| b.kind == "you");
            let placed = taken.iter().filter(|o| is_top_block(o)).count();
            writes = you
                .map(|n| {
                    let mut write = writes.swap_remove(n);
                    if let Operation::Create { data, .. } = &mut write {
                        data.insert("position".into(), json!(placed));
                    }
                    write
                })
                .into_iter()
                .collect();
            blocks.retain(|b| b.kind == "you");
        }
        operations.extend(writes);
        operations.extend(taken);
        for (position, point) in proposal.points.iter().enumerate() {
            let Some(block) = block_for(&blocks, point.block.as_deref(), !point.cells.is_empty())
            else {
                continue;
            };
            let cells = match block.kind {
                "collection" => json!(point.cells),
                _ => json!({}),
            };
            operations.push(Operation::Create {
                model: ModelKind::Point,
                id: Uuid::now_v7(),
                data: fields([
                    ("topic_id", json!(id)),
                    ("block_id", json!(block.id)),
                    ("text", json!(point.text)),
                    ("cells", cells),
                    ("position", json!(position)),
                    ("source", json!("You said")),
                ]),
            });
        }
        // Offered under the answer that proposed them, to add with a click.
        for reminder in &proposal.reminders {
            operations.push(offer(id, *message_id, reminder, &[], None));
        }
        let mut started = proposal.clone();
        started.topic_id = Some(id);
        let data = match request.sub_topic {
            Some(n) => {
                sub_topics[n as usize] = started;
                fields([("sub_topics", json!(sub_topics))])
            }
            None => fields([("proposal", json!(started))]),
        };
        operations.push(Operation::Update {
            model: ModelKind::Message,
            id: *message_id,
            data,
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
            let (writes, blocks) = template_blocks(id, name, None, &[]);
            operations.extend(writes);
            let cards = blocks.iter().find(|b| b.rows).map(|b| b.id);
            Some((name.to_string(), cards))
        }
        _ => None,
    };
    write(&state, actor, operations).await?;
    // On a server, the desktop app sets it up with Claude Code on the
    // asker's Mac: the question goes back with the topic.
    let mut handed = None;
    let their_mac = match proposed.is_none() && request.here == Some(true) {
        true if state.claude_code.is_none() => {
            let (name, sticks_to): (String, String) = sqlx::query_as(TOPIC)
                .bind(id)
                .bind(actor.workspace_id)
                .fetch_one(&state.pool)
                .await?;
            let prompt = set_up_prompt(&state, actor, &name, &sticks_to, picked.as_ref()).await?;
            let (asking, answered) = Asking::new(actor, id, Mode::SetUp).handed();
            let open = state.questions.open(asking);
            let system = format!("{SET_UP}\n\n{SET_UP_WITH_TOOL}");
            handed = Some(handed::question(&Question {
                system: &system,
                prompt: &prompt,
                images: &[],
                web: false,
                tools: Mode::SetUp.tools(),
                token: &open.token,
            }));
            Some((open, answered))
        }
        _ => None,
    };
    if proposed.is_none() {
        tokio::spawn(async move {
            if let Err(error) = set_up(&state, actor, id, &given, picked, their_mac).await {
                tracing::warn!(%error, topic = %id, "couldn't set up the topic");
            }
        });
    }
    Ok((
        StatusCode::CREATED,
        Json(StartedTopic { id, set_up: handed }),
    ))
}

/// `POST /api/w/{id}/messages/{message_id}/undo`: puts back what an answer
/// changed in topics, for those still there, and a page it laid out anew
/// as it was.
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
        if let Value::Object(data) = patch_fields(&change.before)
            && !data.is_empty()
        {
            operations.push(Operation::Update {
                model: ModelKind::Topic,
                id: change.topic_id,
                data,
            });
        }
        if let Some(layout) = &change.layout
            && let Some(page) = Page::load(&state.pool, actor.workspace_id, change.topic_id).await?
        {
            let blocks: Vec<Uuid> = page.blocks.iter().map(|b| b.block.id).collect();
            let points: Vec<(Uuid, Uuid)> = page
                .blocks
                .iter()
                .flat_map(|b| b.points.iter().map(|p| (p.id, p.block_id)))
                .collect();
            operations.extend(crate::layout::undo(
                change.topic_id,
                layout,
                &blocks,
                &points,
            ));
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

/// A set-up handed to the asker's Mac: its token, open until it's done,
/// and where the Mac says it is.
type TheirMac = (Open, oneshot::Receiver<Answered>);

/// Has Claude write the rest of a new topic, leaving what you `given` as
/// it is: what it sticks to becomes a brief with all of yours in it. A template you `picked` laid out its page already, its cards
/// block named once Claude says what the cards are; otherwise the page
/// starts as the template Claude picks, or a collection when Claude can't
/// answer. With no Claude the topic keeps what it started with.
async fn set_up(
    state: &AppState,
    actor: Actor,
    id: Uuid,
    given: &[&str],
    picked: Option<(String, Option<Uuid>)>,
    their_mac: Option<TheirMac>,
) -> Result<(), AppError> {
    let (name, sticks_to): (String, String) = sqlx::query_as(TOPIC)
        .bind(id)
        .bind(actor.workspace_id)
        .fetch_one(&state.pool)
        .await?;
    let asked = ask_set_up(
        state,
        actor,
        id,
        &name,
        &sticks_to,
        picked.as_ref(),
        their_mac,
    );
    let set_up = match asked.await {
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
            operations.extend(template_blocks(id, name, cards.as_deref(), &[]).0);
        }
    }
    if let Some(set_up) = set_up {
        let mut data = Map::new();
        let brief = with_theirs(&sticks_to, &set_up.sticks_to);
        if !brief.is_empty() {
            data.insert("sticks_to".into(), json!(brief));
        }
        let mut set = |field: &str, value: Value| {
            if !given.contains(&field) {
                data.insert(field.into(), value);
            }
        };
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

/// The brief Claude wrote, with all of what they wrote in it: theirs comes
/// first when Claude's leaves any of it out.
fn with_theirs(theirs: &str, written: &str) -> String {
    let theirs = theirs.trim();
    let written = cut(written, STICKS_TO_MAX);
    if theirs.is_empty() || written.contains(theirs) {
        written
    } else if written.is_empty() {
        theirs.to_string()
    } else {
        cut(&format!("{theirs}\n{written}"), STICKS_TO_MAX)
    }
}

/// What a new topic's set-up is asked: its name, what it sticks to, the
/// template picked, the groups so far and the icons.
async fn set_up_prompt(
    state: &AppState,
    actor: Actor,
    name: &str,
    sticks_to: &str,
    picked: Option<&(String, Option<Uuid>)>,
) -> Result<String, sqlx::Error> {
    let groups: Vec<String> = sqlx::query_scalar(GROUPS)
        .bind(actor.workspace_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(format!(
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
    ))
}

/// What Claude sets up for a new topic, or nothing when there's no Claude
/// or it didn't answer. Handed to the asker's Mac, what Claude Code there
/// did; when it can't, the server's key does it.
#[allow(clippy::too_many_arguments)]
async fn ask_set_up(
    state: &AppState,
    actor: Actor,
    id: Uuid,
    name: &str,
    sticks_to: &str,
    picked: Option<&(String, Option<Uuid>)>,
    their_mac: Option<TheirMac>,
) -> Result<Option<SetUp>, AppError> {
    if let Some((open, answered)) = their_mac {
        match handed::wait(answered, std::future::pending()).await {
            Ok(_) => return Ok(open.asking.take_set_up()),
            // Claude Code there can't: the key does it, below.
            Err(Failure::NotInstalled | Failure::SignedOut) => {}
            Err(failure) => {
                tracing::info!(?failure, topic = %id, "their Mac didn't set up the topic");
                return Ok(None);
            }
        }
    }
    let prompt = set_up_prompt(state, actor, name, sticks_to, picked).await?;
    match state.answerer().await? {
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
                tracing::info!(?failure, topic = %id, "it didn't set up the topic");
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
    /// The collection its rows go in: the first with no check.
    pub rows: bool,
}

/// The writes that give a topic the blocks of a template, the collection
/// its rows go in titled `cards` when that's given, with these `fields`,
/// a checklist with its check, and the blocks they make.
pub fn template_blocks(
    topic_id: Uuid,
    name: &str,
    cards: Option<&str>,
    fields_of: &[Field],
) -> (Vec<Operation>, Vec<Started>) {
    let blocks = template(name).unwrap_or_default();
    let rows = blocks
        .iter()
        .position(|(kind, _, check)| *kind == "collection" && check.is_empty());
    let started: Vec<Started> = blocks
        .iter()
        .enumerate()
        .map(|(n, (kind, title, _))| Started {
            id: Uuid::now_v7(),
            kind,
            title: match cards.map(|c| cut(c, 60)) {
                Some(cards) if rows == Some(n) && !cards.is_empty() => cards,
                _ => title.to_string(),
            },
            rows: rows == Some(n),
        })
        .collect();
    let writes = started
        .iter()
        .zip(blocks)
        .enumerate()
        .map(|(position, (block, (_, _, check)))| Operation::Create {
            model: ModelKind::Block,
            id: block.id,
            data: fields([
                ("topic_id", json!(topic_id)),
                ("kind", json!(block.kind)),
                ("title", json!(block.title)),
                (
                    "fields",
                    if block.rows {
                        json!(fields_of)
                    } else if check.is_empty() {
                        json!([])
                    } else {
                        json!([Field {
                            name: check.to_string(),
                            kind: "check".into(),
                            unit: String::new(),
                            goal: None,
                            options: Vec::new(),
                        }])
                    },
                ),
                ("position", json!(position)),
            ]),
        })
        .collect();
    (writes, started)
}

/// What a sub-topic takes from its parent's page: the writes that make it
/// again there, and the ones that take it off the parent's.
pub struct Taken {
    pub made: Vec<Operation>,
    pub gone: Vec<Operation>,
}

impl Taken {
    /// The blocks it makes on the sub-topic's page and the points in them,
    /// which Undo takes off; their rows' pages go with the rows.
    pub fn on_the_page(&self) -> (Vec<Uuid>, Vec<Uuid>) {
        let blocks: Vec<Uuid> = self
            .made
            .iter()
            .filter(|o| is_top_block(o))
            .map(|o| match o {
                Operation::Create { id, .. } => *id,
                _ => Uuid::nil(),
            })
            .collect();
        let points = self
            .made
            .iter()
            .filter_map(|o| match o {
                Operation::Create {
                    model: ModelKind::Point,
                    id,
                    data,
                } => data
                    .get("block_id")
                    .and_then(Value::as_str)
                    .and_then(|b| b.parse().ok())
                    .filter(|b| blocks.contains(b))
                    .map(|_| *id),
                _ => None,
            })
            .collect();
        (blocks, points)
    }
}

/// What a sub-topic (`to`) `takes` from its `parent`'s page, from the
/// position `first` on: a block whole, a row's page as the sub-topic's
/// own, each with its points and its rows' pages, made again there and
/// gone here. What's no longer on the parent's page is left out.
pub async fn take(
    state: &AppState,
    workspace_id: Uuid,
    parent: Uuid,
    takes: &[Uuid],
    to: Uuid,
    first: i32,
) -> Result<Taken, AppError> {
    let mut db = state.pool.acquire().await?;
    // Blocks to make again: as they are, on the row they go on.
    let mut blocks: Vec<(Value, Option<Uuid>, Option<i32>)> = Vec::new();
    let mut gone = Vec::new();
    // The blocks the rows it takes are in.
    let mut rows_from: Vec<Uuid> = Vec::new();
    let mut position = first;
    for &id in takes {
        let block: Option<Value> = sqlx::query_scalar(TAKEN_BLOCK)
            .bind(workspace_id)
            .bind(parent)
            .bind(id)
            .fetch_optional(&mut *db)
            .await?;
        if let Some(block) = block {
            blocks.push((block, None, Some(position)));
            position += 1;
            gone.push(Operation::Delete {
                model: ModelKind::Block,
                id,
            });
            continue;
        }
        let page: Vec<Uuid> = sqlx::query_scalar(TAKEN_ROW)
            .bind(workspace_id)
            .bind(parent)
            .bind(id)
            .fetch_all(&mut *db)
            .await?;
        if page.is_empty() {
            continue;
        }
        for block in page {
            let block: Value = sqlx::query_scalar(BLOCK)
                .bind(workspace_id)
                .bind(block)
                .fetch_one(&mut *db)
                .await?;
            blocks.push((block, None, Some(position)));
            position += 1;
        }
        let from = sqlx::query_scalar::<_, Value>(ROW)
            .bind(workspace_id)
            .bind(id)
            .fetch_optional(&mut *db)
            .await?
            .and_then(|row| row.get("block_id")?.as_str()?.parse::<Uuid>().ok());
        if let Some(from) = from.filter(|b| !rows_from.contains(b)) {
            rows_from.push(from);
        }
        // The row goes, and its page with it.
        gone.push(Operation::Delete {
            model: ModelKind::Point,
            id,
        });
    }
    // What the rows leave behind goes with them.
    for from in rows_from.into_iter().filter(|b| !takes.contains(b)) {
        let block: Value = sqlx::query_scalar(BLOCK)
            .bind(workspace_id)
            .bind(from)
            .fetch_one(&mut *db)
            .await?;
        let rows: Vec<Value> = sqlx::query_scalar(BLOCK_POINTS)
            .bind(workspace_id)
            .bind(from)
            .fetch_all(&mut *db)
            .await?;
        let (taken, kept): (Vec<Value>, Vec<Value>) = rows
            .into_iter()
            .partition(|row| id_of(row).is_some_and(|id| takes.contains(&id)));
        let cells = |rows: Vec<Value>| -> Vec<Value> {
            rows.into_iter().map(|r| r["cells"].clone()).collect()
        };
        let kind = block["kind"].as_str().unwrap_or_default();
        let had: Vec<Field> = serde_json::from_value(block["fields"].clone()).unwrap_or_default();
        match left_behind(kind, &had, &cells(taken), &cells(kept)) {
            Left::As => {}
            Left::Empty => gone.push(Operation::Delete {
                model: ModelKind::Block,
                id: from,
            }),
            Left::Lanes(lanes) => gone.push(Operation::Update {
                model: ModelKind::Block,
                id: from,
                data: fields([("fields", json!(lanes))]),
            }),
        }
    }
    let mut operations = Vec::new();
    while let Some((block, row_id, position)) = blocks.pop() {
        let new_block = Uuid::now_v7();
        let mut data = kept_of(
            &block,
            &[
                "kind", "title", "facets", "fields", "width", "view", "position",
            ],
        );
        data.insert("topic_id".into(), json!(to));
        data.insert("row_id".into(), json!(row_id));
        if let Some(position) = position {
            data.insert("position".into(), json!(position));
        }
        operations.push(Operation::Create {
            model: ModelKind::Block,
            id: new_block,
            data,
        });
        let points: Vec<Value> = sqlx::query_scalar(BLOCK_POINTS)
            .bind(workspace_id)
            .bind(id_of(&block))
            .fetch_all(&mut *db)
            .await?;
        for point in points {
            let new_point = Uuid::now_v7();
            let mut data = kept_of(
                &point,
                &[
                    "theme",
                    "text",
                    "happened",
                    "facet",
                    "look",
                    "amount",
                    "note",
                    "done",
                    "cells",
                    "source",
                    "source_url",
                    "seen",
                    "position",
                ],
            );
            data.insert("topic_id".into(), json!(to));
            data.insert("block_id".into(), json!(new_block));
            operations.push(Operation::Create {
                model: ModelKind::Point,
                id: new_point,
                data,
            });
            let page: Vec<Uuid> = sqlx::query_scalar(ROW_BLOCKS)
                .bind(workspace_id)
                .bind(id_of(&point))
                .fetch_all(&mut *db)
                .await?;
            for block in page {
                let block: Value = sqlx::query_scalar(BLOCK)
                    .bind(workspace_id)
                    .bind(block)
                    .fetch_one(&mut *db)
                    .await?;
                blocks.push((block, Some(new_point), None));
            }
        }
    }
    // A block's points and a row's page go with it.
    Ok(Taken {
        made: operations,
        gone,
    })
}

/// What rows moving off a block leave of it.
#[derive(Debug, PartialEq)]
pub enum Left {
    As,
    /// No rows: the block goes too.
    Empty,
    /// A choice's lanes none of the rows left use, with the fields that stay.
    Lanes(Vec<Field>),
}

/// What a block of `kind` with `fields` is left as when rows with the
/// cells `taken` move off it and those with `kept` stay: nothing when no
/// row stays, and a choice's lanes only the moved rows used go with them.
/// You stays, rows or none.
pub fn left_behind(kind: &str, fields: &[Field], taken: &[Value], kept: &[Value]) -> Left {
    if taken.is_empty() {
        return Left::As;
    }
    if kept.is_empty() && kind != "you" {
        return Left::Empty;
    }
    let words = |rows: &[Value], name: &str| -> Vec<String> {
        rows.iter()
            .filter_map(|cells| cells.get(name)?.as_str().map(str::to_string))
            .collect()
    };
    let mut fields = fields.to_vec();
    let mut dropped = false;
    for field in fields.iter_mut().filter(|f| f.kind == "choice") {
        let (went, stay) = (words(taken, &field.name), words(kept, &field.name));
        let before = field.options.len();
        field
            .options
            .retain(|lane| !went.contains(lane) || stay.contains(lane));
        dropped |= field.options.len() != before;
    }
    if dropped {
        Left::Lanes(fields)
    } else {
        Left::As
    }
}

/// A record's id, as the database has it.
fn id_of(record: &Value) -> Option<Uuid> {
    record.get("id")?.as_str()?.parse().ok()
}

/// A record's `keys`, as the database has them.
fn kept_of(record: &Value, keys: &[&str]) -> Map<String, Value> {
    keys.iter()
        .filter_map(|key| Some((key.to_string(), record.get(*key)?.clone())))
        .filter(|(_, value)| !value.is_null())
        .collect()
}

/// Whether a write makes a block on the topic's own page.
fn is_top_block(operation: &Operation) -> bool {
    matches!(operation, Operation::Create { model: ModelKind::Block, data, .. }
        if data.get("row_id").is_none_or(Value::is_null))
}

/// The block a proposed point goes in: the one it names by title, or the
/// collection its rows go in, and a sentence without cells the first
/// paragraph or You when there's none.
fn block_for<'a>(blocks: &'a [Started], named: Option<&str>, row: bool) -> Option<&'a Started> {
    let named = named.map(|n| n.trim().to_lowercase()).unwrap_or_default();
    blocks
        .iter()
        .find(|b| !named.is_empty() && b.title.to_lowercase() == named)
        .or_else(|| blocks.iter().find(|b| b.rows))
        .or_else(|| {
            (!row)
                .then_some(["paragraph", "you"])
                .into_iter()
                .flatten()
                .find_map(|kind| blocks.iter().find(|b| b.kind == kind))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_that_move_take_what_only_they_had() {
        let field = |kind: &str| Field {
            name: "Part".into(),
            kind: kind.into(),
            unit: String::new(),
            goal: None,
            options: vec!["Basics".into(), "Notifications".into()],
        };
        let (basics, notes) = (
            [json!({ "Part": "Basics" })],
            [json!({ "Part": "Notifications" })],
        );
        assert_eq!(
            left_behind("collection", &[field("choice")], &notes, &[]),
            Left::Empty
        );
        assert_eq!(left_behind("you", &[], &[json!({})], &[]), Left::As);
        let Left::Lanes(fields) = left_behind("collection", &[field("choice")], &notes, &basics)
        else {
            panic!("the lane goes");
        };
        assert_eq!(fields[0].options, ["Basics"]);
        // A lane another row stays in stays, and a status keeps its steps.
        assert_eq!(
            left_behind("collection", &[field("choice")], &notes, &notes),
            Left::As
        );
        assert_eq!(
            left_behind("collection", &[field("status")], &notes, &basics),
            Left::As
        );
    }

    #[test]
    fn the_brief_keeps_what_they_wrote() {
        let theirs = "Zimmerpflanzen, ich schicke Fotos";
        let around = "Zimmerpflanzen, ich schicke Fotos: gießen, Licht, umtopfen.\nImmer Deutsch.";
        assert_eq!(with_theirs(theirs, around), around);
        assert_eq!(
            with_theirs(theirs, "Houseplants and their care.\nAnswers in German."),
            "Zimmerpflanzen, ich schicke Fotos\nHouseplants and their care.\nAnswers in German."
        );
        assert_eq!(with_theirs(theirs, " "), theirs);
        assert_eq!(with_theirs("", "Houseplants."), "Houseplants.");
        let long = "x".repeat(STICKS_TO_MAX);
        assert_eq!(with_theirs(theirs, &long).chars().count(), STICKS_TO_MAX);
    }
}
