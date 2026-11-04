//! Reading an account's mail over IMAP: signing in, keeping a copy of its
//! Inbox's newest mail in `inbox_mails`, and listening for more with IDLE;
//! and, live, its folders, their newest mail, and searches.
//!
//! The copy holds what the list shows (sender, subject, the start of the
//! text, when it came, read or not) for the newest [`KEEP`] mails. It's
//! made again from the server whenever the server says its UIDs changed
//! meaning (UIDVALIDITY).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use async_imap::types::{Fetch, Flag, NameAttribute};
use base64::Engine;
use chrono::{DateTime, NaiveDate, Utc};
use futures::TryStreamExt;
use postrider_domain::FolderRole;
use sqlx::PgPool;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use uuid::Uuid;

use crate::rules::uid_set;

/// How many of an Inbox's newest mails Postrider keeps a copy of.
pub const KEEP: u32 = 200;

/// How much of each mail it reads: the header and the start of the text.
const PEEK: u32 = 16 * 1024;

/// How many mails it asks for at once.
const BATCH: usize = 50;

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

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

/// What a server answered to something done to mail: its NO or BAD as
/// refused, as trying again won't help; anything else as offline, so what
/// was done waits for it (`queued.rs`).
fn answered(e: async_imap::error::Error) -> Failure {
    match e {
        async_imap::error::Error::No(words) | async_imap::error::Error::Bad(words) => {
            Failure::Refused(words)
        }
        e => e.into(),
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

    // The newest mails, by sequence number, and those flagged however old,
    // with whether they're read and flagged.
    let mut on_server: HashMap<i64, (bool, bool)> = HashMap::new();
    if mailbox.exists > 0 {
        let first = mailbox.exists.saturating_sub(KEEP - 1).max(1);
        let fetches: Vec<Fetch> = session
            .fetch(format!("{first}:*"), "(UID FLAGS)")
            .await?
            .try_collect()
            .await?;
        for fetch in &fetches {
            if let Some(uid) = fetch.uid {
                on_server.insert(i64::from(uid), (is_seen(fetch), is_flagged(fetch)));
            }
        }
        let newest: Vec<i64> = on_server.keys().copied().collect();
        let flagged = flagged_besides(session, &newest).await?;
        if !flagged.is_empty() {
            let fetches: Vec<Fetch> = session
                .uid_fetch(uid_list(&flagged), "(UID FLAGS)")
                .await?
                .try_collect()
                .await?;
            for fetch in &fetches {
                if let Some(uid) = fetch.uid {
                    on_server.insert(i64::from(uid), (is_seen(fetch), is_flagged(fetch)));
                }
            }
        }
    }

    let kept: HashMap<i64, (bool, bool)> = sqlx::query_as::<_, (i64, bool, bool)>(
        "SELECT uid, seen, flagged FROM inbox_mails WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(uid, seen, flagged)| (uid, (seen, flagged)))
    .collect();

    // What waits for the server (`queued.rs`) stays as it was done here:
    // a mail moved away isn't copied back, one moved here keeps its line,
    // and one marked read or unread, flagged or not, stays so.
    let waiting: Vec<(i64, bool, bool, bool)> = sqlx::query_as(
        "SELECT uid, to_path IS NOT NULL, seen IS NOT NULL, flagged IS NOT NULL \
         FROM queued_mails WHERE account_id = $1 AND path = 'INBOX' AND uid_validity = $2",
    )
    .bind(account_id)
    .bind(validity)
    .fetch_all(pool)
    .await?;
    let moved_away: HashSet<i64> = waiting.iter().filter(|w| w.1).map(|w| w.0).collect();
    let marked: HashSet<i64> = waiting.iter().filter(|w| w.2).map(|w| w.0).collect();
    let flag_marked: HashSet<i64> = waiting.iter().filter(|w| w.3).map(|w| w.0).collect();
    let moved_here: HashSet<i64> = sqlx::query_scalar(
        "SELECT placeholder FROM queued_mails WHERE account_id = $1 AND to_path = 'INBOX'",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    let read_below: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(below), 0) FROM queued_reads \
         WHERE account_id = $1 AND path = 'INBOX' AND uid_validity = $2",
    )
    .bind(account_id)
    .bind(validity)
    .fetch_one(pool)
    .await?;
    for (uid, (seen, _)) in &mut on_server {
        *seen |= *uid < read_below;
    }

    // Gone from the server, or no longer among the newest.
    let gone: Vec<i64> = kept
        .keys()
        .filter(|uid| !on_server.contains_key(uid) && !moved_here.contains(uid))
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

    // Read or unread, flagged or not, somewhere else since.
    let mut flipped: Vec<(i64, bool, bool)> = Vec::new();
    for (uid, (seen, flagged)) in &on_server {
        let Some(&(was_seen, was_flagged)) = kept.get(uid) else {
            continue;
        };
        let seen = if marked.contains(uid) {
            was_seen
        } else {
            *seen
        };
        let flagged = if flag_marked.contains(uid) {
            was_flagged
        } else {
            *flagged
        };
        if (seen, flagged) != (was_seen, was_flagged) {
            flipped.push((*uid, seen, flagged));
        }
    }
    if !flipped.is_empty() {
        let uids: Vec<i64> = flipped.iter().map(|f| f.0).collect();
        let seen: Vec<bool> = flipped.iter().map(|f| f.1).collect();
        let flagged: Vec<bool> = flipped.iter().map(|f| f.2).collect();
        sqlx::query(
            "UPDATE inbox_mails m SET seen = f.seen, flagged = f.flagged \
             FROM unnest($2::bigint[], $3::bool[], $4::bool[]) AS f(uid, seen, flagged) \
             WHERE m.account_id = $1 AND m.uid = f.uid",
        )
        .bind(account_id)
        .bind(&uids)
        .bind(&seen)
        .bind(&flagged)
        .execute(pool)
        .await?;
        changed = true;
    }

    // New mail: read its header and the start of its text.
    let mut new: Vec<i64> = on_server
        .keys()
        .filter(|uid| !kept.contains_key(uid) && !moved_away.contains(uid))
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
                 (account_id, uid, from_name, from_address, subject, snippet, date, seen, \
                 message_id, refs, to_addresses, reply_to, flagged) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
                 ON CONFLICT (account_id, uid) DO UPDATE \
                 SET seen = EXCLUDED.seen, flagged = EXCLUDED.flagged",
            )
            .bind(account_id)
            .bind(uid)
            .bind(&summary.from_name)
            .bind(&summary.from_address)
            .bind(&summary.subject)
            .bind(&summary.snippet)
            .bind(summary.date)
            .bind(is_seen(fetch) || uid < read_below)
            .bind(&summary.message_id)
            .bind(&summary.threading.refs)
            .bind(&summary.threading.to_addresses)
            .bind(&summary.threading.reply_to)
            .bind(is_flagged(fetch))
            .execute(pool)
            .await?;
            changed = true;
        }
    }

    // Mail copied before conversations were: what joins it to others.
    let unthreaded: Vec<u32> = sqlx::query_scalar::<_, i64>(
        "SELECT uid FROM inbox_mails WHERE account_id = $1 AND refs IS NULL",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(|uid| u32::try_from(uid).ok())
    .collect();
    for headed in fetch_headers(session, &unthreaded, THREADING).await? {
        let threading = Threading::read(&headed.header);
        sqlx::query(
            "UPDATE inbox_mails SET refs = $3, to_addresses = $4, reply_to = $5 \
             WHERE account_id = $1 AND uid = $2",
        )
        .bind(account_id)
        .bind(i64::from(headed.uid))
        .bind(&threading.refs)
        .bind(&threading.to_addresses)
        .bind(&threading.reply_to)
        .execute(pool)
        .await?;
        changed = true;
    }
    Ok(changed)
}

/// The header fields [`Threading`] reads.
const THREADING: &str = "IN-REPLY-TO REFERENCES TO CC REPLY-TO";

/// A mail's whole source, from one of the account's folders, without
/// marking it read. `None` when it's no longer there, or the folder's UIDs
/// changed meaning since `validity`, when that's known.
pub async fn source(
    session: &mut Session,
    folder: &str,
    validity: Option<i64>,
    uid: i64,
) -> Result<Option<Vec<u8>>, Failure> {
    let mailbox = session.examine(folder).await?;
    if validity.is_some_and(|v| i64::from(mailbox.uid_validity.unwrap_or(0)) != v) {
        return Ok(None);
    }
    let fetches: Vec<Fetch> = session
        .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
        .await?
        .try_collect()
        .await?;
    Ok(fetches
        .iter()
        .find(|f| f.uid.map(i64::from) == Some(uid))
        .and_then(|f| f.body())
        .map(<[u8]>::to_vec))
}

/// How much of a mail past its header [`glances`] reads: the first lines of
/// any mail, and the names of the files that come before a big one.
pub const GLANCE: u32 = 64 * 1024;

/// The header and the first [`GLANCE`] bytes of the rest of each mail at
/// `uids` in `folder`, as a source: the whole of a mail no longer than
/// that. In one FETCH, without marking anything read. A mail
/// no longer there is left out; `None` when the folder's UIDs changed
/// meaning since `validity`, when that's known.
pub async fn glances(
    session: &mut Session,
    folder: &str,
    validity: Option<i64>,
    uids: &[u32],
) -> Result<Option<HashMap<u32, Vec<u8>>>, Failure> {
    let mailbox = session.examine(folder).await?;
    if validity.is_some_and(|v| i64::from(mailbox.uid_validity.unwrap_or(0)) != v) {
        return Ok(None);
    }
    // Each mail's header and text, as they come.
    type Parts = (Option<Vec<u8>>, Option<Vec<u8>>);
    let mut parts: HashMap<u32, Parts> = HashMap::new();
    for batch in uids.chunks(BATCH) {
        let fetches: Vec<Fetch> = session
            .uid_fetch(
                uid_set(batch),
                format!("(UID BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.{GLANCE}>)"),
            )
            .await?
            .try_collect()
            .await?;
        // A server may answer a mail's parts in more than one line.
        for fetch in &fetches {
            let Some(uid) = fetch.uid.filter(|uid| batch.contains(uid)) else {
                continue;
            };
            let (header, text) = parts.entry(uid).or_default();
            if header.is_none() {
                *header = fetch.header().map(<[u8]>::to_vec);
            }
            if text.is_none() {
                *text = fetch.text().map(<[u8]>::to_vec);
            }
        }
    }
    Ok(Some(
        parts
            .into_iter()
            .filter_map(|(uid, (header, text))| Some((uid, glance(header?, text))))
            .collect(),
    ))
}

/// A mail's source from its header and the start of its text, as
/// `BODY[HEADER]` and `BODY[TEXT]<0.GLANCE>` give them: the header ends
/// with its blank line, so the two together are the source.
fn glance(mut header: Vec<u8>, text: Option<Vec<u8>>) -> Vec<u8> {
    header.extend_from_slice(&text.unwrap_or_default());
    header
}

/// The whole source of each mail at `uids` in `folder` no bigger than
/// `at_most` bytes, read without marking anything read, a few at a time.
/// A mail no longer there, or bigger, is left out; `None` when the
/// folder's UIDs changed meaning since `validity`.
pub async fn sources(
    session: &mut Session,
    folder: &str,
    validity: i64,
    uids: &[i64],
    at_most: u32,
) -> Result<Option<HashMap<i64, Vec<u8>>>, Failure> {
    let mut read = HashMap::new();
    if uids.is_empty() {
        return Ok(Some(read));
    }
    let mailbox = session.examine(folder).await?;
    if i64::from(mailbox.uid_validity.unwrap_or(0)) != validity {
        return Ok(None);
    }
    let set = |uids: &[i64]| {
        uids.iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let sized: Vec<Fetch> = session
        .uid_fetch(set(uids), "(UID RFC822.SIZE)")
        .await?
        .try_collect()
        .await?;
    let small: Vec<i64> = sized
        .iter()
        .filter(|f| f.size.is_some_and(|size| size <= at_most))
        .filter_map(|f| f.uid.map(i64::from))
        .filter(|uid| uids.contains(uid))
        .collect();
    for batch in small.chunks(SOURCES_AT_ONCE) {
        let fetches: Vec<Fetch> = session
            .uid_fetch(set(batch), "(UID BODY.PEEK[])")
            .await?
            .try_collect()
            .await?;
        for fetch in &fetches {
            if let (Some(uid), Some(body)) = (fetch.uid.map(i64::from), fetch.body())
                && batch.contains(&uid)
            {
                read.insert(uid, body.to_vec());
            }
        }
    }
    Ok(Some(read))
}

/// How many whole mails [`sources`] reads in one FETCH.
const SOURCES_AT_ONCE: usize = 10;

/// Marks a mail read, as opening it does.
pub async fn mark_read(session: &mut Session, folder: &str, uid: i64) -> Result<(), Failure> {
    session.select(folder).await.map_err(answered)?;
    session
        .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Seen)")
        .await
        .map_err(answered)?
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}

/// How many UIDs one STORE names at most, so its line stays short enough
/// for any server however scattered they are.
const STORED: usize = 5000;

/// Marks every unread mail in a folder read, in one STORE: the folder's
/// UIDVALIDITY, and the mails it marked, for Undo.
pub async fn mark_all_read(
    session: &mut Session,
    folder: &str,
) -> Result<(u32, Vec<u32>), Failure> {
    let mailbox = session.select(folder).await.map_err(answered)?;
    let uids = read_all(session, "UNSEEN").await.map_err(answered)?;
    Ok((mailbox.uid_validity.unwrap_or(0), uids))
}

/// Marks read what `search` finds unread in the folder selected: the mails
/// it marked.
async fn read_all(
    session: &mut Session,
    search: &str,
) -> Result<Vec<u32>, async_imap::error::Error> {
    let mut uids: Vec<u32> = session.uid_search(search).await?.into_iter().collect();
    uids.sort_unstable();
    for some in uids.chunks(STORED) {
        session
            .uid_store(uid_set(some), "+FLAGS.SILENT (\\Seen)")
            .await?
            .try_collect::<Vec<_>>()
            .await?;
    }
    Ok(uids)
}

/// Marks the mails [`mark_all_read`] marked unread again, unless the
/// folder's UIDs were numbered anew since.
pub async fn mark_unread(
    session: &mut Session,
    folder: &str,
    validity: u32,
    uids: &[u32],
) -> Result<(), Failure> {
    let mailbox = session.select(folder).await?;
    if mailbox.uid_validity.unwrap_or(0) != validity {
        return Err(Failure::Refused(
            "The folder changed on its server since: its mail stays read.".into(),
        ));
    }
    for some in uids.chunks(STORED) {
        session
            .uid_store(uid_set(some), "-FLAGS.SILENT (\\Seen)")
            .await?
            .try_collect::<Vec<_>>()
            .await?;
    }
    Ok(())
}

/// Deletes everything in a folder for good, as emptying the Trash does:
/// how many mails it held.
pub async fn empty(session: &mut Session, folder: &str) -> Result<u32, Failure> {
    let mailbox = session.select(folder).await?;
    if mailbox.exists > 0 {
        session
            .uid_store("1:*", "+FLAGS.SILENT (\\Deleted)")
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        session.expunge().await?.try_collect::<Vec<_>>().await?;
    }
    Ok(mailbox.exists)
}

/// Deletes a folder, its mail moved to `trash` first: how many mails it
/// held.
pub async fn delete_folder(session: &mut Session, path: &str, trash: &str) -> Result<u32, Failure> {
    let mailbox = session.select(path).await?;
    if mailbox.exists > 0 {
        session.uid_mv("1:*", trash).await?;
    }
    // Dovecot hangs up on a connection whose folder is deleted under it.
    session.select("INBOX").await?;
    let _ = session.unsubscribe(path).await;
    session.delete(path).await?;
    Ok(mailbox.exists)
}

/// Moves a mail to another folder (MOVE, RFC 6851).
pub async fn move_mail(
    session: &mut Session,
    from: &str,
    uid: i64,
    to: &str,
) -> Result<(), Failure> {
    session.select(from).await.map_err(answered)?;
    session
        .uid_mv(uid.to_string(), to)
        .await
        .map_err(answered)?;
    Ok(())
}

/// Selects a folder to do there what waited for its server: `false` when
/// it's gone, or its UIDs were numbered anew since `validity`.
pub async fn select_queued(
    session: &mut Session,
    folder: &str,
    validity: i64,
) -> Result<bool, Failure> {
    match session.select(folder).await {
        Ok(mailbox) => Ok(i64::from(mailbox.uid_validity.unwrap_or(0)) == validity),
        Err(e) => took(Err(e)),
    }
}

/// Does what waited for one mail of the folder selected: marks it read or
/// unread, flagged or not, then moves it. Whether its server took it.
pub async fn replay_mail(
    session: &mut Session,
    uid: i64,
    seen: Option<bool>,
    flagged: Option<bool>,
    to: Option<&str>,
) -> Result<bool, Failure> {
    let done = async {
        store_marks(session, uid, seen, flagged).await?;
        if let Some(to) = to {
            session.uid_mv(uid.to_string(), to).await?;
        }
        Ok(())
    }
    .await;
    took(done)
}

/// Marks one mail of `folder` read or unread, flagged or not: what's
/// `None` stays as it is. Its server's NO is [`Failure::Refused`].
pub async fn mark(
    session: &mut Session,
    folder: &str,
    uid: i64,
    seen: Option<bool>,
    flagged: Option<bool>,
) -> Result<(), Failure> {
    session.select(folder).await.map_err(answered)?;
    store_marks(session, uid, seen, flagged)
        .await
        .map_err(answered)
}

/// Sets or takes away `\Seen` and `\Flagged` on one mail of the folder
/// selected.
async fn store_marks(
    session: &mut Session,
    uid: i64,
    seen: Option<bool>,
    flagged: Option<bool>,
) -> Result<(), async_imap::error::Error> {
    for (on, flag) in [(seen, "\\Seen"), (flagged, "\\Flagged")] {
        let Some(on) = on else { continue };
        let sign = if on { '+' } else { '-' };
        session
            .uid_store(uid.to_string(), format!("{sign}FLAGS.SILENT ({flag})"))
            .await?
            .try_collect::<Vec<_>>()
            .await?;
    }
    Ok(())
}

/// Marks read every unread mail of the folder selected under UID `below`:
/// the mails it marked, or `None` when its server refused.
pub async fn read_below(session: &mut Session, below: i64) -> Result<Option<Vec<u32>>, Failure> {
    if below <= 1 {
        return Ok(Some(Vec::new()));
    }
    match read_all(session, &format!("UNSEEN UID 1:{}", below - 1)).await {
        Ok(uids) => Ok(Some(uids)),
        Err(e) => took(Err(e)).map(|_| None),
    }
}

/// Whether the server took what waited for it: its NO or BAD is `false`,
/// and what it refused is let go; anything else is offline, and waits.
fn took(done: Result<(), async_imap::error::Error>) -> Result<bool, Failure> {
    match done {
        Ok(()) => Ok(true),
        Err(async_imap::error::Error::No(words) | async_imap::error::Error::Bad(words)) => {
            tracing::info!("its server refused what waited for it: {words}");
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

/// Marks a mail answered, as sending its reply does.
pub async fn mark_answered(session: &mut Session, folder: &str, uid: i64) -> Result<(), Failure> {
    session.select(folder).await?;
    session
        .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Answered)")
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}

/// Puts a mail in a folder, read: a sent one in Sent.
pub async fn put(session: &mut Session, folder: &str, raw: &[u8]) -> Result<(), Failure> {
    session.append(folder, Some("(\\Seen)"), None, raw).await?;
    Ok(())
}

/// Puts a draft in Drafts: read, and marked a draft, as mail apps keep
/// theirs.
pub async fn put_draft(session: &mut Session, folder: &str, raw: &[u8]) -> Result<(), Failure> {
    session
        .append(folder, Some("(\\Seen \\Draft)"), None, raw)
        .await?;
    Ok(())
}

/// The UIDs of the mails in the folder selected with the Message-ID `id`,
/// as a set to name them by: `None` when there's none.
async fn with_id(session: &mut Session, id: &str) -> Result<Option<String>, Failure> {
    let mut uids: Vec<u32> = session
        .uid_search(format!("HEADER Message-ID {}", quoted(id)?))
        .await?
        .into_iter()
        .collect();
    uids.sort_unstable();
    Ok((!uids.is_empty()).then(|| {
        uids.iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }))
}

/// The source of the mail in `folder` with the Message-ID `id`, and its
/// UID: a draft, found again after it was kept. `None` when it's gone.
pub async fn source_by_id(
    session: &mut Session,
    folder: &str,
    id: &str,
) -> Result<Option<Vec<u8>>, Failure> {
    session.examine(folder).await?;
    let Some(uids) = with_id(session, id).await? else {
        return Ok(None);
    };
    let fetches: Vec<Fetch> = session
        .uid_fetch(uids, "(UID BODY.PEEK[])")
        .await?
        .try_collect()
        .await?;
    Ok(fetches
        .iter()
        .max_by_key(|f| f.uid)
        .and_then(|f| f.body())
        .map(<[u8]>::to_vec))
}

/// Deletes the mails in `folder` with the Message-ID `id` for good: a
/// draft kept anew since, or sent. A server without UID EXPUNGE (RFC 4315)
/// expunges the folder.
pub async fn delete_by_id(session: &mut Session, folder: &str, id: &str) -> Result<(), Failure> {
    session.select(folder).await?;
    let Some(uids) = with_id(session, id).await? else {
        return Ok(());
    };
    session
        .uid_store(&uids, "+FLAGS.SILENT (\\Deleted)")
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    let expunged = match session.uid_expunge(&uids).await {
        Ok(stream) => stream.try_collect::<Vec<_>>().await.map(drop),
        Err(e) => Err(e),
    };
    if expunged.is_err() {
        session.expunge().await?.try_collect::<Vec<_>>().await?;
    }
    Ok(())
}

/// Moves the mails in `from` with the Message-ID `id` to `to`: a draft
/// thrown away, to Trash.
pub async fn move_by_id(
    session: &mut Session,
    from: &str,
    id: &str,
    to: &str,
) -> Result<(), Failure> {
    session.select(from).await?;
    if let Some(uids) = with_id(session, id).await? {
        session.uid_mv(uids, to).await?;
    }
    Ok(())
}

/// Makes a folder called `name`, for `role` if it has one, beside the
/// account's others: under `INBOX.` where its server keeps them so. It's
/// subscribed, so the account's other mail apps show it too. One made
/// meanwhile will do.
pub async fn make_folder(
    session: &mut Session,
    name: &str,
    role: Option<FolderRole>,
) -> Result<Place, Failure> {
    let names: Vec<_> = session
        .list(Some(""), Some("*"))
        .await?
        .try_collect()
        .await?;
    let under_inbox = names.iter().find_map(|listed| {
        let delimiter = listed.delimiter()?;
        let rest = listed.name().strip_prefix("INBOX")?;
        rest.starts_with(delimiter)
            .then(|| format!("INBOX{delimiter}"))
    });
    let path = format!("{}{}", under_inbox.unwrap_or_default(), path_name(name));
    match session.create(&path).await {
        Ok(()) => {}
        Err(async_imap::error::Error::No(answer))
            if answer.to_ascii_lowercase().contains("exists") => {}
        Err(e) => return Err(e.into()),
    }
    if let Err(e) = session.subscribe(&path).await {
        tracing::info!("couldn't subscribe to {path}: {e}");
    }
    Ok(Place {
        path,
        name: name.to_string(),
        role,
    })
}

/// Subscribes the folders at `paths` that aren't, so the account's other
/// mail apps show them; how many it subscribed.
pub async fn subscribe(session: &mut Session, paths: &[String]) -> Result<usize, Failure> {
    let subscribed: Vec<String> = session
        .lsub(Some(""), Some("*"))
        .await?
        .map_ok(|name| name.name().to_string())
        .try_collect()
        .await?;
    let mut count = 0;
    for path in paths.iter().filter(|path| !subscribed.contains(path)) {
        match session.subscribe(path).await {
            Ok(()) => count += 1,
            Err(e) => tracing::info!("couldn't subscribe to {path}: {e}"),
        }
    }
    Ok(count)
}

/// Renames a folder where it is, among the same folders: "GitHub" to
/// "Dev-Kram". What it holds goes with it.
pub async fn rename_folder(
    session: &mut Session,
    place: &Place,
    name: &str,
) -> Result<Place, Failure> {
    let listed: Vec<_> = session
        .list(Some(""), Some(&place.path))
        .await?
        .try_collect()
        .await?;
    let delimiter = listed
        .first()
        .and_then(|listed| listed.delimiter())
        .unwrap_or_default()
        .to_string();
    if !delimiter.is_empty() && name.contains(&delimiter) {
        return Err(Failure::Refused(format!(
            "A folder's name can't have “{delimiter}” in it here."
        )));
    }
    let parent = match delimiter.is_empty() {
        true => "",
        false => place
            .path
            .rfind(&delimiter)
            .map_or("", |at| &place.path[..at + delimiter.len()]),
    };
    let path = format!("{parent}{}", path_name(name));
    // Dovecot hangs up on a connection whose folder goes from under it.
    session.select("INBOX").await?;
    match session.rename(&place.path, &path).await {
        Ok(()) => {}
        Err(async_imap::error::Error::No(answer))
            if answer.to_ascii_lowercase().contains("exists") =>
        {
            return Err(Failure::Refused(format!(
                "There's a folder called {name} already."
            )));
        }
        Err(e) => return Err(e.into()),
    }
    let _ = session.unsubscribe(&place.path).await;
    if let Err(e) = session.subscribe(&path).await {
        tracing::info!("couldn't subscribe to {path}: {e}");
    }
    let shown = match place.name.rsplit_once(" / ") {
        Some((parent, _)) => format!("{parent} / {name}"),
        None => name.to_string(),
    };
    Ok(Place {
        path,
        name: shown,
        role: place.role,
    })
}

/// A folder on an account's server.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// What the server calls it, as commands name it: `INBOX.Rechnungen`.
    pub path: String,
    /// What Postrider shows: "Rechnungen", "Projects / Layer".
    pub name: String,
    pub role: Option<FolderRole>,
}

impl Place {
    /// The Inbox, as a folder like the others.
    pub fn inbox() -> Place {
        Place {
            path: "INBOX".into(),
            name: "Inbox".into(),
            role: None,
        }
    }

    /// Its key, the same for each account's: its role's, else its name.
    pub fn key(&self) -> String {
        self.role
            .map_or_else(|| self.name.clone(), |role| role.key().to_string())
    }

    /// Whether it's the folder `key` names: by role, or by name in any case.
    pub fn is(&self, key: &str) -> bool {
        self.key().to_lowercase() == key.to_lowercase()
    }
}

/// The account's folders, but the Inbox.
pub async fn folders(session: &mut Session) -> Result<Vec<Place>, Failure> {
    let names: Vec<_> = session
        .list(Some(""), Some("*"))
        .await?
        .try_collect()
        .await?;
    // Every account has an Inbox: a list without one came from a connection
    // the server hung up on, which reads as nothing rather than failing.
    if !names
        .iter()
        .any(|name| name.name().eq_ignore_ascii_case("INBOX"))
    {
        return Err(Failure::Offline("The server listed no folders.".into()));
    }
    let listed = names
        .iter()
        .map(|name| {
            let attributes = name.attributes();
            let selectable = !attributes.iter().any(|a| match a {
                NameAttribute::NoSelect => true,
                NameAttribute::Extension(other) => other.eq_ignore_ascii_case("\\NonExistent"),
                _ => false,
            });
            let role = attributes.iter().find_map(|a| match a {
                NameAttribute::Sent => Some(FolderRole::Sent),
                NameAttribute::Archive => Some(FolderRole::Archive),
                NameAttribute::Drafts => Some(FolderRole::Drafts),
                NameAttribute::Trash => Some(FolderRole::Trash),
                NameAttribute::Junk => Some(FolderRole::Junk),
                _ => None,
            });
            Listed {
                path: name.name().to_string(),
                delimiter: name.delimiter().map(str::to_string),
                role,
                selectable,
            }
        })
        .collect();
    Ok(places(listed))
}

/// What STATUS counts in a folder: all it holds, what's unread, the UID
/// its next mail gets, and what its UIDs mean. Mail that came, left, or was
/// read or marked unread elsewhere changes one of them; with CONDSTORE
/// (RFC 7162), `modseq` changes with any flag too, so a mail flagged
/// elsewhere is read again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counted {
    pub total: u32,
    pub unseen: u32,
    pub next: u32,
    pub validity: u32,
    pub modseq: u64,
}

impl Counted {
    /// The counts as one text, as [`crate::kept`] keeps what its lines were
    /// read under.
    pub fn stamp(&self) -> String {
        let counts = format!(
            "{}:{}:{}:{}",
            self.validity, self.next, self.total, self.unseen
        );
        if self.modseq == 0 {
            counts
        } else {
            format!("{counts}:{}", self.modseq)
        }
    }
}

/// What STATUS counts in each folder, by path. Every STATUS goes in one
/// write and the answers are read as they come, so the folders cost the
/// server one round trip, not one each (IMAP lets a client send commands
/// without waiting, RFC 3501 5.5). A folder the server says no to is left
/// out. `modseq` asks for HIGHESTMODSEQ too, which only a server with
/// CONDSTORE knows.
pub async fn counts(
    session: &mut Session,
    folders: &[String],
    modseq: bool,
) -> Result<HashMap<String, Counted>, Failure> {
    use async_imap::imap_proto::{MailboxDatum, Response, Status, StatusAttribute};

    let mut counted = HashMap::new();
    if folders.is_empty() {
        return Ok(counted);
    }
    // Tags of our own, apart from the session's (A0001…), which waits on none.
    let mut waiting: HashMap<String, &str> = HashMap::new();
    let mut commands = Vec::new();
    for (n, folder) in folders.iter().enumerate() {
        let tag = format!("S{n}");
        let more = if modseq { " HIGHESTMODSEQ" } else { "" };
        commands.push(format!(
            "{tag} STATUS {} (MESSAGES UNSEEN UIDNEXT UIDVALIDITY{more})",
            quoted(folder)?
        ));
        waiting.insert(tag, folder);
    }
    session.run_command_untagged(commands.join("\r\n")).await?;
    while !waiting.is_empty() {
        let Some(answer) = session
            .read_response()
            .await
            .map_err(|e| Failure::Offline(e.to_string()))?
        else {
            return Err(Failure::Offline("the server hung up.".into()));
        };
        match answer.parsed() {
            Response::MailboxData(MailboxDatum::Status { mailbox, status }) => {
                let mut count = Counted::default();
                for attribute in status {
                    match attribute {
                        StatusAttribute::Unseen(n) => count.unseen = *n,
                        StatusAttribute::Messages(n) => count.total = *n,
                        StatusAttribute::UidNext(n) => count.next = *n,
                        StatusAttribute::UidValidity(n) => count.validity = *n,
                        StatusAttribute::HighestModSeq(n) => count.modseq = *n,
                        _ => {}
                    }
                }
                counted.insert(mailbox.to_string(), count);
            }
            Response::Done { tag, status, .. } => {
                if let Some(folder) = waiting.remove(&tag.0)
                    && *status != Status::Ok
                {
                    counted.remove(folder);
                    tracing::debug!(?status, "a folder couldn't be counted");
                }
            }
            _ => {}
        }
    }
    Ok(counted)
}

/// A folder as LIST names it.
struct Listed {
    path: String,
    delimiter: Option<String>,
    role: Option<FolderRole>,
    selectable: bool,
}

/// Names for the folders that say no role of their own, from the names
/// mail apps give them, in English and German.
const ROLE_NAMES: [(FolderRole, &[&str]); 6] = [
    (
        FolderRole::Sent,
        &[
            "sent",
            "sent messages",
            "sent items",
            "sent mail",
            "gesendet",
            "gesendete objekte",
            "gesendete elemente",
        ],
    ),
    (FolderRole::Archive, &["archive", "archives", "archiv"]),
    (FolderRole::Drafts, &["drafts", "entwürfe"]),
    (
        FolderRole::Trash,
        &[
            "trash",
            "deleted messages",
            "deleted items",
            "bin",
            "papierkorb",
            "gelöschte objekte",
            "gelöschte elemente",
        ],
    ),
    (
        FolderRole::Junk,
        &["junk", "spam", "junk e-mail", "junk-e-mail"],
    ),
    (FolderRole::Newsletters, &["newsletters", "newsletter"]),
];

/// The folders worth showing, by name: every one that can hold mail but
/// the Inbox. Under `INBOX.`, as some servers keep them, they read as their
/// own; a folder in a folder reads "Projects / Layer". A top folder that
/// says no role gets one by its name, when no other has that role.
fn places(listed: Vec<Listed>) -> Vec<Place> {
    let mut taken: Vec<FolderRole> = listed.iter().filter_map(|l| l.role).collect();
    let mut places = Vec::new();
    for folder in listed {
        if !folder.selectable || folder.path.eq_ignore_ascii_case("INBOX") {
            continue;
        }
        let decoded = folder_name(&folder.path);
        let parts: Vec<&str> = match folder.delimiter.as_deref() {
            Some(delimiter) if !delimiter.is_empty() => decoded.split(delimiter).collect(),
            _ => vec![decoded.as_str()],
        };
        let parts = match parts.split_first() {
            Some((first, rest)) if first.eq_ignore_ascii_case("INBOX") && !rest.is_empty() => {
                rest.to_vec()
            }
            _ => parts,
        };
        let name = parts.join(" / ");
        let role = folder.role.or_else(|| {
            let lower = name.to_lowercase();
            let role = ROLE_NAMES
                .iter()
                .find(|(_, names)| parts.len() == 1 && names.contains(&lower.as_str()))
                .map(|(role, _)| *role)
                .filter(|role| !taken.contains(role))?;
            taken.push(role);
            Some(role)
        });
        places.push(Place {
            path: folder.path,
            name,
            role,
        });
    }
    places
}

/// A folder's name as IMAP sends it, in modified UTF-7 (RFC 3501, 5.1.3):
/// `Entw&APw-rfe` is "Entwürfe".
pub(crate) fn folder_name(path: &str) -> String {
    let mut name = String::new();
    let mut rest = path;
    while let Some(start) = rest.find('&') {
        name.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else {
            name.push_str(&rest[start..]);
            return name;
        };
        let encoded = &after[..end];
        let units: Option<Vec<u16>> = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(encoded.replace(',', "/"))
            .ok()
            .filter(|bytes| bytes.len() % 2 == 0)
            .map(|bytes| {
                bytes
                    .chunks(2)
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect()
            });
        match units {
            _ if encoded.is_empty() => name.push('&'),
            Some(units) => name.push_str(&String::from_utf16_lossy(&units)),
            None => name.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    name.push_str(rest);
    name
}

/// A name as a folder's path has it, in modified UTF-7: "Entwürfe" is
/// `Entw&APw-rfe`.
fn path_name(name: &str) -> String {
    fn wide_ones(path: &mut String, wide: &mut Vec<u16>) {
        if wide.is_empty() {
            return;
        }
        let bytes: Vec<u8> = wide.iter().flat_map(|unit| unit.to_be_bytes()).collect();
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
        let _ = write!(path, "&{}-", encoded.replace('/', ","));
        wide.clear();
    }
    let mut path = String::new();
    let mut wide = Vec::new();
    for c in name.chars() {
        match c {
            '&' => {
                wide_ones(&mut path, &mut wide);
                path.push_str("&-");
            }
            ' '..='~' => {
                wide_ones(&mut path, &mut wide);
                path.push(c);
            }
            _ => wide.extend_from_slice(c.encode_utf16(&mut [0; 2])),
        }
    }
    wide_ones(&mut path, &mut wide);
    path
}

/// A mail in a folder, as a list shows it.
#[derive(Debug, Clone)]
pub struct Line {
    pub uid: i64,
    pub summary: Summary,
    pub seen: bool,
    pub flagged: bool,
}

/// Mails read from a folder, under its UIDVALIDITY.
pub struct Lines {
    pub validity: i64,
    pub lines: Vec<Line>,
}

/// A folder's newest `count` mails. What `known` has (read under
/// `known_validity`) isn't read again.
pub async fn newest(
    session: &mut Session,
    folder: &str,
    count: u32,
    known: &HashMap<i64, Summary>,
    known_validity: i64,
) -> Result<Lines, Failure> {
    let mailbox = session.examine(folder).await?;
    let validity = i64::from(mailbox.uid_validity.unwrap_or(0));
    let mut uids = Vec::new();
    if mailbox.exists > 0 {
        let first = mailbox.exists.saturating_sub(count - 1).max(1);
        let fetches: Vec<Fetch> = session
            .fetch(format!("{first}:*"), "(UID)")
            .await?
            .try_collect()
            .await?;
        uids = fetches
            .iter()
            .filter_map(|f| f.uid.map(i64::from))
            .collect();
        uids.extend(flagged_besides(session, &uids).await?);
    }
    let empty = HashMap::new();
    let known = if validity == known_validity {
        known
    } else {
        &empty
    };
    let lines = lines(session, &uids, known).await?;
    Ok(Lines { validity, lines })
}

/// The flagged mails of the folder selected that `uids` doesn't have,
/// however old: so the Flagged list has them all.
/// UIDs as an IMAP set: `4,8,15`.
fn uid_list(uids: &[i64]) -> String {
    uids.iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

async fn flagged_besides(session: &mut Session, uids: &[i64]) -> Result<Vec<i64>, Failure> {
    let flagged: HashSet<u32> = session.uid_search("FLAGGED").await?;
    let mut besides: Vec<i64> = flagged
        .into_iter()
        .map(i64::from)
        .filter(|uid| !uids.contains(uid))
        .collect();
    besides.sort_unstable();
    Ok(besides)
}

/// The newest `count` mails in a folder that `query` (an IMAP search, as
/// [`words_query`] and [`from_query`] write it) finds.
pub async fn search(
    session: &mut Session,
    folder: &str,
    query: &str,
    count: usize,
    known: &HashMap<i64, Summary>,
    known_validity: i64,
) -> Result<Lines, Failure> {
    let mailbox = session.examine(folder).await?;
    let validity = i64::from(mailbox.uid_validity.unwrap_or(0));
    let mut uids: Vec<i64> = session
        .uid_search(query)
        .await?
        .into_iter()
        .map(i64::from)
        .collect();
    uids.sort_unstable_by(|a, b| b.cmp(a));
    uids.truncate(count);
    let empty = HashMap::new();
    let known = if validity == known_validity {
        known
    } else {
        &empty
    };
    let lines = lines(session, &uids, known).await?;
    Ok(Lines { validity, lines })
}

/// What [`filed`] read of a folder: the UIDs of its mail since the day
/// asked, and the lines of those it didn't know, under its UIDVALIDITY.
pub struct Filed {
    pub validity: i64,
    pub there: Vec<i64>,
    pub lines: Vec<Line>,
}

/// The newest `at_most` mails in `folder` that came since `since`, the ones
/// `kept` doesn't have under `kept_validity` read in full.
pub async fn filed(
    session: &mut Session,
    folder: &str,
    since: NaiveDate,
    at_most: usize,
    kept: &HashSet<i64>,
    kept_validity: i64,
) -> Result<Filed, Failure> {
    let mailbox = session.examine(folder).await?;
    let validity = i64::from(mailbox.uid_validity.unwrap_or(0));
    let mut there: Vec<i64> = session
        .uid_search(format!("SINCE {}", since.format("%d-%b-%Y")))
        .await?
        .into_iter()
        .map(i64::from)
        .collect();
    there.sort_unstable_by(|a, b| b.cmp(a));
    there.truncate(at_most);
    let new: Vec<i64> = there
        .iter()
        .filter(|uid| validity != kept_validity || !kept.contains(uid))
        .copied()
        .collect();
    let lines = lines(session, &new, &HashMap::new()).await?;
    Ok(Filed {
        validity,
        there,
        lines,
    })
}

/// The mails with every one of `words` in them, in a header or the text:
/// `TEXT "word"` for each word, quoted; in UTF-8 when one needs it.
pub fn words_query(words: &[String]) -> Result<String, Failure> {
    let terms = words
        .iter()
        .map(|word| Ok(format!("TEXT {}", quoted(word)?)))
        .collect::<Result<Vec<_>, Failure>>()?
        .join(" ");
    Ok(if words.iter().all(|word| word.is_ascii()) {
        terms
    } else {
        format!("CHARSET UTF-8 {terms}")
    })
}

/// The mails from `address`: `FROM "address"`, in UTF-8 when it needs it.
pub fn from_query(address: &str) -> Result<String, Failure> {
    let term = format!("FROM {}", quoted(address)?);
    Ok(if address.is_ascii() {
        term
    } else {
        format!("CHARSET UTF-8 {term}")
    })
}

/// A string as an IMAP command quotes it. A quoted string can't hold a
/// line break or NUL (RFC 3501): the server would end the command there
/// and run what follows as commands of its own, as a subject written to
/// delete mail would. So text with one is refused, never sent.
pub(crate) fn quoted(text: &str) -> Result<String, Failure> {
    if text.contains(['\r', '\n', '\0']) {
        return Err(Failure::Refused(
            "Postrider can't look for text with a line break in it.".into(),
        ));
    }
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// What the list shows of these mails in the open folder: read or not from
/// the server, the rest from `known`, else read too.
async fn lines(
    session: &mut Session,
    uids: &[i64],
    known: &HashMap<i64, Summary>,
) -> Result<Vec<Line>, Failure> {
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    let set = |uids: &[i64]| {
        uids.iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let fetches: Vec<Fetch> = session
        .uid_fetch(set(uids), "(UID FLAGS)")
        .await?
        .try_collect()
        .await?;
    let marks: HashMap<i64, (bool, bool)> = fetches
        .iter()
        .filter_map(|f| {
            f.uid
                .map(|uid| (i64::from(uid), (is_seen(f), is_flagged(f))))
        })
        .collect();
    let mut read: HashMap<i64, Summary> = HashMap::new();
    let missing: Vec<i64> = uids
        .iter()
        .filter(|uid| !known.contains_key(uid))
        .copied()
        .collect();
    for batch in missing.chunks(BATCH) {
        let fetches: Vec<Fetch> = session
            .uid_fetch(
                set(batch),
                format!("(UID INTERNALDATE BODY.PEEK[]<0.{PEEK}>)"),
            )
            .await?
            .try_collect()
            .await?;
        for fetch in &fetches {
            let Some(uid) = fetch.uid.map(i64::from) else {
                continue;
            };
            let arrived = fetch.internal_date().map(|d| d.with_timezone(&Utc));
            read.insert(uid, summarize(fetch.body().unwrap_or_default(), arrived));
        }
    }
    Ok(uids
        .iter()
        .filter_map(|uid| {
            let summary = known.get(uid).or_else(|| read.get(uid))?.clone();
            let (seen, flagged) = *marks.get(uid)?;
            Some(Line {
                uid: *uid,
                summary,
                seen,
                flagged,
            })
        })
        .collect())
}

/// A mail's header fields, with when it came and whether it's read.
#[derive(Clone)]
pub struct Headed {
    pub uid: u32,
    pub arrived: Option<DateTime<Utc>>,
    pub seen: bool,
    pub header: Vec<u8>,
}

/// The header `fields` of each mail in `folder` that `query` finds, read
/// without marking anything read.
pub async fn headers(
    session: &mut Session,
    folder: &str,
    query: &str,
    fields: &str,
) -> Result<Vec<Headed>, Failure> {
    session.examine(folder).await?;
    let uids: Vec<u32> = session.uid_search(query).await?.into_iter().collect();
    fetch_headers(session, &uids, fields).await
}

/// A folder's mail with its header `fields`, as [`all_headers`] keeps it:
/// under the folder's UIDVALIDITY, with its UIDNEXT when it was read.
#[derive(Clone, Default)]
pub struct KeptHeaders {
    pub validity: u32,
    pub next: u32,
    pub mails: HashMap<u32, Headed>,
}

/// Every mail in `folder` with its header `fields`, read without marking
/// anything read: from `kept`, but what came since, while the folder's UIDs
/// still mean what they did, and with whether each is read now.
pub async fn all_headers(
    session: &mut Session,
    folder: &str,
    fields: &str,
    mut kept: KeptHeaders,
) -> Result<KeptHeaders, Failure> {
    let mailbox = session.examine(folder).await?;
    let validity = mailbox.uid_validity.unwrap_or(0);
    if kept.validity != validity {
        kept = KeptHeaders {
            validity,
            ..KeptHeaders::default()
        };
    }
    kept.next = mailbox.uid_next.unwrap_or(0);
    let there: HashSet<u32> = session.uid_search("ALL").await?;
    let seen: HashSet<u32> = session.uid_search("SEEN").await?;
    kept.mails.retain(|uid, _| there.contains(uid));
    for (uid, mail) in &mut kept.mails {
        mail.seen = seen.contains(uid);
    }
    let mut new: Vec<u32> = there
        .into_iter()
        .filter(|uid| !kept.mails.contains_key(uid))
        .collect();
    new.sort_unstable();
    for headed in fetch_headers(session, &new, fields).await? {
        kept.mails.insert(headed.uid, headed);
    }
    Ok(kept)
}

/// The header `fields` of the mails at `uids` in the folder selected.
async fn fetch_headers(
    session: &mut Session,
    uids: &[u32],
    fields: &str,
) -> Result<Vec<Headed>, Failure> {
    let mut headed = Vec::new();
    for batch in uids.chunks(BATCH * 10) {
        let set = batch
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches: Vec<Fetch> = session
            .uid_fetch(
                set,
                format!("(UID FLAGS INTERNALDATE BODY.PEEK[HEADER.FIELDS ({fields})])"),
            )
            .await?
            .try_collect()
            .await?;
        for fetch in &fetches {
            let Some(uid) = fetch.uid else {
                continue;
            };
            headed.push(Headed {
                uid,
                arrived: fetch.internal_date().map(|d| d.with_timezone(&Utc)),
                seen: is_seen(fetch),
                header: fetch.header().unwrap_or_default().to_vec(),
            });
        }
    }
    Ok(headed)
}

fn is_seen(fetch: &Fetch) -> bool {
    fetch.flags().any(|flag| flag == Flag::Seen)
}

fn is_flagged(fetch: &Fetch) -> bool {
    fetch.flags().any(|flag| flag == Flag::Flagged)
}

/// What the list shows of a mail.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub from_name: String,
    pub from_address: String,
    /// The first it went to, by name, else address.
    pub to_name: String,
    pub subject: String,
    pub snippet: String,
    pub date: DateTime<Utc>,
    /// Its Message-ID, without the brackets; empty when it has none.
    pub message_id: String,
    /// Whom it answers, and where it goes in a conversation.
    pub threading: Threading,
}

/// What joins a mail to others in a conversation: the Message-IDs it
/// answers (In-Reply-To, then References), whom it went to (To and Cc) and
/// whom answers go to (Reply-To). Addresses in lower case.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Threading {
    pub refs: Vec<String>,
    pub to_addresses: Vec<String>,
    pub reply_to: String,
}

impl Threading {
    /// Read from a mail's header, or the start of the mail.
    pub fn read(header: &[u8]) -> Threading {
        mail_parser::MessageParser::new()
            .parse(header)
            .map(|message| Threading::of(&message))
            .unwrap_or_default()
    }

    fn of(message: &mail_parser::Message) -> Threading {
        let ids = |list: Option<mail_parser::TextList>| {
            list.map(|ids| ids.iter().map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let mut refs = ids(message.in_reply_to());
        for id in ids(message.references()) {
            if !refs.contains(&id) {
                refs.push(id);
            }
        }
        let addresses = |list: Option<mail_parser::AddressList>| {
            list.map(|list| {
                list.mailboxes()
                    .filter_map(|mailbox| mailbox.address().map(str::to_lowercase))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        };
        let mut to_addresses = addresses(message.to());
        to_addresses.extend(addresses(message.cc()));
        Threading {
            refs,
            to_addresses,
            reply_to: addresses(message.reply_to())
                .into_iter()
                .next()
                .unwrap_or_default(),
        }
    }
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
    let to = message
        .as_ref()
        .and_then(|m| m.to())
        .and_then(|list| list.first());
    let to_name = to
        .and_then(|t| t.name().map(str::trim).filter(|name| !name.is_empty()))
        .or_else(|| to.and_then(|t| t.address()))
        .unwrap_or_default()
        .to_string();
    let subject = message
        .as_ref()
        .and_then(|m| m.subject())
        .map(one_line)
        .unwrap_or_default();
    let snippet = message
        .as_ref()
        .and_then(|m| m.body_preview(4000))
        .map(|text| crate::preview::preview(&text))
        .unwrap_or_default();
    let dated = message
        .as_ref()
        .and_then(|m| m.date())
        .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0));
    Summary {
        from_name,
        from_address,
        to_name,
        subject,
        snippet,
        date: arrived.or(dated).unwrap_or_else(Utc::now),
        message_id: message
            .as_ref()
            .and_then(|m| m.message_id())
            .unwrap_or_default()
            .to_string(),
        threading: message.as_ref().map(Threading::of).unwrap_or_default(),
    }
}

/// Text on one line, its runs of space and line breaks made one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// TLS that trusts what macOS trusts.
pub(crate) fn tls() -> Result<TlsConnector, Failure> {
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

    /// A mail as [`glances`] reads it: `BODY[HEADER]`, with its blank line,
    /// and the first [`GLANCE`] bytes of `BODY[TEXT]`.
    fn glanced(raw: &[u8]) -> Vec<u8> {
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let text = &raw[split..];
        let text = &text[..text.len().min(GLANCE as usize)];
        glance(raw[..split].to_vec(), Some(text.to_vec()))
    }

    fn with_files(files: &[(&str, usize)]) -> Vec<u8> {
        let mut raw = String::from(
            "From: Shop <billing@shop.test>\r\nTo: me@example.test\r\n\
             Subject: Your invoice\r\nDate: Mon, 2 Nov 2026 09:12:00 +0100\r\n\
             MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n\
             --b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n\
             Thanks for your order. Your invoice is attached.\r\n",
        );
        for (name, size) in files {
            let line = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2\r\n";
            let _ = write!(
                raw,
                "--b\r\nContent-Type: application/pdf; name=\"{name}\"\r\n\
                 Content-Disposition: attachment; filename=\"{name}\"\r\n\
                 Content-Transfer-Encoding: base64\r\n\r\n{}",
                line.repeat(size / line.len() + 1)
            );
        }
        raw.push_str("--b--\r\n");
        raw.into_bytes()
    }

    #[tokio::test]
    async fn folders_are_counted_in_one_round_trip() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (read, mut write) = socket.into_split();
            let mut lines = BufReader::new(read).lines();
            write.write_all(b"* OK ready\r\n").await.unwrap();
            let login = lines.next_line().await.unwrap().unwrap();
            let tag = login.split(' ').next().unwrap();
            write
                .write_all(format!("{tag} OK signed in\r\n").as_bytes())
                .await
                .unwrap();
            // All three come before any answer.
            let mut asked = Vec::new();
            for _ in 0..3 {
                asked.push(lines.next_line().await.unwrap().unwrap());
            }
            // Answered out of order, with news of the open folder between.
            write
                .write_all(
                    b"* 4 EXISTS\r\n\
                      * STATUS \"Rechnungen 2026\" (MESSAGES 7 UNSEEN 2 UIDNEXT 12 UIDVALIDITY 5)\r\n\
                      S1 OK done\r\n\
                      * STATUS Archive (MESSAGES 3 UNSEEN 0 UIDNEXT 4 UIDVALIDITY 5)\r\n\
                      S0 OK done\r\n\
                      S2 NO [NONEXISTENT] no such folder\r\n",
                )
                .await
                .unwrap();
            asked
        });
        let mut session = sign_in(&Login {
            host: "127.0.0.1",
            port,
            username: "anna",
            password: "dev",
        })
        .await
        .unwrap();
        let folders = ["Archive", "Rechnungen 2026", "Gone"].map(String::from);
        let counted = counts(&mut session, &folders, false).await.unwrap();
        assert_eq!(
            server.await.unwrap(),
            [
                "S0 STATUS \"Archive\" (MESSAGES UNSEEN UIDNEXT UIDVALIDITY)",
                "S1 STATUS \"Rechnungen 2026\" (MESSAGES UNSEEN UIDNEXT UIDVALIDITY)",
                "S2 STATUS \"Gone\" (MESSAGES UNSEEN UIDNEXT UIDVALIDITY)",
            ]
        );
        assert_eq!(counted.len(), 2);
        let archive = Counted {
            total: 3,
            unseen: 0,
            next: 4,
            validity: 5,
            modseq: 0,
        };
        assert_eq!(counted["Archive"], archive);
        assert_eq!(counted["Archive"].stamp(), "5:4:3:0");
        assert_eq!(counted["Rechnungen 2026"].unseen, 2);
        assert_eq!(counted["Rechnungen 2026"].total, 7);
    }

    #[test]
    fn a_glance_is_read_as_the_whole_mail() {
        let read = |raw: &[u8]| crate::ask::as_read(raw, uuid::Uuid::nil(), 600);
        // A mail shorter than a glance is all of it.
        let small = with_files(&[("Rechnung.pdf", 20_000)]);
        assert_eq!(glanced(&small), small);
        // One with a big file reads the same: its text and the file's name
        // come before the cut.
        let big = with_files(&[("Rechnung.pdf", 2_000_000)]);
        assert!(glanced(&big).len() < 70_000);
        assert_eq!(read(&glanced(&big)), read(&big));
        // A file that starts after a big one isn't seen.
        let two = with_files(&[("Rechnung.pdf", 2_000_000), ("AGB.pdf", 1_000)]);
        let seen = read(&glanced(&two));
        assert!(seen.contains("Files: Rechnung.pdf\n"), "{seen}");
        assert!(seen.contains("Thanks for your order."));
        assert!(read(&two).contains("Files: Rechnung.pdf, AGB.pdf\n"));
    }

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

    fn listed(path: &str, role: Option<FolderRole>) -> Listed {
        Listed {
            path: path.into(),
            delimiter: Some(".".into()),
            role,
            selectable: true,
        }
    }

    #[test]
    fn folders_read_by_their_names_and_roles() {
        let mut parent = listed("INBOX.Projects", None);
        parent.selectable = false;
        let places = places(vec![
            listed("INBOX", None),
            listed("INBOX.Sent", Some(FolderRole::Sent)),
            listed("INBOX.Gesendet", None),
            listed("INBOX.Archiv", None),
            listed("INBOX.Entw&APw-rfe", None),
            parent,
            listed("INBOX.Projects.Layer", None),
            listed("Rechnungen", None),
        ]);
        let shown: Vec<(&str, Option<FolderRole>)> = places
            .iter()
            .map(|place| (place.name.as_str(), place.role))
            .collect();
        assert_eq!(
            shown,
            [
                ("Sent", Some(FolderRole::Sent)),
                // Sent is taken: this one stays a folder of its own.
                ("Gesendet", None),
                ("Archiv", Some(FolderRole::Archive)),
                ("Entwürfe", Some(FolderRole::Drafts)),
                ("Projects / Layer", None),
                ("Rechnungen", None),
            ]
        );
        assert_eq!(places[2].key(), "archive");
        assert!(places[5].is("rechnungen"));
    }

    #[test]
    fn folder_names_decode_modified_utf7() {
        assert_eq!(folder_name("Entw&APw-rfe"), "Entwürfe");
        assert_eq!(folder_name("Gel&APY-schte Objekte"), "Gelöschte Objekte");
        assert_eq!(folder_name("Tom &- Jerry"), "Tom & Jerry");
        assert_eq!(folder_name("&ZeVnLIqe-"), "日本語");
        assert_eq!(folder_name("Plain"), "Plain");
    }

    #[test]
    fn new_folder_names_encode_as_they_decode() {
        assert_eq!(path_name("Entwürfe"), "Entw&APw-rfe");
        assert_eq!(path_name("Tom & Jerry"), "Tom &- Jerry");
        assert_eq!(path_name("日本語"), "&ZeVnLIqe-");
        for name in ["Bücher & Hefte", "Dev-Kram", "Grüße 🦉"] {
            assert_eq!(folder_name(&path_name(name)), name);
        }
    }

    #[test]
    fn a_search_asks_for_every_word_quoted() {
        let words = |text: &str| {
            text.split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        let query = |text: &str| words_query(&words(text)).ok();
        assert_eq!(
            query("contract anna").as_deref(),
            Some("TEXT \"contract\" TEXT \"anna\"")
        );
        assert_eq!(
            query(r#"say "hi"\"#).as_deref(),
            Some(r#"TEXT "say" TEXT "\"hi\"\\""#)
        );
        assert_eq!(
            query("Entwürfe").as_deref(),
            Some("CHARSET UTF-8 TEXT \"Entwürfe\"")
        );
    }

    #[test]
    fn a_sender_is_asked_for_quoted() {
        assert_eq!(
            from_query("news@coop.se").ok().as_deref(),
            Some("FROM \"news@coop.se\"")
        );
        assert_eq!(
            from_query("nyheter@förening.se").ok().as_deref(),
            Some("CHARSET UTF-8 FROM \"nyheter@förening.se\"")
        );
    }

    #[test]
    fn a_line_break_never_reaches_a_command() {
        let deleting = "x\"\r\nA1 UID STORE 1:* +FLAGS (\\Deleted)\r\nA2 EXPUNGE\r\n";
        for text in [deleting, "a\nb", "a\rb", "a\0b"] {
            assert!(matches!(quoted(text), Err(Failure::Refused(_))), "{text:?}");
            assert!(from_query(text).is_err());
            assert!(words_query(&[text.to_string()]).is_err());
        }
        assert_eq!(quoted("tab\there").ok().as_deref(), Some("\"tab\there\""));
    }
}
