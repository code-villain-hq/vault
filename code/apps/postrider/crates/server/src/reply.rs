//! Replying, with R: `POST /api/w/{workspace}/mails/{account}/{uid}/reply`
//! takes what you wrote ([`Reply`]) and sends it over SMTP from the account
//! the mail came to, or the one you picked, to whom it asks replies to go.
//! Then a copy goes to that account's Sent and the mail is marked answered.
//! A new mail, with C, is `POST /api/w/{workspace}/mails` ([`NewMail`]),
//! sent the same way.
//!
//! SMTP is the account's IMAP server (`imap.` read as `smtp.`) on 465, with
//! TLS, and the same user name and password. The stand-ins at 127.0.0.1 are
//! Mailpit's, on 3025 without TLS: nothing leaves this Mac.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Local};
use lettre::message::header::ContentType;
use lettre::message::{Mailbox, Mailboxes, MessageBuilder};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use mail_parser::MessageParser;
use postrider_domain::{FolderRole, MailAccount, NewMail, Reply, Validate};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap;
use crate::mail::Mail;
use crate::state::AppState;

/// Where the SMTP stand-in listens, beside the IMAP one.
const STAND_IN_PORT: u16 = 3025;

/// How long a server may take to answer while sending.
const TIMEOUT: Duration = Duration::from_secs(30);

/// How much of the mail a reply quotes below it, in characters.
const QUOTED: usize = 20_000;

impl Mail {
    /// Sends `reply` to the mail `uid` of the account `account_id`.
    pub async fn reply(
        self: &Arc<Self>,
        workspace_id: Uuid,
        user_id: Uuid,
        account_id: Uuid,
        uid: i64,
        reply: &Reply,
    ) -> Result<(), AppError> {
        reply.validate()?;
        let account = self.account(workspace_id, account_id).await?;
        let sender = match reply.from_account {
            Some(id) if id != account_id => self.account(workspace_id, id).await?,
            _ => account.clone(),
        };
        let (place, raw) = self
            .source_of(&account, reply.folder.as_deref(), uid)
            .await?;
        let from = self.sender(&sender, user_id).await?;
        let message = compose(&raw, from, &reply.text)?;
        let password = self
            .password(&sender)
            .await
            .map_err(|failure| AppError::refused(failure.message()))?;
        send(&sender, password, message.clone()).await?;

        // Sent, it's done: the copy and the mark follow without waiting.
        let mail = Arc::clone(self);
        tokio::spawn(async move {
            mail.file_sent(&sender, message.formatted()).await;
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

    /// Sends `mail`, a new one, from the account it names.
    pub async fn send_new(
        self: &Arc<Self>,
        workspace_id: Uuid,
        user_id: Uuid,
        mail: &NewMail,
    ) -> Result<(), AppError> {
        mail.validate()?;
        let sender = self.account(workspace_id, mail.from_account).await?;
        let from = self.sender(&sender, user_id).await?;
        let message = write_new(from, &mail.to, &mail.subject, &mail.text)?;
        let password = self
            .password(&sender)
            .await
            .map_err(|failure| AppError::refused(failure.message()))?;
        send(&sender, password, message.clone()).await?;
        let this = Arc::clone(self);
        tokio::spawn(async move { this.file_sent(&sender, message.formatted()).await });
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
    /// apps would; its server doesn't.
    async fn file_sent(&self, account: &MailAccount, raw: Vec<u8>) {
        let filed = async {
            let sent = self.folder_for(account, FolderRole::Sent).await?;
            self.on_server(account, |session| {
                let (path, raw) = (sent.path.clone(), raw.clone());
                Box::pin(async move { imap::put(session, &path, &raw).await })
            })
            .await
        };
        if let Err(failure) = filed.await {
            tracing::info!(address = %account.address, "couldn't put it in Sent: {}", failure.message());
        }
    }
}

/// The reply to the mail `raw`, from `from`: to its Reply-To, else its
/// sender; "Re: " its subject; in its thread; `text` with the mail
/// quoted below.
fn compose(raw: &[u8], from: Mailbox, text: &str) -> Result<lettre::Message, AppError> {
    let original = MessageParser::new()
        .parse(raw)
        .ok_or_else(|| AppError::invalid("This mail can't be read, so it can't be answered."))?;
    let to = original
        .reply_to()
        .and_then(|list| list.first())
        .or_else(|| original.from().and_then(|list| list.first()))
        .and_then(|mailbox| {
            let address = mailbox.address()?.parse().ok()?;
            let name = mailbox.name().map(str::trim).filter(|n| !n.is_empty());
            Some(Mailbox::new(name.map(str::to_string), address))
        })
        .ok_or_else(|| AppError::invalid("This mail doesn't say who it's from."))?;

    let subject = original.subject().unwrap_or_default().trim();
    let subject = if ["re:", "aw:"].iter().any(|re| {
        subject
            .get(..3)
            .is_some_and(|start| start.eq_ignore_ascii_case(re))
    }) {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    };

    let domain = from.email.domain().to_string();
    let mut builder: MessageBuilder = lettre::Message::builder()
        .from(from)
        .to(to.clone())
        .subject(subject)
        .message_id(Some(format!("<{}@{domain}>", Uuid::now_v7())))
        .date_now();
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

    let mut body = text.trim_end().replace("\r\n", "\n");
    let said: String = original
        .text_bodies()
        .collect::<Vec<_>>()
        .join("\n\n")
        .chars()
        .take(QUOTED)
        .collect();
    if !said.trim().is_empty() {
        let when = original
            .date()
            .and_then(|date| DateTime::from_timestamp(date.to_timestamp(), 0))
            .map(|date| {
                format!(
                    "On {}, ",
                    date.with_timezone(&Local).format("%a, %-d %b %Y at %H:%M")
                )
            })
            .unwrap_or_default();
        body.push_str(&format!("\n\n{when}{to} wrote:\n"));
        for line in said.trim_end().lines() {
            body.push('>');
            if !line.is_empty() && !line.starts_with('>') {
                body.push(' ');
            }
            body.push_str(line);
            body.push('\n');
        }
    }
    builder
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .map_err(|e| AppError::invalid(format!("This reply can't be sent: {e}.")))
}

/// A new mail from `from` to `to`, the addresses you typed.
fn write_new(
    from: Mailbox,
    to: &str,
    subject: &str,
    text: &str,
) -> Result<lettre::Message, AppError> {
    let to = addresses(to)?;
    let domain = from.email.domain().to_string();
    lettre::Message::builder()
        .from(from)
        .mailbox(lettre::message::header::To::from(to))
        .subject(subject.trim())
        .message_id(Some(format!("<{}@{domain}>", Uuid::now_v7())))
        .date_now()
        .header(ContentType::TEXT_PLAIN)
        .body(text.trim_end().replace("\r\n", "\n"))
        .map_err(|e| AppError::invalid(format!("This mail can't be sent: {e}.")))
}

/// The addresses in what you typed in To: "anna@example.test", or "Anna
/// Weber <anna@example.test>", with commas or semicolons between. One
/// that isn't an address stops it, named, rather than going missing.
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
    if to.iter().next().is_none() {
        return Err(AppError::invalid("Say whom it goes to."));
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

/// `POST /api/w/{workspace}/mails`: sends a new mail.
pub async fn send_new(
    State(state): State<AppState>,
    member: Member,
    Json(mail): Json<NewMail>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .send_new(member.actor.workspace_id, member.actor.user_id, &mail)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/w/{workspace}/mails/{account}/{uid}/reply`: sends a reply.
pub async fn reply(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid)): Path<(Uuid, Uuid, i64)>,
    Json(reply): Json<Reply>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .reply(
            member.actor.workspace_id,
            member.actor.user_id,
            account_id,
            uid,
            &reply,
        )
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

    #[test]
    fn a_reply_goes_to_reply_to_in_the_thread() {
        let sent =
            String::from_utf8(compose(MAIL, from(), "Yes, it does.").unwrap().formatted()).unwrap();
        assert!(sent.contains("To: Team <team@example.test>"), "{sent}");
        assert!(sent.contains("Subject: Re: Layer for our team?"), "{sent}");
        assert!(sent.contains("In-Reply-To: <q1@example.test>"), "{sent}");
        assert!(
            sent.contains("References: <q0@example.test> <q1@example.test>"),
            "{sent}"
        );
        assert!(sent.contains("@lazarski.test>\r\n"), "{sent}");
        assert!(sent.contains("Yes, it does."), "{sent}");
        assert!(sent.contains("> Does it work for three Macs?"), "{sent}");
        assert!(sent.contains(">> earlier words"), "{sent}");
    }

    #[test]
    fn a_subject_already_answered_stays() {
        let mail = String::from_utf8_lossy(MAIL).replace("Subject: Layer", "Subject: AW: Layer");
        let sent = String::from_utf8(compose(mail.as_bytes(), from(), "Ja.").unwrap().formatted())
            .unwrap();
        assert!(sent.contains("Subject: AW: Layer for our team?"), "{sent}");
    }

    #[test]
    fn a_new_mail_goes_to_every_address() {
        let sent = write_new(
            from(),
            "Anna Weber <anna@example.test>; \"Doe, Jo\" <jo@example.test>, kim@example.test,",
            " Layer ",
            "Hi.",
        )
        .unwrap();
        let sent = String::from_utf8(sent.formatted()).unwrap();
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
    fn what_isnt_an_address_is_named() {
        for typed in [
            "anna",
            "anna@example.test kim",
            "Anna <anna@example.test",
            " , ",
        ] {
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
