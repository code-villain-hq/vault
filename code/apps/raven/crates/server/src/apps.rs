//! The apps a workspace watches. Admins add, change and remove them through
//! sync (`App`); this is what sync can't do: Raven itself, which only the
//! server adds, and the keys apps send their calls with, which never sync.

use axum::Json;
use axum::extract::{Path, State};
use raven_domain::{Actor, AppKey, ModelKind, SyncAction};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::{Member, hash_token, new_token};
use crate::error::AppError;
use crate::state::AppState;
use crate::telemetry::timed;

/// Raven's own service name.
pub const RAVEN: &str = "raven";

/// Adds Raven itself to the workspace, unless another workspace watches it
/// already: the first workspace on a server, or on this Mac, gets it.
pub async fn adopt_self(db: &mut PgConnection, actor: Actor) -> Result<(), AppError> {
    const SQL: &str = "INSERT INTO apps (id, workspace_id, key, name, position, is_self) \
        VALUES ($1, $2, $3, 'Raven', 0, true) \
        ON CONFLICT (is_self) WHERE is_self DO NOTHING RETURNING to_jsonb(apps.*)";
    let id = Uuid::now_v7();
    let record: Option<Value> = sqlx::query_scalar(SQL)
        .bind(id)
        .bind(actor.workspace_id)
        .bind(RAVEN)
        .fetch_optional(&mut *db)
        .await?;
    if let Some(record) = record {
        raven_sync::record_change(
            db,
            actor,
            ModelKind::App,
            id,
            SyncAction::Insert,
            Some(record),
        )
        .await?;
    }
    Ok(())
}

/// A watched app, as the pages need it.
pub struct Watched {
    pub id: Uuid,
    pub name: String,
    pub health_url: Option<String>,
    pub is_self: bool,
    /// Whether it has a key to send its calls with.
    pub sending: bool,
}

/// The workspace's app with that service name, or 404.
pub async fn find(pool: &PgPool, workspace_id: Uuid, key: &str) -> Result<Watched, AppError> {
    const SQL: &str = "SELECT a.id, a.name, a.health_url, a.is_self, k.app_id IS NOT NULL \
        FROM apps a LEFT JOIN app_keys k ON k.app_id = a.id \
        WHERE a.workspace_id = $1 AND a.key = $2";
    let row: Option<(Uuid, String, Option<String>, bool, bool)> = timed(
        "find app",
        SQL,
        sqlx::query_as(SQL)
            .bind(workspace_id)
            .bind(key)
            .fetch_optional(pool),
    )
    .await?;
    let (id, name, health_url, is_self, sending) =
        row.ok_or_else(|| AppError::not_found("this workspace doesn't watch that app"))?;
    Ok(Watched {
        id,
        name,
        health_url,
        is_self,
        sending,
    })
}

/// `POST /api/w/{workspace}/apps/{app}/key`: a new key for the app to send
/// its calls with, shown only now. It replaces the one before. Admins only.
pub async fn new_key(
    State(state): State<AppState>,
    member: Member,
    Path((_, app)): Path<(Uuid, String)>,
) -> Result<Json<AppKey>, AppError> {
    member.require_admin()?;
    let app = find(&state.pool, member.actor.workspace_id, &app).await?;
    if app.is_self {
        return Err(AppError::invalid(
            "Raven records its own calls: it needs no key",
        ));
    }
    let key = new_token();
    const SQL: &str = "INSERT INTO app_keys (app_id, key_hash) VALUES ($1, $2) \
        ON CONFLICT (app_id) DO UPDATE SET key_hash = EXCLUDED.key_hash, created_at = now()";
    timed(
        "new key",
        SQL,
        sqlx::query(SQL)
            .bind(app.id)
            .bind(hash_token(&key))
            .execute(&state.pool),
    )
    .await?;
    Ok(Json(AppKey { key }))
}
