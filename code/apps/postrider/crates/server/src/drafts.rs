//! Drafts: what you write, kept in its account's Drafts as you go, so it's
//! there after Postrider quits, and in the account's other mail apps.
//! `POST /drafts` keeps it, each time anew in place of the one before,
//! found by its Message-ID. `GET /drafts/{account}/{uid}` opens one from
//! Drafts to write on. `POST /drafts/trash` throws one away, to Trash, and
//! `POST /drafts/delete` deletes one with nothing left in it. Sending it
//! deletes it (`send.rs`).
//!
//! While Drafts can't be reached, a draft waits in the outbox
//! (`outbox.rs`), shown in Drafts, and goes there once it can.
//!
//! A draft is written as its mail would go: your words, what it quotes or
//! forwards below them, its files and its thread. What Postrider needs
//! besides goes in its note, the `X-Postrider-Draft` header: whom it goes
//! to as typed, how much of its text is below your words, and the mail it
//! answers or forwards. A draft another mail app kept has none: all of its
//! text is yours to write on.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use lettre::message::header::{Header, HeaderName, HeaderValue};
use mail_parser::{AddressList, MessageParser};
use postrider_domain::{Draft, DraftRef, FolderRole, MailAccount, MailRef, Outgoing};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap::{self, Failure, Place};
use crate::mail::Mail;
use crate::message;
use crate::send::{self, Purpose, lf};
use crate::state::AppState;

/// The header that holds a draft's note.
const NOTE: &str = "X-Postrider-Draft";

/// What a draft holds for Postrider besides the mail it would be.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct DraftNote {
    /// Whom it goes to, as typed: an address half typed too.
    pub to: String,
    pub cc: String,
    pub bcc: String,
    /// How many characters at the end of its text are below your words.
    pub below: usize,
    pub answers: Option<MailRef>,
    pub forwards: Option<MailRef>,
}

impl DraftNote {
    /// As its header: in JSON, in base64, in pieces short enough for any
    /// server's lines.
    pub(crate) fn header(&self) -> NoteHeader {
        let json = serde_json::to_vec(self).expect("a note is JSON");
        let packed = STANDARD.encode(json);
        let pieces: Vec<&str> = packed
            .as_bytes()
            .chunks(64)
            .map(|piece| std::str::from_utf8(piece).expect("base64 is ASCII"))
            .collect();
        NoteHeader(pieces.join(" "))
    }

    /// The note a draft holds: `None` for one another app kept.
    fn of(draft: &mail_parser::Message<'_>) -> Option<DraftNote> {
        let raw = draft.headers().get(NOTE)?.raw_value();
        let packed: String = String::from_utf8_lossy(raw)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        serde_json::from_slice(&STANDARD.decode(packed).ok()?).ok()
    }
}

/// A draft's note, as lettre writes headers.
#[derive(Debug, Clone)]
pub(crate) struct NoteHeader(String);

impl Header for NoteHeader {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str(NOTE)
    }

    fn parse(s: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(s.to_string()))
    }

    fn display(&self) -> HeaderValue {
        HeaderValue::new(Self::name(), self.0.clone())
    }
}

/// What's below your words in a draft: as much of the end of its text as
/// its note says.
pub(crate) fn below(draft: &mail_parser::Message<'_>) -> String {
    let below = DraftNote::of(draft).map_or(0, |note| note.below);
    split(&text_of(draft), below).1
}

/// A draft's text, its line breaks as `\n`.
fn text_of(draft: &mail_parser::Message<'_>) -> String {
    lf(draft
        .text_bodies()
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim_end())
}

/// A draft's text split where your words end: them, and what's below.
fn split(text: &str, below: usize) -> (String, String) {
    let yours = text.chars().count().saturating_sub(below);
    let at = text
        .char_indices()
        .nth(yours)
        .map_or(text.len(), |(at, _)| at);
    (text[..at].trim_end().to_string(), text[at..].to_string())
}

/// A draft, from its source, to write on.
fn read(account_id: Uuid, raw: &[u8]) -> Result<Draft, AppError> {
    let draft = MessageParser::new()
        .parse(raw)
        .ok_or_else(|| AppError::invalid("This draft can't be read."))?;
    let note = DraftNote::of(&draft);
    let (text, _) = split(&text_of(&draft), note.as_ref().map_or(0, |n| n.below));
    let typed = |field: Option<AddressList<'_>>| {
        field
            .map(|list| {
                list.mailboxes()
                    .filter_map(|mailbox| {
                        let address = mailbox.address()?;
                        Some(written(mailbox.name().unwrap_or_default(), address))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    let note = note.unwrap_or_else(|| DraftNote {
        to: typed(draft.to()),
        cc: typed(draft.cc()),
        bcc: typed(draft.bcc()),
        ..DraftNote::default()
    });
    Ok(Draft {
        draft: DraftRef {
            account_id,
            message_id: draft
                .message_id()
                .map(|id| format!("<{id}>"))
                .unwrap_or_default(),
        },
        to: note.to,
        cc: note.cc,
        bcc: note.bcc,
        subject: draft.subject().unwrap_or_default().trim().to_string(),
        text,
        files: message::attachments(raw),
        answers: note.answers,
        forwards: note.forwards,
    })
}

/// Someone as To takes them, as the page writes them: "Anna Weber
/// <anna@…>", the name quoted where it needs it.
fn written(name: &str, address: &str) -> String {
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case(address) {
        return address.to_string();
    }
    if name.contains(['"', ',', ';', '<', '>', '@']) {
        format!("\"{}\" <{address}>", name.replace('"', ""))
    } else {
        format!("{name} <{address}>")
    }
}

impl Mail {
    /// Keeps `outgoing` in Drafts, in its account's, in place of the draft
    /// it was kept as last: the draft it's kept as now.
    pub async fn keep_draft(
        self: &Arc<Self>,
        workspace_id: Uuid,
        user_id: Uuid,
        outgoing: &Outgoing,
    ) -> Result<DraftRef, AppError> {
        outgoing.check_size()?;
        let refused = |failure: Failure| AppError::refused(failure.message());
        let account = self.account(workspace_id, outgoing.from_account).await?;
        let from = self.sender(&account, user_id).await?;
        let gathered = self.gather(workspace_id, user_id, outgoing).await?;
        let written = send::write(
            from,
            outgoing,
            &gathered.below,
            &gathered.files,
            Purpose::Draft,
        )?;
        let kept = async {
            let drafts = self.folder_for(&account, FolderRole::Drafts).await?;
            self.on_server(&account, |session| {
                let (path, raw) = (drafts.path.clone(), written.kept.clone());
                Box::pin(async move { imap::put_draft(session, &path, &raw).await })
            })
            .await
        };
        let waits = self.waits_to_send(account.id);
        match if waits { Ok(()) } else { kept.await } {
            Ok(()) if !waits => {
                if let Some((account_id, message_id)) =
                    self.replaced(outgoing.draft.as_ref()).await?
                {
                    self.delete_draft(account_id, &message_id).await;
                }
            }
            Ok(()) | Err(Failure::Offline(_)) => {
                let replaces = self.replaced(outgoing.draft.as_ref()).await?;
                self.post(&account, crate::outbox::Posted::Draft, &written, replaces)
                    .await?;
            }
            Err(failure) => return Err(refused(failure)),
        }
        Ok(DraftRef {
            account_id: account.id,
            message_id: written.message_id,
        })
    }

    /// Opens the draft at `uid` in the account's Drafts.
    pub async fn open_draft(
        &self,
        workspace_id: Uuid,
        account_id: Uuid,
        uid: i64,
    ) -> Result<Draft, AppError> {
        let refused = |failure: Failure| AppError::refused(failure.message());
        let account = self.account(workspace_id, account_id).await?;
        if let Some(raw) = self.waiting_at(account.id, uid).await? {
            return read(account.id, &raw);
        }
        let gone = || AppError::not_found("This draft isn't in Drafts any more.");
        let place = self
            .drafts_of(&account)
            .await
            .map_err(refused)?
            .ok_or_else(gone)?;
        let raw = self
            .on_server(&account, |session| {
                let path = place.path.clone();
                Box::pin(async move { imap::source(session, &path, None, uid).await })
            })
            .await
            .map_err(refused)?
            .ok_or_else(gone)?;
        read(account.id, &raw)
    }

    /// Throws a draft away: to its account's Trash, or for good when
    /// there's nothing left in it.
    pub async fn drop_draft(
        &self,
        workspace_id: Uuid,
        draft: &DraftRef,
        to_trash: bool,
    ) -> Result<(), AppError> {
        let refused = |failure: Failure| AppError::refused(failure.message());
        let account = self.account(workspace_id, draft.account_id).await?;
        // One waiting here goes at once; what it replaces in Drafts waits
        // to be deleted there, as one out of reach does.
        let Some((account_id, message_id)) = self.replaced(Some(draft)).await? else {
            return Ok(());
        };
        let account = if account_id == account.id {
            account
        } else {
            self.account(workspace_id, account_id).await?
        };
        let dropped = async {
            if self.waits_to_send(account.id) {
                return Err(Failure::Offline("its changes wait".into()));
            }
            let Some(place) = self.drafts_of(&account).await? else {
                return Ok(());
            };
            if !to_trash {
                return self
                    .on_server(&account, |session| {
                        let (path, id) = (place.path.clone(), message_id.clone());
                        Box::pin(async move { imap::delete_by_id(session, &path, &id).await })
                    })
                    .await;
            }
            let trash = self.folder_for(&account, FolderRole::Trash).await?;
            self.on_server(&account, |session| {
                let (from, id, to) = (place.path.clone(), message_id.clone(), trash.path.clone());
                Box::pin(async move { imap::move_by_id(session, &from, &id, &to).await })
            })
            .await
        };
        match dropped.await {
            Ok(()) => Ok(()),
            Err(Failure::Offline(_)) => Ok(self.post_deletion(&account, &message_id).await?),
            Err(failure) => Err(refused(failure)),
        }
    }

    /// A draft's source: waiting here, or found in its account's Drafts
    /// by its Message-ID. `None` once it's gone.
    pub(crate) async fn draft_source(
        &self,
        account: &MailAccount,
        message_id: &str,
    ) -> Result<Option<Vec<u8>>, Failure> {
        if message_id.is_empty() {
            return Ok(None);
        }
        if let Some(raw) = self.waiting_draft(account.id, message_id).await? {
            return Ok(Some(raw));
        }
        let Some(place) = self.drafts_of(account).await? else {
            return Ok(None);
        };
        self.on_server(account, |session| {
            let (path, id) = (place.path.clone(), message_id.to_string());
            Box::pin(async move { imap::source_by_id(session, &path, &id).await })
        })
        .await
    }

    /// Deletes a draft for good, as one kept anew is, in the account
    /// `account_id`. One out of reach waits to be deleted.
    pub(crate) async fn delete_draft(&self, account_id: Uuid, message_id: &str) {
        let deleted = async {
            let account = self
                .account_by_id(account_id)
                .await
                .map_err(|e| Failure::Offline(e.to_string()))?;
            let Some(place) = self.drafts_of(&account).await? else {
                return Ok(());
            };
            let found = self
                .on_server(&account, |session| {
                    let (path, id) = (place.path.clone(), message_id.to_string());
                    Box::pin(async move { imap::delete_by_id(session, &path, &id).await })
                })
                .await;
            if let Err(Failure::Offline(_)) = found {
                self.post_deletion(&account, message_id)
                    .await
                    .map_err(|e| Failure::Offline(e.to_string()))?;
                return Ok(());
            }
            found
        };
        if let Err(failure) = deleted.await {
            tracing::info!("couldn't delete a draft: {}", failure.message());
        }
    }

    /// The account's Drafts: `None` while it has none.
    pub(crate) async fn drafts_of(&self, account: &MailAccount) -> Result<Option<Place>, Failure> {
        Ok(self
            .places(account)
            .await?
            .into_iter()
            .find(|place| place.role == Some(FolderRole::Drafts)))
    }
}

/// `POST /api/w/{workspace}/drafts`: keeps what you're writing in Drafts.
pub async fn keep(
    State(state): State<AppState>,
    member: Member,
    Json(outgoing): Json<Outgoing>,
) -> Result<Json<DraftRef>, AppError> {
    Ok(Json(
        state
            .mail
            .keep_draft(member.actor.workspace_id, member.actor.user_id, &outgoing)
            .await?,
    ))
}

/// `GET /api/w/{workspace}/drafts/{account_id}/{uid}`: a draft, to write on.
pub async fn open(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid)): Path<(Uuid, Uuid, i64)>,
) -> Result<Json<Draft>, AppError> {
    Ok(Json(
        state
            .mail
            .open_draft(member.actor.workspace_id, account_id, uid)
            .await?,
    ))
}

/// `POST /api/w/{workspace}/drafts/trash`: throws a draft away, to Trash.
pub async fn trash(
    State(state): State<AppState>,
    member: Member,
    Json(draft): Json<DraftRef>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .drop_draft(member.actor.workspace_id, &draft, true)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/w/{workspace}/drafts/delete`: deletes a draft with nothing
/// left in it.
pub async fn delete(
    State(state): State<AppState>,
    member: Member,
    Json(draft): Json<DraftRef>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .drop_draft(member.actor.workspace_id, &draft, false)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use lettre::message::Mailbox;
    use postrider_domain::Attach;

    use super::*;
    use crate::send::{Below, write};

    fn from() -> Mailbox {
        "Michael Lazarski <michael@lazarski.test>".parse().unwrap()
    }

    fn outgoing(to: &str, text: &str) -> Outgoing {
        Outgoing {
            from_account: Uuid::nil(),
            to: to.into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Layer for our team?".into(),
            text: text.into(),
            answers: None,
            forwards: None,
            files: Vec::new(),
            draft: None,
        }
    }

    const MAIL: &[u8] = b"From: Anna Weber <anna@example.test>\r\n\
To: Michael <michael@lazarski.test>\r\n\
Subject: Layer for our team?\r\n\
Date: Mon, 3 Nov 2025 08:12:00 +0100\r\n\
Message-ID: <q1@example.test>\r\n\
\r\n\
Does it work for three Macs?\r\n";

    #[test]
    fn a_draft_keeps_what_you_typed_and_reads_back_your_words() {
        let mut mail = outgoing("Anna Weber <anna@example.test>, kim@", "Yes.\n\nIt does.\n");
        mail.bcc = "\"Weber, Team\" <team@example.test>".into();
        mail.answers = Some(MailRef {
            account_id: Uuid::nil(),
            folder: None,
            uid: 7,
        });
        let written = write(
            from(),
            &mail,
            &Below::Answers(MAIL.to_vec()),
            &[],
            Purpose::Draft,
        )
        .unwrap();
        let kept = String::from_utf8(written.kept.clone()).unwrap();
        // Only the whole addresses go in its header; Bcc is kept.
        assert!(kept.contains("To: \"Anna Weber\" <anna@example.test>\r\n"));
        assert!(kept.contains("Bcc: =?utf-8?b?V2ViZXIsIFRlYW0=?= <team@example.test>"));
        assert!(kept.contains("In-Reply-To: <q1@example.test>"));

        let draft = read(Uuid::nil(), &written.kept).unwrap();
        assert_eq!(draft.to, mail.to);
        assert_eq!(draft.bcc, mail.bcc);
        assert_eq!(draft.text, "Yes.\n\nIt does.");
        assert_eq!(draft.subject, "Layer for our team?");
        assert_eq!(draft.answers.map(|m| m.uid), Some(7));
        assert_eq!(draft.draft.message_id, written.message_id);

        // Written again from the draft, it quotes and threads as before.
        let mut again = mail.clone();
        again.text = "Yes, it does.".into();
        again.to = "Anna Weber <anna@example.test>".into();
        let rewritten = write(
            from(),
            &again,
            &Below::Drafted(written.kept),
            &[],
            Purpose::Send,
        )
        .unwrap();
        let sent = String::from_utf8(rewritten.kept).unwrap();
        assert!(sent.contains("In-Reply-To: <q1@example.test>"));
        assert!(sent.contains("Yes, it does.\r\n\r\nOn "));
        assert!(sent.contains("> Does it work for three Macs?"));
        assert!(!sent.contains(NOTE));
    }

    #[test]
    fn a_forward_kept_without_a_note_takes_one_later() {
        let mut mail = outgoing("", "");
        mail.forwards = Some(MailRef {
            account_id: Uuid::nil(),
            folder: Some("archive".into()),
            uid: 3,
        });
        mail.files = vec![Attach::Kept { index: 0 }];
        let written = write(
            from(),
            &mail,
            &Below::Forwards(MAIL.to_vec()),
            &[],
            Purpose::Draft,
        )
        .unwrap();
        let draft = read(Uuid::nil(), &written.kept).unwrap();
        assert_eq!(draft.text, "");
        assert_eq!(draft.to, "");
        assert!(draft.forwards.is_some());

        let mut again = mail.clone();
        again.to = "kim@example.test".into();
        again.text = "For you.".into();
        let sent = write(
            from(),
            &again,
            &Below::Drafted(written.kept),
            &[],
            Purpose::Send,
        )
        .unwrap();
        let sent = String::from_utf8(sent.kept).unwrap();
        assert!(sent.contains("For you.\r\n\r\n---------- Forwarded message ----------\r\n"));
    }

    #[test]
    fn another_apps_draft_is_all_yours() {
        let raw = b"From: Michael <michael@lazarski.test>\r\n\
To: \"Weber, Anna\" <anna@example.test>\r\n\
Subject: Plans\r\n\
Message-ID: <d1@lazarski.test>\r\n\
\r\n\
Hi Anna,\r\n\r\n> quoted by hand\r\n";
        let draft = read(Uuid::nil(), raw).unwrap();
        assert_eq!(draft.to, "\"Weber, Anna\" <anna@example.test>");
        assert_eq!(draft.text, "Hi Anna,\n\n> quoted by hand");
        assert_eq!(draft.draft.message_id, "<d1@lazarski.test>");
    }

    #[test]
    fn your_words_end_where_the_note_says() {
        assert_eq!(
            split("Hi.\n\nOn Mon, Anna wrote:\n> ü", 23),
            ("Hi.".into(), "On Mon, Anna wrote:\n> ü".into())
        );
        assert_eq!(split("quoted", 99), (String::new(), "quoted".into()));
        assert_eq!(split("mine", 0), ("mine".into(), String::new()));
    }
}
