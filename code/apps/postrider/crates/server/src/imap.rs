//! Reading an account's Inbox over IMAP: signing in, keeping a copy of its
//! newest mail in `inbox_mails`, and listening for more with IDLE.
//!
//! The copy holds what the list shows (sender, subject, the start of the
//! text, when it came, read or not) for the newest [`KEEP`] mails. It's
//! made again from the server whenever the server says its UIDs changed
//! meaning (UIDVALIDITY).

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::types::{Fetch, Flag};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use sqlx::PgPool;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use uuid::Uuid;

/// How many of an Inbox's newest mails Postrider keeps a copy of.
pub const KEEP: u32 = 200;

/// How much of each mail it reads: the header and the start of the text.
const PEEK: u32 = 16 * 1024;

/// How many mails it asks for at once.
const BATCH: usize = 50;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub type Session = async_imap::Session<Stream>;

/// Why an account can't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The server can't be reached, or the connection broke: try again later.
    Offline(String),
    /// The server said no, as to a wrong password: trying again won't help.
    Refused(String),
}

impl Failure {
    /// What went wrong, as a sentence for the account's line: capitalised,
    /// with its full stop, whoever worded it. A server's name keeps its case.
    pub fn message(&self) -> String {
        let (Failure::Offline(message) | Failure::Refused(message)) = self;
        let message = message.trim();
        let first_word = message.split_whitespace().next().unwrap_or_default();
        let mut chars = message.chars();
        let mut sentence: String = match chars.next() {
            Some(_) if first_word.trim_end_matches(':').contains('.') => message.to_string(),
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => return "Something went wrong.".into(),
        };
        if !sentence.ends_with(['.', '!', '?']) {
            sentence.push('.');
        }
        sentence
    }
}

impl From<async_imap::error::Error> for Failure {
    fn from(e: async_imap::error::Error) -> Self {
        let message = e.to_string();
        // An answer the parser can't read comes back whole, in bytes: that
        // goes to the log, not to the account's line.
        if message.contains(" during parsing of ") {
            tracing::warn!("unreadable IMAP answer: {message}");
            return Failure::Offline("its server answered in a way Postrider can't read.".into());
        }
        Failure::Offline(message)
    }
}

impl From<sqlx::Error> for Failure {
    fn from(e: sqlx::Error) -> Self {
        Failure::Offline(format!("couldn't keep the Inbox: {e}"))
    }
}

/// Where and as whom to sign in.
pub struct Login<'a> {
    pub host: &'a str,
    pub port: u16,
    pub username: &'a str,
    pub password: &'a str,
}

/// Connects and signs in. Over TLS (port 993, as Uberspace has it); only
/// a stand-in on this Mac, on another port, is read without.
pub async fn sign_in(login: &Login<'_>) -> Result<Session, Failure> {
    let offline = |e: &dyn std::fmt::Display| Failure::Offline(format!("{}: {e}", login.host));
    let tcp = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((login.host, login.port)),
    )
    .await
    .map_err(|_| Failure::Offline(format!("{} didn't answer", login.host)))?
    .map_err(|e| unreachable(login, &e))?;
    let stream = if is_loopback(login.host) && login.port != 993 {
        Stream::Plain(tcp)
    } else {
        let name = rustls::pki_types::ServerName::try_from(login.host.to_string())
            .map_err(|e| offline(&e))?;
        let tls = tokio::time::timeout(CONNECT_TIMEOUT, tls()?.connect(name, tcp))
            .await
            .map_err(|_| Failure::Offline(format!("{} didn't answer", login.host)))?
            .map_err(|e| offline(&e))?;
        Stream::Tls(Box::new(tls))
    };
    let mut client = async_imap::Client::new(stream);
    client
        .read_response()
        .await
        .map_err(|e| offline(&e))?
        .ok_or_else(|| Failure::Offline(format!("{} closed the connection", login.host)))?;
    client
        .login(login.username, login.password)
        .await
        .map_err(|(e, _)| match e {
            async_imap::error::Error::No(answer) | async_imap::error::Error::Bad(answer) => {
                refusal(login, &answer)
            }
            e => offline(&e),
        })
}

/// Why the connection didn't open, in words: the two that happen most,
/// a mistyped server and a wrong port, say so.
fn unreachable(login: &Login<'_>, e: &std::io::Error) -> Failure {
    let host = login.host;
    Failure::Offline(match e.kind() {
        std::io::ErrorKind::ConnectionRefused => {
            format!("{host} takes no connections on port {}", login.port)
        }
        // getaddrinfo's error has no kind of its own.
        _ if e.to_string().contains("lookup address") => format!("there's no server called {host}"),
        _ => format!("{host}: {e}"),
    })
}

/// What the server's no to signing in means. async-imap hands its answer
/// over as `code: None, info: Some("[AUTHENTICATIONFAILED] Authentication
/// failed.")`. A wrong password, the usual reason, names the user name, so
/// a typo in it shows; a server that can't check passwords right now is
/// tried again.
fn refusal(login: &Login<'_>, answer: &str) -> Failure {
    let info = answer
        .split_once("info: Some(\"")
        .and_then(|(_, rest)| rest.rsplit_once("\")"))
        .map_or(answer, |(info, _)| info)
        .trim();
    let (code, words) = info
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .map_or(("", info), |(code, words)| (code, words.trim()));
    match code {
        "UNAVAILABLE" | "SERVERBUG" => {
            Failure::Offline(format!("{} can't check passwords right now", login.host))
        }
        _ if code.is_empty() || code.starts_with("AUTH") || words.is_empty() => {
            Failure::Refused(format!(
                "couldn't sign in as {}. Check the address and password",
                login.username
            ))
        }
        _ => Failure::Refused(words.to_string()),
    }
}

/// Brings the copy of the account's Inbox up to date. Returns whether
/// anything in it changed.
pub async fn sync_inbox(
    session: &mut Session,
    pool: &PgPool,
    account_id: Uuid,
) -> Result<bool, Failure> {
    let mailbox = session.select("INBOX").await?;
    let validity = i64::from(mailbox.uid_validity.unwrap_or(0));
    let mut changed = false;

    let kept: Option<i64> =
        sqlx::query_scalar("SELECT uid_validity FROM inboxes WHERE account_id = $1")
            .bind(account_id)
            .fetch_optional(pool)
            .await?;
    if kept != Some(validity) {
        // New, or the UIDs mean other mails now: start the copy over.
        let mut db = pool.begin().await?;
        sqlx::query("DELETE FROM inboxes WHERE account_id = $1")
            .bind(account_id)
            .execute(&mut *db)
            .await?;
        sqlx::query("INSERT INTO inboxes (account_id, uid_validity) VALUES ($1, $2)")
            .bind(account_id)
            .bind(validity)
            .execute(&mut *db)
            .await?;
        db.commit().await?;
        changed = kept.is_some();
    }

    // The newest mails, by sequence number, with whether they're read.
    let mut on_server: HashMap<i64, bool> = HashMap::new();
    if mailbox.exists > 0 {
        let first = mailbox.exists.saturating_sub(KEEP - 1).max(1);
        let fetches: Vec<Fetch> = session
            .fetch(format!("{first}:*"), "(UID FLAGS)")
            .await?
            .try_collect()
            .await?;
        for fetch in &fetches {
            if let Some(uid) = fetch.uid {
                on_server.insert(i64::from(uid), is_seen(fetch));
            }
        }
    }

    let kept: HashMap<i64, bool> =
        sqlx::query_as::<_, (i64, bool)>("SELECT uid, seen FROM inbox_mails WHERE account_id = $1")
            .bind(account_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();

    // Gone from the server, or no longer among the newest.
    let gone: Vec<i64> = kept
        .keys()
        .filter(|uid| !on_server.contains_key(uid))
        .copied()
        .collect();
    if !gone.is_empty() {
        sqlx::query("DELETE FROM inbox_mails WHERE account_id = $1 AND uid = ANY($2)")
            .bind(account_id)
            .bind(&gone)
            .execute(pool)
            .await?;
        changed = true;
    }

    // Read or unread somewhere else since.
    let (flipped, flipped_seen): (Vec<i64>, Vec<bool>) = on_server
        .iter()
        .filter(|(uid, seen)| kept.get(uid).is_some_and(|was| was != *seen))
        .map(|(uid, seen)| (*uid, *seen))
        .unzip();
    if !flipped.is_empty() {
        sqlx::query(
            "UPDATE inbox_mails m SET seen = f.seen \
             FROM unnest($2::bigint[], $3::bool[]) AS f(uid, seen) \
             WHERE m.account_id = $1 AND m.uid = f.uid",
        )
        .bind(account_id)
        .bind(&flipped)
        .bind(&flipped_seen)
        .execute(pool)
        .await?;
        changed = true;
    }

    // New mail: read its header and the start of its text.
    let mut new: Vec<i64> = on_server
        .keys()
        .filter(|uid| !kept.contains_key(uid))
        .copied()
        .collect();
    new.sort_unstable();
    for batch in new.chunks(BATCH) {
        let set = batch
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches: Vec<Fetch> = session
            .uid_fetch(
                set,
                format!("(UID FLAGS INTERNALDATE BODY.PEEK[]<0.{PEEK}>)"),
            )
            .await?
            .try_collect()
            .await?;
        let mut seen_uids = HashSet::new();
        for fetch in &fetches {
            let Some(uid) = fetch.uid.map(i64::from) else {
                continue;
            };
            if !seen_uids.insert(uid) {
                continue;
            }
            let arrived = fetch.internal_date().map(|d| d.with_timezone(&Utc));
            let summary = summarize(fetch.body().unwrap_or_default(), arrived);
            sqlx::query(
                "INSERT INTO inbox_mails \
                 (account_id, uid, from_name, from_address, subject, snippet, date, seen) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                 ON CONFLICT (account_id, uid) DO UPDATE SET seen = EXCLUDED.seen",
            )
            .bind(account_id)
            .bind(uid)
            .bind(&summary.from_name)
            .bind(&summary.from_address)
            .bind(&summary.subject)
            .bind(&summary.snippet)
            .bind(summary.date)
            .bind(is_seen(fetch))
            .execute(pool)
            .await?;
            changed = true;
        }
    }
    Ok(changed)
}

fn is_seen(fetch: &Fetch) -> bool {
    fetch.flags().any(|flag| flag == Flag::Seen)
}

/// What the list shows of a mail.
#[derive(Debug, PartialEq)]
pub struct Summary {
    pub from_name: String,
    pub from_address: String,
    pub subject: String,
    pub snippet: String,
    pub date: DateTime<Utc>,
}

/// Reads the list's line from the start of a mail: who it's from, its
/// subject, the start of its text on one line, and when it came (when the
/// server got it, else its Date).
pub fn summarize(raw: &[u8], arrived: Option<DateTime<Utc>>) -> Summary {
    let message = mail_parser::MessageParser::new().parse(raw);
    let from = message
        .as_ref()
        .and_then(|m| m.from())
        .and_then(|list| list.first());
    let from_address = from
        .and_then(|f| f.address())
        .unwrap_or_default()
        .to_string();
    let from_name = from
        .and_then(|f| f.name())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(|| from_address.clone(), str::to_string);
    let subject = message
        .as_ref()
        .and_then(|m| m.subject())
        .map(one_line)
        .unwrap_or_default();
    let snippet = message
        .as_ref()
        .and_then(|m| m.body_preview(400))
        .map(|text| one_line(&text))
        .map(|text| text.chars().take(160).collect())
        .unwrap_or_default();
    let dated = message
        .as_ref()
        .and_then(|m| m.date())
        .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0));
    Summary {
        from_name,
        from_address,
        subject,
        snippet,
        date: arrived.or(dated).unwrap_or_else(Utc::now),
    }
}

/// Text on one line, its runs of space and line breaks made one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// TLS that trusts what macOS trusts.
fn tls() -> Result<TlsConnector, Failure> {
    static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, String>> = OnceLock::new();
    let config = CONFIG.get_or_init(|| {
        use rustls_platform_verifier::BuilderVerifierExt;
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_platform_verifier()
            .map(|builder| Arc::new(builder.with_no_client_auth()))
            .map_err(|e| e.to_string())
    });
    config
        .clone()
        .map(TlsConnector::from)
        .map_err(Failure::Offline)
}

/// The connection to a mail server: over TLS, or plain to a stand-in.
#[derive(Debug)]
pub enum Stream {
    Tls(Box<TlsStream<TcpStream>>),
    Plain(TcpStream),
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mail_reads_as_one_line_of_the_list() {
        let raw = b"From: \"Anna Weber\" <anna@example.test>\r\n\
            Subject: Contract\r\n draft\r\n\
            Date: Mon, 2 Nov 2026 09:12:00 +0100\r\n\
            Content-Type: text/plain\r\n\r\n\
            Hi Michael,\r\n\r\nthe contract is attached.\r\n";
        let summary = summarize(raw, None);
        assert_eq!(summary.from_name, "Anna Weber");
        assert_eq!(summary.from_address, "anna@example.test");
        assert_eq!(summary.subject, "Contract draft");
        assert_eq!(summary.snippet, "Hi Michael, the contract is attached.");
        assert_eq!(summary.date.to_rfc3339(), "2026-11-02T08:12:00+00:00");
    }

    #[test]
    fn a_sender_without_a_name_is_their_address() {
        let summary = summarize(b"From: noreply@example.test\r\n\r\nHello", None);
        assert_eq!(summary.from_name, "noreply@example.test");
    }

    #[test]
    fn a_refusal_says_what_to_check() {
        let login = Login {
            host: "styx.uberspace.de",
            port: 993,
            username: "michael@lazarski.test",
            password: "",
        };
        let wrong = refusal(
            &login,
            r#"code: None, info: Some("[AUTHENTICATIONFAILED] Authentication failed.")"#,
        );
        assert_eq!(
            wrong.message(),
            "Couldn't sign in as michael@lazarski.test. Check the address and password."
        );
        assert_eq!(refusal(&login, "code: None, info: None"), wrong);
        assert_eq!(
            refusal(
                &login,
                r#"code: None, info: Some("[UNAVAILABLE] Temporary authentication failure.")"#
            ),
            Failure::Offline("styx.uberspace.de can't check passwords right now".into())
        );
        assert_eq!(
            refusal(
                &login,
                r#"code: None, info: Some("[EXPIRED] Your password has expired.")"#
            ),
            Failure::Refused("Your password has expired.".into())
        );
    }

    #[test]
    fn a_failure_reads_as_a_sentence() {
        let failure = Failure::Offline("there's no server called nowhere.invalid".into());
        assert_eq!(
            failure.message(),
            "There's no server called nowhere.invalid."
        );
        let failure = Failure::Refused("Authentication failed.".into());
        assert_eq!(failure.message(), "Authentication failed.");
        let failure = Failure::Offline("styx.uberspace.de didn't answer".into());
        assert_eq!(failure.message(), "styx.uberspace.de didn't answer.");
    }
}
