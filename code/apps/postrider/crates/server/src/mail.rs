//! The unified Inbox: one watcher per account keeps its copy up to date
//! ([`crate::imap`]) and says where its server stands; the web app reads
//! the whole of it at `GET /api/w/{workspace}/inbox`, and waits there for
//! the next change with `?after=`. A mail opened from it is read at
//! `GET /api/w/{workspace}/mails/{account}/{uid}`.

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use postrider_domain::{AccountState, AccountStatus, Inbox, MailAccount, MailSummary, Message};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::{Notify, watch};
use tokio::task::AbortHandle;
use uuid::Uuid;

use crate::auth::Member;
use crate::conversations::{self, Joins};
use crate::error::AppError;
use crate::folders::Marked;
use crate::imap::{self, Failure, KeptHeaders, Login, Place, Session, Summary};
use crate::message;
use crate::secrets::Secrets;
use crate::send::Uploaded;
use crate::state::AppState;

/// How long a server may stay quiet in IDLE before Postrider looks again
/// itself (RFC 2177 asks for under 29 minutes).
const IDLE: Duration = Duration::from_secs(5 * 60);

/// How often a server without IDLE is asked.
const POLL: Duration = Duration::from_secs(2 * 60);

/// How long `?after=` waits for a change before answering with none.
const LONG_POLL: Duration = Duration::from_secs(25);

/// How long a server may take to answer the end of IDLE before its
/// connection is taken as gone, as one the Mac slept through is.
const ANSWER: Duration = Duration::from_secs(20);

/// How long a sync waits for every account to ask its server.
const SYNC: Duration = Duration::from_secs(30);

/// How long a kept connection may take to answer NOOP before it's taken as
/// gone, and another used.
const STILL_THERE: Duration = Duration::from_secs(10);

/// Waits between tries while a server can't be reached: doubling, up to
/// the last.
const RETRY: [u64; 5] = [5, 15, 30, 60, 300];

/// How many connections an account keeps for reading, beside its watcher's:
/// a search reads that many of its folders at once.
pub(crate) const READERS: usize = 3;

/// What lists showed of each account's folders: by account and path, the
/// folder's UIDVALIDITY and its mails' lines by UID.
pub(crate) type KeptLines = HashMap<(Uuid, String), (i64, HashMap<i64, Summary>)>;

pub struct Mail {
    pub(crate) pool: PgPool,
    secrets: Secrets,
    /// Goes up whenever an Inbox or an account's state changes.
    version: watch::Sender<i64>,
    statuses: Mutex<HashMap<Uuid, AccountStatus>>,
    watchers: Mutex<HashMap<Uuid, Watcher>>,
    /// How many times each account's watcher has asked its server, whether
    /// or not it answered: what a sync waits on.
    rounds: watch::Sender<HashMap<Uuid, u64>>,
    /// Connections per account for reading (a mail, a folder, a search),
    /// kept between uses, up to [`READERS`]: the watcher's own waits in IDLE.
    readers: Mutex<HashMap<Uuid, Vec<Session>>>,
    /// Each account's folders, as its server listed them last: kept here
    /// too (`kept.rs`), and read from there when Postrider starts.
    pub(crate) places: Mutex<HashMap<Uuid, Vec<Place>>>,
    /// What searches found in each account's folders (by path), under the
    /// folder's UIDVALIDITY, so it isn't read again. Each folder's newest
    /// mail is kept in Postgres (`kept.rs`).
    pub(crate) lines: Mutex<KeptLines>,
    /// Asks the keeper of the folders (`kept::keep`) to look now.
    pub(crate) keeping: Notify,
    /// Why setting each account's newsletters aside failed, until it works.
    pub(crate) set_aside_failed: Mutex<HashMap<Uuid, String>>,
    /// What the Newsletters page read of each account's Newsletters (by
    /// path), so it reads only what came since.
    pub(crate) newsletter_headers: Mutex<HashMap<(Uuid, String), KeptHeaders>>,
    /// What marking a folder read marked, for Undo, by the id the page got.
    /// It lasts until the server stops.
    pub(crate) marked: Mutex<HashMap<Uuid, Marked>>,
    /// Files added to mails being written, by the id adding them gave,
    /// until their mail goes or a day has passed (`send::upload`).
    pub(crate) uploads: Mutex<HashMap<Uuid, Uploaded>>,
    /// Held while what waited for a server is done there (`queued.rs`), so
    /// the watcher and the keeper never both do it.
    pub(crate) replaying: tokio::sync::Mutex<()>,
    /// Held while what waits in the outbox is sent (`outbox.rs`), so it
    /// never goes twice.
    pub(crate) sending: tokio::sync::Mutex<()>,
}

impl Mail {
    pub fn new(pool: PgPool, secrets: Secrets) -> Self {
        Mail {
            pool,
            secrets,
            version: watch::Sender::new(1),
            statuses: Mutex::default(),
            watchers: Mutex::default(),
            rounds: watch::Sender::new(HashMap::new()),
            readers: Mutex::default(),
            places: Mutex::default(),
            lines: Mutex::default(),
            keeping: Notify::new(),
            set_aside_failed: Mutex::default(),
            newsletter_headers: Mutex::default(),
            marked: Mutex::default(),
            uploads: Mutex::default(),
            replaying: tokio::sync::Mutex::default(),
            sending: tokio::sync::Mutex::default(),
        }
    }

    /// Starts watching every account. The server does this when it starts.
    pub async fn watch_all(self: &Arc<Self>) -> Result<(), sqlx::Error> {
        for account in self.all_accounts().await? {
            self.watch(account);
        }
        Ok(())
    }

    /// Every workspace's accounts.
    pub(crate) async fn all_accounts(&self) -> Result<Vec<MailAccount>, sqlx::Error> {
        Ok(
            sqlx::query_scalar("SELECT to_jsonb(a.*) FROM mail_accounts a ORDER BY position")
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .filter_map(|row: serde_json::Value| serde_json::from_value(row).ok())
                .collect(),
        )
    }

    /// The workspace's accounts, in their order.
    pub(crate) async fn accounts(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<MailAccount>, sqlx::Error> {
        Ok(sqlx::query_scalar(
            "SELECT to_jsonb(a.*) FROM mail_accounts a WHERE workspace_id = $1 ORDER BY position",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter_map(|row: serde_json::Value| serde_json::from_value(row).ok())
        .collect())
    }

    /// One of the workspace's accounts.
    pub(crate) async fn account(
        &self,
        workspace_id: Uuid,
        account_id: Uuid,
    ) -> Result<MailAccount, AppError> {
        let account: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT to_jsonb(a.*) FROM mail_accounts a WHERE id = $1 AND workspace_id = $2",
        )
        .bind(account_id)
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;
        account
            .and_then(|row| serde_json::from_value(row).ok())
            .ok_or_else(|| AppError::not_found("That account isn't here any more."))
    }

    /// Starts watching an account, or starts again with what it is now.
    pub fn watch(self: &Arc<Self>, account: MailAccount) {
        let id = account.id;
        self.set(id, AccountState::Connecting, None);
        let wake = Arc::new(Notify::new());
        let mail = Arc::clone(self);
        let woken = Arc::clone(&wake);
        let task = tokio::spawn(async move { mail.run(account, woken).await });
        let watcher = Watcher {
            task: task.abort_handle(),
            wake,
        };
        if let Some(old) = self.lock_watchers().insert(id, watcher) {
            old.task.abort();
        }
    }

    /// Asks every account's server now: a watcher waiting in IDLE or
    /// between tries looks at once, and one that was refused tries again.
    /// What's read next goes over new connections, in case the kept ones
    /// died unnoticed. The folders kept here are looked at too, without
    /// waiting for them. Done once each account has asked, or after [`SYNC`].
    pub async fn sync(self: &Arc<Self>, workspace_id: Uuid) -> Result<(), sqlx::Error> {
        let accounts = self.accounts(workspace_id).await?;
        self.keep_now();
        let mut rounds = self.rounds.subscribe();
        let before = rounds.borrow_and_update().clone();
        for account in &accounts {
            self.lock_readers().remove(&account.id);
            let wake = self
                .lock_watchers()
                .get(&account.id)
                .filter(|watcher| !watcher.task.is_finished())
                .map(|watcher| Arc::clone(&watcher.wake));
            match wake {
                Some(wake) => wake.notify_one(),
                None => self.watch(account.clone()),
            }
        }
        let asked = |now: &HashMap<Uuid, u64>| {
            accounts
                .iter()
                .all(|account| now.get(&account.id) > before.get(&account.id))
        };
        let _ = tokio::time::timeout(SYNC, rounds.wait_for(asked)).await;
        Ok(())
    }

    /// Stops watching an account that's gone.
    pub fn forget(&self, account_id: Uuid) {
        if let Some(watcher) = self.lock_watchers().remove(&account_id) {
            watcher.task.abort();
        }
        self.lock_statuses().remove(&account_id);
        self.lock_readers().remove(&account_id);
        self.lock_places().remove(&account_id);
        self.lock_lines().retain(|(id, _), _| *id != account_id);
        self.bump();
    }

    /// Stops every watcher, when the server stops. The Inbox's long polls
    /// answer now, so the server needn't wait for them.
    pub fn stop(&self) {
        for (_, watcher) in self.lock_watchers().drain() {
            watcher.task.abort();
        }
        self.lock_readers().clear();
        self.bump();
    }

    pub fn version(&self) -> i64 {
        *self.version.borrow()
    }

    /// Waits until the version is no longer `after`, or for [`LONG_POLL`].
    pub async fn changed_since(&self, after: i64) {
        let _ = tokio::time::timeout(LONG_POLL, self.changed_from(after)).await;
    }

    /// Waits until the version is no longer `after`, however long it takes.
    pub(crate) async fn changed_from(&self, after: i64) {
        let mut version = self.version.subscribe();
        let _ = version.wait_for(|v| *v != after).await;
    }

    /// The workspace's Inbox: its accounts' states, and their newest mail,
    /// newest first.
    pub async fn inbox(&self, workspace_id: Uuid) -> Result<Inbox, sqlx::Error> {
        // Read before the copy, so a change while reading shows next time.
        let version = self.version();
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM mail_accounts WHERE workspace_id = $1 ORDER BY position",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        let mut accounts: Vec<AccountStatus> = {
            let statuses = self.lock_statuses();
            ids.iter()
                .map(|id| {
                    statuses.get(id).cloned().unwrap_or(AccountStatus {
                        account_id: *id,
                        state: AccountState::Connecting,
                        message: None,
                        waiting: 0,
                    })
                })
                .collect()
        };
        type Row = (
            Uuid,
            i64,
            String,
            String,
            String,
            String,
            DateTime<Utc>,
            bool,
            bool,
            String,
            Option<Vec<String>>,
            Vec<String>,
            String,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.account_id, m.uid, m.from_name, m.from_address, m.subject, m.snippet, \
             m.date, m.seen, m.flagged, m.message_id, m.refs, m.to_addresses, m.reply_to \
             FROM inbox_mails m JOIN mail_accounts a ON a.id = m.account_id \
             WHERE a.workspace_id = $1 ORDER BY m.date DESC, m.uid DESC LIMIT $2",
        )
        .bind(workspace_id)
        .bind(i64::from(imap::KEEP))
        .fetch_all(&self.pool)
        .await?;
        let inbox: Vec<(MailSummary, Joins)> = rows
            .into_iter()
            .map(
                |(
                    account_id,
                    uid,
                    from_name,
                    from_address,
                    subject,
                    snippet,
                    date,
                    seen,
                    flagged,
                    message_id,
                    refs,
                    to_addresses,
                    reply_to,
                )| {
                    let summary = MailSummary {
                        account_id,
                        folder: None,
                        uid,
                        from_name,
                        from_address,
                        subject,
                        snippet,
                        date,
                        seen,
                        flagged,
                        to_name: None,
                        unsent: false,
                    };
                    let joins = Joins {
                        message_id,
                        refs: refs.unwrap_or_default(),
                        to_addresses,
                        reply_to,
                    };
                    (summary, joins)
                },
            )
            .collect();
        let mails = inbox.iter().map(|(summary, _)| summary.clone()).collect();
        let own: HashSet<String> =
            sqlx::query_scalar("SELECT lower(address) FROM mail_accounts WHERE workspace_id = $1")
                .bind(workspace_id)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .collect();
        let filed = self.filed(workspace_id).await?;
        let conversations = conversations::conversations(inbox, filed, &own);
        self.count_waiting(&mut accounts).await?;
        Ok(Inbox {
            version,
            accounts,
            mails,
            conversations,
        })
    }

    /// A mail of the workspace's, opened: from the copy of its source kept
    /// here, else from its server, which then keeps it. Opening marks it
    /// read on the server, as any mail app does, without waiting for it.
    pub async fn open(
        self: &Arc<Self>,
        workspace_id: Uuid,
        account_id: Uuid,
        uid: i64,
    ) -> Result<Message, AppError> {
        let account = self.account(workspace_id, account_id).await?;
        type Row = (String, String, String, String, DateTime<Utc>, bool, bool);
        let row: Option<Row> = sqlx::query_as(
            "SELECT from_name, from_address, subject, snippet, date, seen, flagged \
             FROM inbox_mails WHERE account_id = $1 AND uid = $2",
        )
        .bind(account_id)
        .bind(uid)
        .fetch_optional(&self.pool)
        .await?;
        let Some((from_name, from_address, subject, snippet, date, seen, flagged)) = row else {
            return Err(AppError::not_found(
                "This mail isn't in the Inbox any more.",
            ));
        };
        let raw = self.inbox_source(&account, uid).await?;

        if !seen {
            let mail = Arc::clone(self);
            tokio::spawn(async move { mail.mark_read(account, "INBOX".into(), uid).await });
        }

        let summary = MailSummary {
            account_id,
            folder: None,
            uid,
            from_name,
            from_address,
            subject,
            snippet,
            date,
            seen: true,
            flagged,
            to_name: None,
            unsent: false,
        };
        Ok(message::opened(&raw, summary))
    }

    /// A mail's source, and the folder it's in: the Inbox, or the one `key`
    /// names. From the copy kept here, else from its server, and kept
    /// while its line is. Reading it marks nothing.
    pub(crate) async fn source_of(
        &self,
        account: &MailAccount,
        key: Option<&str>,
        uid: i64,
    ) -> Result<(Place, Vec<u8>), AppError> {
        let Some(key) = key else {
            return Ok((Place::inbox(), self.inbox_source(account, uid).await?));
        };
        let refused = |failure: Failure| AppError::refused(failure.message());
        let place = self
            .place(account, key)
            .await
            .map_err(refused)?
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "{} has no folder called {key} any more.",
                    account.name
                ))
            })?;
        if let Some(raw) = self.kept_source(account.id, &place.path, uid).await? {
            return Ok((place, raw));
        }
        let validity = self
            .kept_line(account.id, &place.path, uid)
            .await?
            .map(|(_, validity)| validity);
        let (path, at, validity) = self
            .on_server_at(account, &place.path, uid, validity)
            .await?;
        let raw = self
            .on_server(account, |session| {
                let path = path.clone();
                Box::pin(async move { imap::source(session, &path, validity, at).await })
            })
            .await
            .map_err(refused)?
            .ok_or_else(|| AppError::not_found("This mail isn't in its folder any more."))?;
        self.keep_source(account.id, &place.path, uid, &raw).await?;
        Ok((place, raw))
    }

    /// An Inbox mail's source: the copy kept here once it was opened, else
    /// read from its server, and kept.
    async fn inbox_source(&self, account: &MailAccount, uid: i64) -> Result<Vec<u8>, AppError> {
        let row: Option<(i64, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT i.uid_validity, s.raw FROM inbox_mails m \
             JOIN inboxes i ON i.account_id = m.account_id \
             LEFT JOIN mail_sources s ON s.account_id = m.account_id AND s.uid = m.uid \
             WHERE m.account_id = $1 AND m.uid = $2",
        )
        .bind(account.id)
        .bind(uid)
        .fetch_optional(&self.pool)
        .await?;
        let Some((validity, kept)) = row else {
            return Err(AppError::not_found(
                "This mail isn't in the Inbox any more.",
            ));
        };
        if let Some(raw) = kept {
            return Ok(raw);
        }
        let (path, at, validity) = self
            .on_server_at(account, "INBOX", uid, Some(validity))
            .await?;
        let raw = self
            .on_server(account, |session| {
                let path = path.clone();
                Box::pin(async move { imap::source(session, &path, validity, at).await })
            })
            .await
            .map_err(|failure| AppError::refused(failure.message()))?
            .ok_or_else(|| AppError::not_found("This mail isn't on the server any more."))?;
        // Kept only while its line is: a mail gone meanwhile keeps nothing.
        sqlx::query(
            "INSERT INTO mail_sources (account_id, uid, raw) \
             SELECT account_id, uid, $3 FROM inbox_mails WHERE account_id = $1 AND uid = $2 \
             ON CONFLICT DO NOTHING",
        )
        .bind(account.id)
        .bind(uid)
        .bind(&raw)
        .execute(&self.pool)
        .await?;
        Ok(raw)
    }

    /// Where its server has a mail this Mac's copy shows in the folder at
    /// `path`, under `uid` and the folder's UIDVALIDITY: there, unless it
    /// was moved while its server couldn't be reached (`queued.rs`).
    async fn on_server_at(
        &self,
        account: &MailAccount,
        path: &str,
        uid: i64,
        validity: Option<i64>,
    ) -> Result<(String, i64, Option<i64>), AppError> {
        if uid > 0 {
            return Ok((path.to_string(), uid, validity));
        }
        let (path, uid, validity) = self
            .origin(account.id, uid)
            .await?
            .ok_or_else(|| AppError::not_found("This mail isn't here any more."))?;
        Ok((path, uid, Some(validity)))
    }

    /// The header and the start of each of these mails, for judging and
    /// sorting: in the Inbox, or the folder `key` names. An Inbox mail kept
    /// here once opened is read whole from here; the rest come in one
    /// fetch, marking nothing and keeping nothing. A mail that isn't there
    /// any more is left out.
    pub(crate) async fn glances_of(
        &self,
        account: &MailAccount,
        key: Option<&str>,
        uids: &[u32],
    ) -> Result<HashMap<u32, Vec<u8>>, AppError> {
        let refused = |failure: Failure| AppError::refused(failure.message());
        let mut glances = HashMap::new();
        let (path, validity) = match key {
            None => {
                let wanted: Vec<i64> = uids.iter().copied().map(i64::from).collect();
                let validity: Option<i64> =
                    sqlx::query_scalar("SELECT uid_validity FROM inboxes WHERE account_id = $1")
                        .bind(account.id)
                        .fetch_optional(&self.pool)
                        .await?;
                let Some(validity) = validity else {
                    return Ok(glances);
                };
                let kept: Vec<(i64, Vec<u8>)> = sqlx::query_as(
                    "SELECT uid, raw FROM mail_sources WHERE account_id = $1 AND uid = ANY($2)",
                )
                .bind(account.id)
                .bind(&wanted)
                .fetch_all(&self.pool)
                .await?;
                for (uid, raw) in kept {
                    if let Ok(uid) = u32::try_from(uid) {
                        glances.insert(uid, raw);
                    }
                }
                ("INBOX".to_string(), Some(validity))
            }
            Some(key) => {
                let place = self.place(account, key).await.map_err(refused)?;
                let Some(place) = place else {
                    return Ok(glances);
                };
                (place.path, None)
            }
        };
        let wanted: Vec<u32> = uids
            .iter()
            .copied()
            .filter(|uid| !glances.contains_key(uid))
            .collect();
        if wanted.is_empty() {
            return Ok(glances);
        }
        let read = self
            .on_server(account, |session| {
                let path = path.clone();
                let wanted = wanted.clone();
                Box::pin(async move { imap::glances(session, &path, validity, &wanted).await })
            })
            .await
            .map_err(refused)?;
        glances.extend(read.unwrap_or_default());
        Ok(glances)
    }

    /// Takes a mail's line out of the Inbox's copy at once, when it has
    /// left the Inbox: before the watcher hears so from its server.
    pub(crate) async fn forget_inbox_line(
        &self,
        account_id: Uuid,
        uid: i64,
    ) -> Result<(), sqlx::Error> {
        let deleted = sqlx::query("DELETE FROM inbox_mails WHERE account_id = $1 AND uid = $2")
            .bind(account_id)
            .bind(uid)
            .execute(&self.pool)
            .await?;
        if deleted.rows_affected() > 0 {
            self.bump();
        }
        Ok(())
    }

    /// Marks an opened mail read, on its server, and in the copy kept here
    /// of its folder. While its server can't be reached, that waits for it
    /// (`queued.rs`).
    pub(crate) async fn mark_read(&self, account: MailAccount, folder: String, uid: i64) {
        let mut waits = self.waits(account.id, &folder, uid).await.unwrap_or(true);
        if !waits {
            let marked = self
                .on_server(&account, |session| {
                    let folder = folder.clone();
                    Box::pin(async move { imap::mark_read(session, &folder, uid).await })
                })
                .await;
            match marked {
                Ok(()) => {}
                Err(Failure::Offline(why)) => {
                    tracing::info!(address = %account.address, "couldn't mark read, so it waits: {why}");
                    waits = true;
                }
                Err(failure) => {
                    tracing::info!(address = %account.address, "couldn't mark read: {}", failure.message());
                    return;
                }
            }
        }
        if waits {
            if let Err(e) = self
                .queue_marks(account.id, &folder, &[uid], Some(true), None)
                .await
            {
                tracing::warn!("couldn't keep a mail read for its server: {e}");
                return;
            }
            self.keep_now();
        }
        if folder != "INBOX" {
            if let Err(e) = self.seen_kept(account.id, &folder, &[uid], true).await {
                tracing::warn!("couldn't keep a mail read: {e}");
            }
            return;
        }
        let updated =
            sqlx::query("UPDATE inbox_mails SET seen = true WHERE account_id = $1 AND uid = $2")
                .bind(account.id)
                .bind(uid)
                .execute(&self.pool)
                .await;
        if updated.is_ok_and(|done| done.rows_affected() > 0) {
            self.bump();
        }
    }

    /// Does `work` once, over one of the account's connections for reading
    /// that still answers, opened when none is free. One the server has
    /// closed since is passed over before the work begins, never after.
    pub(crate) async fn on_server<T>(
        &self,
        account: &MailAccount,
        work: impl for<'s> Fn(&'s mut Session) -> BoxFuture<'s, Result<T, Failure>>,
    ) -> Result<T, Failure> {
        let (done, session) = once_ready(
            || self.lock_readers().get_mut(&account.id).and_then(Vec::pop),
            |session: &mut Session| {
                Box::pin(async move {
                    matches!(
                        tokio::time::timeout(STILL_THERE, session.noop()).await,
                        Ok(Ok(()))
                    )
                })
            },
            || self.sign_in(account),
            work,
        )
        .await?;
        self.keep_reader(account.id, session);
        Ok(done)
    }

    /// Keeps a connection for the next reading, unless enough are kept.
    fn keep_reader(&self, account_id: Uuid, session: Session) {
        let mut readers = self.lock_readers();
        let kept = readers.entry(account_id).or_default();
        if kept.len() < READERS {
            kept.push(session);
        }
    }

    /// Signs in to the account's server with its password from the keychain.
    async fn sign_in(&self, account: &MailAccount) -> Result<Session, Failure> {
        let password = self.password(account).await?;
        let port = u16::try_from(account.port).unwrap_or(993);
        imap::sign_in(&Login {
            host: &account.host,
            port,
            username: &account.username,
            password: &password,
        })
        .await
    }

    /// The account's password, from the keychain: for IMAP and SMTP alike.
    pub(crate) async fn password(&self, account: &MailAccount) -> Result<String, Failure> {
        self.secrets
            .get(&account.address)
            .await
            .map_err(|e| Failure::Offline(format!("couldn't read the keychain: {e}")))?
            .ok_or_else(|| {
                Failure::Refused(
                    "Its password isn't in the keychain. Add the account again.".into(),
                )
            })
    }

    /// Keeps one account's copy up to date for as long as it's watched.
    /// `wake` asks it to look now.
    async fn run(self: Arc<Self>, account: MailAccount, wake: Arc<Notify>) {
        let mut tries = 0;
        loop {
            let Err(failure) = self.follow(&account, &mut tries, &wake).await;
            let message = failure.message();
            match &failure {
                Failure::Refused(_) => {
                    tracing::warn!(address = %account.address, "refused: {message}");
                    self.set(account.id, AccountState::Refused, Some(message));
                    self.asked(account.id);
                    // Trying again won't help until the password changes,
                    // which adds the account again, or a sync asks.
                    return;
                }
                Failure::Offline(_) => {
                    tracing::info!(address = %account.address, "offline: {message}");
                    self.set(account.id, AccountState::Offline, Some(message));
                    self.asked(account.id);
                }
            }
            let wait = RETRY[tries.min(RETRY.len() - 1)];
            tries += 1;
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(wait)) => {}
                () = wake.notified() => {}
            }
        }
    }

    /// Signs in, copies the Inbox, and follows it until the connection
    /// breaks. Once in, the waits between tries start over.
    async fn follow(
        &self,
        account: &MailAccount,
        tries: &mut usize,
        wake: &Notify,
    ) -> Result<Infallible, Failure> {
        let began = Instant::now();
        let mut session = self.sign_in(account).await?;
        let idle = session.capabilities().await?.has_str("IDLE");
        // What was done while it couldn't be reached, before the Inbox is
        // read: a connection that broke meanwhile breaks the read too.
        if let Err(failure) = self.replay_over(account, &mut session).await {
            tracing::info!(
                address = %account.address,
                "couldn't do what waited for its server: {}",
                failure.message()
            );
        }
        if imap::sync_inbox(&mut session, &self.pool, account.id).await? {
            self.bump();
        }
        self.set(account.id, AccountState::Online, None);
        // The keeper sends what's in the outbox (`outbox.rs`) now.
        self.keep_now();
        tracing::debug!(address = %account.address, "online in {} ms", began.elapsed().as_millis());
        let set_aside = self.set_newsletters_aside(account).await;
        {
            let mut failed = self.set_aside_failed.lock().expect("never poisoned");
            match set_aside {
                Ok(()) => failed.remove(&account.id),
                Err(failure) => {
                    tracing::warn!(
                        address = %account.address,
                        "couldn't set the newsletters aside: {}",
                        failure.message()
                    );
                    failed.insert(account.id, failure.message())
                }
            };
        }
        self.asked(account.id);
        *tries = 0;
        loop {
            session = if idle {
                wait_idle(session, wake).await?
            } else {
                tokio::select! {
                    () = tokio::time::sleep(POLL) => {}
                    () = wake.notified() => {}
                }
                session
            };
            if imap::sync_inbox(&mut session, &self.pool, account.id).await? {
                self.bump();
            }
            self.asked(account.id);
        }
    }

    /// Counts that an account's watcher has asked its server, after its
    /// state is set.
    fn asked(&self, account_id: Uuid) {
        self.rounds
            .send_modify(|rounds| *rounds.entry(account_id).or_default() += 1);
    }

    /// Where an account's server stands and why, once its watcher has looked.
    pub(crate) fn status_of(&self, account_id: Uuid) -> Option<AccountStatus> {
        self.lock_statuses().get(&account_id).cloned()
    }

    /// Where an account's server stands, once its watcher has looked.
    pub(crate) fn state_of(&self, account_id: Uuid) -> Option<AccountState> {
        self.lock_statuses()
            .get(&account_id)
            .map(|status| status.state)
    }

    fn set(&self, account_id: Uuid, state: AccountState, message: Option<String>) {
        let status = AccountStatus {
            account_id,
            state,
            message,
            waiting: 0,
        };
        self.lock_statuses().insert(account_id, status);
        self.bump();
    }

    pub(crate) fn bump(&self) {
        self.version.send_modify(|v| *v += 1);
    }

    fn lock_statuses(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, AccountStatus>> {
        self.statuses.lock().expect("never poisoned")
    }

    fn lock_watchers(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Watcher>> {
        self.watchers.lock().expect("never poisoned")
    }

    fn lock_readers(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Vec<Session>>> {
        self.readers.lock().expect("never poisoned")
    }

    pub(crate) fn lock_places(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Vec<Place>>> {
        self.places.lock().expect("never poisoned")
    }

    pub(crate) fn lock_lines(&self) -> std::sync::MutexGuard<'_, KeptLines> {
        self.lines.lock().expect("never poisoned")
    }

    pub(crate) fn lock_marked(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Marked>> {
        self.marked.lock().expect("never poisoned")
    }
}

/// An account's watcher: its task, and what asks it to look now.
struct Watcher {
    task: AbortHandle,
    wake: Arc<Notify>,
}

/// Waits in IDLE until the server says something changed, [`IDLE`] passes
/// or `wake` asks. A server that doesn't answer the end of it within
/// [`ANSWER`] is taken as gone.
async fn wait_idle(session: Session, wake: &Notify) -> Result<Session, Failure> {
    let mut handle = session.idle();
    handle.init().await?;
    {
        // The stop source ends the wait when dropped: keep it until then.
        let (wait, stop) = handle.wait_with_timeout(IDLE);
        tokio::pin!(wait);
        tokio::select! {
            done = &mut wait => {
                done?;
            }
            () = wake.notified() => {
                drop(stop);
                wait.await?;
            }
        }
    }
    let done = tokio::time::timeout(ANSWER, handle.done())
        .await
        .map_err(|_| Failure::Offline("its server stopped answering".into()))?;
    Ok(done?)
}

/// Does `work` once, over the first of the `kept` sessions that is still
/// `ready`, else one it `open`s, and gives the session back with what the
/// work did. Only a session that fails before the work begins is passed
/// over: work that failed may have been done all the same (an APPEND to
/// Sent before the connection dropped, a MOVE before a later STATUS), and
/// doing it again would do it twice, or find nothing left to do. A session
/// whose work failed isn't kept.
async fn once_ready<S, T, Opened>(
    mut kept: impl FnMut() -> Option<S>,
    ready: impl for<'s> Fn(&'s mut S) -> BoxFuture<'s, bool>,
    open: impl FnOnce() -> Opened,
    work: impl for<'s> Fn(&'s mut S) -> BoxFuture<'s, Result<T, Failure>>,
) -> Result<(T, S), Failure>
where
    Opened: Future<Output = Result<S, Failure>>,
{
    let mut answering = None;
    while let Some(mut session) = kept() {
        if ready(&mut session).await {
            answering = Some(session);
            break;
        }
    }
    let mut session = match answering {
        Some(session) => session,
        None => open().await?,
    };
    let done = work(&mut session).await?;
    Ok((done, session))
}

#[derive(Deserialize)]
pub struct InboxParams {
    after: Option<i64>,
}

/// `POST /api/w/{workspace}/mail/sync`: asks every account's server now, and
/// answers once each has (or after a while).
pub async fn sync(State(state): State<AppState>, member: Member) -> Result<StatusCode, AppError> {
    state.mail.sync(member.actor.workspace_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/w/{workspace}/inbox[?after=<version>]`: the unified Inbox;
/// with `after`, once it's no longer that version (or after a while).
pub async fn inbox(
    State(state): State<AppState>,
    member: Member,
    Query(params): Query<InboxParams>,
) -> Result<Json<Inbox>, AppError> {
    if let Some(after) = params.after {
        state.mail.changed_since(after).await;
    }
    Ok(Json(state.mail.inbox(member.actor.workspace_id).await?))
}

#[derive(Deserialize)]
pub struct OpenParams {
    pub(crate) folder: Option<String>,
}

/// `GET /api/w/{workspace}/mails/{account}/{uid}[?folder=<key>]`: a mail
/// from the Inbox, or from a folder, opened, which marks it read.
pub async fn open(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid)): Path<(Uuid, Uuid, i64)>,
    Query(params): Query<OpenParams>,
) -> Result<Json<Message>, AppError> {
    let workspace_id = member.actor.workspace_id;
    Ok(Json(match params.folder {
        Some(folder) => {
            state
                .mail
                .open_in(workspace_id, account_id, &folder, uid)
                .await?
        }
        None => state.mail.open(workspace_id, account_id, uid).await?,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A connection: whether it still answers, and the work done over it.
    struct Fake {
        answers: bool,
        done: Arc<AtomicUsize>,
    }

    async fn run(
        kept: Vec<bool>,
        fails: bool,
        done: &Arc<AtomicUsize>,
    ) -> (Result<bool, Failure>, usize) {
        let mut kept: Vec<Fake> = kept
            .into_iter()
            .map(|answers| Fake {
                answers,
                done: done.clone(),
            })
            .collect();
        let opened = AtomicUsize::new(0);
        let result = once_ready(
            || kept.pop(),
            |fake: &mut Fake| Box::pin(async move { fake.answers }),
            || async {
                opened.fetch_add(1, Ordering::SeqCst);
                Ok(Fake {
                    answers: true,
                    done: done.clone(),
                })
            },
            |fake: &mut Fake| {
                Box::pin(async move {
                    // Done on the server, whatever is answered after.
                    fake.done.fetch_add(1, Ordering::SeqCst);
                    if fails {
                        Err(Failure::Offline("the connection broke".into()))
                    } else {
                        Ok(fake.answers)
                    }
                })
            },
        )
        .await
        .map(|(done, _)| done);
        (result, opened.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn work_that_failed_is_never_done_again() {
        let done = Arc::new(AtomicUsize::new(0));
        let (result, opened) = run(vec![true], true, &done).await;
        assert!(result.is_err());
        assert_eq!((done.load(Ordering::SeqCst), opened), (1, 0));
        let done = Arc::new(AtomicUsize::new(0));
        let (result, opened) = run(Vec::new(), true, &done).await;
        assert!(result.is_err());
        assert_eq!((done.load(Ordering::SeqCst), opened), (1, 1));
    }

    #[tokio::test]
    async fn a_kept_connection_that_stopped_answering_is_passed_over_first() {
        let done = Arc::new(AtomicUsize::new(0));
        // Popped from the end: the gone one first, then one that answers.
        let (result, opened) = run(vec![true, false], false, &done).await;
        assert!(result.is_ok_and(|kept| kept));
        assert_eq!((done.load(Ordering::SeqCst), opened), (1, 0));
        let done = Arc::new(AtomicUsize::new(0));
        let (result, opened) = run(vec![false, false], false, &done).await;
        assert!(result.is_ok());
        assert_eq!((done.load(Ordering::SeqCst), opened), (1, 1));
    }
}
