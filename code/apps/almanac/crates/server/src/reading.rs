//! Reading the workspace from another app, such as Claude Code, over MCP
//! (`POST /api/mcp/read`): its topics, a topic's page, and a search of what
//! the pages keep. Nothing here writes. A member makes the token in
//! Settings → Claude (`read_tokens`, one each, shown once), and it reads
//! what they may read for as long as they're in the workspace and not a
//! guest, who sees no topics.

use almanac_domain::{Actor, ReadToken, ReadTokenInfo};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::fmt::Write;
use uuid::Uuid;

use crate::auth::{Member, hash_token, new_token};
use crate::error::AppError;
use crate::mcp::PROTOCOL_VERSION;
use crate::page::{Page, Workspace};
use crate::state::AppState;
use crate::topics::cut;

const SHOW: &str = "SELECT made_at, used_at FROM read_tokens \
     WHERE workspace_id = $1 AND user_id = $2";
const MAKE: &str = "INSERT INTO read_tokens (workspace_id, user_id, token_hash) VALUES ($1, $2, $3) \
     ON CONFLICT (workspace_id, user_id) DO UPDATE \
     SET token_hash = excluded.token_hash, made_at = now(), used_at = NULL";
const REVOKE: &str = "DELETE FROM read_tokens WHERE workspace_id = $1 AND user_id = $2";
/// Whose token it is, while they're still in its workspace and see its topics.
const READER: &str = "UPDATE read_tokens r SET used_at = now() \
     FROM workspace_members m \
     WHERE r.token_hash = $1 AND m.workspace_id = r.workspace_id AND m.user_id = r.user_id \
       AND m.role <> 'guest' \
     RETURNING r.workspace_id, r.user_id";
const TOPIC_NAMED: &str = "SELECT id FROM topics \
     WHERE workspace_id = $1 AND kind = 'topic' AND lower(name) = lower($2) \
     ORDER BY created_at LIMIT 1";
const NAMES: &str = "SELECT name FROM topics WHERE workspace_id = $1 AND kind = 'topic' \
     ORDER BY group_name = '', group_name, position, created_at";
const FOUND: &str = "SELECT t.name, b.title, p.text \
     FROM points p \
     JOIN topics t ON t.id = p.topic_id \
     JOIN blocks b ON b.id = p.block_id \
     WHERE p.workspace_id = $1 AND t.kind = 'topic' \
       AND (p.text ILIKE $2 ESCAPE '\\' OR p.cells::text ILIKE $2 ESCAPE '\\') \
     ORDER BY t.group_name, t.position, b.position, p.position \
     LIMIT 41";
/// What one search shows at most.
const MOST_FOUND: usize = 40;

/// `GET /api/w/{id}/reading`: when your token was made and last used.
pub async fn show(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<ReadTokenInfo>, AppError> {
    member.require_not_guest()?;
    let made: Option<(DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(SHOW)
        .bind(member.actor.workspace_id)
        .bind(member.actor.user_id)
        .fetch_optional(&state.pool)
        .await?;
    Ok(Json(ReadTokenInfo {
        made_at: made.map(|(made, _)| made),
        used_at: made.and_then(|(_, used)| used),
    }))
}

/// `POST /api/w/{id}/reading`: a new token, in place of your last one.
pub async fn make(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<ReadToken>, AppError> {
    member.require_not_guest()?;
    let token = new_token();
    sqlx::query(MAKE)
        .bind(member.actor.workspace_id)
        .bind(member.actor.user_id)
        .bind(hash_token(&token))
        .execute(&state.pool)
        .await?;
    Ok(Json(ReadToken { token }))
}

/// `DELETE /api/w/{id}/reading`: your token stops reading.
pub async fn revoke(State(state): State<AppState>, member: Member) -> Result<StatusCode, AppError> {
    sqlx::query(REVOKE)
        .bind(member.actor.workspace_id)
        .bind(member.actor.user_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/mcp/read`: one JSON-RPC message from the app reading.
pub async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let reader: Option<(Uuid, Uuid)> = match token {
        Some(token) => sqlx::query_as(READER)
            .bind(hash_token(token))
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let Some((workspace_id, user_id)) = reader else {
        return (
            StatusCode::UNAUTHORIZED,
            "This token doesn't read Almanac: make one in Settings → Claude.",
        )
            .into_response();
    };
    let actor = Actor {
        user_id,
        workspace_id,
    };
    let id = message["id"].clone();
    if id.is_null() {
        return StatusCode::ACCEPTED.into_response();
    }
    let result = match message["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "almanac", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Almanac keeps what someone learns, by topic: each topic a page \
                 of blocks and points. Read it to answer from what they kept.",
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or("");
            let arguments = &message["params"]["arguments"];
            let (text, failed) = match call(&state, actor, name, arguments).await {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            };
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

async fn call(
    state: &AppState,
    actor: Actor,
    name: &str,
    arguments: &Value,
) -> Result<String, String> {
    let argument = |key: &str| cut(arguments[key].as_str().unwrap_or("").trim(), 200);
    match name {
        "list_topics" => {
            let workspace = Workspace::load(&state.pool, actor.workspace_id)
                .await
                .map_err(|_| "Almanac couldn't read its topics.".to_string())?;
            Ok(workspace.describe())
        }
        "read_topic" => {
            let named = argument("topic");
            let id: Option<Uuid> = sqlx::query_scalar(TOPIC_NAMED)
                .bind(actor.workspace_id)
                .bind(&named)
                .fetch_optional(&state.pool)
                .await
                .map_err(|_| "Almanac couldn't look at its topics.".to_string())?;
            let Some(id) = id else {
                let names: Vec<String> = sqlx::query_scalar(NAMES)
                    .bind(actor.workspace_id)
                    .fetch_all(&state.pool)
                    .await
                    .unwrap_or_default();
                return Err(format!(
                    "No topic is called {named}. The topics: {}.",
                    names.join(", ")
                ));
            };
            let page = Page::load(&state.pool, actor.workspace_id, id)
                .await
                .map_err(|_| "Almanac couldn't read that topic.".to_string())?
                .ok_or("That topic is gone.")?;
            Ok(page.as_read())
        }
        "search" => {
            let words = argument("words");
            if words.is_empty() {
                return Err("Say what to look for.".into());
            }
            let like = format!(
                "%{}%",
                words
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            let found: Vec<(String, String, String)> = sqlx::query_as(FOUND)
                .bind(actor.workspace_id)
                .bind(&like)
                .fetch_all(&state.pool)
                .await
                .map_err(|_| "Almanac couldn't search.".to_string())?;
            if found.is_empty() {
                return Ok(format!("No page keeps \"{words}\"."));
            }
            let mut out = String::new();
            for (topic, block, text) in found.iter().take(MOST_FOUND) {
                let _ = if block.is_empty() {
                    writeln!(out, "- {topic}: {text}")
                } else {
                    writeln!(out, "- {topic} › {block}: {text}")
                };
            }
            if found.len() > MOST_FOUND {
                let _ = writeln!(out, "And more: narrow it down, or read a topic whole.");
            }
            Ok(out)
        }
        _ => Err(format!("There's no {name} here.")),
    }
}

fn tools() -> Value {
    let only_reads = json!({ "readOnlyHint": true });
    json!([
        {
            "name": "list_topics",
            "description": "Lists the workspace's topics by group: what each sticks to, its \
                 blocks, how many points, and its last talk.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": only_reads,
        },
        {
            "name": "read_topic",
            "description": "Reads one topic's page: its brief, its sub-topics, and its blocks \
                 with their points and rows.",
            "inputSchema": {
                "type": "object",
                "properties": { "topic": { "type": "string", "description": "The topic's name." } },
                "required": ["topic"],
                "additionalProperties": false,
            },
            "annotations": only_reads,
        },
        {
            "name": "search",
            "description": "Finds the points and rows that hold some words, across every \
                 topic's page, each with its topic and block.",
            "inputSchema": {
                "type": "object",
                "properties": { "words": { "type": "string", "description": "What to look for, as it's written." } },
                "required": ["words"],
                "additionalProperties": false,
            },
            "annotations": only_reads,
        },
    ])
}
