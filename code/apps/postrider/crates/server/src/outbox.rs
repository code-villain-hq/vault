//! The outbox: mail you send and drafts you keep, until their servers have
//! them (`outbox`). A mail sent goes over SMTP at once when it can; else
//! it waits here and goes once its server takes it. Either way what
//! follows waits here too, for the account's IMAP server: its copy in
//! Sent, the draft it was written from deleted, the mail it answers
//! marked answered. A draft kept while Drafts can't be reached waits
//! here, and goes there once it can.
//!
//! Each shows at once where it's going, Sent or Drafts, under a UID below
//! zero, until its server has it. The keeper ([`crate::kept::keep`]) and
//! the account's watcher send what waits once they're in, oldest first.
//! What its server refuses then goes back to Drafts, to be sent again.

use lettre::Address;
use lettre::address::Envelope;
use postrider_domain::{AccountState, DraftRef, FolderRole, MailAccount, MailSummary};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppError;
use crate::imap::{self, Failure, Place};
use crate::mail::Mail;
use crate::send::{Unsent, Written, deliver};

/// What goes in the outbox.
pub(crate) enum Posted {
    /// A mail sent: `unsent` (its envelope and what goes over SMTP) while
    /// it hasn't gone yet, and the mail it answers, by account, folder
    /// and UID.
    Send {
        unsent: Option<(Envelope, Vec<u8>)>,
        answered: Option<(Uuid, String, i64)>,
    },
    /// A draft, for Drafts.
    Draft,
}

/// What waits in the outbox.
#[derive(sqlx::FromRow)]
struct Waiting {
    id: Uuid,
    kind: String,
    envelope_from: Option<String>,
    envelope_to: Vec<String>,
    raw: Option<Vec<u8>>,
    kept: Option<Vec<u8>>,
    shown_path: Option<String>,
    placeholder: Option<i64>,
    replaces_account: Option<Uuid>,
    replaces_message_id: Option<String>,
    answered_account: Option<Uuid>,
    answered_path: Option<String>,
    answered_uid: Option<i64>,
}

impl Mail {
    /// Whether what's sent from or kept in the account waits: its server
    /// can't be reached now.
    pub(crate) fn waits_to_send(&self, account_id: Uuid) -> bool {
        self.state_of(account_id) != Some(AccountState::Online)
    }

    /// An account by its id alone.
    pub(crate) async fn account_by_id(&self, account_id: Uuid) -> Result<MailAccount, AppError> {
        let account: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT to_jsonb(a.*) FROM mail_accounts a WHERE id = $1")
                .bind(account_id)
                .fetch_optional(&self.pool)
                .await?;
        account
            .and_then(|row| serde_json::from_value(row).ok())
            .ok_or_else(|| AppError::not_found("That account isn't here any more."))
    }

    /// The draft a mail was written from, by account and Message-ID, to
    /// delete on its server once the mail is kept anew or sent. One that
    /// waits here is let go at once, for what it replaced there.
    pub(crate) async fn replaced(
        &self,
        draft: Option<&DraftRef>,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error> {
        let Some(draft) = draft.filter(|draft| !draft.message_id.is_empty()) else {
            return Ok(None);
        };
        // Not while the outbox puts it on its server: it would stay there.
        let _one = self.sending.lock().await;
        let mut tx = self.pool.begin().await?;
        let waiting: Option<Waiting> = sqlx::query_as(
            "DELETE FROM outbox WHERE account_id = $1 AND kind = 'draft' AND message_id = $2 \
             RETURNING *",
        )
        .bind(draft.account_id)
        .bind(&draft.message_id)
        .fetch_optional(&mut *tx)
        .await?;
        let replaced = match waiting {
            Some(waiting) => {
                unshow(&mut tx, draft.account_id, &waiting).await?;
                waiting.replaces_account.zip(waiting.replaces_message_id)
            }
            None => Some((draft.account_id, draft.message_id.clone())),
        };
        tx.commit().await?;
        Ok(replaced)
    }

    /// Puts what was written in the outbox, shown in Sent or Drafts.
    pub(crate) async fn post(
        &self,
        account: &MailAccount,
        posted: Posted,
        written: &Written,
        replaces: Option<(Uuid, String)>,
    ) -> Result<(), sqlx::Error> {
        let (kind, role, unsent, answered) = match posted {
            Posted::Send { unsent, answered } => ("send", FolderRole::Sent, unsent, answered),
            Posted::Draft => ("draft", FolderRole::Drafts, None, None),
        };
        let shown = self
            .places(account)
            .await
            .ok()
            .and_then(|places| places.into_iter().find(|place| place.role == Some(role)));
        let (envelope_from, envelope_to, raw) = match unsent {
            Some((envelope, raw)) => (
                envelope.from().map(ToString::to_string),
                envelope.to().iter().map(ToString::to_string).collect(),
                Some(raw),
            ),
            None => (None, Vec::new(), None),
        };
        let (replaces_account, replaces_message_id) = replaces.unzip();
        let (answered_account, answered_path, answered_uid) = match answered {
            Some((account_id, path, uid)) => (Some(account_id), Some(path), Some(uid)),
            None => (None, None, None),
        };
        let mut tx = self.pool.begin().await?;
        let placeholder: Option<i64> = match &shown {
            Some(_) => Some(
                sqlx::query_scalar("SELECT -nextval('queued_placeholders')")
                    .fetch_one(&mut *tx)
                    .await?,
            ),
            None => None,
        };
        sqlx::query(
            "INSERT INTO outbox (id, account_id, kind, message_id, envelope_from, envelope_to, \
             raw, kept, shown_path, placeholder, replaces_account, replaces_message_id, \
             answered_account, answered_path, answered_uid) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(Uuid::now_v7())
        .bind(account.id)
        .bind(kind)
        .bind(&written.message_id)
        .bind(envelope_from)
        .bind(&envelope_to)
        .bind(raw)
        .bind(&written.kept)
        .bind(shown.as_ref().map(|place| &place.path))
        .bind(placeholder)
        .bind(replaces_account)
        .bind(replaces_message_id)
        .bind(answered_account)
        .bind(answered_path)
        .bind(answered_uid)
        .execute(&mut *tx)
        .await?;
        if let (Some(place), Some(uid)) = (&shown, placeholder) {
            show(&mut tx, account.id, place, uid, &written.kept).await?;
        }
        tx.commit().await?;
        self.bump();
        Ok(())
    }

    /// Keeps a draft to delete from the account's Drafts once it can be
    /// reached.
    pub(crate) async fn post_deletion(
        &self,
        account: &MailAccount,
        message_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO outbox (id, account_id, kind, message_id, replaces_account, \
             replaces_message_id) VALUES ($1, $2, 'draft', '', $2, $3)",
        )
        .bind(Uuid::now_v7())
        .bind(account.id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A draft that waits here, by its Message-ID.
    pub(crate) async fn waiting_draft(
        &self,
        account_id: Uuid,
        message_id: &str,
    ) -> Result<Option<Vec<u8>>, sqlx::Error> {
        Ok(sqlx::query_scalar(
            "SELECT kept FROM outbox WHERE account_id = $1 AND kind = 'draft' \
             AND message_id = $2 AND kept IS NOT NULL",
        )
        .bind(account_id)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    /// What waits here, shown under `placeholder`.
    pub(crate) async fn waiting_at(
        &self,
        account_id: Uuid,
        placeholder: i64,
    ) -> Result<Option<Vec<u8>>, sqlx::Error> {
        if placeholder >= 0 {
            return Ok(None);
        }
        Ok(
            sqlx::query_scalar(
                "SELECT kept FROM outbox WHERE account_id = $1 AND placeholder = $2",
            )
            .bind(account_id)
            .bind(placeholder)
            .fetch_optional(&self.pool)
            .await?
            .flatten(),
        )
    }

    /// Refuses to move or mark what waits here: it isn't on its server yet.
    pub(crate) async fn not_waiting(
        &self,
        account_id: Uuid,
        path: &str,
        uid: i64,
    ) -> Result<(), AppError> {
        if uid >= 0 {
            return Ok(());
        }
        let waits: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM outbox \
             WHERE account_id = $1 AND shown_path = $2 AND placeholder = $3)",
        )
        .bind(account_id)
        .bind(path)
        .bind(uid)
        .fetch_one(&self.pool)
        .await?;
        if waits {
            return Err(AppError::conflict(
                "It waits to reach its server. Once it has, it can.",
            ));
        }
        Ok(())
    }

    /// Marks the mails of a list that wait to be sent.
    pub(crate) async fn mark_unsent(&self, mails: &mut [MailSummary]) -> Result<(), sqlx::Error> {
        if !mails.iter().any(|mail| mail.uid < 0) {
            return Ok(());
        }
        let unsent: Vec<(Uuid, i64)> = sqlx::query_as(
            "SELECT account_id, placeholder FROM outbox \
             WHERE kind = 'send' AND placeholder IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;
        for mail in mails {
            mail.unsent = unsent.contains(&(mail.account_id, mail.uid));
        }
        Ok(())
    }

    /// Sends what waits for the account's servers, oldest first, as far
    /// as they can be reached.
    pub(crate) async fn send_waiting(&self, account: &MailAccount) {
        let _one = self.sending.lock().await;
        match self.send_waiting_now(account).await {
            Ok(0) => {}
            Ok(done) => {
                tracing::info!(address = %account.address, done, "sent what waited in the outbox");
                self.bump();
                self.keep_now();
            }
            // It waits for the keeper's next round: waking it now would
            // only try again at once.
            Err(failure) => {
                tracing::info!(
                    address = %account.address,
                    "couldn't send what waits in the outbox: {}",
                    failure.message()
                );
            }
        }
    }

    async fn send_waiting_now(&self, account: &MailAccount) -> Result<usize, Failure> {
        let waiting: Vec<Waiting> =
            sqlx::query_as("SELECT * FROM outbox WHERE account_id = $1 ORDER BY at")
                .bind(account.id)
                .fetch_all(&self.pool)
                .await?;
        let mut done = 0;
        for mut waiting in waiting {
            if let Some(raw) = &waiting.raw {
                let password = self.password(account).await?;
                match deliver(account, password, &envelope_of(&waiting), raw).await {
                    Ok(()) => {
                        sqlx::query("UPDATE outbox SET raw = NULL WHERE id = $1")
                            .bind(waiting.id)
                            .execute(&self.pool)
                            .await?;
                    }
                    Err(Unsent::Waits(why)) => return Err(Failure::Offline(why)),
                    Err(Unsent::Refused(why)) => {
                        tracing::warn!(
                            address = %account.address,
                            "its server refused a mail that waited, so it goes back to Drafts: {why}"
                        );
                        let mut tx = self.pool.begin().await?;
                        unshow(&mut tx, account.id, &waiting).await?;
                        sqlx::query(
                            "UPDATE outbox SET kind = 'draft', raw = NULL, shown_path = NULL, \
                             placeholder = NULL, answered_account = NULL, answered_path = NULL, \
                             answered_uid = NULL WHERE id = $1",
                        )
                        .bind(waiting.id)
                        .execute(&mut *tx)
                        .await?;
                        tx.commit().await?;
                        self.bump();
                        waiting.kind = "draft".into();
                        (waiting.shown_path, waiting.placeholder) = (None, None);
                        (waiting.answered_account, waiting.answered_uid) = (None, None);
                    }
                }
            }
            // What follows needs the IMAP server.
            if self.waits_to_send(account.id) {
                continue;
            }
            if let Some(kept) = &waiting.kept {
                let role = if waiting.kind == "send" {
                    FolderRole::Sent
                } else {
                    FolderRole::Drafts
                };
                let draft = waiting.kind == "draft";
                let put = async {
                    let place = self.folder_for(account, role).await?;
                    self.on_server(account, |session| {
                        let (path, raw) = (place.path.clone(), kept.clone());
                        Box::pin(async move {
                            if draft {
                                imap::put_draft(session, &path, &raw).await
                            } else {
                                imap::put(session, &path, &raw).await
                            }
                        })
                    })
                    .await
                };
                match put.await {
                    // A draft is never let go: it stays here, in Drafts,
                    // until its server takes it.
                    Err(Failure::Refused(why)) if draft => {
                        tracing::warn!(address = %account.address, "its server didn't take a draft, so it stays here: {why}");
                        continue;
                    }
                    put => let_go(put, account, "put it in Sent")?,
                }
                self.done_with(waiting.id, Step::Kept).await?;
            }
            if let (Some(account_id), Some(message_id)) =
                (waiting.replaces_account, &waiting.replaces_message_id)
            {
                let Some(other) = self.reachable(account, account_id).await else {
                    continue;
                };
                let deleted = async {
                    let Some(place) = self.drafts_of(&other).await? else {
                        return Ok(());
                    };
                    self.on_server(&other, |session| {
                        let (path, id) = (place.path.clone(), message_id.clone());
                        Box::pin(async move { imap::delete_by_id(session, &path, &id).await })
                    })
                    .await
                };
                if let_go(deleted.await, &other, "delete its draft").is_err() {
                    continue;
                }
                self.done_with(waiting.id, Step::Replaced).await?;
            }
            if let (Some(account_id), Some(path), Some(uid)) = (
                waiting.answered_account,
                &waiting.answered_path,
                waiting.answered_uid,
            ) {
                let Some(other) = self.reachable(account, account_id).await else {
                    continue;
                };
                let marked = self
                    .on_server(&other, |session| {
                        let path = path.clone();
                        Box::pin(async move { imap::mark_answered(session, &path, uid).await })
                    })
                    .await;
                if let_go(marked, &other, "mark answered").is_err() {
                    continue;
                }
            }
            let mut tx = self.pool.begin().await?;
            unshow(&mut tx, account.id, &waiting).await?;
            sqlx::query("DELETE FROM outbox WHERE id = $1")
                .bind(waiting.id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            done += 1;
        }
        Ok(done)
    }

    /// Notes a step of what waits as done.
    async fn done_with(&self, id: Uuid, step: Step) -> Result<(), sqlx::Error> {
        sqlx::query(match step {
            Step::Kept => "UPDATE outbox SET kept = NULL WHERE id = $1",
            Step::Replaced => {
                "UPDATE outbox SET replaces_account = NULL, replaces_message_id = NULL WHERE id = $1"
            }
        })
        .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The account `account_id`, when its server can be reached now.
    async fn reachable(&self, account: &MailAccount, account_id: Uuid) -> Option<MailAccount> {
        if self.waits_to_send(account_id) {
            return None;
        }
        if account_id == account.id {
            return Some(account.clone());
        }
        self.account_by_id(account_id).await.ok()
    }
}

/// A step of what waits, done.
enum Step {
    /// It's where it goes.
    Kept,
    /// The draft it replaces is gone.
    Replaced,
}

/// Done, or let go when the server refused it; out of reach, it waits.
fn let_go(done: Result<(), Failure>, account: &MailAccount, what: &str) -> Result<(), Failure> {
    match done {
        Ok(()) => Ok(()),
        Err(Failure::Offline(why)) => Err(Failure::Offline(why)),
        Err(failure) => {
            tracing::info!(address = %account.address, "couldn't {what}, so let it go: {}", failure.message());
            Ok(())
        }
    }
}

/// Whom a mail that waits goes to.
fn envelope_of(waiting: &Waiting) -> Envelope {
    let from = waiting
        .envelope_from
        .as_deref()
        .and_then(|from| from.parse::<Address>().ok());
    let to = waiting
        .envelope_to
        .iter()
        .filter_map(|to| to.parse::<Address>().ok())
        .collect();
    Envelope::new(from, to).unwrap_or_else(|_| {
        let nobody: Address = "nobody@invalid".parse().expect("an address");
        Envelope::new(None, vec![nobody]).expect("an envelope")
    })
}

/// Shows what waits in its folder, under `uid`, as read and with its
/// source, so it opens.
async fn show(
    db: &mut PgConnection,
    account_id: Uuid,
    place: &Place,
    uid: i64,
    raw: &[u8],
) -> Result<(), sqlx::Error> {
    let line = imap::summarize(raw, None);
    let shown = sqlx::query(
        "INSERT INTO folder_mails (account_id, path, uid, message_id, from_name, from_address, \
         to_name, subject, snippet, date, seen, flagged) \
         SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, true, false \
         WHERE EXISTS (SELECT 1 FROM kept_folders WHERE account_id = $1 AND path = $2) \
         ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(&place.path)
    .bind(uid)
    .bind(&line.message_id)
    .bind(&line.from_name)
    .bind(&line.from_address)
    .bind(&line.to_name)
    .bind(&line.subject)
    .bind(&line.snippet)
    .bind(line.date)
    .execute(&mut *db)
    .await?;
    if shown.rows_affected() > 0 {
        sqlx::query(
            "INSERT INTO folder_sources (account_id, path, uid, raw) VALUES ($1, $2, $3, $4) \
             ON CONFLICT DO NOTHING",
        )
        .bind(account_id)
        .bind(&place.path)
        .bind(uid)
        .bind(raw)
        .execute(&mut *db)
        .await?;
    }
    Ok(())
}

/// Takes what waited out of the folder it showed in.
async fn unshow(
    db: &mut PgConnection,
    account_id: Uuid,
    waiting: &Waiting,
) -> Result<(), sqlx::Error> {
    let (Some(path), Some(uid)) = (&waiting.shown_path, waiting.placeholder) else {
        return Ok(());
    };
    for delete in [
        "DELETE FROM folder_sources WHERE account_id = $1 AND path = $2 AND uid = $3",
        "DELETE FROM folder_mails WHERE account_id = $1 AND path = $2 AND uid = $3",
    ] {
        sqlx::query(delete)
            .bind(account_id)
            .bind(path)
            .bind(uid)
            .execute(&mut *db)
            .await?;
    }
    Ok(())
}
