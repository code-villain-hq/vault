//! What this Mac keeps of every account's folders but the Inbox, so the
//! sidebar and each folder show at once, and offline: the folders as their
//! server listed them last, with what STATUS counted in each
//! (`kept_folders`); each folder's newest [`NEWEST`] mails as its list
//! shows them (`folder_mails`); and the whole of the newest [`AHEAD`] of
//! those, read ahead so they open at once (`folder_sources`). The Inbox
//! has its own copy (`inbox_mails`, [`imap::sync_inbox`]); the whole of
//! its newest [`AHEAD_INBOX`] is read ahead here too (`mail_sources`).
//!
//! The keeper ([`keep`]) brings it up to date in the background: when an
//! Inbox or an account changes, when the app asks for the folders, and
//! every [`EVERY`] besides. Each account's folders are listed and counted
//! in one round trip; only a folder whose count changed is read again, and
//! of it only what this Mac doesn't have yet. What Postrider does to mail
//! (opening, moving, marking read) changes the copy here as soon as the
//! server has done it, and the keeper looks again; while it can't be
//! reached, at once (`queued.rs`), and the keeper does it there first.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use postrider_domain::{AccountState, FolderRole, MailAccount, MailSummary};
use uuid::Uuid;

use crate::folders::{role_name, summary};
use crate::imap::{self, Counted, Failure, Line, Lines, Place, Summary};
use crate::mail::Mail;

/// How many of a folder's newest mails are kept, and its list shows.
pub(crate) const NEWEST: u32 = 100;

/// How many of a folder's newest mails are read whole ahead of opening.
const AHEAD: i64 = 25;

/// How many of an Inbox's newest mails are read whole ahead of opening.
const AHEAD_INBOX: i64 = 50;

/// The biggest mail read ahead, in bytes: a bigger one is read when it's
/// opened, and kept then.
const AT_MOST: u32 = 1024 * 1024;

/// How often the keeper looks, when nothing asks it to sooner: for mail
/// filed or read elsewhere.
const EVERY: Duration = Duration::from_secs(60);

/// How long the keeper waits once asked, for what comes with it.
const SETTLE: Duration = Duration::from_secs(2);

/// Mails too big to read ahead, by account, folder and UID: asked about
/// once each.
type TooBig = HashSet<(Uuid, String, i64)>;

/// Keeps every account's folders on this Mac for as long as Postrider
/// runs, each whose server answers.
pub async fn keep(mail: Arc<Mail>) {
    let mut too_big = TooBig::new();
    loop {
        let mut seen = mail.version();
        let accounts = match mail.all_accounts().await {
            Ok(accounts) => accounts
                .into_iter()
                .filter(|account| mail.state_of(account.id) == Some(AccountState::Online))
                .collect(),
            Err(e) => {
                tracing::warn!("couldn't read the accounts to keep their folders: {e}");
                Vec::new()
            }
        };
        let mut counted = Vec::new();
        let mut changed = false;
        for account in &accounts {
            // What was done while it couldn't be reached goes first, so
            // what's read next has it.
            if let Err(problem) = mail.replay(account).await {
                tracing::info!(
                    address = %account.address,
                    "couldn't do what waited for its server: {}",
                    problem.message()
                );
                continue;
            }
            // Then what was sent or kept while it couldn't be reached.
            mail.send_waiting(account).await;
            match mail.count(account).await {
                Ok((kept, folders)) => {
                    changed |= kept;
                    counted.push((account, folders));
                }
                Err(problem) => {
                    tracing::info!(address = %account.address, "couldn't count its folders: {problem}");
                }
            }
        }
        if changed {
            mail.bump();
            seen = mail.version();
        }
        changed = false;
        for (account, folders) in &counted {
            match mail.read_changed(account, folders).await {
                Ok(kept) => changed |= kept,
                Err(problem) => {
                    tracing::info!(address = %account.address, "couldn't read its folders: {problem}");
                }
            }
        }
        if changed {
            mail.bump();
            seen = mail.version();
        }
        for (account, _) in &counted {
            if let Err(problem) = mail.read_ahead(account, &mut too_big).await {
                tracing::info!(address = %account.address, "couldn't read its mail ahead: {problem}");
            }
        }
        tokio::select! {
            () = tokio::time::sleep(EVERY) => {}
            () = mail.changed_from(seen) => {}
            () = mail.keeping.notified() => {}
        }
        tokio::time::sleep(SETTLE).await;
    }
}

impl Mail {
    /// Asks the keeper to look now, without waiting for it.
    pub(crate) fn keep_now(&self) {
        self.keeping.notify_one();
    }

    /// Lists and counts an account's folders, and keeps that: whether what
    /// the sidebar shows changed, and each folder with its count.
    async fn count(&self, account: &MailAccount) -> Result<(bool, Vec<(Place, Counted)>), String> {
        let (places, counts) = self
            .on_server(account, |session| {
                Box::pin(async move {
                    let places = imap::folders(session).await?;
                    let paths: Vec<String> = places.iter().map(|p| p.path.clone()).collect();
                    let modseq = session.capabilities().await?.has_str("CONDSTORE");
                    let counts = imap::counts(session, &paths, modseq).await?;
                    Ok((places, counts))
                })
            })
            .await
            .map_err(|failure| failure.message())?;
        self.lock_places().insert(account.id, places.clone());
        let changed = self
            .keep_places(account.id, &places, Some(&counts))
            .await
            .map_err(|e| e.to_string())?;
        if changed {
            tracing::debug!(address = %account.address, "unread of all: {}", tally(&places, &counts));
        }
        let counted = places
            .into_iter()
            .filter_map(|place| {
                let count = counts.get(&place.path).copied()?;
                Some((place, count))
            })
            .collect();
        Ok((changed, counted))
    }

    /// Reads again each of an account's folders whose count changed since
    /// its lines were read, those for a role first: whether any changed.
    async fn read_changed(
        &self,
        account: &MailAccount,
        counted: &[(Place, Counted)],
    ) -> Result<bool, String> {
        let stamps: HashMap<String, Option<String>> =
            sqlx::query_as("SELECT path, stamp FROM kept_folders WHERE account_id = $1")
                .bind(account.id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .collect();
        let mut order: Vec<&(Place, Counted)> = counted.iter().collect();
        order.sort_by_key(|(place, _)| place.role.map_or(FolderRole::ALL.len(), |r| r as usize));
        let mut changed = false;
        for (place, count) in order {
            let stamp = count.stamp();
            if stamps.get(&place.path).and_then(Option::as_deref) == Some(stamp.as_str()) {
                continue;
            }
            let (_, kept) = self
                .read_newest(account, place, Some(&stamp))
                .await
                .map_err(|failure| failure.message())?;
            changed |= kept;
        }
        Ok(changed)
    }

    /// A folder's newest mail, read from its server (what's kept of it
    /// isn't read again) and kept: under `stamp`, what STATUS said before,
    /// when it's known. Its lines, and whether what's kept changed.
    pub(crate) async fn read_newest(
        &self,
        account: &MailAccount,
        place: &Place,
        stamp: Option<&str>,
    ) -> Result<(Lines, bool), Failure> {
        let (validity, known) = self
            .kept_summaries(account.id, &place.path)
            .await
            .map_err(not_kept)?;
        let read = self
            .on_server(account, |session| {
                let (path, known) = (place.path.clone(), known.clone());
                Box::pin(
                    async move { imap::newest(session, &path, NEWEST, &known, validity).await },
                )
            })
            .await?;
        let changed = self
            .keep_lines(account.id, place, &read, stamp)
            .await
            .map_err(not_kept)?;
        Ok((read, changed))
    }

    /// Keeps an account's folders as its server listed them, in its order,
    /// with what `counts` counted in each; one not counted keeps what it
    /// had. A folder no longer listed goes, with its mail. Returns whether
    /// anything kept changed.
    pub(crate) async fn keep_places(
        &self,
        account_id: Uuid,
        places: &[Place],
        counts: Option<&HashMap<String, Counted>>,
    ) -> Result<bool, sqlx::Error> {
        type Row = (String, String, Option<String>, i32, i64, i64);
        let before: HashMap<String, (String, Option<String>, i32, i64, i64)> = sqlx::query_as(
            "SELECT path, name, role, position, total, unread FROM kept_folders \
             WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(path, name, role, position, total, unread): Row| {
            (path, (name, role, position, total, unread))
        })
        .collect();
        let paths: Vec<&str> = places.iter().map(|place| place.path.as_str()).collect();
        let mut tx = self.pool.begin().await?;
        let gone =
            sqlx::query("DELETE FROM kept_folders WHERE account_id = $1 AND NOT path = ANY($2)")
                .bind(account_id)
                .bind(&paths)
                .execute(&mut *tx)
                .await?;
        let mut changed = gone.rows_affected() > 0;
        for (position, place) in places.iter().enumerate() {
            let had = before.get(&place.path);
            let (total, unread) = match counts.and_then(|counts| counts.get(&place.path)) {
                Some(count) => (i64::from(count.total), i64::from(count.unseen)),
                None => had.map_or((0, 0), |had| (had.3, had.4)),
            };
            let now = (
                place.name.clone(),
                place.role.map(|role| role.key().to_string()),
                i32::try_from(position).unwrap_or(i32::MAX),
                total,
                unread,
            );
            if had == Some(&now) {
                continue;
            }
            sqlx::query(
                "INSERT INTO kept_folders (account_id, path, name, role, position, total, unread) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) \
                 ON CONFLICT (account_id, path) DO UPDATE SET name = EXCLUDED.name, \
                 role = EXCLUDED.role, position = EXCLUDED.position, total = EXCLUDED.total, \
                 unread = EXCLUDED.unread",
            )
            .bind(account_id)
            .bind(&place.path)
            .bind(&now.0)
            .bind(&now.1)
            .bind(now.2)
            .bind(now.3)
            .bind(now.4)
            .execute(&mut *tx)
            .await?;
            changed = true;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// An account's folders as they were kept last, in their server's order.
    pub(crate) async fn kept_places(&self, account_id: Uuid) -> Result<Vec<Place>, sqlx::Error> {
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT path, name, role FROM kept_folders WHERE account_id = $1 ORDER BY position",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(path, name, role)| place(path, name, role.as_deref()))
            .collect())
    }

    /// A folder made since its account's folders were kept, kept after them.
    pub(crate) async fn keep_made(
        &self,
        account_id: Uuid,
        place: &Place,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO kept_folders (account_id, path, name, role, position) \
             SELECT $1, $2, $3, $4, coalesce(max(position) + 1, 0) FROM kept_folders \
             WHERE account_id = $1 \
             ON CONFLICT (account_id, path) DO NOTHING",
        )
        .bind(account_id)
        .bind(&place.path)
        .bind(&place.name)
        .bind(place.role.map(FolderRole::key))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Folders renamed on their server, under their new paths: what's
    /// kept of them goes along.
    pub(crate) async fn move_kept(
        &self,
        account_id: Uuid,
        moved: &[(String, String)],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        for (from, to) in moved {
            // And what waits for the server there (`queued.rs`).
            for update in [
                "UPDATE kept_folders SET path = $3 WHERE account_id = $1 AND path = $2",
                "UPDATE queued_mails SET path = $3 WHERE account_id = $1 AND path = $2",
                "UPDATE queued_mails SET to_path = $3 WHERE account_id = $1 AND to_path = $2",
                "UPDATE queued_reads SET path = $3 WHERE account_id = $1 AND path = $2",
            ] {
                sqlx::query(update)
                    .bind(account_id)
                    .bind(from)
                    .bind(to)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await
    }

    /// A folder gone from its server, gone from here with its mail.
    pub(crate) async fn forget_kept_folder(
        &self,
        account_id: Uuid,
        path: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM kept_folders WHERE account_id = $1 AND path = $2")
            .bind(account_id)
            .bind(path)
            .execute(&self.pool)
            .await?;
        self.bump();
        Ok(())
    }

    /// What's unread in each of the accounts' kept folders, and all each
    /// holds, by account and path.
    pub(crate) async fn kept_counts(
        &self,
        account_ids: &[Uuid],
    ) -> Result<HashMap<(Uuid, String), (i64, i64)>, sqlx::Error> {
        let rows: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(
            "SELECT account_id, path, unread, total FROM kept_folders WHERE account_id = ANY($1)",
        )
        .bind(account_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(account_id, path, unread, total)| ((account_id, path), (unread, total)))
            .collect())
    }

    /// A folder's newest mail as kept here, newest first; `None` when its
    /// lines were never read. The Inbox has a copy of its own.
    pub(crate) async fn kept_listing(
        &self,
        account_id: Uuid,
        place: &Place,
    ) -> Result<Option<Vec<MailSummary>>, sqlx::Error> {
        if place.path == Place::inbox().path {
            return Ok(None);
        }
        let stamp: Option<Option<String>> = sqlx::query_scalar(
            "SELECT stamp FROM kept_folders WHERE account_id = $1 AND path = $2",
        )
        .bind(account_id)
        .bind(&place.path)
        .fetch_optional(&self.pool)
        .await?;
        if stamp.flatten().is_none() {
            return Ok(None);
        }
        let rows: Vec<KeptRow> = sqlx::query_as(
            "SELECT uid, message_id, from_name, from_address, to_name, subject, snippet, date, \
             seen, flagged FROM folder_mails WHERE account_id = $1 AND path = $2 \
             ORDER BY date DESC, uid DESC LIMIT $3",
        )
        .bind(account_id)
        .bind(&place.path)
        .bind(i64::from(NEWEST))
        .fetch_all(&self.pool)
        .await?;
        Ok(Some(
            rows.into_iter()
                .map(|row| summary(account_id, place, line(row)))
                .collect(),
        ))
    }

    /// Every kept line of the accounts' folders, with its folder; only the
    /// flagged ones with `flagged_only`.
    pub(crate) async fn kept_lines(
        &self,
        account_ids: &[Uuid],
        flagged_only: bool,
    ) -> Result<Vec<(Uuid, Place, Line)>, sqlx::Error> {
        type Row = (
            Uuid,
            String,
            String,
            Option<String>,
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            DateTime<Utc>,
            bool,
            bool,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.account_id, f.path, f.name, f.role, m.uid, m.message_id, m.from_name, \
             m.from_address, m.to_name, m.subject, m.snippet, m.date, m.seen, m.flagged \
             FROM folder_mails m JOIN kept_folders f USING (account_id, path) \
             WHERE m.account_id = ANY($1) AND (m.flagged OR NOT $2)",
        )
        .bind(account_ids)
        .bind(flagged_only)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    account_id,
                    path,
                    name,
                    role,
                    uid,
                    message_id,
                    from_name,
                    from_address,
                    to_name,
                    subject,
                    snippet,
                    date,
                    seen,
                    flagged,
                )| {
                    let row = (
                        uid,
                        message_id,
                        from_name,
                        from_address,
                        to_name,
                        subject,
                        snippet,
                        date,
                        seen,
                        flagged,
                    );
                    (account_id, place(path, name, role.as_deref()), line(row))
                },
            )
            .collect())
    }

    /// A kept folder's lines by UID, and the UIDVALIDITY they were read under.
    pub(crate) async fn kept_summaries(
        &self,
        account_id: Uuid,
        path: &str,
    ) -> Result<(i64, HashMap<i64, Summary>), sqlx::Error> {
        let validity: Option<i64> = sqlx::query_scalar(
            "SELECT uid_validity FROM kept_folders WHERE account_id = $1 AND path = $2",
        )
        .bind(account_id)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;
        let Some(validity) = validity else {
            return Ok((0, HashMap::new()));
        };
        let rows: Vec<KeptRow> = sqlx::query_as(
            "SELECT uid, message_id, from_name, from_address, to_name, subject, snippet, date, \
             seen, flagged FROM folder_mails WHERE account_id = $1 AND path = $2",
        )
        .bind(account_id)
        .bind(path)
        .fetch_all(&self.pool)
        .await?;
        Ok((
            validity,
            rows.into_iter()
                .map(|row| {
                    let line = line(row);
                    (line.uid, line.summary)
                })
                .collect(),
        ))
    }

    /// One kept line of a folder's, and the UIDVALIDITY it's under.
    pub(crate) async fn kept_line(
        &self,
        account_id: Uuid,
        path: &str,
        uid: i64,
    ) -> Result<Option<(Line, i64)>, sqlx::Error> {
        type Row = (
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            DateTime<Utc>,
            bool,
            bool,
            i64,
        );
        let row: Option<Row> = sqlx::query_as(
            "SELECT m.uid, m.message_id, m.from_name, m.from_address, m.to_name, m.subject, \
             m.snippet, m.date, m.seen, m.flagged, f.uid_validity \
             FROM folder_mails m JOIN kept_folders f USING (account_id, path) \
             WHERE m.account_id = $1 AND m.path = $2 AND m.uid = $3",
        )
        .bind(account_id)
        .bind(path)
        .bind(uid)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(
            |(
                uid,
                message_id,
                from_name,
                from_address,
                to_name,
                subject,
                snippet,
                date,
                seen,
                flagged,
                validity,
            )| {
                let row = (
                    uid,
                    message_id,
                    from_name,
                    from_address,
                    to_name,
                    subject,
                    snippet,
                    date,
                    seen,
                    flagged,
                );
                (line(row), validity)
            },
        ))
    }

    /// A kept mail's source, when it was read ahead or opened before.
    pub(crate) async fn kept_source(
        &self,
        account_id: Uuid,
        path: &str,
        uid: i64,
    ) -> Result<Option<Vec<u8>>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT raw FROM folder_sources WHERE account_id = $1 AND path = $2 AND uid = $3",
        )
        .bind(account_id)
        .bind(path)
        .bind(uid)
        .fetch_optional(&self.pool)
        .await
    }

    /// Keeps a mail's source, while its line is kept: a mail gone meanwhile
    /// keeps nothing.
    pub(crate) async fn keep_source(
        &self,
        account_id: Uuid,
        path: &str,
        uid: i64,
        raw: &[u8],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO folder_sources (account_id, path, uid, raw) \
             SELECT account_id, path, uid, $4 FROM folder_mails \
             WHERE account_id = $1 AND path = $2 AND uid = $3 \
             ON CONFLICT DO NOTHING",
        )
        .bind(account_id)
        .bind(path)
        .bind(uid)
        .bind(raw)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mails that left a folder, as when they were moved: gone from what's
    /// kept of it, and from its count, at once. The keeper looks again.
    pub(crate) async fn forget_kept(
        &self,
        account_id: Uuid,
        path: &str,
        uids: &[i64],
    ) -> Result<(), sqlx::Error> {
        let changed = sqlx::query(
            "WITH gone AS (DELETE FROM folder_mails \
               WHERE account_id = $1 AND path = $2 AND uid = ANY($3) RETURNING seen) \
             UPDATE kept_folders SET \
               total = greatest(total - (SELECT count(*) FROM gone), 0), \
               unread = greatest(unread - (SELECT count(*) FROM gone WHERE NOT seen), 0) \
             WHERE account_id = $1 AND path = $2 AND EXISTS (SELECT 1 FROM gone)",
        )
        .bind(account_id)
        .bind(path)
        .bind(uids)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() > 0 {
            self.bump();
        }
        self.keep_now();
        Ok(())
    }

    /// A folder's kept mails read or unread, as their server has them now,
    /// and its count with them.
    pub(crate) async fn seen_kept(
        &self,
        account_id: Uuid,
        path: &str,
        uids: &[i64],
        seen: bool,
    ) -> Result<(), sqlx::Error> {
        let changed = sqlx::query(
            "WITH changed AS (UPDATE folder_mails SET seen = $4 \
               WHERE account_id = $1 AND path = $2 AND uid = ANY($3) AND seen <> $4 RETURNING 1) \
             UPDATE kept_folders SET unread = greatest(unread + \
               (CASE WHEN $4 THEN -1 ELSE 1 END) * (SELECT count(*) FROM changed), 0) \
             WHERE account_id = $1 AND path = $2 AND EXISTS (SELECT 1 FROM changed)",
        )
        .bind(account_id)
        .bind(path)
        .bind(uids)
        .bind(seen)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() > 0 {
            self.bump();
        }
        Ok(())
    }

    /// A folder's kept mail flagged or not, as its server has it now.
    pub(crate) async fn flagged_kept(
        &self,
        account_id: Uuid,
        path: &str,
        uid: i64,
        flagged: bool,
    ) -> Result<(), sqlx::Error> {
        let changed = sqlx::query(
            "UPDATE folder_mails SET flagged = $4 \
             WHERE account_id = $1 AND path = $2 AND uid = $3 AND flagged <> $4",
        )
        .bind(account_id)
        .bind(path)
        .bind(uid)
        .bind(flagged)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() > 0 {
            self.bump();
        }
        Ok(())
    }

    /// A folder whose every mail was marked read, or that was emptied
    /// (`gone`): so it's kept.
    pub(crate) async fn all_kept(
        &self,
        account_id: Uuid,
        path: &str,
        gone: bool,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        if gone {
            sqlx::query("DELETE FROM folder_mails WHERE account_id = $1 AND path = $2")
                .bind(account_id)
                .bind(path)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query(
                "UPDATE folder_mails SET seen = true \
                 WHERE account_id = $1 AND path = $2 AND NOT seen",
            )
            .bind(account_id)
            .bind(path)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "UPDATE kept_folders SET unread = 0, total = CASE WHEN $3 THEN 0 ELSE total END \
             WHERE account_id = $1 AND path = $2",
        )
        .bind(account_id)
        .bind(path)
        .bind(gone)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.bump();
        Ok(())
    }

    /// Keeps the lines read from a folder: those no longer among its
    /// newest go, those new come, and the rest are read or unread as now.
    /// All go when its UIDs changed meaning. `stamp` is what STATUS said
    /// before they were read, when it's known. Returns whether anything
    /// kept changed.
    async fn keep_lines(
        &self,
        account_id: Uuid,
        place: &Place,
        read: &Lines,
        stamp: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        if place.path == Place::inbox().path {
            return Ok(false);
        }
        self.keep_made(account_id, place).await?;
        let mut tx = self.pool.begin().await?;
        let validity: i64 = sqlx::query_scalar(
            "SELECT uid_validity FROM kept_folders WHERE account_id = $1 AND path = $2 FOR UPDATE",
        )
        .bind(account_id)
        .bind(&place.path)
        .fetch_one(&mut *tx)
        .await?;
        let mut changed = false;
        if validity != read.validity {
            let gone = sqlx::query("DELETE FROM folder_mails WHERE account_id = $1 AND path = $2")
                .bind(account_id)
                .bind(&place.path)
                .execute(&mut *tx)
                .await?;
            changed |= gone.rows_affected() > 0;
        }
        let uids: Vec<i64> = read.lines.iter().map(|line| line.uid).collect();
        // What waits for the server (`queued.rs`) stays as it was done
        // here: a mail moved here keeps its line, one moved away isn't
        // kept again, and one marked read or unread, flagged or not, stays
        // so. So does a mail in the outbox (`outbox.rs`).
        let gone = sqlx::query(
            "DELETE FROM folder_mails m WHERE account_id = $1 AND path = $2 \
             AND NOT uid = ANY($3) AND NOT EXISTS (SELECT 1 FROM queued_mails q \
               WHERE q.account_id = $1 AND q.to_path = $2 AND q.placeholder = m.uid) \
             AND NOT EXISTS (SELECT 1 FROM outbox o \
               WHERE o.account_id = $1 AND o.shown_path = $2 AND o.placeholder = m.uid)",
        )
        .bind(account_id)
        .bind(&place.path)
        .bind(&uids)
        .execute(&mut *tx)
        .await?;
        changed |= gone.rows_affected() > 0;
        let column = |pick: fn(&Summary) -> &String| -> Vec<&str> {
            read.lines
                .iter()
                .map(|line| pick(&line.summary).as_str())
                .collect()
        };
        let dates: Vec<DateTime<Utc>> = read.lines.iter().map(|line| line.summary.date).collect();
        let seen: Vec<bool> = read.lines.iter().map(|line| line.seen).collect();
        let flagged: Vec<bool> = read.lines.iter().map(|line| line.flagged).collect();
        let kept = sqlx::query(
            "WITH l AS (SELECT l.uid, l.message_id, l.from_name, l.from_address, l.to_name, \
               l.subject, l.snippet, l.date, \
               CASE WHEN k.uid IS NULL OR (NOT EXISTS (SELECT 1 FROM queued_mails q \
                   WHERE q.account_id = $1 AND q.path = $2 AND q.uid = l.uid \
                   AND q.seen IS NOT NULL) \
                 AND NOT EXISTS (SELECT 1 FROM queued_reads r WHERE r.account_id = $1 \
                   AND r.path = $2 AND l.uid < r.below AND NOT l.seen)) \
               THEN l.seen ELSE k.seen END AS seen, \
               CASE WHEN k.uid IS NULL OR NOT EXISTS (SELECT 1 FROM queued_mails q \
                   WHERE q.account_id = $1 AND q.path = $2 AND q.uid = l.uid \
                   AND q.flagged IS NOT NULL) \
               THEN l.flagged ELSE k.flagged END AS flagged \
             FROM UNNEST($3::bigint[], $4::text[], $5::text[], $6::text[], $7::text[], \
               $8::text[], $9::text[], $10::timestamptz[], $11::boolean[], $12::boolean[]) \
               AS l(uid, message_id, from_name, from_address, to_name, subject, snippet, date, \
               seen, flagged) \
             LEFT JOIN folder_mails k ON k.account_id = $1 AND k.path = $2 AND k.uid = l.uid) \
             INSERT INTO folder_mails (account_id, path, uid, message_id, from_name, \
             from_address, to_name, subject, snippet, date, seen, flagged) \
             SELECT $1, $2, l.* FROM l \
             WHERE NOT EXISTS (SELECT 1 FROM queued_mails q WHERE q.account_id = $1 \
               AND q.path = $2 AND q.uid = l.uid AND q.to_path IS NOT NULL) \
             ON CONFLICT (account_id, path, uid) \
             DO UPDATE SET seen = EXCLUDED.seen, flagged = EXCLUDED.flagged \
             WHERE folder_mails.seen <> EXCLUDED.seen OR folder_mails.flagged <> EXCLUDED.flagged",
        )
        .bind(account_id)
        .bind(&place.path)
        .bind(&uids)
        .bind(column(|s| &s.message_id))
        .bind(column(|s| &s.from_name))
        .bind(column(|s| &s.from_address))
        .bind(column(|s| &s.to_name))
        .bind(column(|s| &s.subject))
        .bind(column(|s| &s.snippet))
        .bind(&dates)
        .bind(&seen)
        .bind(&flagged)
        .execute(&mut *tx)
        .await?;
        changed |= kept.rows_affected() > 0;
        sqlx::query(
            "UPDATE kept_folders SET uid_validity = $3, stamp = coalesce($4, stamp, '') \
             WHERE account_id = $1 AND path = $2",
        )
        .bind(account_id)
        .bind(&place.path)
        .bind(read.validity)
        .bind(stamp)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(changed)
    }

    /// Reads the whole of an account's newest mails ahead of opening: its
    /// Inbox's, then its folders', those for a role first. Not the Drafts,
    /// Trash or Junk, nor a mail bigger than [`AT_MOST`].
    async fn read_ahead(&self, account: &MailAccount, too_big: &mut TooBig) -> Result<(), String> {
        let db = |e: sqlx::Error| e.to_string();
        let validity: Option<i64> =
            sqlx::query_scalar("SELECT uid_validity FROM inboxes WHERE account_id = $1")
                .bind(account.id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if let Some(validity) = validity {
            let wanted: Vec<i64> = sqlx::query_scalar(
                "SELECT uid FROM (SELECT uid, date FROM inbox_mails \
                   WHERE account_id = $1 AND uid > 0 ORDER BY date DESC, uid DESC LIMIT $2) m \
                 WHERE NOT EXISTS (SELECT 1 FROM mail_sources s \
                   WHERE s.account_id = $1 AND s.uid = m.uid)",
            )
            .bind(account.id)
            .bind(AHEAD_INBOX)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
            let inbox = Place::inbox().path;
            for (uid, raw) in self
                .read_sources(account, &inbox, validity, &wanted, too_big)
                .await?
            {
                sqlx::query(
                    "INSERT INTO mail_sources (account_id, uid, raw) \
                     SELECT account_id, uid, $3 FROM inbox_mails \
                     WHERE account_id = $1 AND uid = $2 ON CONFLICT DO NOTHING",
                )
                .bind(account.id)
                .bind(uid)
                .bind(&raw)
                .execute(&self.pool)
                .await
                .map_err(db)?;
            }
        }
        let folders: Vec<(String, Option<String>, i64)> = sqlx::query_as(
            "SELECT path, role, uid_validity FROM kept_folders \
             WHERE account_id = $1 AND stamp IS NOT NULL \
             AND (role IS NULL OR role NOT IN ('drafts', 'trash', 'junk')) \
             ORDER BY role IS NULL, position",
        )
        .bind(account.id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for (path, _, validity) in folders {
            let wanted: Vec<i64> = sqlx::query_scalar(
                "SELECT uid FROM (SELECT uid FROM folder_mails WHERE account_id = $1 \
                   AND path = $2 AND uid > 0 ORDER BY date DESC, uid DESC LIMIT $3) m \
                 WHERE NOT EXISTS (SELECT 1 FROM folder_sources s \
                   WHERE s.account_id = $1 AND s.path = $2 AND s.uid = m.uid)",
            )
            .bind(account.id)
            .bind(&path)
            .bind(AHEAD)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
            for (uid, raw) in self
                .read_sources(account, &path, validity, &wanted, too_big)
                .await?
            {
                self.keep_source(account.id, &path, uid, &raw)
                    .await
                    .map_err(db)?;
            }
        }
        Ok(())
    }

    /// The sources of the `wanted` mails in a folder not known to be too
    /// big, read from its server; those too big are noted.
    async fn read_sources(
        &self,
        account: &MailAccount,
        path: &str,
        validity: i64,
        wanted: &[i64],
        too_big: &mut TooBig,
    ) -> Result<HashMap<i64, Vec<u8>>, String> {
        let wanted: Vec<i64> = wanted
            .iter()
            .copied()
            .filter(|uid| !too_big.contains(&(account.id, path.to_string(), *uid)))
            .collect();
        if wanted.is_empty() {
            return Ok(HashMap::new());
        }
        let read = self
            .on_server(account, |session| {
                let (path, wanted) = (path.to_string(), wanted.clone());
                Box::pin(
                    async move { imap::sources(session, &path, validity, &wanted, AT_MOST).await },
                )
            })
            .await
            .map_err(|failure| failure.message())?
            .unwrap_or_default();
        for uid in &wanted {
            if !read.contains_key(uid) {
                too_big.insert((account.id, path.to_string(), *uid));
            }
        }
        Ok(read)
    }
}

/// A kept line: its UID, Message-ID, who sent it and to whom, its
/// subject, start and date, and whether it's read and flagged.
type KeptRow = (
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    bool,
    bool,
);

fn line(row: KeptRow) -> Line {
    let (uid, message_id, from_name, from_address, to_name, subject, snippet, date, seen, flagged) =
        row;
    Line {
        uid,
        summary: Summary {
            from_name,
            from_address,
            to_name,
            subject,
            snippet,
            date,
            message_id,
            threading: imap::Threading::default(),
        },
        seen,
        flagged,
    }
}

/// A kept folder, by its path, name and role's key.
fn place(path: String, name: String, role: Option<&str>) -> Place {
    Place {
        path,
        name,
        role: role.and_then(|key| FolderRole::ALL.into_iter().find(|r| r.key() == key)),
    }
}

/// Why what's read couldn't be kept here.
fn not_kept(e: sqlx::Error) -> Failure {
    Failure::Offline(format!("couldn't keep it on this Mac: {e}"))
}

/// "Archive 0 of 812, Newsletters 3 of 940, own 2 of 61 in 4": an
/// account's counts, for the log, without its folders' own names.
fn tally(places: &[Place], counts: &HashMap<String, Counted>) -> String {
    let mut own = (0, 0, 0);
    let mut said: Vec<String> = Vec::new();
    for place in places {
        let Some(count) = counts.get(&place.path) else {
            continue;
        };
        match place.role {
            Some(role) => said.push(format!(
                "{} {} of {}",
                role_name(role),
                count.unseen,
                count.total
            )),
            None => own = (own.0 + count.unseen, own.1 + count.total, own.2 + 1),
        }
    }
    said.sort();
    said.push(format!("own {} of {} in {}", own.0, own.1, own.2));
    said.join(", ")
}
