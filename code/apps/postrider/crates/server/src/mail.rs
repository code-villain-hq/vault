//! The unified Inbox: one watcher per account keeps its copy up to date
//! ([`crate::imap`]) and says where its server stands; the web app reads
//! the whole of it at `GET /api/w/{workspace}/inbox`, and waits there for
//! the next change with `?after=`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use postrider_domain::{AccountState, AccountStatus, Inbox, MailAccount, MailSummary};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::watch;
use tokio::task::AbortHandle;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap::{self, Failure, Login, Session};
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

pub struct Mail {
    pool: PgPool,
    secrets: Secrets,
    /// Goes up whenever an Inbox or an account's state changes.
    version: watch::Sender<i64>,
    statuses: Mutex<HashMap<Uuid, AccountStatus>>,
    watchers: Mutex<HashMap<Uuid, AbortHandle>>,
}

impl Mail {
    pub fn new(pool: PgPool, secrets: Secrets) -> Self {
        Mail {
            pool,
            secrets,
            version: watch::Sender::new(1),
            statuses: Mutex::default(),
            watchers: Mutex::default(),
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
        self.bump();
    }

    /// Stops every watcher, when the server stops.
    pub fn stop(&self) {
        for (_, task) in self.lock_watchers().drain() {
            task.abort();
        }
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
                        uid,
                        from_name,
                        from_address,
                        subject,
                        snippet,
                        date,
                        seen,
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
        let mut session = imap::sign_in(&Login {
            host: &account.host,
            port,
            username: &account.username,
            password: &password,
        })
        .await?;
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
