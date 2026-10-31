//! The photos a question comes with: of a shampoo, a plant, a page. The
//! question's message names them (`Message::photos`, id and size), and
//! their bytes stay in `photos`, which doesn't sync: the app fetches each
//! one (`GET /api/w/{id}/photos/{photo}`). They go with their message.
//!
//! Claude sees a talk's photos with each question in it, so a later
//! question can ask about an earlier photo.
//!
//! A photo may also belong to no message: one dropped on a topic's cover or
//! a row (`POST /api/w/{id}/photos`), or one a talk fetched from the web
//! for a row or the cover ([`from_web`]).

use std::collections::HashMap;

use almanac_domain::{Actor, Photo, PhotoUpload, Transaction};
use almanac_sync::Origin;
use axum::Json;
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
const SAVE_ALONE: &str = "INSERT INTO photos (id, workspace_id, position, media_type, data) \
     VALUES ($1, $2, 0, $3, $4)";
const ONE: &str = "SELECT media_type, data FROM photos WHERE id = $1 AND workspace_id = $2";
const OF_TALK: &str = "SELECT p.id, p.message_id, p.media_type, p.data FROM photos p \
     JOIN messages m ON m.id = p.message_id \
     WHERE m.talk_id = $1 AND p.workspace_id = $2 ORDER BY p.message_id, p.position";

/// A photo as Claude reads it: its type and its bytes in base64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Which photo it is, for a collection's photo cell.
    pub id: Uuid,
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
    let rows: Vec<(Uuid, Uuid, String, Vec<u8>)> = sqlx::query_as(OF_TALK)
        .bind(talk_id)
        .bind(workspace_id)
        .fetch_all(pool)
        .await?;
    let mut out: HashMap<Uuid, Vec<Image>> = HashMap::new();
    for (id, message_id, media_type, bytes) in rows {
        out.entry(message_id).or_default().push(Image {
            id,
            media_type,
            data: STANDARD.encode(bytes),
        });
    }
    Ok(out)
}

/// `POST /api/w/{id}/photos`: keeps a photo that belongs to no question,
/// for a topic's cover or a row's photo cell, which the app then sets.
pub async fn upload(
    State(state): State<AppState>,
    member: Member,
    Json(upload): Json<PhotoUpload>,
) -> Result<Json<Photo>, AppError> {
    member.require_not_guest()?;
    let [checked] = <[Upload; 1]>::try_from(check(std::slice::from_ref(&upload))?)
        .map_err(|_| AppError::invalid("Add one photo."))?;
    sqlx::query(SAVE_ALONE)
        .bind(checked.photo.id)
        .bind(member.actor.workspace_id)
        .bind(&checked.media_type)
        .bind(&checked.bytes)
        .execute(&state.pool)
        .await?;
    Ok(Json(checked.photo))
}

/// How long fetching a picture from the web may take.
const FETCH_FOR: std::time::Duration = std::time::Duration::from_secs(20);

/// Fetches a picture from the web and keeps it, for a talk: `address` is
/// the picture's, or a page's whose picture (its `og:image`) it is. Only
/// public https addresses, a known type, up to 5 MB. What goes wrong comes
/// back in words for Claude.
pub async fn from_web(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    address: &str,
) -> Result<Uuid, String> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_FOR)
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) Almanac")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > 5 || !public(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| "Almanac can't fetch pictures right now.".to_string())?;
    let url = reqwest::Url::parse(address.trim())
        .map_err(|_| format!("{address} isn't a web address."))?;
    let (media_type, bytes) = match fetch(&client, &url).await? {
        Fetched::Picture(media_type, bytes) => (media_type, bytes),
        Fetched::Page(html) => {
            let picture = picture_of(&html, &url).ok_or_else(|| {
                format!("{url} names no picture of its own. Give the picture's address.")
            })?;
            match fetch(&client, &picture).await? {
                Fetched::Picture(media_type, bytes) => (media_type, bytes),
                Fetched::Page(_) => return Err(format!("{picture} isn't a picture.")),
            }
        }
    };
    let id = Uuid::now_v7();
    sqlx::query(SAVE_ALONE)
        .bind(id)
        .bind(workspace_id)
        .bind(&media_type)
        .bind(&bytes)
        .execute(pool)
        .await
        .map_err(|_| "Almanac couldn't keep the picture.".to_string())?;
    Ok(id)
}

enum Fetched {
    Picture(String, Vec<u8>),
    Page(String),
}

/// A public https address: no other scheme, no name or address of this
/// Mac or a private network.
fn public(url: &reqwest::Url) -> bool {
    use std::net::IpAddr;
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            !(ip.is_loopback() || ip.is_unspecified() || ip.is_private() || ip.is_link_local())
        }
        Ok(IpAddr::V6(ip)) => !(ip.is_loopback() || ip.is_unspecified()),
        Err(_) => {
            !(host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local"))
        }
    }
}

async fn fetch(client: &reqwest::Client, url: &reqwest::Url) -> Result<Fetched, String> {
    if !public(url) {
        return Err(format!("{url} isn't a public https address."));
    }
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| format!("{url} didn't answer."))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}.", response.status().as_u16()));
    }
    let media_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let page = media_type == "text/html" || media_type == "application/xhtml+xml";
    if !page && !TYPES.contains(&media_type.as_str()) {
        return Err(format!(
            "{url} is {media_type}: Almanac keeps JPEG, PNG, WebP and GIF pictures."
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| format!("{url} stopped answering."))?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > LARGEST {
            if page {
                break;
            }
            return Err(format!("{url} is larger than 5 MB."));
        }
    }
    if page {
        return Ok(Fetched::Page(String::from_utf8_lossy(&bytes).into_owned()));
    }
    if bytes.is_empty() {
        return Err(format!("{url} came empty."));
    }
    Ok(Fetched::Picture(media_type, bytes))
}

/// The picture a page names for itself: its `og:image`, or else its
/// `twitter:image`, as an address from the page's.
fn picture_of(html: &str, page: &reqwest::Url) -> Option<reqwest::Url> {
    let lower = html.to_ascii_lowercase();
    ["og:image:secure_url", "og:image", "twitter:image"]
        .iter()
        .find_map(|name| {
            let mut from = 0;
            while let Some(at) = lower[from..].find("<meta") {
                let start = from + at;
                let end = start + lower[start..].find('>')?;
                let tag = &lower[start..end];
                from = end;
                let names = [
                    format!("property=\"{name}\""),
                    format!("name=\"{name}\""),
                    format!("property='{name}'"),
                    format!("name='{name}'"),
                ];
                if !names.iter().any(|n| tag.contains(n.as_str())) {
                    continue;
                }
                let content = tag.find("content=")? + "content=".len();
                let quote = tag[content..].chars().next()?;
                if quote != '"' && quote != '\'' {
                    continue;
                }
                let value = &html[start + content + 1..end];
                let value = &value[..value.find(quote)?];
                let value = value.trim().replace("&amp;", "&");
                if let Ok(url) = page.join(&value) {
                    return Some(url);
                }
            }
            None
        })
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
    fn a_page_names_its_picture() {
        let page = reqwest::Url::parse("https://shop.example/p/felix").unwrap();
        let html = r#"<head><meta name="description" content="Felix">
            <meta property="og:image" content="/img/felix.jpg?w=800&amp;h=600"></head>"#;
        assert_eq!(
            picture_of(html, &page).map(|u| u.to_string()).as_deref(),
            Some("https://shop.example/img/felix.jpg?w=800&h=600")
        );
        assert!(picture_of("<meta name=\"x\" content=\"y\">", &page).is_none());
    }

    #[test]
    fn only_public_https_addresses_are_fetched() {
        let ok = |a: &str| public(&reqwest::Url::parse(a).unwrap());
        assert!(ok("https://www.zooplus.de/felix.jpg"));
        assert!(!ok("http://www.zooplus.de/felix.jpg"));
        assert!(!ok("https://localhost/x.png"));
        assert!(!ok("https://127.0.0.1:7785/api"));
        assert!(!ok("https://192.168.1.4/x.png"));
        assert!(!ok("https://[::1]/x.png"));
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
