//! The translation models in the server's models file, and starting and
//! stopping the ones it may run itself.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use glyph_domain::{ModelInfo, ModelProcess};
use glyph_llm::{Client, Model, ProcessError, Status};

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::state::AppState;

/// How long a model server gets to stop before it's killed.
pub const GRACE: Duration = Duration::from_secs(5);

/// `GET /api/models`: every model, and whether it answers.
pub async fn list(State(state): State<AppState>, _user: CurrentUser) -> Json<Vec<ModelInfo>> {
    let infos = state.models.models.iter().map(|m| info(&state, m));
    Json(futures::future::join_all(infos).await)
}

/// `POST /api/models/{name}/start`.
pub async fn start(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(name): Path<String>,
) -> Result<Json<ModelInfo>, AppError> {
    let model = find(&state, &name)?;
    let args = model.start_args().map_err(AppError::invalid)?;
    let binary = match state.models.llm_binary.trim() {
        "" => "llama-server",
        binary => binary,
    };
    state
        .processes
        .start(&model.name, binary, &args)
        .map_err(|e| match e {
            ProcessError::AlreadyRunning => AppError::conflict(e.to_string()),
            ProcessError::Spawn { .. } => AppError::invalid(format!(
                "Couldn't start {binary}. Is it installed? ({e})"
            )),
            ProcessError::NotRunning => AppError::internal(e),
        })?;
    tracing::info!(model = model.name, binary, "started a model server");
    Ok(Json(info(&state, model).await))
}

/// `POST /api/models/{name}/stop`: stops a server this one started.
pub async fn stop(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(name): Path<String>,
) -> Result<Json<ModelInfo>, AppError> {
    let model = find(&state, &name)?;
    state
        .processes
        .stop(&model.name, GRACE)
        .await
        .map_err(|_| AppError::invalid("It wasn't started from here, so it can't be stopped from here."))?;
    Ok(Json(info(&state, model).await))
}

fn find<'a>(state: &'a AppState, name: &str) -> Result<&'a Model, AppError> {
    state
        .models
        .find(name)
        .ok_or_else(|| AppError::not_found("no model by that name"))
}

async fn info(state: &AppState, model: &Model) -> ModelInfo {
    let client = Client::new(model);
    ModelInfo {
        name: model.name.clone(),
        endpoint: client.endpoint().to_string(),
        reachable: client.reachable().await,
        can_start: model.can_start(),
        process: state.processes.status(&model.name).map(process),
    }
}

fn process(status: Status) -> ModelProcess {
    ModelProcess {
        pid: status.pid,
        started_at: status.started_at,
        exited: status.exited,
        recent_output: status.recent_output,
    }
}
