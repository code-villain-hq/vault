//! Marking one mail: ⇧U read or unread and S flagged or not, at
//! `POST /api/w/{workspace}/mails/{account}/{uid}/mark[?folder=<key>]`;
//! and every account's flagged mail, in the Inbox and the folders a search
//! asks, at `GET /api/w/{workspace}/flagged`. While its server can't be
//! reached, a mark waits for it (`queued.rs`), and shows at once.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use postrider_domain::{AccountState, Listing, MailSummary, MarkMail};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::folders::{listing, searched, summary};
use crate::imap::{self, Failure, Place};
use crate::mail::{Mail, OpenParams};
use crate::state::AppState;

impl Mail {
    /// Marks a mail of the Inbox, or of the folder `key` names, read or
    /// unread, flagged or not: on its server, else here until it can be
    /// reached.
    pub async fn mark(
        self: &Arc<Self>,
        workspace_id: Uuid,
        account_id: Uuid,
        key: Option<&str>,
        uid: i64,
        marks: MarkMail,
    ) -> Result<(), AppError> {
        if marks.seen.is_none() && marks.flagged.is_none() {
            return Ok(());
        }
        let refused = |failure: Failure| AppError::refused(failure.message());
        let account = self.account(workspace_id, account_id).await?;
        let place = match key {
            None => Place::inbox(),
            Some(key) => self
                .place(&account, key)
                .await
                .map_err(refused)?
                .ok_or_else(|| {
                    AppError::not_found(format!(
                        "{} has no folder called {key} any more.",
                        account.name
                    ))
                })?,
        };
        let mut waits = self.waits(account.id, &place.path, uid).await?;
        if !waits {
            let marked = self
                .on_server(&account, |session| {
                    let path = place.path.clone();
                    Box::pin(async move {
                        imap::mark(session, &path, uid, marks.seen, marks.flagged).await
                    })
                })
                .await;
            match marked {
                Ok(()) => {}
                Err(Failure::Offline(why)) => {
                    tracing::info!(address = %account.address, "couldn't mark a mail, so it waits: {why}");
                    waits = true;
                }
                Err(failure) => return Err(refused(failure)),
            }
        }
        if waits {
            self.queue_marks(account.id, &place.path, &[uid], marks.seen, marks.flagged)
                .await?;
            self.keep_now();
        }
        self.marked_here(account.id, &place.path, uid, marks)
            .await?;
        Ok(())
    }

    /// This Mac's copy of a mail, marked as its server has it now.
    async fn marked_here(
        &self,
        account_id: Uuid,
        path: &str,
        uid: i64,
        marks: MarkMail,
    ) -> Result<(), sqlx::Error> {
        if path == Place::inbox().path {
            let changed = sqlx::query(
                "UPDATE inbox_mails SET seen = coalesce($3, seen), \
                 flagged = coalesce($4, flagged) \
                 WHERE account_id = $1 AND uid = $2 \
                 AND (seen <> coalesce($3, seen) OR flagged <> coalesce($4, flagged))",
            )
            .bind(account_id)
            .bind(uid)
            .bind(marks.seen)
            .bind(marks.flagged)
            .execute(&self.pool)
            .await?;
            if changed.rows_affected() > 0 {
                self.bump();
            }
            return Ok(());
        }
        if let Some(seen) = marks.seen {
            self.seen_kept(account_id, path, &[uid], seen).await?;
        }
        if let Some(flagged) = marks.flagged {
            self.flagged_kept(account_id, path, uid, flagged).await?;
        }
        Ok(())
    }

    /// Every account's flagged mail, newest first, as this Mac keeps it:
    /// in the Inbox, and in the folders a search asks (not Drafts, Trash
    /// or Junk). An account whose server can't be reached says so.
    pub async fn flagged(&self, workspace_id: Uuid) -> Result<Listing, AppError> {
        self.keep_now();
        let accounts = self.accounts(workspace_id).await?;
        let ids: Vec<Uuid> = accounts.iter().map(|account| account.id).collect();
        let mut found: HashMap<Uuid, Vec<MailSummary>> = HashMap::new();
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
            "SELECT account_id, uid, from_name, from_address, subject, snippet, date, seen \
             FROM inbox_mails WHERE account_id = ANY($1) AND flagged",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        for (account_id, uid, from_name, from_address, subject, snippet, date, seen) in rows {
            found.entry(account_id).or_default().push(MailSummary {
                account_id,
                // Inbox mail, opened and moved as from the Inbox.
                folder: None,
                uid,
                from_name,
                from_address,
                subject,
                snippet,
                date,
                seen,
                flagged: true,
                to_name: None,
            });
        }
        for (account_id, place, line) in self.kept_lines(&ids, true).await? {
            if searched(&place) {
                found
                    .entry(account_id)
                    .or_default()
                    .push(summary(account_id, &place, line));
            }
        }
        let read = accounts
            .iter()
            .map(|account| Ok(found.remove(&account.id).unwrap_or_default()))
            .collect();
        let mut listed = listing(&accounts, read, HashMap::new());
        for account in &accounts {
            if let Some(status) = self.status_of(account.id).filter(|status| {
                matches!(status.state, AccountState::Offline | AccountState::Refused)
            }) {
                listed.problems.push(status);
            }
        }
        self.count_waiting(&mut listed.problems).await?;
        Ok(listed)
    }
}

/// `POST /api/w/{workspace}/mails/{account}/{uid}/mark[?folder=<key>]`:
/// the mail read or unread, flagged or not, as the body says.
pub async fn mark(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid)): Path<(Uuid, Uuid, i64)>,
    Query(params): Query<OpenParams>,
    Json(marks): Json<MarkMail>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .mark(
            member.actor.workspace_id,
            account_id,
            params.folder.as_deref(),
            uid,
            marks,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/w/{workspace}/flagged`: every account's flagged mail.
pub async fn flagged(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Listing>, AppError> {
    Ok(Json(state.mail.flagged(member.actor.workspace_id).await?))
}
