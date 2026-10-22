//! Signing out, the signed-in user, and making workspaces and their members.

use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use forge_domain::{Actor, SyncAction, team_key_from};
use forge_domain::{
    Me, Role, UpdateProfileRequest, UpdateWorkspaceRequest, UserInfo, WorkspaceInfo, clean_name,
    url_key_from,
};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::{
    CLIENT_HEADER, CurrentUser, SESSION_HEADER, clear_session_cookie, hash_token, session_cookie,
    session_token,
};
use crate::error::AppError;
use crate::state::AppState;
use crate::{members, teams};

/// Ends the current session, if there is one, and clears the cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(hash_token(&token))
            .execute(&state.pool)
            .await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(SET_COOKIE, clear_session_cookie(state.cookie_secure))],
    )
        .into_response())
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> Result<Json<Me>, AppError> {
    Ok(Json(load_me(&state.pool, user.id).await?))
}

/// `PATCH /api/w/{workspace}`: renames the workspace (admins only).
pub async fn rename_workspace(
    State(state): State<AppState>,
    member: crate::auth::Member,
    Json(request): Json<UpdateWorkspaceRequest>,
) -> Result<Json<Me>, AppError> {
    member.require_admin()?;
    let name = clean_name(&request.name, "workspace name")?;
    sqlx::query("UPDATE workspaces SET name = $2 WHERE id = $1")
        .bind(member.actor.workspace_id)
        .bind(&name)
        .execute(&state.pool)
        .await?;
    Ok(Json(load_me(&state.pool, member.actor.user_id).await?))
}

/// `PUT /api/me`: changes your name, everywhere you're a member.
pub async fn update_profile(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<Me>, AppError> {
    let name = clean_name(&request.name, "name")?;
    let mut db = state.pool.begin().await?;
    sqlx::query("UPDATE users SET name = $2 WHERE id = $1")
        .bind(user.id)
        .bind(&name)
        .execute(&mut *db)
        .await?;
    let workspaces: Vec<Uuid> =
        // In a fixed order, so two renames at once can't lock workspaces in
        // opposite orders.
        sqlx::query_scalar(
            "SELECT workspace_id FROM workspace_members WHERE user_id = $1 ORDER BY workspace_id",
        )
            .bind(user.id)
            .fetch_all(&mut *db)
            .await?;
    for workspace_id in workspaces {
        let actor = Actor {
            user_id: user.id,
            workspace_id,
        };
        members::log_user(&mut db, actor, user.id, SyncAction::Update).await?;
    }
    db.commit().await?;
    Ok(Json(load_me(&state.pool, user.id).await?))
}

/// The response to signing in: who you are, and the session as a cookie,
/// or for the desktop app as a header it keeps in the keychain.
pub async fn signed_in(
    state: &AppState,
    headers: &HeaderMap,
    token: &str,
    user_id: Uuid,
) -> Result<Response, AppError> {
    let me = load_me(&state.pool, user_id).await?;
    if headers.get(CLIENT_HEADER).is_some_and(|v| v == "desktop") {
        return Ok(([(SESSION_HEADER, token)], Json(me)).into_response());
    }
    Ok((
        [(SET_COOKIE, session_cookie(token, state.cookie_secure))],
        Json(me),
    )
        .into_response())
}

pub async fn load_me(pool: &PgPool, user_id: Uuid) -> Result<Me, AppError> {
    let (id, email, name): (Uuid, String, String) =
        sqlx::query_as("SELECT id, email, name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    let rows: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT w.id, w.name, w.url_key, m.role FROM workspace_members m \
         JOIN workspaces w ON w.id = m.workspace_id WHERE m.user_id = $1 ORDER BY m.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let workspaces = rows
        .into_iter()
        .map(|(id, name, url_key, role)| {
            let role = Role::parse(&role)
                .ok_or_else(|| AppError::internal(format!("unknown role {role:?}")))?;
            Ok(WorkspaceInfo {
                id,
                name,
                url_key,
                role,
            })
        })
        .collect::<Result<_, AppError>>()?;
    Ok(Me {
        user: UserInfo { id, email, name },
        workspaces,
    })
}

/// Creates a workspace with `admin` as its first member and a first team
/// named after it. Its URL key comes from the name, with a number added if the
/// key is taken.
pub async fn create_workspace(
    db: &mut PgConnection,
    name: &str,
    admin: Uuid,
) -> Result<Uuid, AppError> {
    let base = url_key_from(name);
    for attempt in 1..=100 {
        let url_key = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let id = Uuid::now_v7();
        let created =
            sqlx::query("INSERT INTO workspaces (id, name, url_key) VALUES ($1, $2, $3) ON CONFLICT (url_key) DO NOTHING")
                .bind(id)
                .bind(name)
                .bind(&url_key)
                .execute(&mut *db)
                .await?
                .rows_affected();
        if created == 1 {
            add_member(&mut *db, id, admin, Role::Admin).await?;
            let actor = Actor {
                user_id: admin,
                workspace_id: id,
            };
            teams::create_team(&mut *db, actor, name, &team_key_from(name)).await?;
            return Ok(id);
        }
    }
    Err(AppError::conflict(
        "couldn't find a free URL for that workspace name",
    ))
}

/// Adds someone to a workspace, an admin or member in every team. Someone
/// already in it keeps their role.
pub async fn add_member(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    role: Role,
) -> Result<(), AppError> {
    let added = sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(role.as_str())
    .execute(&mut *db)
    .await?
    .rows_affected();
    if added == 1 {
        let actor = Actor {
            user_id,
            workspace_id,
        };
        members::log_user(db, actor, user_id, SyncAction::Insert).await?;
        // A guest joins no team until an admin adds them to one.
        if role != Role::Guest {
            forge_sync::join_every_team(db, actor, user_id).await?;
        }
    }
    Ok(())
}
