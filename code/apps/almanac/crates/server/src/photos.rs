//! The photos a question comes with: of a shampoo, a plant, a page. The
//! question's message names them (`Message::photos`, id and size), and
//! their bytes stay in `photos`, which doesn't sync: the app fetches each
//! one (`GET /api/w/{id}/photos/{photo}`). They go with their message.
//!
//! Claude sees a talk's photos with each question in it, so a later
//! question can ask about an earlier photo.

use std::collections::HashMap;

use almanac_domain::{Actor, Photo, PhotoUpload, Transaction};
use almanac_sync::Origin;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::state::AppState;

/// The most photos one question takes.
pub const MOST: usize = 6;
/// Claude reads a photo of up to 5 MB.
const LARGEST: usize = 5 * 1024 * 1024;
const TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

const SAVE: &str = "INSERT INTO photos (id, workspace_id, message_id, position, media_type, data) \
     VALUES ($1, $2, $3, $4, $5, $6)";
const ONE: &str = "SELECT media_type, data FROM photos WHERE id = $1 AND workspace_id = $2";
const OF_TALK: &str = "SELECT p.message_id, p.media_type, p.data FROM photos p \
     JOIN messages m ON m.id = p.message_id \
     WHERE m.talk_id = $1 AND p.workspace_id = $2 ORDER BY p.message_id, p.position";

/// A photo as Claude reads it: its type and its bytes in base64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub media_type: String,
    pub data: String,
}

/// A photo checked and ready to keep.
pub struct Upload {
    pub photo: Photo,
    media_type: String,
    bytes: Vec<u8>,
}

/// The photos of a question, checked: a known type, bytes that decode, no
/// larger than Claude reads, and no more than [`MOST`].
pub fn check(uploads: &[PhotoUpload]) -> Result<Vec<Upload>, AppError> {
    if uploads.len() > MOST {
        return Err(AppError::invalid(format!(
            "A question takes up to {MOST} photos."
        )));
    }
    uploads
        .iter()
        .map(|upload| {
            if !TYPES.contains(&upload.media_type.as_str()) {
                return Err(AppError::invalid(
                    "Almanac reads JPEG, PNG, WebP and GIF photos.",
                ));
            }
            let bytes = STANDARD
                .decode(&upload.data)
                .map_err(|_| AppError::invalid("That photo didn't come through. Add it again."))?;
            if bytes.is_empty() {
                return Err(AppError::invalid(
                    "That photo didn't come through. Add it again.",
                ));
            }
            if bytes.len() > LARGEST {
                return Err(AppError::invalid(
                    "That photo is larger than 5 MB. Add a smaller one.",
                ));
            }
            Ok(Upload {
                photo: Photo {
                    id: Uuid::now_v7(),
                    width: upload.width.clamp(1, 20_000),
                    height: upload.height.clamp(1, 20_000),
                },
                media_type: upload.media_type.clone(),
                bytes,
            })
        })
        .collect()
}

/// Writes `transaction` and keeps the photos of its question `message_id`
/// with it, at once: the message never names a photo that isn't there.
pub async fn write_with(
    state: &AppState,
    actor: Actor,
    transaction: &Transaction,
    message_id: Uuid,
    uploads: &[Upload],
) -> Result<(), AppError> {
    let mut db = state.pool.begin().await?;
    almanac_sync::apply_in(&mut db, actor, transaction, Origin::Server).await?;
    for (position, upload) in uploads.iter().enumerate() {
        sqlx::query(SAVE)
            .bind(upload.photo.id)
            .bind(actor.workspace_id)
            .bind(message_id)
            .bind(position as i32)
            .bind(&upload.media_type)
            .bind(&upload.bytes)
            .execute(&mut *db)
            .await?;
    }
    db.commit().await?;
    Ok(())
}

/// The photos of a talk's questions, by message, in the order they came.
pub async fn of_talk(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    talk_id: Uuid,
) -> Result<HashMap<Uuid, Vec<Image>>, sqlx::Error> {
    let rows: Vec<(Uuid, String, Vec<u8>)> = sqlx::query_as(OF_TALK)
        .bind(talk_id)
        .bind(workspace_id)
        .fetch_all(pool)
        .await?;
    let mut out: HashMap<Uuid, Vec<Image>> = HashMap::new();
    for (message_id, media_type, bytes) in rows {
        out.entry(message_id).or_default().push(Image {
            media_type,
            data: STANDARD.encode(bytes),
        });
    }
    Ok(out)
}

/// `GET /api/w/{id}/photos/{photo_id}`: the photo itself. It never
/// changes, so the app may keep it.
pub async fn show(
    State(state): State<AppState>,
    member: Member,
    Path((_, photo_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    // Guests see no topics, so no talk's photos either.
    member.require_not_guest()?;
    let (media_type, bytes): (String, Vec<u8>) = sqlx::query_as(ONE)
        .bind(photo_id)
        .bind(member.actor.workspace_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("photo not found"))?;
    Ok((
        [
            (header::CONTENT_TYPE, media_type),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable".to_string(),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload(media_type: &str, data: &str) -> PhotoUpload {
        PhotoUpload {
            media_type: media_type.into(),
            data: data.into(),
            width: 800,
            height: 600,
        }
    }

    #[test]
    fn a_photo_is_a_known_type_that_decodes() {
        let photo = STANDARD.encode(b"jpeg bytes");
        assert!(check(&[upload("image/jpeg", &photo)]).is_ok());
        assert!(check(&[upload("image/heic", &photo)]).is_err());
        assert!(check(&[upload("image/png", "not base64!")]).is_err());
        assert!(check(&[upload("image/png", "")]).is_err());
    }

    #[test]
    fn a_question_takes_a_few_photos() {
        let photo = STANDARD.encode(b"jpeg bytes");
        let some: Vec<_> = (0..MOST).map(|_| upload("image/jpeg", &photo)).collect();
        assert_eq!(check(&some).map(|u| u.len()).ok(), Some(MOST));
        let more: Vec<_> = (0..=MOST).map(|_| upload("image/jpeg", &photo)).collect();
        assert!(check(&more).is_err());
    }
}
