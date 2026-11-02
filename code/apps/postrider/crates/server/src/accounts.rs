//! Signing out, the signed-in user, and making workspaces and their members.

use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use postrider_domain::{
    Actor, Me, ModelKind, Role, SyncAction, UserInfo, WorkspaceInfo, url_key_from,
};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::{CurrentUser, clear_session_cookie, hash_token, session_token};
use crate::error::AppError;
use crate::state::AppState;

/// Ends the current session, if there is one, and clears the cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(token) = session_token(&headers) {
        code_villain_auth::end_session(&state.pool, &hash_token(&token)).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(SET_COOKIE, clear_session_cookie(false))],
    )
        .into_response())
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> Result<Json<Me>, AppError> {
    Ok(Json(load_me(&state.pool, user.id).await?))
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

/// Creates a workspace with `admin` as its first member. Its URL key comes
/// from the name, with a number added if the key is taken.
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
        let created = sqlx::query(
            "INSERT INTO workspaces (id, name, url_key) VALUES ($1, $2, $3) \
             ON CONFLICT (url_key) DO NOTHING",
        )
        .bind(id)
        .bind(name)
        .bind(&url_key)
        .execute(&mut *db)
        .await?
        .rows_affected();
        if created == 1 {
            add_member(&mut *db, id, admin, Role::Admin).await?;
            return Ok(id);
        }
    }
    Err(AppError::conflict(
        "couldn't find a free URL for that workspace name",
    ))
}

/// Adds someone to a workspace. Someone already in it keeps their role.
pub async fn add_member(
    db: &mut PgConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    role: Role,
) -> Result<(), AppError> {
    let added = sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3) \
         ON CONFLICT DO NOTHING",
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
        let data: Value = sqlx::query_scalar(
            "SELECT to_jsonb(u.*) FROM workspace_users u WHERE u.id = $1 AND u.workspace_id = $2",
        )
        .bind(user_id)
        .bind(workspace_id)
        .fetch_one(&mut *db)
        .await?;
        postrider_sync::record_change(
            db,
            actor,
            ModelKind::User,
            user_id,
            SyncAction::Insert,
            Some(data),
        )
        .await?;
    }
    Ok(())
}
