//! Sending what you wrote ([`Outgoing`]) with `POST /api/w/{workspace}/mails`:
//! a new mail (C), a reply (R, or A to all) or a forward (F). It goes over
//! SMTP from the account you picked, to To, Cc and Bcc. Then a copy goes
//! to that account's Sent, Bcc kept, and the mail it answers is marked
//! answered. A reply goes in its mail's thread, quoting it below; a forward
//! writes its mail out below.
//!
//! What you write is kept in Drafts as you go (`drafts.rs`), and sending
//! it deletes its draft there. While the draft is in Drafts, what's below
//! your words, and the thread it goes in, come from it.
//!
//! Its files are named in `files`: one of the forwarded mail's or of its
//! draft's, by its place, or one you added. A file you add comes first, alone, with
//! `POST /uploads`, and waits in memory, for you only, until its mail goes
//! or a day has passed. Each file holds up to 25 MB, and a mail's files
//! together as much.
//!
//! SMTP is the account's IMAP server (`imap.` read as `smtp.`) on 465, with
//! TLS, and the same user name and password. The stand-ins at 127.0.0.1 are
//! Mailpit's, on 3025 without TLS: nothing leaves this Mac.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Local};
use lettre::address::Envelope;
use lettre::message::header::{Bcc, Cc, ContentType, To};
use lettre::message::{Attachment, Mailbox, Mailboxes, MessageBuilder, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use mail_parser::MessageParser;
use postrider_domain::{Attach, MailAccount, NewUpload, Outgoing, Upload, Validate, signed};
use uuid::Uuid;

use crate::auth::Member;
use crate::drafts::{self, DraftNote};
use crate::error::AppError;
use crate::imap::{Failure, Place};
use crate::mail::Mail;
use crate::message;
use crate::outbox::Posted;
use crate::state::AppState;

/// Where the SMTP stand-in listens, beside the IMAP one.
const STAND_IN_PORT: u16 = 3025;

/// How long a server may take to answer while sending.
const TIMEOUT: Duration = Duration::from_secs(30);

/// How much of the mail a reply quotes below it, in characters.
const QUOTED: usize = 20_000;

/// The most a file may hold, and a mail's files together.
const FILE_BYTES: usize = 25_000_000;

/// How long a file you added waits for its mail.
const UPLOAD_KEPT: Duration = Duration::from_secs(24 * 60 * 60);

/// The most that added files may hold together while they wait.
const WAITING_BYTES: usize = 500_000_000;

/// How much an upload's request may hold: a file at its most, in base64.
pub const UPLOAD_BODY: usize = FILE_BYTES / 3 * 4 + 1_000_000;

/// A file you added to a mail you're writing, waiting for it to go.
pub(crate) struct Uploaded {
    workspace_id: Uuid,
    user_id: Uuid,
    file: message::File,
    at: Instant,
}

/// What a mail you write carries below your words.
pub(crate) enum Below {
    Nothing,
    /// The source of the mail it answers: quoted, in its thread.
    Answers(Vec<u8>),
    /// The source of the mail it forwards: written out.
    Forwards(Vec<u8>),
    /// The source of its draft: what's below your words there, in the
    /// thread it's in.
    Drafted(Vec<u8>),
}

/// Whether a mail is written to go, or to be kept in Drafts as it stands:
/// whom it goes to not yet said, or half typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    Send,
    Draft,
}

/// What a mail you write takes besides your words.
pub(crate) struct Gathered {
    pub below: Below,
    pub files: Vec<message::File>,
    /// The mail it answers, where it is: marked answered once it's sent.
    pub answered: Option<(MailAccount, Place, i64)>,
}

/// A mail written: as it goes out, and as it's kept, with its Bcc.
pub(crate) struct Written {
    message: lettre::Message,
    pub kept: Vec<u8>,
    pub message_id: String,
}

impl Mail {
    /// Sends `outgoing` from the account it names. While its server can't
    /// be reached, it waits in the outbox (`outbox.rs`) and goes once it
    /// can; either way its copy shows in Sent at once.
    pub async fn send_mail(
        self: &Arc<Self>,
        workspace_id: Uuid,
        user_id: Uuid,
        outgoing: &Outgoing,
    ) -> Result<(), AppError> {
        outgoing.validate()?;
        let sender = self.account(workspace_id, outgoing.from_account).await?;
        let from = self.sender(&sender, user_id).await?;
        let gathered = self.gather(workspace_id, user_id, outgoing).await?;
        // Signed as it goes, by the account it goes from: its draft keeps
        // only your words, so picking another account signs with its.
        let signed = Outgoing {
            text: signed(&outgoing.text, &sender.signature),
            ..outgoing.clone()
        };
        let written = write(
            from,
            &signed,
            &gathered.below,
            &gathered.files,
            Purpose::Send,
        )?;
        let password = self
            .password(&sender)
            .await
            .map_err(|failure| AppError::refused(failure.message()))?;
        let envelope = written.message.envelope().clone();
        let raw = written.message.formatted();
        let unsent = match deliver(&sender, password, &envelope, &raw).await {
            Ok(()) => None,
            Err(Unsent::Waits(why)) => {
                tracing::info!(address = %sender.address, "couldn't send a mail, so it waits: {why}");
                Some((envelope, raw))
            }
            Err(Unsent::Refused(why)) => return Err(AppError::refused(why)),
        };
        self.forget_uploads(&outgoing.files);

        // Sent or waiting, it's done here: its copy in Sent, its draft
        // gone and the mail it answers marked follow, now or once its
        // server can be reached.
        let replaces = self.replaced(outgoing.draft.as_ref()).await?;
        let answered = gathered
            .answered
            .map(|(account, place, uid)| (account.id, place.path, uid));
        self.post(
            &sender,
            Posted::Send { unsent, answered },
            &written,
            replaces,
        )
        .await?;
        let (mail, account) = (Arc::clone(self), sender);
        tokio::spawn(async move { mail.send_waiting(&account).await });
        Ok(())
    }

    /// What `outgoing` takes besides your words. While its draft is in
    /// Drafts, what's below them and its thread come from there; else from
    /// the mail it answers or forwards.
    pub(crate) async fn gather(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        outgoing: &Outgoing,
    ) -> Result<Gathered, AppError> {
        let refused = |failure: Failure| AppError::refused(failure.message());
        let mut draft_raw = None;
        if let Some(draft) = &outgoing.draft {
            let account = self.account(workspace_id, draft.account_id).await?;
            match self.draft_source(&account, &draft.message_id).await {
                Ok(raw) => draft_raw = raw,
                // Out of reach, what's below your words comes from the
                // mail it answers, as before it was kept.
                Err(Failure::Offline(_)) => {}
                Err(failure) => return Err(refused(failure)),
            }
        }
        let (mut answered, mut answers_raw, mut forwards_raw) = (None, None, None);
        if let Some(mail) = &outgoing.answers {
            let account = self.account(workspace_id, mail.account_id).await?;
            if draft_raw.is_some() {
                // Its draft quotes it already: it's only found, to be
                // marked answered.
                let place = match mail.folder.as_deref() {
                    None => Some(Place::inbox()),
                    Some(key) => self.place(&account, key).await.ok().flatten(),
                };
                answered = place.map(|place| (account, place, mail.uid));
            } else {
                let (place, raw) = self
                    .source_of(&account, mail.folder.as_deref(), mail.uid)
                    .await?;
                answered = Some((account, place, mail.uid));
                answers_raw = Some(raw);
            }
        } else if let Some(mail) = &outgoing.forwards {
            let keeps = outgoing
                .files
                .iter()
                .any(|file| matches!(file, Attach::Kept { .. }));
            if draft_raw.is_none() || keeps {
                let account = self.account(workspace_id, mail.account_id).await?;
                let (_, raw) = self
                    .source_of(&account, mail.folder.as_deref(), mail.uid)
                    .await?;
                forwards_raw = Some(raw);
            }
        }
        let files = self.files_for(
            workspace_id,
            user_id,
            &outgoing.files,
            forwards_raw.as_deref(),
            draft_raw.as_deref(),
        )?;
        let below = match (draft_raw, answers_raw, forwards_raw) {
            (Some(raw), _, _) => Below::Drafted(raw),
            (None, Some(raw), _) => Below::Answers(raw),
            (None, None, Some(raw)) => Below::Forwards(raw),
            (None, None, None) => Below::Nothing,
        };
        Ok(Gathered {
            below,
            files,
            answered,
        })
    }

    /// Keeps a file you added until its mail goes, or a day has passed.
    pub fn add_upload(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        new: &NewUpload,
    ) -> Result<Upload, AppError> {
        let name = file_name(&new.name);
        let bytes = STANDARD
            .decode(new.data.as_bytes())
            .map_err(|_| AppError::invalid(format!("{name} didn't arrive whole. Add it again.")))?;
        if bytes.len() > FILE_BYTES {
            return Err(AppError::invalid(format!(
                "{name} holds {}; a mail takes files up to {}.",
                megabytes(bytes.len()),
                megabytes(FILE_BYTES)
            )));
        }
        let kind = new.content_type.trim();
        let content_type = if ContentType::parse(kind).is_ok() {
            kind.to_string()
        } else {
            "application/octet-stream".to_string()
        };
        let mut uploads = self.uploads.lock().expect("never poisoned");
        uploads.retain(|_, waiting| waiting.at.elapsed() < UPLOAD_KEPT);
        let waiting: usize = uploads.values().map(|w| w.file.bytes.len()).sum();
        if waiting + bytes.len() > WAITING_BYTES {
            return Err(AppError::refused(
                "Too many added files are waiting. Send or close a mail first.",
            ));
        }
        let (id, size) = (Uuid::now_v7(), bytes.len());
        uploads.insert(
            id,
            Uploaded {
                workspace_id,
                user_id,
                file: message::File {
                    name: name.clone(),
                    content_type,
                    bytes,
                },
                at: Instant::now(),
            },
        );
        Ok(Upload {
            id,
            name,
            size: size as i64,
        })
    }

    /// The files `files` names: kept from `kept_from`, the mail it
    /// forwards, from `drafted_from`, its draft, or added by you.
    fn files_for(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        files: &[Attach],
        kept_from: Option<&[u8]>,
        drafted_from: Option<&[u8]>,
    ) -> Result<Vec<message::File>, AppError> {
        let mut found = Vec::with_capacity(files.len());
        for attach in files {
            let file = match attach {
                Attach::Kept { index } => kept_from
                    .and_then(|raw| message::file(raw, *index as usize))
                    .ok_or_else(|| {
                        AppError::invalid("A file of the mail it forwards isn't there any more.")
                    })?,
                Attach::Drafted { index } => drafted_from
                    .and_then(|raw| message::file(raw, *index as usize))
                    .ok_or_else(|| {
                        AppError::invalid("A file of its draft isn't in Drafts any more.")
                    })?,
                Attach::Added { upload } => self
                    .uploads
                    .lock()
                    .expect("never poisoned")
                    .get(upload)
                    .filter(|w| w.workspace_id == workspace_id && w.user_id == user_id)
                    .map(|w| w.file.clone())
                    .ok_or_else(|| {
                        AppError::invalid("A file you added waited too long. Add it again.")
                    })?,
            };
            found.push(file);
        }
        let together: usize = found.iter().map(|file| file.bytes.len()).sum();
        if together > FILE_BYTES {
            return Err(AppError::invalid(format!(
                "Its files hold {} together; a mail takes up to {}.",
                megabytes(together),
                megabytes(FILE_BYTES)
            )));
        }
        Ok(found)
    }

    /// Lets go of the files you added to a mail that went.
    fn forget_uploads(&self, files: &[Attach]) {
        let mut uploads = self.uploads.lock().expect("never poisoned");
        for attach in files {
            if let Attach::Added { upload } = attach {
                uploads.remove(upload);
            }
        }
    }

    /// Whom mail from `account` comes from: you, by name, at its address.
    pub(crate) async fn sender(
        &self,
        account: &MailAccount,
        user_id: Uuid,
    ) -> Result<Mailbox, AppError> {
        let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?;
        let address = account
            .address
            .parse()
            .map_err(|_| AppError::invalid("This account's address isn't one to send from."))?;
        Ok(Mailbox::new(Some(name), address))
    }
}

/// `outgoing`, written from `from`, with what goes `below` it and `files`:
/// to go, or as a draft, which takes whom it goes to as typed so far.
pub(crate) fn write(
    from: Mailbox,
    outgoing: &Outgoing,
    below: &Below,
    files: &[message::File],
    purpose: Purpose,
) -> Result<Written, AppError> {
    let original = match below {
        Below::Answers(raw) | Below::Forwards(raw) | Below::Drafted(raw) => Some(
            MessageParser::new()
                .parse(raw)
                .ok_or_else(|| AppError::invalid("That mail can't be read any more."))?,
        ),
        Below::Nothing => None,
    };
    let [to, cc, bcc] = match purpose {
        Purpose::Send => [
            addresses(&outgoing.to)?,
            addresses(&outgoing.cc)?,
            addresses(&outgoing.bcc)?,
        ],
        Purpose::Draft => [
            addresses_so_far(&outgoing.to),
            addresses_so_far(&outgoing.cc),
            addresses_so_far(&outgoing.bcc),
        ],
    };
    if purpose == Purpose::Send
        && [&to, &cc, &bcc]
            .iter()
            .all(|list| list.iter().next().is_none())
    {
        return Err(AppError::invalid("Say whom it goes to."));
    }

    let mut subject = outgoing.subject.trim().to_string();
    if subject.is_empty()
        && let Some(original) = &original
    {
        let said = original.subject().unwrap_or_default().trim();
        subject = match below {
            Below::Forwards(_) => prefixed("Fwd: ", &["fwd:", "fw:", "wg:"], said),
            Below::Answers(_) => prefixed("Re: ", &["re:", "aw:"], said),
            _ => String::new(),
        };
    }

    let address = from.email.clone();
    let message_id = format!("<{}@{}>", Uuid::now_v7(), address.domain());
    let mut builder: MessageBuilder = lettre::Message::builder()
        .from(from)
        .subject(subject)
        .message_id(Some(message_id.clone()))
        .date_now();
    if to.iter().next().is_some() {
        builder = builder.mailbox(To::from(to));
    }
    if cc.iter().next().is_some() {
        builder = builder.mailbox(Cc::from(cc));
    }
    if bcc.iter().next().is_some() {
        builder = builder.mailbox(Bcc::from(bcc));
    }
    if purpose == Purpose::Draft {
        // A draft goes nowhere: its envelope names only you, so it can be
        // written before it says whom it goes to.
        let envelope = Envelope::new(Some(address.clone()), vec![address])
            .map_err(|e| AppError::invalid(format!("This draft can't be kept: {e}.")))?;
        builder = builder.envelope(envelope);
    }

    // Its thread: the mail it answers, or the one its draft was in.
    let thread = match (below, &original) {
        (Below::Answers(_), Some(original)) => original.message_id().map(|id| {
            let mut thread: Vec<String> = original
                .references()
                .map(|list| list.iter().map(|id| format!("<{id}>")).collect())
                .unwrap_or_default();
            thread.push(format!("<{id}>"));
            (format!("<{id}>"), thread.join(" "))
        }),
        (Below::Drafted(_), Some(draft)) => draft.in_reply_to().and_then(|answers| {
            let answers = answers.first()?;
            let thread: Vec<String> = draft
                .references()
                .map(|list| list.iter().map(|id| format!("<{id}>")).collect())
                .unwrap_or_else(|| vec![format!("<{answers}>")]);
            Some((format!("<{answers}>"), thread.join(" ")))
        }),
        _ => None,
    };
    if let Some((answers, references)) = thread {
        builder = builder.in_reply_to(answers).references(references);
    }

    let text = lf(outgoing.text.trim_end());
    let under = match (below, &original) {
        (Below::Answers(_), Some(original)) => quote(original),
        (Below::Forwards(_), Some(original)) => forwarded(original),
        (Below::Drafted(_), Some(draft)) => drafts::below(draft),
        _ => String::new(),
    };
    let under = lf(under.trim_start_matches(['\r', '\n']).trim_end());
    // With nothing of yours, the mail starts with what's below.
    let body = match (text.is_empty(), under.is_empty()) {
        (_, true) => text,
        (true, false) => format!("{under}\n"),
        (false, false) => format!("{text}\n\n{under}\n"),
    };
    if purpose == Purpose::Draft {
        let note = DraftNote {
            to: outgoing.to.clone(),
            cc: outgoing.cc.clone(),
            bcc: outgoing.bcc.clone(),
            below: under.chars().count(),
            answers: outgoing.answers.clone(),
            forwards: outgoing.forwards.clone(),
        };
        builder = builder.header(note.header());
    }

    let built = |builder: MessageBuilder| {
        let text = SinglePart::builder()
            .header(ContentType::TEXT_PLAIN)
            .body(body.clone());
        let built = if files.is_empty() {
            builder.singlepart(text)
        } else {
            let mut parts = MultiPart::mixed().singlepart(text);
            for file in files {
                let kind = ContentType::parse(&file.content_type)
                    .or_else(|_| ContentType::parse("application/octet-stream"))
                    .expect("application/octet-stream is a type");
                parts = parts
                    .singlepart(Attachment::new(file.name.clone()).body(file.bytes.clone(), kind));
            }
            builder.multipart(parts)
        };
        built.map_err(|e| AppError::invalid(format!("This mail can't be sent: {e}.")))
    };
    Ok(Written {
        message: built(builder.clone())?,
        kept: built(builder.keep_bcc())?.formatted(),
        message_id,
    })
}

/// Text with its line breaks as `\n` alone, as a draft's is read back.
pub(crate) fn lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "")
}

/// `subject` with `prefix` before it, unless it starts with one of `seen`
/// already.
fn prefixed(prefix: &str, seen: &[&str], subject: &str) -> String {
    let lower = subject.to_lowercase();
    if seen.iter().any(|start| lower.starts_with(start)) {
        subject.to_string()
    } else {
        format!("{prefix}{subject}")
    }
}

/// The mail a reply answers, quoted below it: "On …, Anna wrote:", then
/// its text, each line after "> ".
fn quote(original: &mail_parser::Message<'_>) -> String {
    let said: String = text_of(original).chars().take(QUOTED).collect();
    if said.trim().is_empty() {
        return String::new();
    }
    let who = original
        .reply_to()
        .and_then(|list| list.first())
        .or_else(|| original.from().and_then(|list| list.first()))
        .map(|mailbox| named(&mailbox))
        .unwrap_or_default();
    let when = when(original)
        .map(|date| format!("On {date}, "))
        .unwrap_or_default();
    let mut quoted = format!("\n\n{when}{who} wrote:\n");
    for line in said.trim_end().lines() {
        quoted.push('>');
        if !line.is_empty() && !line.starts_with('>') {
            quoted.push(' ');
        }
        quoted.push_str(line);
        quoted.push('\n');
    }
    quoted
}

/// The mail a forward carries, written out below it: who sent it, when,
/// its subject, whom it went to, and its text.
fn forwarded(original: &mail_parser::Message<'_>) -> String {
    let mut out = String::from("\n\n---------- Forwarded message ----------\n");
    let people = |list: Option<mail_parser::AddressList<'_>>| {
        list.map(|list| {
            list.mailboxes()
                .map(|mailbox| named(&mailbox))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|people| !people.is_empty())
    };
    if let Some(from) = people(original.from()) {
        out.push_str(&format!("From: {from}\n"));
    }
    if let Some(date) = when(original) {
        out.push_str(&format!("Date: {date}\n"));
    }
    out.push_str(&format!(
        "Subject: {}\n",
        original.subject().unwrap_or_default().trim()
    ));
    if let Some(to) = people(original.to()) {
        out.push_str(&format!("To: {to}\n"));
    }
    if let Some(cc) = people(original.cc()) {
        out.push_str(&format!("Cc: {cc}\n"));
    }
    out.push('\n');
    out.push_str(text_of(original).trim_end());
    out.push('\n');
    out
}

/// A mail's text: its text parts, or its HTML read as text.
fn text_of(original: &mail_parser::Message<'_>) -> String {
    original.text_bodies().collect::<Vec<_>>().join("\n\n")
}

/// When a mail was sent, in this Mac's time: "Mon, 3 Nov 2025 at 08:12".
fn when(original: &mail_parser::Message<'_>) -> Option<String> {
    original
        .date()
        .and_then(|date| DateTime::from_timestamp(date.to_timestamp(), 0))
        .map(|date| {
            date.with_timezone(&Local)
                .format("%a, %-d %b %Y at %H:%M")
                .to_string()
        })
}

/// A file's name as it goes: its last part, without what a name can't
/// hold, or "File".
fn file_name(given: &str) -> String {
    let last = given.rsplit(['/', '\\']).next().unwrap_or_default();
    let name: String = last.chars().filter(|c| !c.is_control()).take(200).collect();
    match name.trim() {
        "" | "." | ".." => "File".to_string(),
        name => name.to_string(),
    }
}

/// `size` bytes as people read them: "3.2 MB".
fn megabytes(size: usize) -> String {
    format!("{:.1} MB", size as f64 / 1_000_000.0)
}

/// Someone as a mail names them: "Anna Weber <anna@example.test>", or the
/// address alone.
fn named(mailbox: &mail_parser::Mailbox<'_>) -> String {
    let address = mailbox.address().unwrap_or_default();
    match mailbox.name().map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("{name} <{address}>"),
        None => address.to_string(),
    }
}

/// The addresses in what you typed in To, Cc or Bcc: "anna@example.test",
/// or "Anna Weber <anna@example.test>", with commas or semicolons between.
/// One that isn't an address stops it, named, rather than going missing.
fn addresses(typed: &str) -> Result<Mailboxes, AppError> {
    let mut to = Mailboxes::new();
    for part in parts(typed) {
        match address(&part) {
            Some(mailbox) => to.push(mailbox),
            None => {
                return Err(AppError::invalid(format!(
                    "\u{201c}{part}\u{201d} isn't an address to send to."
                )));
            }
        }
    }
    Ok(to)
}

/// The addresses typed so far, as a draft keeps them: those that are
/// whole. The rest stays in its note, as typed.
fn addresses_so_far(typed: &str) -> Mailboxes {
    parts(typed)
        .iter()
        .filter_map(|part| address(part))
        .collect::<Vec<_>>()
        .into()
}

/// One address as typed: `None` when it isn't one, whole.
fn address(part: &str) -> Option<Mailbox> {
    // The parser stops where an address ends: what's left must be none.
    let whole = if part.contains('<') {
        part.ends_with('>')
    } else {
        !part.contains(char::is_whitespace)
    };
    part.parse::<Mailbox>().ok().filter(|_| whole)
}

/// What's typed in To, Cc or Bcc, split at the commas and semicolons
/// between addresses: not those in a quoted name or within `<>`.
fn parts(typed: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut part, mut quoted, mut angled) = (String::new(), false, false);
    for c in typed.chars() {
        match c {
            '"' => quoted = !quoted,
            '<' if !quoted => angled = true,
            '>' if !quoted => angled = false,
            ',' | ';' if !quoted && !angled => {
                parts.push(std::mem::take(&mut part));
                continue;
            }
            _ => {}
        }
        part.push(c);
    }
    parts.push(part);
    parts
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Why a mail didn't go.
pub(crate) enum Unsent {
    /// Its server can't be reached, or can't take it right now: it waits.
    Waits(String),
    /// Its server refused it, or the account's password.
    Refused(String),
}

/// Sends `message` over the account's SMTP server.
pub(crate) async fn send(
    account: &MailAccount,
    password: String,
    message: lettre::Message,
) -> Result<(), AppError> {
    deliver(account, password, message.envelope(), &message.formatted())
        .await
        .map_err(|unsent| match unsent {
            Unsent::Waits(why) | Unsent::Refused(why) => AppError::refused(why),
        })
}

/// Sends `raw` over the account's SMTP server, to whom `envelope` names.
pub(crate) async fn deliver(
    account: &MailAccount,
    password: String,
    envelope: &Envelope,
    raw: &[u8],
) -> Result<(), Unsent> {
    let credentials = Credentials::new(account.username.clone(), password);
    let host = smtp_host(&account.host);
    let transport = if is_this_mac(&host) {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
            .port(STAND_IN_PORT)
            .credentials(credentials)
            .timeout(Some(TIMEOUT))
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .map_err(|e| Unsent::Refused(format!("Couldn't reach {host} to send it: {e}.")))?
            .credentials(credentials)
            .timeout(Some(TIMEOUT))
            .build()
    };
    transport
        .send_raw(envelope, raw)
        .await
        .map(drop)
        .map_err(|e| {
            if e.status().is_some_and(|code| code.to_string() == "535") {
                Unsent::Refused(format!(
                    "{host} didn't take {}'s password for sending.",
                    account.address
                ))
            } else if e.is_permanent() {
                Unsent::Refused(format!("{host} refused it: {e}."))
            } else if e.is_transient() {
                Unsent::Waits(format!(
                    "{host} can't take it right now: {e}. Try again in a moment."
                ))
            } else {
                Unsent::Waits(format!("Couldn't reach {host} to send it: {e}."))
            }
        })
}

/// The account's SMTP server: its IMAP server, `imap.` read as `smtp.`.
fn smtp_host(imap_host: &str) -> String {
    match imap_host.strip_prefix("imap.") {
        Some(rest) => format!("smtp.{rest}"),
        None => imap_host.to_string(),
    }
}

pub(crate) fn is_this_mac(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// `POST /api/w/{workspace}/uploads`: keeps a file you add to a mail.
pub async fn upload(
    State(state): State<AppState>,
    member: Member,
    Json(new): Json<NewUpload>,
) -> Result<Json<Upload>, AppError> {
    Ok(Json(state.mail.add_upload(
        member.actor.workspace_id,
        member.actor.user_id,
        &new,
    )?))
}

/// `POST /api/w/{workspace}/mails`: sends what you wrote.
pub async fn send_mail(
    State(state): State<AppState>,
    member: Member,
    Json(outgoing): Json<Outgoing>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .send_mail(member.actor.workspace_id, member.actor.user_id, &outgoing)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIL: &[u8] = b"From: Anna Weber <anna@example.test>\r\n\
Reply-To: Team <team@example.test>\r\n\
To: Michael <michael@lazarski.test>\r\n\
Subject: Layer for our team?\r\n\
Date: Mon, 3 Nov 2025 08:12:00 +0100\r\n\
Message-ID: <q1@example.test>\r\n\
References: <q0@example.test>\r\n\
\r\n\
Does it work for three Macs?\r\n\
> earlier words\r\n";

    fn from() -> Mailbox {
        "Michael Lazarski <michael@lazarski.test>".parse().unwrap()
    }

    fn outgoing(to: &str, text: &str) -> Outgoing {
        Outgoing {
            from_account: Uuid::nil(),
            to: to.into(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            text: text.into(),
            answers: None,
            forwards: None,
            files: Vec::new(),
            draft: None,
        }
    }

    fn sent(written: Written) -> String {
        String::from_utf8(written.message.formatted()).unwrap()
    }

    #[test]
    fn a_reply_quotes_its_mail_in_the_thread() {
        let mail = outgoing("Team <team@example.test>", "Yes, it does.");
        let sent = sent(
            write(
                from(),
                &mail,
                &Below::Answers(MAIL.to_vec()),
                &[],
                Purpose::Send,
            )
            .unwrap(),
        );
        assert!(sent.contains("To: Team <team@example.test>"), "{sent}");
        assert!(sent.contains("Subject: Re: Layer for our team?"), "{sent}");
        assert!(sent.contains("In-Reply-To: <q1@example.test>"), "{sent}");
        assert!(
            sent.contains("References: <q0@example.test> <q1@example.test>"),
            "{sent}"
        );
        assert!(sent.contains("@lazarski.test>\r\n"), "{sent}");
        assert!(sent.contains("Yes, it does."), "{sent}");
        assert!(sent.contains("Team <team@example.test> wrote:"), "{sent}");
        assert!(sent.contains("> Does it work for three Macs?"), "{sent}");
        assert!(sent.contains(">> earlier words"), "{sent}");
    }

    #[test]
    fn a_subject_already_answered_stays() {
        let mail = String::from_utf8_lossy(MAIL).replace("Subject: Layer", "Subject: AW: Layer");
        let reply = outgoing("anna@example.test", "Ja.");
        let sent = sent(
            write(
                from(),
                &reply,
                &Below::Answers(mail.into_bytes()),
                &[],
                Purpose::Send,
            )
            .unwrap(),
        );
        assert!(sent.contains("Subject: AW: Layer for our team?"), "{sent}");
    }

    #[test]
    fn cc_shows_and_bcc_only_in_sent() {
        let mut mail = outgoing("", "Hi all.");
        mail.cc = "Anna Weber <anna@example.test>, kim@example.test".into();
        mail.bcc = "boss@example.test".into();
        let written = write(from(), &mail, &Below::Nothing, &[], Purpose::Send).unwrap();
        let kept = String::from_utf8(written.kept.clone()).unwrap();
        let recipients: Vec<String> = written
            .message
            .envelope()
            .to()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            recipients,
            ["anna@example.test", "kim@example.test", "boss@example.test"]
        );
        let sent = sent(written);
        assert!(
            sent.contains("Cc: \"Anna Weber\" <anna@example.test>, kim@example.test\r\n"),
            "{sent}"
        );
        assert!(!sent.contains("To:"), "{sent}");
        assert!(!sent.contains("boss@"), "{sent}");
        assert!(kept.contains("Bcc: boss@example.test\r\n"), "{kept}");
    }

    #[test]
    fn a_forward_writes_its_mail_out_with_its_files() {
        let pdf = format!(
            "{}Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\n\
             Content-Type: text/plain\r\n\r\nThe invoice.\r\n--b\r\n\
             Content-Type: application/pdf; name=Invoice.pdf\r\n\
             Content-Disposition: attachment; filename=Invoice.pdf\r\n\
             Content-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n--b--\r\n",
            String::from_utf8_lossy(MAIL)
                .split("\r\n\r\n")
                .next()
                .unwrap()
                .to_string()
                + "\r\n"
        );
        let mut mail = outgoing("kim@example.test", "For you.");
        mail.forwards = Some(postrider_domain::MailRef {
            account_id: Uuid::nil(),
            folder: None,
            uid: 1,
        });
        let files = vec![message::file(pdf.as_bytes(), 0).unwrap()];
        let sent = sent(
            write(
                from(),
                &mail,
                &Below::Forwards(pdf.into_bytes()),
                &files,
                Purpose::Send,
            )
            .unwrap(),
        );
        assert!(sent.contains("Subject: Fwd: Layer for our team?"), "{sent}");
        assert!(!sent.contains("In-Reply-To"), "{sent}");
        assert!(sent.contains("For you."), "{sent}");
        assert!(
            sent.contains("---------- Forwarded message ----------"),
            "{sent}"
        );
        assert!(
            sent.contains("From: Anna Weber <anna@example.test>"),
            "{sent}"
        );
        assert!(sent.contains("The invoice."), "{sent}");
        assert!(sent.contains("filename=\"Invoice.pdf\""), "{sent}");
        assert!(sent.contains("%PDF-"), "{sent}");

        mail.text = String::new();
        let pdf = String::from_utf8_lossy(MAIL).into_owned();
        let sent = super::tests::sent(
            write(
                from(),
                &mail,
                &Below::Forwards(pdf.into_bytes()),
                &[],
                Purpose::Send,
            )
            .unwrap(),
        );
        assert!(sent.contains("\r\n\r\n---------- Forwarded"), "{sent}");
        assert!(!sent.contains("Invoice.pdf"), "{sent}");
    }

    /// Mail with no database: adding files needs none.
    fn mail() -> Mail {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://127.0.0.1:1/none")
            .unwrap();
        Mail::new(pool, crate::secrets::Secrets::memory())
    }

    #[tokio::test]
    async fn an_added_file_goes_once_and_only_for_whom_added_it() {
        let mail = mail();
        let (workspace, you) = (Uuid::now_v7(), Uuid::now_v7());
        let new = NewUpload {
            name: "../Plans/Q4 plan.pdf".into(),
            content_type: "not a type".into(),
            data: STANDARD.encode(b"%PDF-1.7"),
        };
        let upload = mail.add_upload(workspace, you, &new).unwrap();
        assert_eq!((upload.name.as_str(), upload.size), ("Q4 plan.pdf", 8));
        let files = [Attach::Added { upload: upload.id }];

        assert!(
            mail.files_for(workspace, Uuid::now_v7(), &files, None, None)
                .is_err()
        );
        let found = mail.files_for(workspace, you, &files, None, None).unwrap();
        assert_eq!(found[0].content_type, "application/octet-stream");
        let mut written = outgoing("kim@example.test", "");
        written.files = files.to_vec();
        let sent = sent(write(from(), &written, &Below::Nothing, &found, Purpose::Send).unwrap());
        assert!(sent.contains("filename=\"Q4 plan.pdf\""), "{sent}");

        mail.forget_uploads(&files);
        assert!(mail.files_for(workspace, you, &files, None, None).is_err());
        let kept = [Attach::Kept { index: 0 }];
        assert!(mail.files_for(workspace, you, &kept, None, None).is_err());
    }

    #[tokio::test]
    async fn a_file_too_big_is_said() {
        let mail = mail();
        let new = NewUpload {
            name: "film.mov".into(),
            content_type: "video/quicktime".into(),
            data: STANDARD.encode(vec![0; FILE_BYTES + 1]),
        };
        let Err(error) = mail.add_upload(Uuid::nil(), Uuid::nil(), &new) else {
            panic!("a file too big was taken");
        };
        assert!(
            format!("{error:?}").contains("film.mov holds 25.0 MB"),
            "{error:?}"
        );
    }

    #[test]
    fn a_new_mail_goes_to_every_address() {
        let mut mail = outgoing(
            "Anna Weber <anna@example.test>; \"Doe, Jo\" <jo@example.test>, kim@example.test,",
            "Hi.",
        );
        mail.subject = " Layer ".into();
        let sent = sent(write(from(), &mail, &Below::Nothing, &[], Purpose::Send).unwrap());
        // lettre quotes names, and writes one with a comma in base64.
        assert!(
            sent.contains(
                "To: \"Anna Weber\" <anna@example.test>, =?utf-8?b?RG9lLCBKbw==?= \
                 <jo@example.test>, kim@example.test\r\n"
            ),
            "{sent}"
        );
        assert!(sent.contains("Subject: Layer\r\n"), "{sent}");
        assert!(sent.contains("@lazarski.test>\r\n"), "{sent}");
    }

    #[test]
    fn nobody_to_send_to_is_said() {
        let mail = outgoing(" , ", "Hi.");
        assert!(write(from(), &mail, &Below::Nothing, &[], Purpose::Send).is_err());
    }

    #[test]
    fn what_isnt_an_address_is_named() {
        for typed in ["anna", "anna@example.test kim", "Anna <anna@example.test"] {
            assert!(addresses(typed).is_err(), "{typed}");
        }
        assert_eq!(
            addresses("a@example.test;b@example.test")
                .unwrap()
                .iter()
                .count(),
            2
        );
    }

    #[test]
    fn smtp_is_the_imap_server() {
        assert_eq!(smtp_host("imap.example.test"), "smtp.example.test");
        assert_eq!(smtp_host("styx.uberspace.de"), "styx.uberspace.de");
        assert!(is_this_mac(&smtp_host("127.0.0.1")));
    }
}
