//! What a person asks Forge AI (a check, a triage) is answered first by
//! Claude Code on their Mac, wherever the workspace is
//! (`studio/design/behaviour.md`, "Claude"). On this Mac's own Forge the
//! server asks Claude Code itself; on a server's workspace the desktop app
//! asks with `here`, the server hands the run over (`code-villain-claude`'s
//! `handed`), and Claude Code on the asker's Mac calls the run's tools at
//! `/api/ai/mcp` with the run's token. The server's key answers only when
//! Claude Code can't, in the browser, and for the night's runs, which never
//! come here (`Lane`).
//!
//! Claude Code runs as the person is signed in to it, so its runs cost the
//! server nothing; they're still Forge AI's, with the model `claude-code`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use code_villain_claude::mcp::{self, Called, Tools};
use code_villain_claude::{
    Adapter, Answer, Answered, App, Failure, HandedQuestion, Open, Question, bearer, handed,
};
use forge_ai::ModelProvider;
use forge_domain::{Actor, AiRunStatus, ModelKind, Operation};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::run::{Prepared, RunTools, by_claude_code};
use super::{Target, ai_agent, finish, object, prepare, write};
use crate::error::AppError;
use crate::state::AppState;

pub const FORGE: App = App {
    key: "forge",
    name: "Forge",
    version: env!("CARGO_PKG_VERSION"),
};

/// Where Claude Code reaches a run's tools.
pub const MCP_PATH: &str = "/api/ai/mcp";

/// The model a run by Claude Code names.
pub const CLAUDE_CODE: &str = "claude-code";

/// What the app says when it asks.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ask {
    /// The desktop app asks Claude Code on its Mac to run it.
    #[serde(default)]
    pub here: bool,
}

/// Who runs what a person asked.
pub enum Who {
    /// Claude Code on this Mac, for the desktop app's own Forge.
    ThisMac(Adapter),
    /// Claude Code on the asker's Mac, handed the run.
    TheirMac,
    /// The server's key, on the queue for what people ask.
    Key(Arc<dyn ModelProvider>),
}

impl Who {
    /// Claude Code first; the key when the asker can't run it.
    pub fn of(state: &AppState, ask: &Ask) -> Option<Who> {
        if let Some(adapter) = &state.claude_code {
            Some(Who::ThisMac(adapter.clone()))
        } else if ask.here {
            Some(Who::TheirMac)
        } else {
            state.ai.clone().map(Who::Key)
        }
    }

    pub fn model(&self) -> &str {
        match self {
            Who::Key(provider) => provider.model(),
            _ => CLAUDE_CODE,
        }
    }
}

/// A run Claude Code has now: its tools, found by its token.
pub struct MacRun {
    definitions: Vec<Value>,
    tools: Mutex<Box<dyn RunTools>>,
}

impl Tools for MacRun {
    fn list(&self) -> Vec<Value> {
        self.definitions.iter().map(mcp::listed).collect()
    }

    async fn call(&self, name: &str, arguments: Value) -> Called {
        let mut tools = self.tools.lock().await;
        if tools.finished() {
            return Called::failed("This run is finished.");
        }
        let output = tools.call(name, &arguments).await;
        Called {
            text: output.content,
            failed: output.is_error,
        }
    }
}

/// Marks a run Claude Code takes as running as soon as it's queued, in the
/// same transaction, so the key's worker never picks it up.
pub async fn claim(
    db: &mut PgConnection,
    who: &Who,
    workspace_id: Uuid,
    run_id: Uuid,
) -> Result<(), AppError> {
    if matches!(who, Who::Key(_)) {
        return Ok(());
    }
    let agent = ai_agent(db, workspace_id).await?;
    let started = object(json!({ "status": AiRunStatus::Running, "started_at": Utc::now() }));
    write(
        db,
        agent,
        vec![Operation::Update {
            model: ModelKind::AiRun,
            id: run_id,
            data: started,
        }],
    )
    .await
}

/// Starts a run someone asked for, once it's queued (and claimed): the key's
/// worker takes it, or Claude Code here does, or it's handed to the asker's
/// Mac, which gets the question back.
pub async fn start(
    state: &AppState,
    who: Who,
    workspace_id: Uuid,
    run_id: Uuid,
    target: Target,
) -> Result<Option<HandedQuestion>, AppError> {
    let adapter = match who {
        Who::Key(_) => {
            state.asked_wake.notify_one();
            return Ok(None);
        }
        Who::ThisMac(adapter) => Some(adapter),
        Who::TheirMac => None,
    };
    let agent = {
        let mut db = state.pool.acquire().await?;
        ai_agent(&mut db, workspace_id).await?
    };
    tracing::info!(%run_id, here = adapter.is_some(), "AI run started with Claude Code");
    let prepared = match prepare(&state.pool, &state.drift, agent, run_id, target).await {
        Ok(prepared) => prepared,
        Err(error) => {
            finish(&state.pool, agent, run_id, Err(error)).await?;
            return Ok(None);
        }
    };
    let Prepared {
        system,
        context,
        definitions,
        max_turns,
        timeout,
        tools,
        ..
    } = prepared;
    let names: Vec<String> = definitions
        .iter()
        .filter_map(|d| d["name"].as_str().map(String::from))
        .collect();
    let run = MacRun {
        definitions,
        tools: Mutex::new(tools),
    };
    let ended = Ended {
        state: state.clone(),
        agent,
        run_id,
        context,
        max_turns,
    };
    match adapter {
        Some(adapter) => {
            let open = state.runs.open(run);
            tokio::spawn(async move {
                let tools: Vec<&str> = names.iter().map(String::as_str).collect();
                let question = Question {
                    system,
                    prompt: &ended.context,
                    images: &[],
                    web: false,
                    tools: &tools,
                    token: &open.token,
                    timeout: Some(timeout),
                    max_turns: Some(max_turns as u32),
                };
                let answer = adapter.ask(&question, |_| {}).await;
                ended.settle(open, answer).await;
            });
            Ok(None)
        }
        None => {
            let (open, answered) = state.runs.hand(run);
            let tools: Vec<&str> = names.iter().map(String::as_str).collect();
            let question = handed::question(&Question {
                system,
                prompt: &ended.context,
                images: &[],
                web: false,
                tools: &tools,
                token: &open.token,
                timeout: Some(timeout),
                max_turns: Some(max_turns as u32),
            });
            tokio::spawn(async move {
                let answer = handed::wait(answered, std::future::pending(), Some(timeout)).await;
                ended.settle(open, answer).await;
            });
            Ok(Some(question))
        }
    }
}

/// What a run by Claude Code needs once it ends.
struct Ended {
    state: AppState,
    agent: Actor,
    run_id: Uuid,
    context: String,
    max_turns: usize,
}

impl Ended {
    /// Writes what came of it. Claude Code not installed or signed out
    /// hands the run to the key, when the server has one.
    async fn settle(self, open: Open<MacRun>, answer: Result<Answer, Failure>) {
        let Ended {
            state,
            agent,
            run_id,
            context,
            max_turns,
        } = self;
        let outcome = match answer {
            Ok(answer) => {
                let tools = open.what().tools.lock().await;
                Ok(by_claude_code(context, max_turns, answer, tools.as_ref()))
            }
            Err(Failure::NotInstalled | Failure::SignedOut) if state.ai.is_some() => {
                drop(open);
                let model = state.ai.as_ref().map(|p| p.model().to_string());
                match requeue(&state, agent, run_id, model.as_deref().unwrap_or_default()).await {
                    Ok(()) => state.asked_wake.notify_one(),
                    Err(e) => {
                        tracing::error!(%run_id, error = ?e, "couldn't hand the run to the key")
                    }
                }
                return;
            }
            Err(failure) => Err(problem(failure)),
        };
        drop(open);
        if let Err(e) = finish(&state.pool, agent, run_id, outcome).await {
            tracing::error!(%run_id, error = ?e, "couldn't finish the run");
        }
    }
}

/// Puts the run back on the queue for what people ask, for the key.
async fn requeue(
    state: &AppState,
    agent: Actor,
    run_id: Uuid,
    model: &str,
) -> Result<(), AppError> {
    tracing::info!(%run_id, "Claude Code can't run it; the key will");
    let mut db = state.pool.begin().await?;
    let queued =
        object(json!({ "status": AiRunStatus::Queued, "started_at": null, "model": model }));
    write(
        &mut db,
        agent,
        vec![Operation::Update {
            model: ModelKind::AiRun,
            id: run_id,
            data: queued,
        }],
    )
    .await?;
    db.commit().await?;
    Ok(())
}

/// Why Claude Code gave no answer, in words for the run.
fn problem(failure: Failure) -> String {
    match failure {
        Failure::NotInstalled => "Forge AI runs with Claude Code on your Mac, and its ACP adapter \
             isn't installed. Settings → Claude has the command that installs it; then ask again."
            .into(),
        Failure::SignedOut => "Claude Code isn't signed in on your Mac. Run claude in Terminal \
             and sign in there, then ask again."
            .into(),
        Failure::Trouble(said) => said,
    }
}

/// `POST /api/ai/mcp`: a run's tools, for Claude Code, behind the run's
/// token. One JSON-RPC message per request; a notification gets 202.
pub async fn mcp_endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Result<Response, AppError> {
    let run = token(&headers)
        .and_then(|token| state.runs.get(token))
        .ok_or_else(over)?;
    Ok(match mcp::reply(FORGE, &message, &run.what).await {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    })
}

/// `POST /api/answered`: what Claude Code on the asker's Mac made of a run
/// handed to it, behind the run's token.
pub async fn answered(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(answered): Json<Answered>,
) -> Result<StatusCode, AppError> {
    let token = token(&headers).ok_or_else(over)?;
    if state.runs.answered(token, answered) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(over())
    }
}

fn token(headers: &HeaderMap) -> Option<&str> {
    bearer(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
    )
}

fn over() -> AppError {
    AppError::unauthorized("This run is over.")
}
