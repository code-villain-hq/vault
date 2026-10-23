//! Attachments: files people paste or drop into descriptions and comments.
//! Uploads go to `<data dir>/attachments/<workspace>/<id>`; Markdown links
//! to `/api/w/{workspace}/attachments/{id}`, which only members can fetch.
//! Common image types show inline; anything else downloads, so an uploaded
//! page or script never runs on Forge's origin. A download link carries a
//! ticket instead, for the browser the desktop app hands a link to, which
//! has no session for the workspace's server.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{Member, hash_token, new_token};
use crate::error::AppError;
use crate::state::AppState;

/// The largest file people can attach.
pub const MAX_SIZE: usize = 10 * 1024 * 1024;

/// Shown in place, in `<img>`; everything else is a download.
const INLINE: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/// How long a download link works.
const TICKET_LIFE: Duration = Duration::from_secs(60);

/// Download links not yet used, by their ticket's hash: each opens one
/// attachment once, within [`TICKET_LIFE`]. They live only in memory, so a
/// restart ends them.
#[derive(Default)]
pub struct Tickets(Mutex<HashMap<Vec<u8>, Ticket>>);

struct Ticket {
    workspace_id: Uuid,
    attachment_id: Uuid,
    user_id: Uuid,
    expires: Instant,
}

impl Tickets {
    fn issue(&self, workspace_id: Uuid, attachment_id: Uuid, user_id: Uuid) -> String {
        let ticket = new_token();
        let now = Instant::now();
        let mut tickets = self.0.lock().expect("no one panics holding the tickets");
        tickets.retain(|_, t| t.expires > now);
        tickets.insert(
            hash_token(&ticket),
            Ticket {
                workspace_id,
                attachment_id,
                user_id,
                expires: now + TICKET_LIFE,
            },
        );
        ticket
    }

    /// Whether `ticket` opens this attachment; using it ends it.
    fn redeem(&self, ticket: &str, workspace_id: Uuid, attachment_id: Uuid) -> Option<Uuid> {
        let hash = hash_token(ticket);
        let mut tickets = self.0.lock().expect("no one panics holding the tickets");
        let opens = tickets.get(&hash).is_some_and(|t| {
            t.workspace_id == workspace_id
                && t.attachment_id == attachment_id
                && t.expires > Instant::now()
        });
        if opens {
            tickets.remove(&hash).map(|t| t.user_id)
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
pub struct UploadParams {
    name: String,
    issue_id: Option<Uuid>,
    team_id: Option<Uuid>,
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
    let workspace_id = member.actor.workspace_id;
    if let Some(issue) = params.issue_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM issues WHERE id = $1 AND workspace_id = $2)",
        )
        .bind(issue)
        .bind(workspace_id)
        .fetch_one(&state.pool)
        .await?;
        if !exists {
            return Err(AppError::not_found("issue not found"));
        }
        member.require_sees_issue(&state.pool, issue).await?;
    } else if let Some(team) = params.team_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM teams t WHERE t.id = $1 AND t.workspace_id = $2 AND \
             ($3 OR EXISTS (SELECT 1 FROM team_members m WHERE m.team_id = t.id AND m.user_id = $4)))")
            .bind(team).bind(workspace_id).bind(member.role != forge_domain::Role::Guest)
            .bind(member.actor.user_id).fetch_one(&state.pool).await?;
        if !visible {
            return Err(AppError::not_found("team not found"));
        }
    } else {
        member.require_not_guest()?;
    }
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
        "INSERT INTO attachments (id, workspace_id, name, content_type, size, created_by, issue_id, team_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(&name)
    .bind(&content_type)
    .bind(body.len() as i64)
    .bind(member.actor.user_id)
    .bind(params.issue_id)
    .bind(if params.issue_id.is_some() { None } else { params.team_id })
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

#[derive(Deserialize)]
pub struct DownloadParams {
    ticket: Option<String>,
}

#[derive(Serialize)]
pub struct DownloadLink {
    /// The attachment's address with a ticket, for a browser with no session.
    pub url: String,
}

/// `POST /api/w/{workspace}/attachments/{id}/link`: a download link that
/// works once, within a minute, without a session.
pub async fn link(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DownloadLink>, AppError> {
    let workspace_id = member.actor.workspace_id;
    require_visible(&state, member.actor.user_id, workspace_id, id).await?;
    let found: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM attachments WHERE id = $1 AND workspace_id = $2")
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(&state.pool)
            .await?;
    found.ok_or_else(|| AppError::not_found("attachment not found"))?;
    let ticket = state.tickets.issue(workspace_id, id, member.actor.user_id);
    Ok(Json(DownloadLink {
        url: format!("/api/w/{workspace_id}/attachments/{id}?ticket={ticket}"),
    }))
}

/// `GET /api/w/{workspace}/attachments/{id}`, for a member or with a
/// download link's ticket.
pub async fn download(
    State(state): State<AppState>,
    member: Result<Member, AppError>,
    Path((workspace_id, id)): Path<(Uuid, Uuid)>,
    Query(params): Query<DownloadParams>,
) -> Result<Response, AppError> {
    let ticketed = params
        .ticket
        .and_then(|ticket| state.tickets.redeem(&ticket, workspace_id, id));
    let user = match ticketed {
        Some(user) => user,
        None => member?.actor.user_id,
    };
    require_visible(&state, user, workspace_id, id).await?;
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
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

/// Resolve current membership and the attachment's original scope on every
/// read, including ticket redemption. Pasting its URL cannot grant access.
async fn require_visible(
    state: &AppState,
    user: Uuid,
    workspace: Uuid,
    id: Uuid,
) -> Result<(), AppError> {
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM attachments a \
         JOIN workspace_members m ON m.workspace_id = a.workspace_id AND m.user_id = $1 \
         LEFT JOIN issues i ON i.id = a.issue_id AND i.workspace_id = a.workspace_id \
         WHERE a.workspace_id = $2 AND a.id = $3 AND (m.role <> 'guest' OR EXISTS \
         (SELECT 1 FROM team_members tm WHERE tm.user_id = m.user_id AND tm.team_id = \
         CASE WHEN a.issue_id IS NOT NULL THEN i.team_id ELSE a.team_id END)))",
    )
    .bind(user)
    .bind(workspace)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    if visible {
        Ok(())
    } else {
        Err(AppError::not_found("attachment not found"))
    }
}
