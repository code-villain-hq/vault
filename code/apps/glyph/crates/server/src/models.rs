//! The translation models in the server's models file, starting and
//! stopping the ones it may run itself, and, on the desktop app's own
//! server, adding and removing them.

use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use glyph_domain::{ModelCreate, ModelInfo, ModelProcess, Validate};
use glyph_llm::{Client, Model, Models, ProcessError, Status};

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::state::AppState;

/// How long a model server gets to stop before it's killed.
pub const GRACE: Duration = Duration::from_secs(5);

/// The ports models added here are started on: the first free one.
const MODEL_PORTS: std::ops::RangeInclusive<u16> = 7481..=7499;

/// What a model added here is started with, besides its file and port.
const CONTEXT: [&str; 2] = ["-c", "4096"];

/// `GET /api/models`: every model, and whether it answers.
pub async fn list(State(state): State<AppState>, _user: CurrentUser) -> Json<Vec<ModelInfo>> {
    let models = state.models();
    let infos = models.models.iter().map(|m| info(&state, m));
    Json(futures::future::join_all(infos).await)
}

/// `POST /api/models/{name}/start`.
pub async fn start(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(name): Path<String>,
) -> Result<Json<ModelInfo>, AppError> {
    require_not_only_guest(&state, &user).await?;
    let models = state.models();
    let model = find(&models, &name)?;
    let args = model.start_args().map_err(AppError::invalid)?;
    let binary = match models.llm_binary.trim() {
        "" => "llama-server",
        binary => binary,
    };
    state
        .processes
        .start(&model.name, binary, &args)
        .map_err(|e| match e {
            ProcessError::AlreadyRunning => AppError::conflict(e.to_string()),
            ProcessError::Spawn { .. } if binary == "llama-server" => AppError::invalid(format!(
                "Couldn't start llama-server. Install it with `brew install llama.cpp`. ({e})"
            )),
            ProcessError::Spawn { .. } => {
                AppError::invalid(format!("Couldn't start {binary}. Is it installed? ({e})"))
            }
            ProcessError::NotRunning => AppError::internal(e),
        })?;
    tracing::info!(model = model.name, binary, "started a model server");
    Ok(Json(info(&state, model).await))
}

/// `POST /api/models/{name}/stop`: stops a server this one started.
pub async fn stop(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(name): Path<String>,
) -> Result<Json<ModelInfo>, AppError> {
    require_not_only_guest(&state, &user).await?;
    let models = state.models();
    let model = find(&models, &name)?;
    state
        .processes
        .stop(&model.name, GRACE)
        .await
        .map_err(|_| {
            AppError::invalid("It wasn't started from here, so it can't be stopped from here.")
        })?;
    Ok(Json(info(&state, model).await))
}

/// `POST /api/models`: adds a model, on the desktop app's own server.
pub async fn add(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(new): Json<ModelCreate>,
) -> Result<(StatusCode, Json<ModelInfo>), AppError> {
    let file = models_file(&state)?;
    new.validate()?;
    let name = new.name.trim().to_string();
    let model = {
        let mut models = state.models.write().unwrap();
        if models.find(&name).is_some() {
            return Err(AppError::conflict(format!(
                "There's a model called {name} already."
            )));
        }
        let starts_here = new.endpoint.trim().is_empty();
        let endpoint = match starts_here {
            true => format!("http://127.0.0.1:{}", free_port(&models)?),
            false => new.endpoint.trim().trim_end_matches('/').to_string(),
        };
        let model = Model {
            name,
            endpoint,
            model_path: new.model_path.trim().into(),
            hf_repo: new.hf_repo.trim().into(),
            hf_file: new.hf_file.trim().into(),
            extra_args: match starts_here {
                true => CONTEXT.map(String::from).to_vec(),
                false => Vec::new(),
            },
            ..Model::default()
        };
        let mut next = models.clone();
        next.models.push(model.clone());
        next.check().map_err(|e| AppError::invalid(e.to_string()))?;
        next.save(&file).map_err(AppError::internal)?;
        *models = next;
        model
    };
    tracing::info!(model = model.name, "added a model");
    Ok((StatusCode::CREATED, Json(info(&state, &model).await)))
}

/// `DELETE /api/models/{name}`: stops it if it runs from here, and removes
/// it, on the desktop app's own server.
pub async fn remove(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(name): Path<String>,
) -> Result<StatusCode, AppError> {
    let file = models_file(&state)?;
    find(&state.models(), &name)?;
    // Not running from here is fine: there's nothing to stop.
    let _ = state.processes.stop(&name, GRACE).await;
    {
        let mut models = state.models.write().unwrap();
        let mut next = models.clone();
        next.models.retain(|m| m.name != name);
        next.save(&file).map_err(AppError::internal)?;
        *models = next;
    }
    tracing::info!(model = name, "removed a model");
    Ok(StatusCode::NO_CONTENT)
}

/// The server's models are shared by its workspaces, so starting and
/// stopping them is for the people who work in one, not for someone who is
/// only ever a guest.
async fn require_not_only_guest(state: &AppState, user: &CurrentUser) -> Result<(), AppError> {
    let works_here: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_members WHERE user_id = $1 AND role <> 'guest')",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;
    if works_here {
        Ok(())
    } else {
        Err(AppError::forbidden("guests can't do that"))
    }
}

/// The file models are added to, if this server lets Settings change them.
fn models_file(state: &AppState) -> Result<PathBuf, AppError> {
    state
        .models_file
        .clone()
        .ok_or_else(|| AppError::forbidden("This server's models are set in its models file."))
}

/// The first port in [`MODEL_PORTS`] no model uses and nothing listens on.
fn free_port(models: &Models) -> Result<u16, AppError> {
    MODEL_PORTS
        .clone()
        .find(|port| {
            !models.models.iter().any(|m| m.port() == Ok(*port))
                && TcpListener::bind((Ipv4Addr::LOCALHOST, *port)).is_ok()
        })
        .ok_or_else(|| AppError::conflict("Every port for models is taken. Remove one first."))
}

fn find<'a>(models: &'a Models, name: &str) -> Result<&'a Model, AppError> {
    models
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
