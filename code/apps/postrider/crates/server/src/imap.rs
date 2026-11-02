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
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use postrider_domain::FolderRole;
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
                 (account_id, uid, from_name, from_address, subject, snippet, date, seen, \
                 message_id) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
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
            .bind(&summary.message_id)
            .execute(pool)
            .await?;
            changed = true;
        }
    }
    Ok(changed)
}

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

/// Marks a mail read, as opening it does.
pub async fn mark_read(session: &mut Session, folder: &str, uid: i64) -> Result<(), Failure> {
    session.select(folder).await?;
    session
        .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Seen)")
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}

/// Moves a mail to another folder (MOVE, RFC 6851).
pub async fn move_mail(
    session: &mut Session,
    from: &str,
    uid: i64,
    to: &str,
) -> Result<(), Failure> {
    session.select(from).await?;
    session.uid_mv(uid.to_string(), to).await?;
    Ok(())
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

/// How many of a folder's mails are unread.
pub async fn unseen(session: &mut Session, folder: &str) -> Result<u32, Failure> {
    Ok(session
        .status(folder, "(UNSEEN)")
        .await?
        .unseen
        .unwrap_or(0))
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

/// The mails with every one of `words` in them, in a header or the text:
/// `TEXT "word"` for each word, quoted; in UTF-8 when one needs it.
pub fn words_query(words: &[String]) -> String {
    let terms = words
        .iter()
        .map(|word| format!("TEXT {}", quoted(word)))
        .collect::<Vec<_>>()
        .join(" ");
    if words.iter().all(|word| word.is_ascii()) {
        terms
    } else {
        format!("CHARSET UTF-8 {terms}")
    }
}

/// The mails from `address`: `FROM "address"`, in UTF-8 when it needs it.
pub fn from_query(address: &str) -> String {
    let term = format!("FROM {}", quoted(address));
    if address.is_ascii() {
        term
    } else {
        format!("CHARSET UTF-8 {term}")
    }
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
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
    let seen: HashMap<i64, bool> = fetches
        .iter()
        .filter_map(|f| f.uid.map(|uid| (i64::from(uid), is_seen(f))))
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
            Some(Line {
                uid: *uid,
                summary,
                seen: *seen.get(uid)?,
            })
        })
        .collect())
}

/// A mail's header fields, with when it came and whether it's read.
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
        assert_eq!(
            words_query(&words("contract anna")),
            "TEXT \"contract\" TEXT \"anna\""
        );
        assert_eq!(
            words_query(&words(r#"say "hi"\"#)),
            r#"TEXT "say" TEXT "\"hi\"\\""#
        );
        assert_eq!(
            words_query(&words("Entwürfe")),
            "CHARSET UTF-8 TEXT \"Entwürfe\""
        );
    }

    #[test]
    fn a_sender_is_asked_for_quoted() {
        assert_eq!(from_query("news@coop.se"), "FROM \"news@coop.se\"");
        assert_eq!(
            from_query("nyheter@förening.se"),
            "CHARSET UTF-8 FROM \"nyheter@förening.se\""
        );
    }
}
