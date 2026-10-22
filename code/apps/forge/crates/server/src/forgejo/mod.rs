//! The Forgejo connection: an admin gives a Forgejo server, the
//! repositories to watch and a token that can read them. Forge then asks
//! Forgejo about their pull requests about once a minute (Forgejo can't
//! reach a Forge that runs on someone's own machine), links them to the
//! issues they name and moves the issues they work on, as with GitHub.
//! See the "Forgejo connection" spec.

mod check;
pub mod client;

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use forge_domain::Actor;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

pub use check::check;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

/// How often Forge asks Forgejo.
const EVERY: Duration = Duration::from_secs(60);

/// How old the last successful check may be for Forgejo's pull requests to
/// count as up to date.
pub const UP_TO_DATE: chrono::Duration = chrono::Duration::minutes(5);

/// The workspace's "Forgejo" agent, which links pull requests and moves
/// issues, so history says who did it.
pub async fn forgejo_agent(db: &mut PgConnection, workspace_id: Uuid) -> Result<Actor, AppError> {
    crate::agents::forge_agent(db, workspace_id, "forgejo", "Forgejo").await
}

/// Checks every workspace's Forgejo connection about once a minute, and
/// right away when one is connected.
pub async fn worker(state: AppState) {
    loop {
        match sqlx::query_scalar::<_, Uuid>("SELECT workspace_id FROM forgejo_connections")
            .fetch_all(&state.pool)
            .await
        {
            Ok(workspaces) => {
                for workspace_id in workspaces {
                    if let Err(e) = check(&state.pool, workspace_id).await {
                        tracing::error!(%workspace_id, error = ?e, "Forgejo check failed");
                    }
                }
            }
            Err(e) => tracing::error!(error = ?e, "couldn't list Forgejo connections"),
        }
        let _ = tokio::time::timeout(EVERY, state.forgejo_wake.notified()).await;
    }
}

/// What anyone in the workspace may know about its Forgejo connection:
/// never the token.
#[derive(Serialize)]
pub struct ForgejoStatus {
    pub connected: bool,
    pub server: Option<String>,
    pub repositories: Vec<String>,
    pub connected_at: Option<DateTime<Utc>>,
    /// When a check last worked.
    pub checked_at: Option<DateTime<Utc>>,
    /// When the last check failed, if it did, and why.
    pub failed_at: Option<DateTime<Utc>>,
    pub check_error: Option<String>,
    /// Whether the last successful check is recent enough for Forgejo's
    /// pull requests to count as up to date (`UP_TO_DATE`).
    pub up_to_date: bool,
}

type ConnectionRow = (
    String,
    Vec<String>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
);

async fn read_status(state: &AppState, workspace_id: Uuid) -> Result<ForgejoStatus, AppError> {
    let row: Option<ConnectionRow> = sqlx::query_as(
        "SELECT server, repositories, connected_at, checked_at, failed_at, check_error \
         FROM forgejo_connections WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(match row {
        Some((server, repositories, connected_at, checked_at, failed_at, check_error)) => {
            ForgejoStatus {
                connected: true,
                server: Some(server),
                repositories,
                connected_at: Some(connected_at),
                checked_at,
                failed_at,
                check_error,
                // A failure since the last success means Forge can't tell,
                // however recent that success.
                up_to_date: checked_at.is_some_and(|at| {
                    Utc::now() - at < UP_TO_DATE && failed_at.is_none_or(|failed| failed < at)
                }),
            }
        }
        None => ForgejoStatus {
            connected: false,
            server: None,
            repositories: vec![],
            connected_at: None,
            checked_at: None,
            failed_at: None,
            check_error: None,
            up_to_date: false,
        },
    })
}

/// `GET /api/w/{workspace}/forgejo`
pub async fn status(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<ForgejoStatus>, AppError> {
    member.require_not_guest()?;
    Ok(Json(read_status(&state, member.actor.workspace_id).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectRequest {
    pub server: String,
    pub repositories: Vec<String>,
    /// Kept from before when left out, if the server stays the same.
    pub token: Option<String>,
}

/// `PUT /api/w/{workspace}/forgejo`: connects Forgejo, or changes the
/// connection. Saved only once Forgejo answers for every repository with
/// the token; the next check then starts afresh, linking without moving
/// issues for what closed before.
pub async fn connect(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<ConnectRequest>,
) -> Result<Json<ForgejoStatus>, AppError> {
    member.require_admin()?;
    let workspace_id = member.actor.workspace_id;
    let server = client::normalize_server(&request.server).map_err(AppError::invalid)?;
    let mut repositories = Vec::new();
    for repository in &request.repositories {
        let repository = repository.trim().trim_matches('/').to_string();
        if !crate::github::app::valid_full_name(&repository) {
            return Err(AppError::invalid(format!(
                "{repository} isn't a repository name (owner/name)"
            )));
        }
        if !repositories.contains(&repository) {
            repositories.push(repository);
        }
    }
    if repositories.is_empty() {
        return Err(AppError::invalid("name at least one repository to watch"));
    }
    let token = match request.token.map(|t| t.trim().to_string()) {
        Some(token) if !token.is_empty() => token,
        _ => {
            // The old token only ever goes to the server it was given for.
            let kept: Option<String> = sqlx::query_scalar(
                "SELECT token FROM forgejo_connections WHERE workspace_id = $1 AND server = $2",
            )
            .bind(workspace_id)
            .bind(&server)
            .fetch_optional(&state.pool)
            .await?;
            kept.ok_or_else(|| {
                AppError::invalid("give an access token that can read the repositories")
            })?
        }
    };

    let client = client::Client::new(&server, &token);
    for repository in &repositories {
        if let Err(e) = client.repository(repository).await {
            return Err(AppError::invalid(format!(
                "Forgejo refused {repository}: {e}"
            )));
        }
    }
    sqlx::query(
        "INSERT INTO forgejo_connections (workspace_id, server, token, repositories) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (workspace_id) DO UPDATE SET server = $2, token = $3, repositories = $4, \
         connected_at = now(), checked_at = NULL, failed_at = NULL, check_error = NULL",
    )
    .bind(workspace_id)
    .bind(&server)
    .bind(&token)
    .bind(&repositories)
    .execute(&state.pool)
    .await?;
    state.forgejo_wake.notify_one();
    Ok(Json(read_status(&state, workspace_id).await?))
}

/// `DELETE /api/w/{workspace}/forgejo`: Forge stops asking Forgejo; issues
/// keep their links.
pub async fn disconnect(
    State(state): State<AppState>,
    member: Member,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    sqlx::query("DELETE FROM forgejo_connections WHERE workspace_id = $1")
        .bind(member.actor.workspace_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
