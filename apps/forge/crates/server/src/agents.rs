//! Agents: workspace members that act through API tokens instead of signing
//! in, such as a coding agent using the MCP server. Their changes are
//! attributed to them like anyone else's.

use axum::Json;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use chrono::{DateTime, Duration, Utc};
use forge_domain::{
    AGENT_EMAIL_DOMAIN, Actor, AgentInfo, ApiTokenInfo, CreateAgentRequest, CreatedToken, Role,
    SyncAction, UpdateAgentRequest, clean_name,
};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::accounts::add_member;
use crate::auth::{Member, hash_token, new_token, session_token};
use crate::error::AppError;
use crate::members::log_user;
use crate::runners;
use crate::state::AppState;

/// API tokens start with this, so they're easy to recognise (and to scan for if leaked).
const TOKEN_PREFIX: &str = "forge_";

/// Only agents added here are listed, renamed or given keys and runners here.
/// Forge's own agents (Forge AI, GitHub) are agents too (see `forge_agent`),
/// but they're part of Forge.
const ADDED_AGENT_EMAILS: &str = "agent-%";

/// Whether a member can be given work: a person, or an agent added here.
/// Forge's own agents (Forge AI, GitHub) can't.
pub fn takes_work(user: &forge_domain::User) -> bool {
    user.kind == forge_domain::UserKind::Person || user.email.starts_with("agent-")
}

/// One of Forge's own agents in a workspace, such as Forge AI or GitHub,
/// created on first use, with the address `<local>@<workspace>.agents.forge.invalid`.
/// Only an agent with that address is it: people can't sign up with these
/// addresses, and one who somehow has it is never taken for the agent (or
/// added to the workspace as it).
pub async fn forge_agent(
    db: &mut PgConnection,
    workspace_id: Uuid,
    local: &str,
    name: &str,
) -> Result<Actor, AppError> {
    let email = format!("{local}@{workspace_id}.{AGENT_EMAIL_DOMAIN}");
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE email = $1 AND kind = 'agent'")
            .bind(&email)
            .fetch_optional(&mut *db)
            .await?;
    let user_id = match existing {
        Some(id) => id,
        None => {
            let id = Uuid::now_v7();
            let created = sqlx::query(
                "INSERT INTO users (id, email, name, kind) VALUES ($1, $2, $3, 'agent') \
                 ON CONFLICT (email) DO NOTHING",
            )
            .bind(id)
            .bind(&email)
            .bind(name)
            .execute(&mut *db)
            .await?
            .rows_affected();
            if created == 0 {
                return Err(AppError::internal(format!(
                    "a person's account has {email}, the address of the {name} agent"
                )));
            }
            id
        }
    };
    add_member(db, workspace_id, user_id, Role::Member).await?;
    Ok(Actor {
        user_id,
        workspace_id,
    })
}

/// How many of a token's last characters are kept to tell it apart.
const HINT_LENGTH: usize = 4;

/// id, agent, hint, created, last used.
type TokenRow = (
    Uuid,
    Uuid,
    Option<String>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Vec<AgentInfo>>, AppError> {
    let agents: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT u.id, u.name FROM workspace_members m JOIN users u ON u.id = m.user_id \
         WHERE m.workspace_id = $1 AND u.kind = 'agent' AND u.email LIKE $2 \
         ORDER BY m.created_at",
    )
    .bind(member.actor.workspace_id)
    .bind(ADDED_AGENT_EMAILS)
    .fetch_all(&state.pool)
    .await?;
    let tokens: Vec<TokenRow> = sqlx::query_as(
        "SELECT id, user_id, hint, created_at, last_used_at FROM api_tokens \
         WHERE workspace_id = $1 AND revoked_at IS NULL ORDER BY created_at",
    )
    .bind(member.actor.workspace_id)
    .fetch_all(&state.pool)
    .await?;

    let mut runners = runners::of_workspace(&state.pool, member.actor.workspace_id).await?;

    Ok(Json(
        agents
            .into_iter()
            .map(|(id, name)| AgentInfo {
                id,
                name,
                runner: runners
                    .iter()
                    .position(|(agent, _)| *agent == id)
                    .map(|at| runners.swap_remove(at).1),
                tokens: tokens
                    .iter()
                    .filter(|t| t.1 == id)
                    .map(|t| ApiTokenInfo {
                        id: t.0,
                        hint: t.2.clone(),
                        created_at: t.3,
                        last_used_at: t.4,
                    })
                    .collect(),
            })
            .collect(),
    ))
}

/// Creates an agent and its first token. Admins only, like everything about
/// agents' keys and runners: a key acts in the workspace as a member.
pub async fn create(
    State(state): State<AppState>,
    member: Member,
    Json(request): Json<CreateAgentRequest>,
) -> Result<Json<CreatedToken>, AppError> {
    member.require_admin()?;
    let name = clean_name(&request.name, "agent name")?;
    let agent = Uuid::now_v7();
    let mut db = state.pool.begin().await?;
    ensure_name_free(&mut db, member.actor.workspace_id, &name, None).await?;
    // Agents can't sign in: no password, and an address that can't receive mail.
    sqlx::query("INSERT INTO users (id, email, name, kind) VALUES ($1, $2, $3, 'agent')")
        .bind(agent)
        .bind(format!("agent-{agent}@{AGENT_EMAIL_DOMAIN}"))
        .bind(&name)
        .execute(&mut *db)
        .await?;
    add_member(&mut db, member.actor.workspace_id, agent, Role::Member).await?;
    let token = issue_token(&mut db, member.actor, agent).await?;
    db.commit().await?;
    Ok(Json(CreatedToken {
        agent_id: agent,
        token,
    }))
}

/// Another token for an existing agent. Admins only.
pub async fn create_token(
    State(state): State<AppState>,
    member: Member,
    Path((_, agent)): Path<(Uuid, Uuid)>,
) -> Result<Json<CreatedToken>, AppError> {
    member.require_admin()?;
    let mut db = state.pool.begin().await?;
    ensure_agent(&mut db, member.actor.workspace_id, agent).await?;
    let token = issue_token(&mut db, member.actor, agent).await?;
    db.commit().await?;
    Ok(Json(CreatedToken {
        agent_id: agent,
        token,
    }))
}

/// Renames an agent. Admins only, like removing one (which is removing the
/// member).
pub async fn rename(
    State(state): State<AppState>,
    member: Member,
    Path((_, agent)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateAgentRequest>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let name = clean_name(&request.name, "agent name")?;
    let mut db = state.pool.begin().await?;
    ensure_agent(&mut db, member.actor.workspace_id, agent).await?;
    ensure_name_free(&mut db, member.actor.workspace_id, &name, Some(agent)).await?;
    sqlx::query("UPDATE users SET name = $2 WHERE id = $1")
        .bind(agent)
        .bind(&name)
        .execute(&mut *db)
        .await?;
    log_user(&mut db, member.actor, agent, SyncAction::Update).await?;
    db.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn ensure_agent(
    db: &mut PgConnection,
    workspace_id: Uuid,
    agent: Uuid,
) -> Result<(), AppError> {
    let is_agent: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_members m JOIN users u ON u.id = m.user_id \
         WHERE m.workspace_id = $1 AND m.user_id = $2 AND u.kind = 'agent' AND u.email LIKE $3)",
    )
    .bind(workspace_id)
    .bind(agent)
    .bind(ADDED_AGENT_EMAILS)
    .fetch_one(&mut *db)
    .await?;
    if !is_agent {
        return Err(AppError::not_found("agent not found"));
    }
    Ok(())
}

/// Agents are told apart by name (in history, on the Agents page), so two in
/// a workspace can't share one, whatever the case. Locks the workspace's
/// membership so two at once can't both take a name.
async fn ensure_name_free(
    db: &mut PgConnection,
    workspace_id: Uuid,
    name: &str,
    except: Option<Uuid>,
) -> Result<(), AppError> {
    sqlx::query("SELECT 1 FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace_id)
        .execute(&mut *db)
        .await?;
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_members m JOIN users u ON u.id = m.user_id \
         WHERE m.workspace_id = $1 AND u.kind = 'agent' AND lower(u.name) = lower($2) \
         AND u.id IS DISTINCT FROM $3)",
    )
    .bind(workspace_id)
    .bind(name)
    .bind(except)
    .fetch_one(&mut *db)
    .await?;
    if taken {
        return Err(AppError::conflict(format!(
            "there's already an agent called \"{name}\"; give this one a name you'll recognise"
        )));
    }
    Ok(())
}

/// Revokes a token. Admins only.
pub async fn revoke_token(
    State(state): State<AppState>,
    member: Member,
    Path((_, token)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    member.require_admin()?;
    let revoked = sqlx::query(
        "UPDATE api_tokens SET revoked_at = now() \
         WHERE id = $1 AND workspace_id = $2 AND revoked_at IS NULL",
    )
    .bind(token)
    .bind(member.actor.workspace_id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if revoked == 0 {
        return Err(AppError::not_found("token not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn issue_token(
    db: &mut PgConnection,
    creator: Actor,
    agent: Uuid,
) -> Result<String, AppError> {
    let token = format!("{TOKEN_PREFIX}{}", new_token());
    let hint = &token[token.len() - HINT_LENGTH..];
    sqlx::query(
        "INSERT INTO api_tokens (id, workspace_id, user_id, token_hash, hint, created_by) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(creator.workspace_id)
    .bind(agent)
    .bind(hash_token(&token))
    .bind(hint)
    .bind(creator.user_id)
    .execute(&mut *db)
    .await?;
    Ok(token)
}

/// Middleware for agent endpoints: turns `Authorization: Bearer forge_...`
/// into the [`Actor`] it stands for, or refuses the request.
pub async fn require_token(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let actor = token_actor(&state, request.headers()).await?;
    request.extensions_mut().insert(actor);
    Ok(next.run(request).await)
}

async fn token_actor(state: &AppState, headers: &axum::http::HeaderMap) -> Result<Actor, AppError> {
    let unauthorized =
        || AppError::unauthorized("send an agent token as `Authorization: Bearer forge_...`");
    let token = session_token(headers)
        .filter(|t| t.starts_with(TOKEN_PREFIX))
        .ok_or_else(unauthorized)?;
    let found: Option<(Uuid, Uuid, Uuid, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT t.id, t.workspace_id, t.user_id, t.last_used_at FROM api_tokens t \
         JOIN workspace_members m ON m.workspace_id = t.workspace_id AND m.user_id = t.user_id \
         WHERE t.token_hash = $1 AND t.revoked_at IS NULL",
    )
    .bind(hash_token(&token))
    .fetch_optional(&state.pool)
    .await?;
    let (id, workspace_id, user_id, last_used) = found.ok_or_else(unauthorized)?;

    // Note use, at most once a minute.
    if last_used.is_none_or(|at| Utc::now() - at > Duration::minutes(1)) {
        sqlx::query("UPDATE api_tokens SET last_used_at = now() WHERE id = $1")
            .bind(id)
            .execute(&state.pool)
            .await?;
    }
    let actor = Actor {
        user_id,
        workspace_id,
    };
    // Any call shows the agent is still at its work (FOR-24). Not worth
    // failing the call over.
    if let Err(error) = forge_sync::renew_work(&state.pool, actor).await {
        tracing::warn!(%error, "couldn't record that the agent is still working");
    }
    Ok(actor)
}
