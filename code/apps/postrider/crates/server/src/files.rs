//! A file that came with a mail, to save and open:
//! `GET /api/w/{workspace}/mails/{account}/{uid}/files/{index}[?folder=<key>]`,
//! by its place among the mail's attachments.

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::mail::OpenParams;
use crate::message;
use crate::state::AppState;

/// The file, to be saved under its name.
pub async fn file(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid, index)): Path<(Uuid, Uuid, i64, usize)>,
    Query(params): Query<OpenParams>,
) -> Result<Response, AppError> {
    let mail = &state.mail;
    let account = mail.account(member.actor.workspace_id, account_id).await?;
    let (_, raw) = mail
        .source_of(&account, params.folder.as_deref(), uid)
        .await?;
    let file = message::file(&raw, index)
        .ok_or_else(|| AppError::not_found("This mail has no such file."))?;
    let content_type = if is_media_type(&file.content_type) {
        file.content_type
    } else {
        "application/octet-stream".into()
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, attachment(&file.name)),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        ],
        file.bytes,
    )
        .into_response())
}

/// `type/subtype` and nothing more, as a header may carry it.
fn is_media_type(value: &str) -> bool {
    value.split_once('/').is_some_and(|(kind, subtype)| {
        [kind, subtype].iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
        })
    })
}

/// `attachment; filename*=UTF-8''…` (RFC 6266), with a plain `filename`
/// for whatever reads only that.
fn attachment(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("attachment; filename=\"{plain}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_survives_the_header() {
        assert_eq!(
            attachment("Rechnung März.pdf"),
            "attachment; filename=\"Rechnung M_rz.pdf\"; filename*=UTF-8''Rechnung%20M%C3%A4rz.pdf"
        );
    }

    #[test]
    fn only_a_media_type_passes() {
        assert!(is_media_type("application/pdf"));
        assert!(is_media_type("image/svg+xml"));
        assert!(!is_media_type("text/html\r\nSet-Cookie: x"));
        assert!(!is_media_type("pdf"));
    }
}
