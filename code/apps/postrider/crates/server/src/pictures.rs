//! Pictures from the web, which a mail's HTML would load as it opens and
//! so tell its sender it was read, and when. They wait for "Load pictures"
//! on the mail, unless you said its sender's always load
//! (`picture_senders`), or the mail is yours. Pictures sent in the mail
//! itself always show: they're in its HTML already (`message.rs`).
//!
//! `GET /api/w/{workspace}/pictures` lists those senders, and
//! `POST /api/w/{workspace}/pictures` adds or takes one away.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use postrider_domain::{PictureSender, PictureSenders, normalize_email};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

/// Whether mail from `address` loads its pictures as it opens: it's one of
/// yours, or you said its sender's do.
pub async fn load(pool: &PgPool, workspace_id: Uuid, address: &str) -> Result<bool, AppError> {
    let address = address.trim().to_lowercase();
    if address.is_empty() {
        return Ok(false);
    }
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM picture_senders WHERE workspace_id = $1 AND address = $2) \
         OR EXISTS (SELECT 1 FROM mail_accounts WHERE workspace_id = $1 AND lower(address) = $2)",
    )
    .bind(workspace_id)
    .bind(&address)
    .fetch_one(pool)
    .await?)
}

pub async fn list(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<PictureSenders>, AppError> {
    let senders = sqlx::query_scalar(
        "SELECT address FROM picture_senders WHERE workspace_id = $1 ORDER BY address",
    )
    .bind(member.actor.workspace_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(PictureSenders { senders }))
}

pub async fn set(
    State(state): State<AppState>,
    member: Member,
    Json(sender): Json<PictureSender>,
) -> Result<StatusCode, AppError> {
    let address = normalize_email(&sender.address)
        .map_err(|_| AppError::invalid("That isn't an address mail comes from."))?;
    let workspace_id = member.actor.workspace_id;
    let query = if sender.load {
        "INSERT INTO picture_senders (workspace_id, address) VALUES ($1, $2) ON CONFLICT DO NOTHING"
    } else {
        "DELETE FROM picture_senders WHERE workspace_id = $1 AND address = $2"
    };
    sqlx::query(query)
        .bind(workspace_id)
        .bind(&address)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
