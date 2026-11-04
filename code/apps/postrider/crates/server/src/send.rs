//! Sending what you wrote ([`Outgoing`]) with `POST /api/w/{workspace}/mails`:
//! a new mail (C), a reply (R, or A to all) or a forward (F). It goes over
//! SMTP from the account you picked, to To, Cc and Bcc. Then a copy goes
//! to that account's Sent, Bcc kept, and the mail it answers is marked
//! answered. A reply goes in its mail's thread, quoting it below; a forward
//! writes its mail out below, with its files.
//!
//! SMTP is the account's IMAP server (`imap.` read as `smtp.`) on 465, with
//! TLS, and the same user name and password. The stand-ins at 127.0.0.1 are
//! Mailpit's, on 3025 without TLS: nothing leaves this Mac.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Local};
use lettre::message::header::{Bcc, Cc, ContentType, To};
use lettre::message::{Attachment, Mailbox, Mailboxes, MessageBuilder, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use mail_parser::MessageParser;
use postrider_domain::{FolderRole, MailAccount, Outgoing, Validate};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap;
use crate::mail::Mail;
use crate::message;
use crate::state::AppState;

/// Where the SMTP stand-in listens, beside the IMAP one.
const STAND_IN_PORT: u16 = 3025;

/// How long a server may take to answer while sending.
const TIMEOUT: Duration = Duration::from_secs(30);

/// How much of the mail a reply quotes below it, in characters.
const QUOTED: usize = 20_000;

/// What a mail you write carries below your words.
enum Below {
    Nothing,
    /// The source of the mail it answers: quoted, in its thread.
    Answers(Vec<u8>),
    /// The source of the mail it forwards: written out, with its files.
    Forwards(Vec<u8>),
}

/// A mail written: as it goes out, and as it's kept, with its Bcc.
struct Written {
    message: lettre::Message,
    kept: Vec<u8>,
}

impl Mail {
    /// Sends `outgoing` from the account it names.
    pub async fn send_mail(
        self: &Arc<Self>,
        workspace_id: Uuid,
        user_id: Uuid,
        outgoing: &Outgoing,
    ) -> Result<(), AppError> {
        outgoing.validate()?;
        let sender = self.account(workspace_id, outgoing.from_account).await?;
        let from = self.sender(&sender, user_id).await?;
        let mut answered = None;
        let below = if let Some(mail) = &outgoing.answers {
            let account = self.account(workspace_id, mail.account_id).await?;
            let (place, raw) = self
                .source_of(&account, mail.folder.as_deref(), mail.uid)
                .await?;
            answered = Some((account, place, mail.uid));
            Below::Answers(raw)
        } else if let Some(mail) = &outgoing.forwards {
            let account = self.account(workspace_id, mail.account_id).await?;
            let (_, raw) = self
                .source_of(&account, mail.folder.as_deref(), mail.uid)
                .await?;
            Below::Forwards(raw)
        } else {
            Below::Nothing
        };
        let written = write(from, outgoing, &below)?;
        let password = self
            .password(&sender)
            .await
            .map_err(|failure| AppError::refused(failure.message()))?;
        send(&sender, password, written.message).await?;

        // Sent, it's done: the copy and the mark follow without waiting.
        let mail = Arc::clone(self);
        tokio::spawn(async move {
            mail.file_sent(&sender, written.kept).await;
            let Some((account, place, uid)) = answered else {
                return;
            };
            let marked = mail
                .on_server(&account, |session| {
                    let path = place.path.clone();
                    Box::pin(async move { imap::mark_answered(session, &path, uid).await })
                })
                .await;
            if let Err(failure) = marked {
                tracing::info!(address = %account.address, "couldn't mark answered: {}", failure.message());
            }
        });
        Ok(())
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

    /// Puts a sent mail in its account's Sent, as the account's other mail
    /// apps would; its server doesn't. Then the Inbox changes, so its
    /// conversation reads Sent again and shows it (`conversations::keep`).
    async fn file_sent(&self, account: &MailAccount, raw: Vec<u8>) {
        let filed = async {
            let sent = self.folder_for(account, FolderRole::Sent).await?;
            self.on_server(account, |session| {
                let (path, raw) = (sent.path.clone(), raw.clone());
                Box::pin(async move { imap::put(session, &path, &raw).await })
            })
            .await
        };
        match filed.await {
            Ok(()) => self.bump(),
            Err(failure) => {
                tracing::info!(address = %account.address, "couldn't put it in Sent: {}", failure.message())
            }
        }
    }
}

/// `outgoing`, written from `from`, with what goes `below` it.
fn write(from: Mailbox, outgoing: &Outgoing, below: &Below) -> Result<Written, AppError> {
    let original = match below {
        Below::Answers(raw) | Below::Forwards(raw) => Some(
            MessageParser::new()
                .parse(raw)
                .ok_or_else(|| AppError::invalid("That mail can't be read any more."))?,
        ),
        Below::Nothing => None,
    };
    let to = addresses(&outgoing.to)?;
    let cc = addresses(&outgoing.cc)?;
    let bcc = addresses(&outgoing.bcc)?;
    if [&to, &cc, &bcc]
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
            _ => prefixed("Re: ", &["re:", "aw:"], said),
        };
    }

    let domain = from.email.domain().to_string();
    let mut builder: MessageBuilder = lettre::Message::builder()
        .from(from)
        .subject(subject)
        .message_id(Some(format!("<{}@{domain}>", Uuid::now_v7())))
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

    let mut body = outgoing.text.trim_end().replace("\r\n", "\n");
    let mut files = Vec::new();
    match (below, &original) {
        (Below::Answers(_), Some(original)) => {
            if let Some(id) = original.message_id() {
                let mut thread: Vec<String> = original
                    .references()
                    .map(|list| list.iter().map(|id| format!("<{id}>")).collect())
                    .unwrap_or_default();
                thread.push(format!("<{id}>"));
                builder = builder
                    .in_reply_to(format!("<{id}>"))
                    .references(thread.join(" "));
            }
            body.push_str(&quote(original));
        }
        (Below::Forwards(raw), Some(original)) => {
            let forwarded = forwarded(original);
            // With no note, the mail starts with what it forwards.
            body.push_str(if body.is_empty() {
                forwarded.trim_start()
            } else {
                &forwarded
            });
            files = message::files_of(raw);
        }
        _ => {}
    }

    let built = |builder: MessageBuilder| {
        let text = SinglePart::builder()
            .header(ContentType::TEXT_PLAIN)
            .body(body.clone());
        let built = if files.is_empty() {
            builder.singlepart(text)
        } else {
            let mut parts = MultiPart::mixed().singlepart(text);
            for file in &files {
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
    })
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
    let mut to = Mailboxes::new();
    for part in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        // The parser stops where an address ends: what's left must be none.
        let whole = if part.contains('<') {
            part.ends_with('>')
        } else {
            !part.contains(char::is_whitespace)
        };
        match part.parse::<Mailbox>() {
            Ok(mailbox) if whole => to.push(mailbox),
            _ => {
                return Err(AppError::invalid(format!(
                    "\u{201c}{part}\u{201d} isn't an address to send to."
                )));
            }
        }
    }
    Ok(to)
}

/// Sends `message` over the account's SMTP server.
pub(crate) async fn send(
    account: &MailAccount,
    password: String,
    message: lettre::Message,
) -> Result<(), AppError> {
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
            .map_err(|e| AppError::refused(format!("Couldn't reach {host} to send it: {e}.")))?
            .credentials(credentials)
            .timeout(Some(TIMEOUT))
            .build()
    };
    transport.send(message).await.map(drop).map_err(|e| {
        let message = if e.status().is_some_and(|code| code.to_string() == "535") {
            format!(
                "{host} didn't take {}'s password for sending.",
                account.address
            )
        } else if e.is_permanent() {
            format!("{host} refused it: {e}.")
        } else if e.is_transient() {
            format!("{host} can't take it right now: {e}. Try again in a moment.")
        } else {
            format!("Couldn't reach {host} to send it: {e}.")
        };
        AppError::refused(message)
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
        }
    }

    fn sent(written: Written) -> String {
        String::from_utf8(written.message.formatted()).unwrap()
    }

    #[test]
    fn a_reply_quotes_its_mail_in_the_thread() {
        let mail = outgoing("Team <team@example.test>", "Yes, it does.");
        let sent = sent(write(from(), &mail, &Below::Answers(MAIL.to_vec())).unwrap());
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
        let sent = sent(write(from(), &reply, &Below::Answers(mail.into_bytes())).unwrap());
        assert!(sent.contains("Subject: AW: Layer for our team?"), "{sent}");
    }

    #[test]
    fn cc_shows_and_bcc_only_in_sent() {
        let mut mail = outgoing("", "Hi all.");
        mail.cc = "Anna Weber <anna@example.test>, kim@example.test".into();
        mail.bcc = "boss@example.test".into();
        let written = write(from(), &mail, &Below::Nothing).unwrap();
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
        let sent = sent(write(from(), &mail, &Below::Forwards(pdf.into_bytes())).unwrap());
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
        let sent =
            super::tests::sent(write(from(), &mail, &Below::Forwards(pdf.into_bytes())).unwrap());
        assert!(sent.contains("\r\n\r\n---------- Forwarded"), "{sent}");
    }

    #[test]
    fn a_new_mail_goes_to_every_address() {
        let mut mail = outgoing(
            "Anna Weber <anna@example.test>; \"Doe, Jo\" <jo@example.test>, kim@example.test,",
            "Hi.",
        );
        mail.subject = " Layer ".into();
        let sent = sent(write(from(), &mail, &Below::Nothing).unwrap());
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
        assert!(write(from(), &mail, &Below::Nothing).is_err());
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
