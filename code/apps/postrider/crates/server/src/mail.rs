//! The unified Inbox: one watcher per account keeps its copy up to date
//! ([`crate::imap`]) and says where its server stands; the web app reads
//! the whole of it at `GET /api/w/{workspace}/inbox`, and waits there for
//! the next change with `?after=`. A mail opened from it is read at
//! `GET /api/w/{workspace}/mails/{account}/{uid}`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use postrider_domain::{AccountState, AccountStatus, Inbox, MailAccount, MailSummary, Message};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::watch;
use tokio::task::AbortHandle;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap::{self, Failure, Login, Place, Session, Summary};
use crate::message;
use crate::secrets::Secrets;
use crate::state::AppState;

/// How long a server may stay quiet in IDLE before Postrider looks again
/// itself (RFC 2177 asks for under 29 minutes).
const IDLE: Duration = Duration::from_secs(5 * 60);

/// How often a server without IDLE is asked.
const POLL: Duration = Duration::from_secs(2 * 60);

/// How long `?after=` waits for a change before answering with none.
const LONG_POLL: Duration = Duration::from_secs(25);

/// Waits between tries while a server can't be reached: doubling, up to
/// the last.
const RETRY: [u64; 5] = [5, 15, 30, 60, 300];

/// What lists showed of each account's folders: by account and path, the
/// folder's UIDVALIDITY and its mails' lines by UID.
pub(crate) type KeptLines = HashMap<(Uuid, String), (i64, HashMap<i64, Summary>)>;

pub struct Mail {
    pool: PgPool,
    secrets: Secrets,
    /// Goes up whenever an Inbox or an account's state changes.
    version: watch::Sender<i64>,
    statuses: Mutex<HashMap<Uuid, AccountStatus>>,
    watchers: Mutex<HashMap<Uuid, AbortHandle>>,
    /// A connection per account for opening mail, kept between mails: the
    /// watcher's own waits in IDLE.
    readers: Mutex<HashMap<Uuid, Session>>,
    /// Each account's folders, as its server listed them last.
    pub(crate) places: Mutex<HashMap<Uuid, Vec<Place>>>,
    /// What the lists showed of the mail in each account's folders (by
    /// path), under the folder's UIDVALIDITY, so it isn't read again.
    pub(crate) lines: Mutex<KeptLines>,
}

impl Mail {
    pub fn new(pool: PgPool, secrets: Secrets) -> Self {
        Mail {
            pool,
            secrets,
            version: watch::Sender::new(1),
            statuses: Mutex::default(),
            watchers: Mutex::default(),
            readers: Mutex::default(),
            places: Mutex::default(),
            lines: Mutex::default(),
        }
    }

    /// Starts watching every account. The server does this when it starts.
    pub async fn watch_all(self: &Arc<Self>) -> Result<(), sqlx::Error> {
        let accounts: Vec<MailAccount> =
            sqlx::query_scalar("SELECT to_jsonb(a.*) FROM mail_accounts a ORDER BY position")
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .filter_map(|row: serde_json::Value| serde_json::from_value(row).ok())
                .collect();
        for account in accounts {
            self.watch(account);
        }
        Ok(())
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
        let mail = Arc::clone(self);
        let task = tokio::spawn(async move { mail.run(account).await });
        if let Some(old) = self.lock_watchers().insert(id, task.abort_handle()) {
            old.abort();
        }
    }

    /// Stops watching an account that's gone.
    pub fn forget(&self, account_id: Uuid) {
        if let Some(task) = self.lock_watchers().remove(&account_id) {
            task.abort();
        }
        self.lock_statuses().remove(&account_id);
        self.lock_readers().remove(&account_id);
        self.lock_places().remove(&account_id);
        self.lock_lines().retain(|(id, _), _| *id != account_id);
        self.bump();
    }

    /// Stops every watcher, when the server stops.
    pub fn stop(&self) {
        for (_, task) in self.lock_watchers().drain() {
            task.abort();
        }
        self.lock_readers().clear();
    }

    pub fn version(&self) -> i64 {
        *self.version.borrow()
    }

    /// Waits until the version is no longer `after`, or for [`LONG_POLL`].
    pub async fn changed_since(&self, after: i64) {
        let mut version = self.version.subscribe();
        let _ = tokio::time::timeout(LONG_POLL, version.wait_for(|v| *v != after)).await;
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
        let accounts = {
            let statuses = self.lock_statuses();
            ids.iter()
                .map(|id| {
                    statuses.get(id).cloned().unwrap_or(AccountStatus {
                        account_id: *id,
                        state: AccountState::Connecting,
                        message: None,
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
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.account_id, m.uid, m.from_name, m.from_address, m.subject, m.snippet, \
             m.date, m.seen FROM inbox_mails m JOIN mail_accounts a ON a.id = m.account_id \
             WHERE a.workspace_id = $1 ORDER BY m.date DESC, m.uid DESC LIMIT $2",
        )
        .bind(workspace_id)
        .bind(i64::from(imap::KEEP))
        .fetch_all(&self.pool)
        .await?;
        let mails = rows
            .into_iter()
            .map(
                |(account_id, uid, from_name, from_address, subject, snippet, date, seen)| {
                    MailSummary {
                        account_id,
                        folder: None,
                        uid,
                        from_name,
                        from_address,
                        subject,
                        snippet,
                        date,
                        seen,
                        to_name: None,
                    }
                },
            )
            .collect();
        Ok(Inbox {
            version,
            accounts,
            mails,
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
        type Row = (
            String,
            String,
            String,
            String,
            DateTime<Utc>,
            bool,
            i64,
            Option<Vec<u8>>,
        );
        let row: Option<Row> = sqlx::query_as(
            "SELECT m.from_name, m.from_address, m.subject, m.snippet, m.date, m.seen, \
             i.uid_validity, s.raw FROM inbox_mails m \
             JOIN inboxes i ON i.account_id = m.account_id \
             LEFT JOIN mail_sources s ON s.account_id = m.account_id AND s.uid = m.uid \
             WHERE m.account_id = $1 AND m.uid = $2",
        )
        .bind(account_id)
        .bind(uid)
        .fetch_optional(&self.pool)
        .await?;
        let Some((from_name, from_address, subject, snippet, date, seen, validity, kept)) = row
        else {
            return Err(AppError::not_found(
                "This mail isn't in the Inbox any more.",
            ));
        };

        let raw = match kept {
            Some(raw) => raw,
            None => {
                let raw = self
                    .on_server(&account, |session| {
                        Box::pin(imap::source(session, "INBOX", Some(validity), uid))
                    })
                    .await
                    .map_err(|failure| AppError::refused(failure.message()))?
                    .ok_or_else(|| {
                        AppError::not_found("This mail isn't on the server any more.")
                    })?;
                // Kept only while its line is: a mail gone meanwhile keeps nothing.
                sqlx::query(
                    "INSERT INTO mail_sources (account_id, uid, raw) \
                     SELECT account_id, uid, $3 FROM inbox_mails WHERE account_id = $1 AND uid = $2 \
                     ON CONFLICT DO NOTHING",
                )
                .bind(account_id)
                .bind(uid)
                .bind(&raw)
                .execute(&self.pool)
                .await?;
                raw
            }
        };

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
            to_name: None,
        };
        Ok(message::read(&raw, summary))
    }

    /// Marks an opened mail read, on its server, and in the Inbox's copy
    /// when it's there. Offline, it stays unread until it's opened again.
    pub(crate) async fn mark_read(&self, account: MailAccount, folder: String, uid: i64) {
        let marked = self
            .on_server(&account, |session| {
                let folder = folder.clone();
                Box::pin(async move { imap::mark_read(session, &folder, uid).await })
            })
            .await;
        if let Err(failure) = marked {
            tracing::info!(address = %account.address, "couldn't mark read: {}", failure.message());
            return;
        }
        if folder != "INBOX" {
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

    /// Does `work` over the account's connection for reading, opened when
    /// there's none. One the server has closed since is opened again.
    pub(crate) async fn on_server<T>(
        &self,
        account: &MailAccount,
        work: impl for<'s> Fn(&'s mut Session) -> BoxFuture<'s, Result<T, Failure>>,
    ) -> Result<T, Failure> {
        let kept = self.lock_readers().remove(&account.id);
        if let Some(mut session) = kept
            && let Ok(done) = work(&mut session).await
        {
            self.lock_readers().insert(account.id, session);
            return Ok(done);
        }
        let mut session = self.sign_in(account).await?;
        let done = work(&mut session).await?;
        self.lock_readers().insert(account.id, session);
        Ok(done)
    }

    /// Signs in to the account's server with its password from the keychain.
    async fn sign_in(&self, account: &MailAccount) -> Result<Session, Failure> {
        let password = self
            .secrets
            .get(&account.address)
            .await
            .map_err(|e| Failure::Offline(format!("couldn't read the keychain: {e}")))?
            .ok_or_else(|| {
                Failure::Refused(
                    "Its password isn't in the keychain. Add the account again.".into(),
                )
            })?;
        let port = u16::try_from(account.port).unwrap_or(993);
        imap::sign_in(&Login {
            host: &account.host,
            port,
            username: &account.username,
            password: &password,
        })
        .await
    }

    /// Keeps one account's copy up to date for as long as it's watched.
    async fn run(self: Arc<Self>, account: MailAccount) {
        let mut tries = 0;
        loop {
            let Err(failure) = self.follow(&account, &mut tries).await;
            let message = failure.message();
            match &failure {
                Failure::Refused(_) => {
                    tracing::warn!(address = %account.address, "refused: {message}");
                    self.set(account.id, AccountState::Refused, Some(message));
                    // Trying again won't help until the password changes,
                    // which adds the account again.
                    return;
                }
                Failure::Offline(_) => {
                    tracing::info!(address = %account.address, "offline: {message}");
                    self.set(account.id, AccountState::Offline, Some(message));
                }
            }
            let wait = RETRY[tries.min(RETRY.len() - 1)];
            tries += 1;
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
    }

    /// Signs in, copies the Inbox, and follows it until the connection
    /// breaks. Once in, the waits between tries start over.
    async fn follow(
        &self,
        account: &MailAccount,
        tries: &mut usize,
    ) -> Result<Infallible, Failure> {
        let mut session = self.sign_in(account).await?;
        let idle = session.capabilities().await?.has_str("IDLE");
        if imap::sync_inbox(&mut session, &self.pool, account.id).await? {
            self.bump();
        }
        self.set(account.id, AccountState::Online, None);
        *tries = 0;
        loop {
            session = if idle {
                wait_idle(session).await?
            } else {
                tokio::time::sleep(POLL).await;
                session
            };
            if imap::sync_inbox(&mut session, &self.pool, account.id).await? {
                self.bump();
            }
        }
    }

    fn set(&self, account_id: Uuid, state: AccountState, message: Option<String>) {
        let status = AccountStatus {
            account_id,
            state,
            message,
        };
        self.lock_statuses().insert(account_id, status);
        self.bump();
    }

    fn bump(&self) {
        self.version.send_modify(|v| *v += 1);
    }

    fn lock_statuses(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, AccountStatus>> {
        self.statuses.lock().expect("never poisoned")
    }

    fn lock_watchers(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, AbortHandle>> {
        self.watchers.lock().expect("never poisoned")
    }

    fn lock_readers(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Session>> {
        self.readers.lock().expect("never poisoned")
    }

    pub(crate) fn lock_places(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Vec<Place>>> {
        self.places.lock().expect("never poisoned")
    }

    pub(crate) fn lock_lines(&self) -> std::sync::MutexGuard<'_, KeptLines> {
        self.lines.lock().expect("never poisoned")
    }
}

/// Waits in IDLE until the server says something changed, or [`IDLE`]
/// passes.
async fn wait_idle(session: Session) -> Result<Session, Failure> {
    let mut handle = session.idle();
    handle.init().await?;
    {
        // The stop source ends the wait when dropped: keep it until then.
        let (wait, _stop) = handle.wait_with_timeout(IDLE);
        wait.await?;
    }
    Ok(handle.done().await?)
}

#[derive(Deserialize)]
pub struct InboxParams {
    after: Option<i64>,
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
    folder: Option<String>,
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
