//! What's done to mail while its server can't be reached: moving it,
//! opening it (which reads it) and marking a folder read. Each is done on
//! this Mac's copy at once and kept (`queued_mails`, `queued_reads`), then
//! done on the server once it answers: by the account's watcher as soon as
//! it's back in, before it reads the Inbox, and by the keeper
//! ([`crate::kept::keep`]) before it reads the folders. Until then the
//! Inbox's copy and the kept folders leave it as it was done here. What
//! the server refuses then, as for a mail gone meanwhile or a folder
//! deleted, is let go, and the mail shows where its server has it.
//!
//! A mail moved shows in its new folder at once, under a UID below zero
//! until its server gives it its own; moved back, it shows where it was.

use chrono::{DateTime, Utc};
use postrider_domain::{AccountState, AccountStatus, MailAccount};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::imap::{self, Failure, Place, Session};
use crate::mail::Mail;

/// The Inbox's path, whose copy has tables of its own.
const INBOX: &str = "INBOX";

/// What a kept folder's stamp becomes once something done here waits for
/// its server: never what STATUS says, so the keeper reads it again.
const CHANGED_HERE: &str = "changed here";

/// A mail's line as this Mac's copy shows it, and its source when kept:
/// what goes along when it moves.
#[derive(sqlx::FromRow)]
struct Held {
    message_id: String,
    from_name: String,
    from_address: String,
    to_name: String,
    subject: String,
    snippet: String,
    date: DateTime<Utc>,
    seen: bool,
    flagged: bool,
    refs: Option<Vec<String>>,
    to_addresses: Vec<String>,
    reply_to: String,
    #[sqlx(skip)]
    raw: Option<Vec<u8>>,
}

/// A folder marked read here while its server couldn't be reached.
pub(crate) struct ReadHere {
    /// Its id in `queued_reads`.
    pub(crate) id: Uuid,
    /// The folder's UIDVALIDITY.
    pub(crate) validity: u32,
    /// The mails it marked here, those moved there too (below zero).
    pub(crate) uids: Vec<i64>,
    /// What the folder counted unread before.
    pub(crate) unread: i64,
}

/// What was done on an account's server of what waited for it.
#[derive(Default)]
struct Replayed {
    /// How many changes, done or let go.
    done: usize,
    /// Each folder marked read, by its id: under which UIDVALIDITY, and
    /// the mails its server marked.
    reads: Vec<(Uuid, u32, Vec<u32>)>,
}

impl Mail {
    /// Whether what's done to a mail waits for its server: it can't be
    /// reached now, or something done to the mail before still waits.
    pub(crate) async fn waits(
        &self,
        account_id: Uuid,
        path: &str,
        uid: i64,
    ) -> Result<bool, sqlx::Error> {
        if uid < 0 || self.state_of(account_id) != Some(AccountState::Online) {
            return Ok(true);
        }
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM queued_mails \
             WHERE account_id = $1 AND path = $2 AND uid = $3)",
        )
        .bind(account_id)
        .bind(path)
        .bind(uid)
        .fetch_one(&self.pool)
        .await
    }

    /// Moves a mail on this Mac's copy, from where it shows to `to`, and
    /// keeps the move for its server: `false` when it isn't there.
    pub(crate) async fn queue_move(
        &self,
        account_id: Uuid,
        from: &Place,
        uid: i64,
        to: &Place,
    ) -> Result<bool, sqlx::Error> {
        if to.path != INBOX {
            self.keep_made(account_id, to).await?;
        }
        let mut tx = self.pool.begin().await?;
        let moved = shift(&mut tx, account_id, &from.path, uid, &to.path).await?;
        tx.commit().await?;
        if moved {
            if let Some((_, lines)) = self.lock_lines().get_mut(&(account_id, from.path.clone())) {
                lines.remove(&uid);
            }
            self.bump();
        }
        Ok(moved)
    }

    /// Keeps mails marked read or unread, flagged or not, for their
    /// server: those this Mac's copy shows in the folder at `path`. What's
    /// `None` stays as it waits. The copy itself is the caller's to change.
    pub(crate) async fn queue_marks(
        &self,
        account_id: Uuid,
        path: &str,
        uids: &[i64],
        seen: Option<bool>,
        flagged: Option<bool>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        // Mails moved here: their server has them where they were.
        sqlx::query(
            "UPDATE queued_mails SET seen = coalesce($4, seen), flagged = coalesce($5, flagged) \
             WHERE account_id = $1 AND to_path = $2 AND placeholder = ANY($3)",
        )
        .bind(account_id)
        .bind(path)
        .bind(uids)
        .bind(seen)
        .bind(flagged)
        .execute(&mut *tx)
        .await?;
        if let Some(validity) = validity_of(&mut tx, account_id, path).await? {
            sqlx::query(
                "INSERT INTO queued_mails (account_id, path, uid, uid_validity, seen, flagged) \
                 SELECT $1, $2, uid, $5, $4, $6 FROM unnest($3::bigint[]) AS u(uid) \
                 WHERE uid > 0 \
                 ON CONFLICT (account_id, path, uid) DO UPDATE \
                 SET seen = coalesce(EXCLUDED.seen, queued_mails.seen), \
                 flagged = coalesce(EXCLUDED.flagged, queued_mails.flagged)",
            )
            .bind(account_id)
            .bind(path)
            .bind(uids)
            .bind(seen)
            .bind(validity)
            .bind(flagged)
            .execute(&mut *tx)
            .await?;
        }
        changed_here(&mut tx, account_id, path).await?;
        tx.commit().await
    }

    /// Marks a folder read on this Mac's copy, and keeps that for its
    /// server: the id it waits under, the folder's UIDVALIDITY, and the
    /// mails marked here, for Undo. `None` when nothing of it is kept.
    pub(crate) async fn queue_read(
        &self,
        account_id: Uuid,
        path: &str,
    ) -> Result<Option<ReadHere>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let Some(validity) = validity_of(&mut tx, account_id, path).await? else {
            return Ok(None);
        };
        // Nothing unread here as this Mac knows it: nothing to wait for.
        let unread: bool = if path == INBOX {
            sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM inbox_mails WHERE account_id = $1 AND NOT seen)",
            )
            .bind(account_id)
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM kept_folders \
                   WHERE account_id = $1 AND path = $2 AND unread > 0) \
                 OR EXISTS (SELECT 1 FROM folder_mails \
                   WHERE account_id = $1 AND path = $2 AND NOT seen)",
            )
            .bind(account_id)
            .bind(path)
            .fetch_one(&mut *tx)
            .await?
        };
        if !unread {
            return Ok(None);
        }
        // Every mail there by now: those after came since it was looked at.
        let (newest, stamp, unread): (i64, Option<String>, i64) = if path == INBOX {
            sqlx::query_as(
                "SELECT coalesce(max(uid), 0), NULL, 0::bigint FROM inbox_mails \
                 WHERE account_id = $1 AND uid > 0",
            )
            .bind(account_id)
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_as(
                "SELECT coalesce((SELECT max(uid) FROM folder_mails \
                   WHERE account_id = $1 AND path = $2 AND uid > 0), 0), \
                 (SELECT stamp FROM kept_folders WHERE account_id = $1 AND path = $2), \
                 coalesce((SELECT unread FROM kept_folders \
                   WHERE account_id = $1 AND path = $2), 0)",
            )
            .bind(account_id)
            .bind(path)
            .fetch_one(&mut *tx)
            .await?
        };
        let next = stamp
            .as_deref()
            .and_then(|stamp| stamp.split(':').nth(1)?.parse::<i64>().ok())
            .unwrap_or(0);
        let below = (newest + 1).max(next);
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO queued_reads (id, account_id, path, uid_validity, below) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(account_id)
        .bind(path)
        .bind(validity)
        .bind(below)
        .execute(&mut *tx)
        .await?;
        // Mails moved here, and those already waiting to be read or not.
        sqlx::query(
            "UPDATE queued_mails SET seen = true WHERE account_id = $1 \
             AND (to_path = $2 OR (path = $2 AND uid < $3 AND seen IS NOT NULL))",
        )
        .bind(account_id)
        .bind(path)
        .bind(below)
        .execute(&mut *tx)
        .await?;
        let marked: Vec<i64> = if path == INBOX {
            sqlx::query_scalar(
                "UPDATE inbox_mails SET seen = true WHERE account_id = $1 AND NOT seen \
                 RETURNING uid",
            )
            .bind(account_id)
            .fetch_all(&mut *tx)
            .await?
        } else {
            let marked = sqlx::query_scalar(
                "UPDATE folder_mails SET seen = true \
                 WHERE account_id = $1 AND path = $2 AND NOT seen RETURNING uid",
            )
            .bind(account_id)
            .bind(path)
            .fetch_all(&mut *tx)
            .await?;
            sqlx::query("UPDATE kept_folders SET unread = 0 WHERE account_id = $1 AND path = $2")
                .bind(account_id)
                .bind(path)
                .execute(&mut *tx)
                .await?;
            changed_here(&mut tx, account_id, path).await?;
            marked
        };
        tx.commit().await?;
        self.bump();
        Ok(Some(ReadHere {
            id,
            validity: u32::try_from(validity).unwrap_or(0),
            uids: marked,
            unread,
        }))
    }

    /// Takes back a folder marked read while its server couldn't be
    /// reached, if it still waits: what it marked is unread again, here
    /// and in what waits for the server. Whether it still waited.
    pub(crate) async fn unqueue_read(
        &self,
        account_id: Uuid,
        path: &str,
        read: &ReadHere,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let gone = sqlx::query("DELETE FROM queued_reads WHERE id = $1")
            .bind(read.id)
            .execute(&mut *tx)
            .await?;
        if gone.rows_affected() == 0 {
            return Ok(false);
        }
        // Those it set to be read, a mail moved there among them.
        sqlx::query(
            "UPDATE queued_mails SET seen = false WHERE account_id = $1 \
             AND ((path = $2 AND uid = ANY($3) AND seen IS NOT NULL) \
               OR (to_path = $2 AND placeholder = ANY($3)))",
        )
        .bind(account_id)
        .bind(path)
        .bind(&read.uids)
        .execute(&mut *tx)
        .await?;
        if path == INBOX {
            sqlx::query(
                "UPDATE inbox_mails SET seen = false WHERE account_id = $1 AND uid = ANY($2)",
            )
            .bind(account_id)
            .bind(&read.uids)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE folder_mails SET seen = false \
                 WHERE account_id = $1 AND path = $2 AND uid = ANY($3)",
            )
            .bind(account_id)
            .bind(path)
            .bind(&read.uids)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE kept_folders SET unread = $3 WHERE account_id = $1 AND path = $2")
                .bind(account_id)
                .bind(path)
                .bind(read.unread)
                .execute(&mut *tx)
                .await?;
            changed_here(&mut tx, account_id, path).await?;
        }
        tx.commit().await?;
        self.bump();
        Ok(true)
    }

    /// Where its server has a mail moved while it couldn't be reached,
    /// shown under `placeholder`: its folder, its UID there and the
    /// folder's UIDVALIDITY.
    pub(crate) async fn origin(
        &self,
        account_id: Uuid,
        placeholder: i64,
    ) -> Result<Option<(String, i64, i64)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT path, uid, uid_validity FROM queued_mails \
             WHERE account_id = $1 AND placeholder = $2",
        )
        .bind(account_id)
        .bind(placeholder)
        .fetch_optional(&self.pool)
        .await
    }

    /// How many changes wait for each of these accounts' servers.
    pub(crate) async fn count_waiting(
        &self,
        statuses: &mut [AccountStatus],
    ) -> Result<(), sqlx::Error> {
        let ids: Vec<Uuid> = statuses.iter().map(|status| status.account_id).collect();
        let waiting: Vec<(Uuid, i64)> = sqlx::query_as(
            "SELECT account_id, count(*) FROM ( \
               SELECT account_id FROM queued_mails WHERE account_id = ANY($1) \
               UNION ALL SELECT account_id FROM queued_reads WHERE account_id = ANY($1) \
               UNION ALL SELECT account_id FROM outbox WHERE account_id = ANY($1)) q \
             GROUP BY account_id",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        for status in statuses {
            status.waiting = waiting
                .iter()
                .find(|(id, _)| *id == status.account_id)
                .map_or(0, |(_, count)| *count);
        }
        Ok(())
    }

    /// Does on the account's server what waited for it, over the watcher's
    /// own connection, when anything does.
    pub(crate) async fn replay_over(
        &self,
        account: &MailAccount,
        session: &mut Session,
    ) -> Result<(), Failure> {
        let _one = self.replaying.lock().await;
        if !self.anything_waits(account.id).await? {
            return Ok(());
        }
        let replayed = replay(&self.pool, account.id, session).await?;
        self.replayed(account, replayed);
        Ok(())
    }

    /// Does on the account's server what waited for it, over one of its
    /// connections for reading, when anything does.
    pub(crate) async fn replay(&self, account: &MailAccount) -> Result<(), Failure> {
        let _one = self.replaying.lock().await;
        if !self.anything_waits(account.id).await? {
            return Ok(());
        }
        let (pool, account_id) = (self.pool.clone(), account.id);
        let replayed = self
            .on_server(account, move |session| {
                let pool = pool.clone();
                Box::pin(async move { replay(&pool, account_id, session).await })
            })
            .await?;
        self.replayed(account, replayed);
        Ok(())
    }

    /// Whether anything waits for the account's server.
    async fn anything_waits(&self, account_id: Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM queued_mails WHERE account_id = $1) \
             OR EXISTS (SELECT 1 FROM queued_reads WHERE account_id = $1)",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await
    }

    /// What the server did: Undo knows what it marked read, and the
    /// keeper reads the folders again.
    fn replayed(&self, account: &MailAccount, replayed: Replayed) {
        tracing::info!(
            address = %account.address,
            changes = replayed.done,
            "did what waited for its server"
        );
        for (id, validity, uids) in replayed.reads {
            self.marked_on_server(id, validity, uids);
        }
        self.bump();
        self.keep_now();
    }
}

/// Does on the account's server what waited for it, the oldest first:
/// each mail marked or moved, then each folder marked read.
async fn replay(
    pool: &PgPool,
    account_id: Uuid,
    session: &mut Session,
) -> Result<Replayed, Failure> {
    let mut replayed = Replayed::default();
    type Row = (
        String,
        i64,
        i64,
        Option<bool>,
        Option<bool>,
        Option<String>,
        Option<i64>,
    );
    let mails: Vec<Row> = sqlx::query_as(
        "SELECT path, uid, uid_validity, seen, flagged, to_path, placeholder FROM queued_mails \
         WHERE account_id = $1 ORDER BY path, uid_validity, at",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let mut selected: Option<(String, i64, bool)> = None;
    for (path, uid, validity, seen, flagged, to, placeholder) in mails {
        let there = match &selected {
            Some((at, under, there)) if *at == path && *under == validity => *there,
            _ => {
                let there = imap::select_queued(session, &path, validity).await?;
                selected = Some((path.clone(), validity, there));
                there
            }
        };
        let took = there && imap::replay_mail(session, uid, seen, flagged, to.as_deref()).await?;
        let mut tx = pool.begin().await?;
        if !took {
            tracing::info!(path, uid, "let go of a change its server refused");
            if let (Some(to), Some(placeholder)) = (&to, placeholder) {
                // Back where its server has it.
                shift(&mut tx, account_id, to, placeholder, &path).await?;
            }
        }
        sqlx::query("DELETE FROM queued_mails WHERE account_id = $1 AND path = $2 AND uid = $3")
            .bind(account_id)
            .bind(&path)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        replayed.done += 1;
    }
    let reads: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(
        "SELECT id, path, uid_validity, below FROM queued_reads \
         WHERE account_id = $1 ORDER BY at",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    for (id, path, validity, below) in reads {
        let marked = if imap::select_queued(session, &path, validity).await? {
            imap::read_below(session, below).await?
        } else {
            None
        };
        sqlx::query("DELETE FROM queued_reads WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        if let Some(uids) = marked {
            replayed
                .reads
                .push((id, u32::try_from(validity).unwrap_or(0), uids));
        }
        replayed.done += 1;
    }
    Ok(replayed)
}

/// Moves a mail's line on this Mac's copy, from where it shows (`from`,
/// under `uid`) to `to`, and keeps what its server is to do: `false` when
/// it isn't there.
async fn shift(
    db: &mut PgConnection,
    account_id: Uuid,
    from: &str,
    uid: i64,
    to: &str,
) -> Result<bool, sqlx::Error> {
    // Where its server has it.
    let origin: Option<(String, i64)> = if uid < 0 {
        sqlx::query_as(
            "SELECT path, uid FROM queued_mails \
             WHERE account_id = $1 AND placeholder = $2 FOR UPDATE",
        )
        .bind(account_id)
        .bind(uid)
        .fetch_optional(&mut *db)
        .await?
    } else {
        Some((from.to_string(), uid))
    };
    let Some((path, server_uid)) = origin else {
        return Ok(false);
    };
    let Some(mut held) = take(db, account_id, from, uid).await? else {
        return Ok(false);
    };
    if uid < 0 {
        // As it showed where it was, read or unread as it is now.
        let was: Held = sqlx::query_as(
            "SELECT coalesce(message_id, '') AS message_id, \
             coalesce(from_name, '') AS from_name, coalesce(from_address, '') AS from_address, \
             coalesce(to_name, '') AS to_name, coalesce(subject, '') AS subject, \
             coalesce(snippet, '') AS snippet, coalesce(date, now()) AS date, false AS seen, \
             false AS flagged, \
             refs, coalesce(to_addresses, '{}') AS to_addresses, \
             coalesce(reply_to, '') AS reply_to \
             FROM queued_mails WHERE account_id = $1 AND placeholder = $2",
        )
        .bind(account_id)
        .bind(uid)
        .fetch_one(&mut *db)
        .await?;
        held = Held {
            seen: held.seen,
            flagged: held.flagged,
            raw: held.raw,
            ..was
        };
    }
    if to == path {
        put(db, account_id, to, server_uid, &held).await?;
        sqlx::query(
            "UPDATE queued_mails SET to_path = NULL, placeholder = NULL \
             WHERE account_id = $1 AND path = $2 AND uid = $3",
        )
        .bind(account_id)
        .bind(&path)
        .bind(server_uid)
        .execute(&mut *db)
        .await?;
        sqlx::query(
            "DELETE FROM queued_mails \
             WHERE account_id = $1 AND path = $2 AND uid = $3 AND seen IS NULL \
             AND flagged IS NULL",
        )
        .bind(account_id)
        .bind(&path)
        .bind(server_uid)
        .execute(&mut *db)
        .await?;
        return Ok(true);
    }
    if uid < 0 {
        put(db, account_id, to, uid, &held).await?;
        sqlx::query(
            "UPDATE queued_mails SET to_path = $3 WHERE account_id = $1 AND placeholder = $2",
        )
        .bind(account_id)
        .bind(uid)
        .bind(to)
        .execute(&mut *db)
        .await?;
        return Ok(true);
    }
    let placeholder: i64 = sqlx::query_scalar("SELECT -nextval('queued_placeholders')")
        .fetch_one(&mut *db)
        .await?;
    put(db, account_id, to, placeholder, &held).await?;
    let validity = validity_of(db, account_id, &path).await?.unwrap_or(0);
    sqlx::query(
        "INSERT INTO queued_mails (account_id, path, uid, uid_validity, to_path, placeholder, \
         message_id, from_name, from_address, to_name, subject, snippet, date, refs, \
         to_addresses, reply_to) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
         ON CONFLICT (account_id, path, uid) DO UPDATE SET to_path = EXCLUDED.to_path, \
         placeholder = EXCLUDED.placeholder, message_id = EXCLUDED.message_id, \
         from_name = EXCLUDED.from_name, from_address = EXCLUDED.from_address, \
         to_name = EXCLUDED.to_name, subject = EXCLUDED.subject, snippet = EXCLUDED.snippet, \
         date = EXCLUDED.date, refs = EXCLUDED.refs, to_addresses = EXCLUDED.to_addresses, \
         reply_to = EXCLUDED.reply_to",
    )
    .bind(account_id)
    .bind(&path)
    .bind(server_uid)
    .bind(validity)
    .bind(to)
    .bind(placeholder)
    .bind(&held.message_id)
    .bind(&held.from_name)
    .bind(&held.from_address)
    .bind(&held.to_name)
    .bind(&held.subject)
    .bind(&held.snippet)
    .bind(held.date)
    .bind(&held.refs)
    .bind(&held.to_addresses)
    .bind(&held.reply_to)
    .execute(&mut *db)
    .await?;
    Ok(true)
}

/// Takes a mail's line out of where this Mac's copy shows it, with its
/// source when kept, and out of its folder's count: `None` when it isn't
/// there.
async fn take(
    db: &mut PgConnection,
    account_id: Uuid,
    path: &str,
    uid: i64,
) -> Result<Option<Held>, sqlx::Error> {
    if path == INBOX {
        let raw: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT raw FROM mail_sources WHERE account_id = $1 AND uid = $2")
                .bind(account_id)
                .bind(uid)
                .fetch_optional(&mut *db)
                .await?;
        let held: Option<Held> = sqlx::query_as(
            "DELETE FROM inbox_mails WHERE account_id = $1 AND uid = $2 \
             RETURNING message_id, from_name, from_address, '' AS to_name, subject, snippet, \
             date, seen, flagged, refs, to_addresses, reply_to",
        )
        .bind(account_id)
        .bind(uid)
        .fetch_optional(&mut *db)
        .await?;
        return Ok(held.map(|held| Held { raw, ..held }));
    }
    let raw: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT raw FROM folder_sources WHERE account_id = $1 AND path = $2 AND uid = $3",
    )
    .bind(account_id)
    .bind(path)
    .bind(uid)
    .fetch_optional(&mut *db)
    .await?;
    let held: Option<Held> = sqlx::query_as(
        "DELETE FROM folder_mails WHERE account_id = $1 AND path = $2 AND uid = $3 \
         RETURNING message_id, from_name, from_address, to_name, subject, snippet, date, seen, \
         flagged, NULL::text[] AS refs, '{}'::text[] AS to_addresses, '' AS reply_to",
    )
    .bind(account_id)
    .bind(path)
    .bind(uid)
    .fetch_optional(&mut *db)
    .await?;
    let Some(held) = held else {
        return Ok(None);
    };
    sqlx::query(
        "UPDATE kept_folders SET total = greatest(total - 1, 0), \
         unread = greatest(unread - CASE WHEN $3 THEN 0 ELSE 1 END, 0) \
         WHERE account_id = $1 AND path = $2",
    )
    .bind(account_id)
    .bind(path)
    .bind(held.seen)
    .execute(&mut *db)
    .await?;
    changed_here(db, account_id, path).await?;
    Ok(Some(Held { raw, ..held }))
}

/// Puts a mail's line where this Mac's copy shows it, under `uid`, with
/// its source, and into its folder's count. A folder (or Inbox) not kept
/// any more keeps nothing.
async fn put(
    db: &mut PgConnection,
    account_id: Uuid,
    path: &str,
    uid: i64,
    held: &Held,
) -> Result<(), sqlx::Error> {
    if path == INBOX {
        sqlx::query(
            "INSERT INTO inbox_mails (account_id, uid, from_name, from_address, subject, \
             snippet, date, seen, message_id, refs, to_addresses, reply_to, flagged) \
             SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13 \
             WHERE EXISTS (SELECT 1 FROM inboxes WHERE account_id = $1) \
             ON CONFLICT DO NOTHING",
        )
        .bind(account_id)
        .bind(uid)
        .bind(&held.from_name)
        .bind(&held.from_address)
        .bind(&held.subject)
        .bind(&held.snippet)
        .bind(held.date)
        .bind(held.seen)
        .bind(&held.message_id)
        .bind(&held.refs)
        .bind(&held.to_addresses)
        .bind(&held.reply_to)
        .bind(held.flagged)
        .execute(&mut *db)
        .await?;
        if let Some(raw) = &held.raw {
            sqlx::query(
                "INSERT INTO mail_sources (account_id, uid, raw) \
                 SELECT account_id, uid, $3 FROM inbox_mails WHERE account_id = $1 AND uid = $2 \
                 ON CONFLICT DO NOTHING",
            )
            .bind(account_id)
            .bind(uid)
            .bind(raw)
            .execute(&mut *db)
            .await?;
        }
        return Ok(());
    }
    let kept = sqlx::query(
        "INSERT INTO folder_mails (account_id, path, uid, message_id, from_name, from_address, \
         to_name, subject, snippet, date, seen, flagged) \
         SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12 \
         WHERE EXISTS (SELECT 1 FROM kept_folders WHERE account_id = $1 AND path = $2) \
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(path)
    .bind(uid)
    .bind(&held.message_id)
    .bind(&held.from_name)
    .bind(&held.from_address)
    .bind(&held.to_name)
    .bind(&held.subject)
    .bind(&held.snippet)
    .bind(held.date)
    .bind(held.seen)
    .bind(held.flagged)
    .execute(&mut *db)
    .await?;
    if kept.rows_affected() == 0 {
        return Ok(());
    }
    if let Some(raw) = &held.raw {
        sqlx::query(
            "INSERT INTO folder_sources (account_id, path, uid, raw) VALUES ($1, $2, $3, $4) \
             ON CONFLICT DO NOTHING",
        )
        .bind(account_id)
        .bind(path)
        .bind(uid)
        .bind(raw)
        .execute(&mut *db)
        .await?;
    }
    sqlx::query(
        "UPDATE kept_folders SET total = total + 1, \
         unread = unread + CASE WHEN $3 THEN 0 ELSE 1 END \
         WHERE account_id = $1 AND path = $2",
    )
    .bind(account_id)
    .bind(path)
    .bind(held.seen)
    .execute(&mut *db)
    .await?;
    changed_here(db, account_id, path).await
}

/// The UIDVALIDITY this Mac's copy of a folder (or the Inbox) is under:
/// `None` when nothing of it is kept.
async fn validity_of(
    db: &mut PgConnection,
    account_id: Uuid,
    path: &str,
) -> Result<Option<i64>, sqlx::Error> {
    if path == INBOX {
        return sqlx::query_scalar("SELECT uid_validity FROM inboxes WHERE account_id = $1")
            .bind(account_id)
            .fetch_optional(&mut *db)
            .await;
    }
    sqlx::query_scalar("SELECT uid_validity FROM kept_folders WHERE account_id = $1 AND path = $2")
        .bind(account_id)
        .bind(path)
        .fetch_optional(&mut *db)
        .await
}

/// A kept folder changed here: the keeper reads it again once its server
/// has what changed. One never read stays so.
async fn changed_here(
    db: &mut PgConnection,
    account_id: Uuid,
    path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE kept_folders SET stamp = $3 \
         WHERE account_id = $1 AND path = $2 AND stamp IS NOT NULL",
    )
    .bind(account_id)
    .bind(path)
    .bind(CHANGED_HERE)
    .execute(&mut *db)
    .await?;
    Ok(())
}
