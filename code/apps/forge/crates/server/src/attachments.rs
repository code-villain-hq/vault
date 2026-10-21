//! Attachments: files people paste or drop into descriptions and comments.
//! Uploads go to `<data dir>/attachments/<workspace>/<id>`; Markdown links
//! to `/api/w/{workspace}/attachments/{id}`, which only members can fetch.
//! Common image types show inline; anything else downloads, so an uploaded
//! page or script never runs on Forge's origin.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

/// The largest file people can attach.
pub const MAX_SIZE: usize = 10 * 1024 * 1024;

/// Shown in place, in `<img>`; everything else is a download.
const INLINE: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

#[derive(Deserialize)]
pub struct UploadParams {
    name: String,
}

#[derive(Serialize)]
pub struct Uploaded {
    pub id: Uuid,
    pub name: String,
    pub content_type: String,
    /// Where it's served, for Markdown.
    pub url: String,
    /// Whether it shows in place (an image people can see), or downloads.
    pub inline: bool,
}

fn path(state: &AppState, workspace_id: Uuid, id: Uuid) -> std::path::PathBuf {
    state
        .data_dir
        .join("attachments")
        .join(workspace_id.to_string())
        .join(id.to_string())
}

/// `POST /api/w/{workspace}/attachments?name=…`, with the file as the body.
pub async fn upload(
    State(state): State<AppState>,
    member: Member,
    Query(params): Query<UploadParams>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Uploaded>, AppError> {
    if body.is_empty() {
        return Err(AppError::invalid("the file is empty"));
    }
    let name: String = params
        .name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let name = if name.is_empty() { "file".into() } else { name };
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or("").trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "application/octet-stream".into());

    let id = Uuid::now_v7();
    let workspace_id = member.actor.workspace_id;
    let file = path(&state, workspace_id, id);
    if let Some(dir) = file.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(AppError::internal)?;
    }
    tokio::fs::write(&file, &body)
        .await
        .map_err(AppError::internal)?;
    sqlx::query(
        "INSERT INTO attachments (id, workspace_id, name, content_type, size, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(&name)
    .bind(&content_type)
    .bind(body.len() as i64)
    .bind(member.actor.user_id)
    .execute(&state.pool)
    .await?;
    Ok(Json(Uploaded {
        id,
        url: format!("/api/w/{workspace_id}/attachments/{id}"),
        name,
        inline: INLINE.contains(&content_type.as_str()),
        content_type,
    }))
}

/// `GET /api/w/{workspace}/attachments/{id}`
pub async fn download(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    let workspace_id = member.actor.workspace_id;
    let found: Option<(String, String)> = sqlx::query_as(
        "SELECT name, content_type FROM attachments WHERE id = $1 AND workspace_id = $2",
    )
    .bind(id)
    .bind(workspace_id)
    .fetch_optional(&state.pool)
    .await?;
    let (name, content_type) = found.ok_or_else(|| AppError::not_found("attachment not found"))?;
    let bytes = tokio::fs::read(path(&state, workspace_id, id))
        .await
        .map_err(|_| AppError::not_found("the attachment's file is missing"))?;

    let inline = INLINE.contains(&content_type.as_str());
    let served = if inline {
        content_type
    } else {
        "application/octet-stream".into()
    };
    // ASCII only, without quotes, so the header can't be broken out of.
    let fallback: String = name
        .chars()
        .map(|c| {
            if (c.is_ascii_graphic() && c != '"' && c != '\\') || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let disposition = format!(
        "{}; filename=\"{fallback}\"",
        if inline { "inline" } else { "attachment" }
    );
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    let value = |v: &str| HeaderValue::from_str(v).map_err(AppError::internal);
    headers.insert(header::CONTENT_TYPE, value(&served)?);
    headers.insert(header::CONTENT_DISPOSITION, value(&disposition)?);
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'"),
    );
    // Attachments never change, but only the signed-in person may keep a copy.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    Ok(response)
}
