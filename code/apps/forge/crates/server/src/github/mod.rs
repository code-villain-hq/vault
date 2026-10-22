//! The GitHub connection: a workspace installs Forge's GitHub App on its
//! organisation; Forge keeps a clone of each repository for drift checks,
//! links pull requests and commits to issues, and moves issues along as
//! pull requests open and merge.
//!
//! Configured with `FORGE_GITHUB_*` (see `app.rs`); off without them.

pub mod app;
pub mod login;
pub mod webhook;

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::Redirect;
use forge_domain::{Actor, Operation, Role, Transaction};
use forge_sync::{Origin, apply_in};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::Sha256;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub use app::GitHubApp;

use crate::auth::{CurrentUser, Member};
use crate::error::AppError;
use crate::state::AppState;

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

async fn write(
    db: &mut PgConnection,
    actor: Actor,
    operations: Vec<Operation>,
) -> Result<(), AppError> {
    let transaction = Transaction {
        id: Uuid::now_v7(),
        operations,
    };
    apply_in(db, actor, &transaction, Origin::Server).await?;
    Ok(())
}

/// The workspace's "GitHub" agent, which links pull requests and moves
/// issues, so history says who did it.
pub async fn github_agent(db: &mut PgConnection, workspace_id: Uuid) -> Result<Actor, AppError> {
    crate::agents::forge_agent(db, workspace_id, "github", "GitHub").await
}

/// The `state` passed through GitHub's install flow: the workspace, signed,
/// so the callback can trust which workspace to link.
fn install_state(app: &GitHubApp, workspace_id: Uuid) -> String {
    format!("{workspace_id}.{}", sign(app, &workspace_id.to_string()))
}

fn sign(app: &GitHubApp, value: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(app.webhook_secret.as_bytes())
        .expect("HMAC takes any key length");
    mac.update(b"install:");
    mac.update(value.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn verify_state(app: &GitHubApp, state: &str) -> Option<Uuid> {
    let (workspace, signature) = state.split_once('.')?;
    let workspace: Uuid = workspace.parse().ok()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(app.webhook_secret.as_bytes()).ok()?;
    mac.update(b"install:");
    mac.update(workspace.to_string().as_bytes());
    mac.verify_slice(&hex::decode(signature).ok()?).ok()?;
    Some(workspace)
}

#[derive(Serialize)]
pub struct GitHubStatus {
    /// Whether this server has a GitHub App set up.
    pub configured: bool,
    /// Where an admin installs the app for this workspace.
    pub install_url: Option<String>,
    /// Whether connecting an installation checks that the person
    /// connecting it can access it on GitHub (GitHub sign-in is set up).
    pub confirms_ownership: bool,
    pub installations: Vec<InstallationInfo>,
}

#[derive(Serialize)]
pub struct InstallationInfo {
    pub installation_id: i64,
    pub account: String,
    pub repositories: Vec<RepositoryInfo>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RepositoryInfo {
    pub full_name: String,
    pub default_branch: String,
    pub mirrored_at: Option<chrono::DateTime<chrono::Utc>>,
    pub mirror_error: Option<String>,
}

/// `GET /api/w/{workspace}/github`
pub async fn status(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<GitHubStatus>, AppError> {
    member.require_not_guest()?;
    let Some(app) = state.github.as_ref() else {
        return Ok(Json(GitHubStatus {
            configured: false,
            install_url: None,
            confirms_ownership: false,
            installations: vec![],
        }));
    };
    let installations: Vec<(i64, String)> = sqlx::query_as(
        "SELECT installation_id, account FROM github_installations WHERE workspace_id = $1 ORDER BY created_at",
    )
    .bind(member.actor.workspace_id)
    .fetch_all(&state.pool)
    .await?;
    let mut out = Vec::new();
    for (installation_id, account) in installations {
        let repositories: Vec<RepositoryInfo> = sqlx::query_as(
            "SELECT full_name, default_branch, mirrored_at, mirror_error FROM github_repositories \
             WHERE installation_id = $1 ORDER BY full_name",
        )
        .bind(installation_id)
        .fetch_all(&state.pool)
        .await?;
        out.push(InstallationInfo {
            installation_id,
            account,
            repositories,
        });
    }
    let install_url = (member.role == Role::Admin).then(|| {
        format!(
            "https://github.com/apps/{}/installations/new?state={}",
            app.slug,
            install_state(app, member.actor.workspace_id)
        )
    });
    Ok(Json(GitHubStatus {
        configured: true,
        install_url,
        confirms_ownership: state.github_login.is_some(),
        installations: out,
    }))
}

#[derive(Deserialize)]
pub struct SetupParams {
    installation_id: i64,
    state: Option<String>,
    /// From GitHub's user authorization during installation: says who
    /// installed the app.
    code: Option<String>,
}

/// `GET /api/github/setup`: where GitHub sends the admin after installing
/// the app. Links the installation to the workspace named in `state`.
///
/// With GitHub sign-in set up (its client id and secret), the installing
/// person must also be able to access the installation on GitHub, going by
/// the OAuth `code` GitHub sends along when the App has "Request user
/// authorization (OAuth) during installation" on; otherwise anyone who
/// knows an installation's id could link it. Without it, being an admin
/// of the workspace is enough, which suits a server only its owner uses.
pub async fn setup(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<SetupParams>,
) -> Result<Redirect, AppError> {
    let app = state
        .github
        .clone()
        .ok_or_else(|| AppError::not_found("GitHub isn't connected on this server"))?;
    let workspace_id = params
        .state
        .as_deref()
        .and_then(|s| verify_state(&app, s))
        .ok_or_else(|| AppError::invalid("start connecting GitHub from Forge's settings"))?;
    let (url_key, role): (String, String) = sqlx::query_as(
        "SELECT w.url_key, m.role FROM workspaces w JOIN workspace_members m ON m.workspace_id = w.id \
         WHERE w.id = $1 AND m.user_id = $2",
    )
    .bind(workspace_id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("workspace not found"))?;
    if role != "admin" {
        return Err(AppError::forbidden(
            "only workspace admins can connect GitHub",
        ));
    }
    if let Some(login) = state.github_login.as_ref() {
        let code = params.code.as_deref().ok_or_else(|| {
            AppError::invalid(
                "GitHub didn't say who installed the app, so it wasn't connected. Turn on \
                 \"Request user authorization (OAuth) during installation\" in the GitHub App's \
                 settings, then connect GitHub again from Forge's settings",
            )
        })?;
        match login
            .can_access_installation(code, params.installation_id)
            .await
        {
            Some(true) => {}
            Some(false) => {
                return Err(AppError::forbidden(
                    "your GitHub account can't access that installation, so it wasn't connected",
                ));
            }
            None => {
                return Err(AppError::invalid(
                    "GitHub didn't confirm who you are, so the installation wasn't connected; \
                     connect GitHub again from Forge's settings",
                ));
            }
        }
    }

    // Asking GitHub about the installation proves it's one of this app's.
    let account = app
        .installation_account(params.installation_id)
        .await
        .map_err(AppError::invalid)?;
    let linked: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM github_installations WHERE installation_id = $1",
    )
    .bind(params.installation_id)
    .fetch_optional(&state.pool)
    .await?;
    if linked.is_some_and(|w| w != workspace_id) {
        return Err(AppError::conflict(
            "that installation belongs to another workspace",
        ));
    }
    sqlx::query(
        "INSERT INTO github_installations (installation_id, workspace_id, account, created_by) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (installation_id) DO UPDATE SET account = EXCLUDED.account",
    )
    .bind(params.installation_id)
    .bind(workspace_id)
    .bind(&account)
    .bind(user.id)
    .execute(&state.pool)
    .await?;

    let repositories = app
        .installation_repositories(params.installation_id)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "couldn't list the installation's repositories");
            vec![]
        });
    for (name, branch) in repositories {
        remember_repository(
            &state.pool,
            params.installation_id,
            &name,
            branch.as_deref(),
        )
        .await?;
        refresh_clone(state.clone(), params.installation_id, name, branch);
    }
    Ok(Redirect::to(&format!("/{url_key}/settings/github")))
}

/// `POST /api/w/{workspace}/github/refresh`: brings every clone up to date,
/// asking GitHub for each repository's default branch again.
pub async fn refresh(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Value>, AppError> {
    member.require_admin()?;
    let repositories: Vec<(i64, String)> = sqlx::query_as(
        "SELECT r.installation_id, r.full_name FROM github_repositories r \
         JOIN github_installations i ON i.installation_id = r.installation_id WHERE i.workspace_id = $1",
    )
    .bind(member.actor.workspace_id)
    .fetch_all(&state.pool)
    .await?;
    let count = repositories.len();
    for (installation_id, name) in repositories {
        refresh_clone(state.clone(), installation_id, name, None);
    }
    Ok(Json(serde_json::json!({ "refreshing": count })))
}

/// Records a repository of an installation, with its default branch if
/// it's known. A known branch is never replaced by a guess: without one,
/// a new repository starts out as `main` until GitHub says otherwise.
/// Whether the repository is new.
pub async fn remember_repository(
    pool: &PgPool,
    installation_id: i64,
    full_name: &str,
    branch: Option<&str>,
) -> Result<bool, AppError> {
    let new: bool = sqlx::query_scalar(
        "INSERT INTO github_repositories (installation_id, full_name, default_branch) VALUES ($1, $2, COALESCE($3, 'main')) \
         ON CONFLICT (installation_id, full_name) DO UPDATE SET default_branch = COALESCE($3, github_repositories.default_branch) \
         RETURNING xmax = 0",
    )
    .bind(installation_id)
    .bind(full_name)
    .bind(branch)
    .fetch_one(pool)
    .await?;
    Ok(new)
}

/// Clones or updates a repository in the background, recording the
/// outcome. Without its default branch, asks GitHub for it first.
pub fn refresh_clone(
    state: AppState,
    installation_id: i64,
    full_name: String,
    branch: Option<String>,
) {
    refresh_then(state, installation_id, full_name, branch, None);
}

/// Like [`refresh_clone`], then checks the services a push touched, once
/// the clone has the new code.
pub fn refresh_then_check(
    state: AppState,
    installation_id: i64,
    full_name: String,
    branch: String,
    workspace_id: Uuid,
    changed: BTreeSet<String>,
) {
    refresh_then(
        state,
        installation_id,
        full_name,
        Some(branch),
        Some((workspace_id, changed)),
    );
}

fn refresh_then(
    state: AppState,
    installation_id: i64,
    full_name: String,
    branch: Option<String>,
    then_check: Option<(Uuid, BTreeSet<String>)>,
) {
    let Some(app) = state.github.clone() else {
        return;
    };
    app.clone().spawn(async move {
        let branch = match branch {
            Some(branch) => Ok(branch),
            None => app.default_branch(installation_id, &full_name).await,
        };
        let result = match &branch {
            Ok(branch) => app.sync_clone(installation_id, &full_name, branch).await,
            Err(e) => Err(e.clone()),
        };
        let updated = result.is_ok();
        if let Err(e) = &result {
            tracing::warn!(repository = %full_name, error = %e, "couldn't update the clone");
        }
        // The branch is GitHub's own answer, so it replaces what was stored.
        let _ = sqlx::query(
            "UPDATE github_repositories SET default_branch = COALESCE($5, default_branch), \
             mirrored_at = CASE WHEN $3 THEN now() ELSE mirrored_at END, mirror_error = $4 \
             WHERE installation_id = $1 AND full_name = $2",
        )
        .bind(installation_id)
        .bind(&full_name)
        .bind(result.is_ok())
        .bind(result.err())
        .bind(branch.ok())
        .execute(&state.pool)
        .await;
        if updated
            && let Some((workspace_id, changed)) = then_check
            && let Err(e) = check_after_push(&state, workspace_id, &full_name, &changed).await
        {
            tracing::warn!(error = ?e, "couldn't queue checks after a push");
        }
    });
}

/// After a push to the default branch, checks the services whose paths it touched.
pub async fn check_after_push(
    state: &AppState,
    workspace_id: Uuid,
    repository: &str,
    changed: &BTreeSet<String>,
) -> Result<(), AppError> {
    let Some(provider) = state.ai.as_ref() else {
        return Ok(());
    };
    let services: Vec<(Uuid, Vec<String>)> = sqlx::query_as(
        "SELECT id, paths FROM services WHERE workspace_id = $1 AND repository = $2",
    )
    .bind(workspace_id)
    .bind(format!("github:{repository}"))
    .fetch_all(&state.pool)
    .await?;
    for (service, paths) in services {
        if changed
            .iter()
            .any(|path| forge_ai::matches_any(&paths, path))
        {
            crate::drift::queue_after_push(&state.pool, provider.model(), workspace_id, service)
                .await?;
            state.drift_wake.notify_one();
        }
    }
    Ok(())
}
