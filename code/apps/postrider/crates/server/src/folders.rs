//! Folders and searches: every account's folders, each once by its name
//! or role, at `GET /api/w/{workspace}/folders`;
//! a folder's newest mail at `/folders/{key}`; what a search finds in all
//! of them at `/search?q=`; and a mail opened from either at
//! `/mails/{account}/{uid}?folder=<key>`. A folder's menu marks its mail
//! read (`POST /folders/{key}/read`, undone at `/marked/{id}/undo`),
//! renames it, deletes it (`DELETE /folders/{key}`), and empties the Trash
//! and Junk (`POST /folders/{key}/empty`).
//!
//! The folders and their newest mail are read from what this Mac keeps of
//! them (`kept.rs`), at once and offline, while the keeper asks the
//! servers what changed; a folder not kept yet is read from its server.
//! A search finds what this Mac has of it first (`/search?q=&here=true`),
//! at once, while the servers are asked, and an account that can't be
//! asked shows that.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use futures::future::join_all;
use futures::{StreamExt, TryStreamExt, stream};
use postrider_domain::{
    AccountState, AccountStatus, Folder, FolderLink, FolderRole, Listing, MailAccount, MailSummary,
    MarkedRead, Message, RenameFolder,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap::{self, Failure, Line, Place};
use crate::mail::{Mail, READERS};
use crate::message;
use crate::queued::ReadHere;
use crate::rules::{Renamed, leaf};
use crate::state::AppState;

/// How many of the mails a search finds it shows, from each folder.
const FOUND: usize = 50;

/// The most a list shows, every account together.
const SHOWN: usize = 200;

impl Mail {
    /// The workspace's folders with what's unread in them, as this Mac
    /// keeps them: Sent, Archive, Drafts, Trash and Junk, then the rest by
    /// name. The keeper is asked to look again.
    pub async fn folders(self: &Arc<Self>, workspace_id: Uuid) -> Result<Vec<Folder>, AppError> {
        self.keep_now();
        let accounts = self.accounts(workspace_id).await?;
        let ids: Vec<Uuid> = accounts.iter().map(|account| account.id).collect();
        let counts = &self.kept_counts(&ids).await?;
        let listed = join_all(accounts.iter().map(|account| async move {
            // Offline with nothing kept, an account shows no folders.
            let places = self.places(account).await.unwrap_or_default();
            places
                .into_iter()
                .map(|place| {
                    let counted = counts.get(&(account.id, place.path.clone()));
                    (place, counted.copied().unwrap_or((0, 0)))
                })
                .collect::<Vec<_>>()
        }))
        .await;
        let mut folders: Vec<Folder> = Vec::new();
        for (place, (unseen, all)) in listed.into_iter().flatten() {
            match folders.iter_mut().find(|folder| place.is(&folder.key)) {
                Some(folder) => {
                    folder.unread += unseen;
                    folder.total += all;
                }
                None => folders.push(Folder {
                    key: place.key(),
                    name: place.role.map_or_else(|| place.name.clone(), role_name),
                    role: place.role,
                    unread: unseen,
                    total: all,
                }),
            }
        }
        folders.sort_by_key(|f| (f.role.map_or(5, |role| role as u8), f.name.to_lowercase()));
        Ok(folders)
    }

    /// An account's folders, as its server listed them last: as kept here
    /// when Postrider starts, else as it lists them now.
    pub(crate) async fn places(&self, account: &MailAccount) -> Result<Vec<Place>, Failure> {
        if let Some(places) = self.lock_places().get(&account.id) {
            return Ok(places.clone());
        }
        let kept = self
            .kept_places(account.id)
            .await
            .map_err(|e| Failure::Offline(format!("couldn't read this Mac's copy: {e}")))?;
        if !kept.is_empty() {
            // The keeper's, should it have listed them meanwhile.
            return Ok(self.lock_places().entry(account.id).or_insert(kept).clone());
        }
        self.relist(account).await
    }

    /// An account's folders as its server lists them now, kept.
    pub(crate) async fn relist(&self, account: &MailAccount) -> Result<Vec<Place>, Failure> {
        let places = self
            .on_server(account, |session| Box::pin(imap::folders(session)))
            .await?;
        self.lock_places().insert(account.id, places.clone());
        if let Err(e) = self.keep_places(account.id, &places, None).await {
            tracing::warn!(address = %account.address, "couldn't keep its folders: {e}");
        }
        Ok(places)
    }

    /// The account's folder that `key` names: the Inbox, or one of its own.
    pub(crate) async fn place(
        &self,
        account: &MailAccount,
        key: &str,
    ) -> Result<Option<Place>, Failure> {
        if key.eq_ignore_ascii_case("inbox") {
            return Ok(Some(Place::inbox()));
        }
        Ok(self
            .places(account)
            .await?
            .into_iter()
            .find(|place| place.is(key)))
    }

    /// Renames the folder `key` names, in every account that has it. A
    /// folder for a role, like Sent, keeps its name. One account refusing
    /// puts back those renamed before it.
    pub async fn rename(
        self: &Arc<Self>,
        workspace_id: Uuid,
        key: &str,
        name: &str,
    ) -> Result<Vec<Renamed>, AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::invalid("Give the folder a name."));
        }
        if name.contains('/') || name.eq_ignore_ascii_case("inbox") {
            return Err(AppError::invalid(format!(
                "A folder can't be called {name}."
            )));
        }
        let refused = |failure: Failure| AppError::refused(failure.message());
        let mut renaming = Vec::new();
        for account in self.accounts(workspace_id).await? {
            let Some(from) = self.place(&account, key).await.map_err(refused)? else {
                continue;
            };
            if let Some(role) = from.role {
                return Err(AppError::invalid(format!(
                    "{} is the same in every mail app: rename your own folders.",
                    role_name(role)
                )));
            }
            if let Some(there) = self.place(&account, name).await.map_err(refused)?
                && there.path != from.path
            {
                return Err(AppError::invalid(format!(
                    "{} has a folder called {name} already.",
                    account.name
                )));
            }
            renaming.push((account, from));
        }
        if renaming.is_empty() {
            return Err(AppError::not_found(format!(
                "There's no folder called {key}."
            )));
        }
        let mut renamed: Vec<Renamed> = Vec::new();
        for (account, from) in renaming {
            match self.rename_folder(&account, &from, name).await {
                Ok(done) => renamed.push(done),
                Err(failure) => {
                    for done in renamed.iter().rev() {
                        let back = leaf(&done.from.name);
                        let _ = self.rename_folder(&done.account, &done.to, back).await;
                    }
                    return Err(AppError::refused(format!(
                        "{}: {}",
                        account.name,
                        failure.message()
                    )));
                }
            }
        }
        Ok(renamed)
    }

    /// Marks every unread mail in the folder `key` names read, in every
    /// account that has it, each in one STORE, and in the Inbox's copy.
    /// One account refusing puts back those marked before it; one that
    /// can't be reached is marked here, and its server waits for it
    /// (`queued.rs`). What it marked waits for Undo, by the id it answers
    /// with.
    pub async fn mark_all_read(
        self: &Arc<Self>,
        workspace_id: Uuid,
        key: &str,
    ) -> Result<MarkedRead, AppError> {
        let mut marked: Vec<Read> = Vec::new();
        for account in self.accounts(workspace_id).await? {
            let read = async {
                let Some(place) = self.place(&account, key).await? else {
                    return Ok(None);
                };
                let marked = if self.state_of(account.id) == Some(AccountState::Online) {
                    self.on_server(&account, |session| {
                        let path = place.path.clone();
                        Box::pin(async move { imap::mark_all_read(session, &path).await })
                    })
                    .await
                } else {
                    Err(Failure::Offline(String::new()))
                };
                let (validity, uids) = match marked {
                    Ok(marked) => marked,
                    Err(Failure::Offline(_)) => {
                        let Some(here) = self.queue_read(account.id, &place.path).await? else {
                            return Ok(None);
                        };
                        self.keep_now();
                        return Ok(Some(Read {
                            account: account.clone(),
                            place,
                            validity: here.validity,
                            uids: Vec::new(),
                            queued: Some(here),
                        }));
                    }
                    Err(failure) => return Err(failure),
                };
                if place.path == Place::inbox().path {
                    self.seen_here(&account, &place, &uids, true).await?;
                } else {
                    self.all_kept(account.id, &place.path, false).await?;
                }
                Ok::<_, Failure>(Some(Read {
                    account: account.clone(),
                    place,
                    validity,
                    uids,
                    queued: None,
                }))
            }
            .await;
            match read {
                Ok(read) => marked.extend(read),
                Err(failure) => {
                    let _ = self.mark_unread(&marked).await;
                    return Err(AppError::refused(format!(
                        "{}: {}",
                        account.name,
                        failure.message()
                    )));
                }
            }
        }
        let count = marked
            .iter()
            .map(|read| {
                read.queued
                    .as_ref()
                    .map_or(read.uids.len(), |here| here.uids.len())
            })
            .sum::<usize>();
        let id = Uuid::now_v7();
        self.lock_marked().insert(
            id,
            Marked {
                workspace_id,
                marked,
            },
        );
        Ok(MarkedRead {
            id,
            count: i64::try_from(count).unwrap_or(i64::MAX),
        })
    }

    /// Puts back what marking a folder read marked. What couldn't be put
    /// back waits for another try.
    pub async fn undo_read(&self, workspace_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let marked = {
            let mut all = self.lock_marked();
            match all.get(&id) {
                Some(marked) if marked.workspace_id == workspace_id => all.remove(&id),
                _ => None,
            }
        }
        .ok_or_else(|| AppError::not_found("There's nothing to undo here any more."))?;
        if let Err(problem) = self.mark_unread(&marked.marked).await {
            self.lock_marked().insert(id, marked);
            return Err(AppError::refused(problem));
        }
        Ok(())
    }

    /// Marks unread again what marking read marked, in every account; one
    /// that can't be reached is marked here, and its server waits for it
    /// (`queued.rs`). Why one couldn't, if one couldn't.
    async fn mark_unread(&self, marked: &[Read]) -> Result<(), String> {
        let mut failed = None;
        for read in marked {
            let done = async {
                if let Some(here) = &read.queued {
                    // Still waiting: it was never marked on its server.
                    self.unqueue_read(read.account.id, &read.place.path, here)
                        .await?;
                    return Ok(());
                }
                if read.uids.is_empty() {
                    return Ok(());
                }
                let unmarked = if self.state_of(read.account.id) == Some(AccountState::Online) {
                    self.on_server(&read.account, |session| {
                        let (path, uids) = (read.place.path.clone(), read.uids.clone());
                        let validity = read.validity;
                        Box::pin(
                            async move { imap::mark_unread(session, &path, validity, &uids).await },
                        )
                    })
                    .await
                } else {
                    Err(Failure::Offline(String::new()))
                };
                match unmarked {
                    Ok(()) => {}
                    Err(Failure::Offline(_)) => {
                        let uids: Vec<i64> = read.uids.iter().map(|uid| i64::from(*uid)).collect();
                        self.queue_marks(
                            read.account.id,
                            &read.place.path,
                            &uids,
                            Some(false),
                            None,
                        )
                        .await?;
                        self.keep_now();
                    }
                    Err(failure) => return Err(failure),
                }
                self.seen_here(&read.account, &read.place, &read.uids, false)
                    .await?;
                Ok::<_, Failure>(())
            }
            .await;
            if let Err(failure) = done {
                failed = Some(format!("{}: {}", read.account.name, failure.message()));
            }
        }
        failed.map_or(Ok(()), Err)
    }

    /// This Mac's copy of these mails, read or unread as their server has
    /// them now: the Inbox's, or the folder's, whose count the keeper
    /// looks at again.
    async fn seen_here(
        &self,
        account: &MailAccount,
        place: &Place,
        uids: &[u32],
        seen: bool,
    ) -> Result<(), sqlx::Error> {
        if uids.is_empty() {
            return Ok(());
        }
        let uids: Vec<i64> = uids.iter().map(|uid| i64::from(*uid)).collect();
        if place.path != Place::inbox().path {
            self.seen_kept(account.id, &place.path, &uids, seen).await?;
            self.keep_now();
            return Ok(());
        }
        let updated =
            sqlx::query("UPDATE inbox_mails SET seen = $3 WHERE account_id = $1 AND uid = ANY($2)")
                .bind(account.id)
                .bind(&uids)
                .bind(seen)
                .execute(&self.pool)
                .await?;
        if updated.rows_affected() > 0 {
            self.bump();
        }
        Ok(())
    }

    /// Deletes the folder `key` names, in every account that has it: its
    /// mail goes to the account's Trash, and the rules that sort into it
    /// go too. The Inbox, a folder for a role, and one with folders in it
    /// stay.
    pub async fn delete(self: &Arc<Self>, workspace_id: Uuid, key: &str) -> Result<(), AppError> {
        if key.eq_ignore_ascii_case("inbox") {
            return Err(AppError::invalid("The Inbox stays."));
        }
        let refused = |failure: Failure| AppError::refused(failure.message());
        let mut deleting = Vec::new();
        for account in self.accounts(workspace_id).await? {
            let Some(place) = self.place(&account, key).await.map_err(refused)? else {
                continue;
            };
            if let Some(role) = place.role {
                return Err(AppError::invalid(format!(
                    "{} is the same in every mail app: delete your own folders.",
                    role_name(role)
                )));
            }
            let inside = format!("{} / ", place.name);
            if self
                .places(&account)
                .await
                .map_err(refused)?
                .iter()
                .any(|other| other.name.starts_with(&inside))
            {
                return Err(AppError::invalid(format!(
                    "{} has folders in it: delete those first.",
                    place.name
                )));
            }
            deleting.push((account, place));
        }
        if deleting.is_empty() {
            return Err(AppError::not_found(format!(
                "There's no folder called {key}."
            )));
        }
        for (account, place) in deleting {
            self.delete_folder(&account, &place)
                .await
                .map_err(|failure| {
                    AppError::refused(format!("{}: {}", account.name, failure.message()))
                })?;
        }
        Ok(())
    }

    /// Empties the Trash or Junk, in every account: what's in it is gone
    /// for good. An account that can't be reached keeps its own, and says
    /// so.
    pub async fn empty(self: &Arc<Self>, workspace_id: Uuid, key: &str) -> Result<(), AppError> {
        let role = [FolderRole::Trash, FolderRole::Junk]
            .into_iter()
            .find(|role| role.key().eq_ignore_ascii_case(key))
            .ok_or_else(|| AppError::invalid("Only the Trash and Junk are emptied."))?;
        let mut failed = None;
        for account in self.accounts(workspace_id).await? {
            let emptied = async {
                let Some(place) = self.place(&account, role.key()).await? else {
                    return Ok(0);
                };
                let gone = self
                    .on_server(&account, |session| {
                        let path = place.path.clone();
                        Box::pin(async move { imap::empty(session, &path).await })
                    })
                    .await?;
                self.lock_lines().remove(&(account.id, place.path.clone()));
                self.all_kept(account.id, &place.path, true).await?;
                Ok::<_, Failure>(gone)
            }
            .await;
            match emptied {
                Ok(gone) => {
                    tracing::info!(address = %account.address, "emptied {gone} from {}", role_name(role));
                }
                Err(failure) => {
                    failed = Some(format!("{}: {}", account.name, failure.message()));
                }
            }
        }
        failed.map_or(Ok(()), |problem| Err(AppError::refused(problem)))
    }

    /// A folder's newest mail, from every account that has it: as kept
    /// here, else from its server. An account whose server can't be
    /// reached says so beside what's kept of it. The keeper is asked to
    /// look again.
    pub async fn folder(
        self: &Arc<Self>,
        workspace_id: Uuid,
        key: &str,
    ) -> Result<Listing, AppError> {
        self.keep_now();
        let accounts = self.accounts(workspace_id).await?;
        let read = join_all(accounts.iter().map(|account| async move {
            let Some(place) = self.place(account, key).await? else {
                return Ok(Vec::new());
            };
            let kept = self
                .kept_listing(account.id, &place)
                .await
                .map_err(|e| Failure::Offline(format!("couldn't read this Mac's copy: {e}")))?;
            match kept {
                Some(kept) => Ok(kept),
                None => self.read(account, &place, None).await,
            }
        }))
        .await;
        let mut listed = listing(&accounts, read, HashMap::new());
        for account in &accounts {
            if listed
                .problems
                .iter()
                .any(|problem| problem.account_id == account.id)
            {
                continue;
            }
            if let Some(status) = self.status_of(account.id).filter(|status| {
                matches!(status.state, AccountState::Offline | AccountState::Refused)
            }) {
                listed.problems.push(status);
            }
        }
        self.count_waiting(&mut listed.problems).await?;
        Ok(listed)
    }

    /// The mails with every word of `query` in them, in the Inbox and the
    /// folders of every account: not in Drafts, Trash or Junk. An account
    /// that can't be asked shows what this Mac has of it.
    pub async fn search(
        self: &Arc<Self>,
        workspace_id: Uuid,
        query: &str,
    ) -> Result<Listing, AppError> {
        let words: Vec<String> = query.split_whitespace().map(str::to_string).collect();
        if words.is_empty() {
            return Ok(Listing {
                mails: Vec::new(),
                problems: Vec::new(),
            });
        }
        let accounts = self.accounts(workspace_id).await?;
        let here = self.found_here(&accounts, &words).await?;
        let query =
            &imap::words_query(&words).map_err(|failure| AppError::invalid(failure.message()))?;
        let read = join_all(accounts.iter().map(|account| async move {
            let mut places = vec![Place::inbox()];
            places.extend(self.places(account).await?.into_iter().filter(searched));
            // A few folders at once, each over a connection of its own.
            let found: Vec<Vec<MailSummary>> = stream::iter(places)
                .map(|place| async move { self.read(account, &place, Some(query)).await })
                .buffer_unordered(READERS)
                .try_collect()
                .await?;
            Ok(found.concat())
        }))
        .await;
        Ok(listing(&accounts, read, here))
    }

    /// What a search finds in what this Mac has read already, without
    /// asking a server: shown at once, while [`Mail::search`] asks them.
    pub async fn search_here(&self, workspace_id: Uuid, query: &str) -> Result<Listing, AppError> {
        let words: Vec<String> = query.split_whitespace().map(str::to_string).collect();
        let accounts = self.accounts(workspace_id).await?;
        let mut here = self.found_here(&accounts, &words).await?;
        let read = accounts
            .iter()
            .map(|account| Ok(here.remove(&account.id).unwrap_or_default()))
            .collect();
        Ok(listing(&accounts, read, HashMap::new()))
    }

    /// The mails with every word in what a list shows of them, each
    /// account's: in the Inbox's copy, in the folders kept here, and in
    /// the lines searches read from the folders a search asks. A line only
    /// a search read shows read until the servers answer.
    async fn found_here(
        &self,
        accounts: &[MailAccount],
        words: &[String],
    ) -> Result<HashMap<Uuid, Vec<MailSummary>>, AppError> {
        let mut found: HashMap<Uuid, Vec<MailSummary>> = HashMap::new();
        if words.is_empty() {
            return Ok(found);
        }
        let ids: Vec<Uuid> = accounts.iter().map(|account| account.id).collect();
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
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT account_id, uid, from_name, from_address, subject, snippet, date, seen, \
             flagged FROM inbox_mails WHERE account_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        let inbox = Place::inbox();
        for (account_id, uid, from_name, from_address, subject, snippet, date, seen, flagged) in
            rows
        {
            let mail = MailSummary {
                account_id,
                folder: Some(inbox.key()),
                uid,
                from_name,
                from_address,
                subject,
                snippet,
                date,
                seen,
                flagged,
                to_name: None,
            };
            if has_words(&mail, words) {
                found.entry(account_id).or_default().push(mail);
            }
        }
        let mut kept: HashSet<(Uuid, String, i64)> = HashSet::new();
        for (account_id, place, line) in self.kept_lines(&ids, false).await? {
            if !searched(&place) {
                continue;
            }
            kept.insert((account_id, place.path.clone(), line.uid));
            let mail = summary(account_id, &place, line);
            if has_words(&mail, words) {
                found.entry(account_id).or_default().push(mail);
            }
        }
        let places = self.lock_places().clone();
        let lines = self.lock_lines();
        for ((account_id, path), (_, lines)) in lines.iter() {
            let place = if *path == inbox.path {
                inbox.clone()
            } else {
                let Some(place) = places
                    .get(account_id)
                    .and_then(|places| places.iter().find(|place| place.path == *path))
                else {
                    continue;
                };
                place.clone()
            };
            if !ids.contains(account_id) || !searched(&place) {
                continue;
            }
            let mails = found.entry(*account_id).or_default();
            for (uid, line) in lines {
                if kept.contains(&(*account_id, path.clone(), *uid)) {
                    continue;
                }
                // The Inbox's copy has it already, with its read state.
                if place.path == inbox.path
                    && mails
                        .iter()
                        .any(|mail| mail.uid == *uid && mail.folder == Some(inbox.key()))
                {
                    continue;
                }
                let line = Line {
                    uid: *uid,
                    summary: line.clone(),
                    seen: true,
                    flagged: false,
                };
                let mail = summary(*account_id, &place, line);
                if has_words(&mail, words) {
                    mails.push(mail);
                }
            }
        }
        Ok(found)
    }

    /// A folder's newest mail, kept here, or what `query` (an IMAP search)
    /// finds in it, as the list shows it. Lines read or kept before aren't
    /// read again.
    pub(crate) async fn read(
        &self,
        account: &MailAccount,
        place: &Place,
        query: Option<&str>,
    ) -> Result<Vec<MailSummary>, Failure> {
        let Some(query) = query else {
            let (read, _) = self.read_newest(account, place, None).await?;
            return Ok(read
                .lines
                .into_iter()
                .map(|line| summary(account.id, place, line))
                .collect());
        };
        let at = (account.id, place.path.clone());
        let (mut validity, mut known) = self.lock_lines().get(&at).cloned().unwrap_or_default();
        let (kept_validity, kept) = self
            .kept_summaries(account.id, &place.path)
            .await
            .map_err(|e| Failure::Offline(format!("couldn't read this Mac's copy: {e}")))?;
        if !kept.is_empty() {
            if kept_validity != validity {
                known.clear();
                validity = kept_validity;
            }
            known.extend(kept);
        }
        let read = self
            .on_server(account, |session| {
                let (path, known, query) = (place.path.clone(), known.clone(), query.to_string());
                Box::pin(async move {
                    imap::search(session, &path, &query, FOUND, &known, validity).await
                })
            })
            .await?;
        {
            let mut kept = self.lock_lines();
            let (validity, lines) = kept.entry(at).or_default();
            if *validity != read.validity {
                *validity = read.validity;
                lines.clear();
            }
            for line in &read.lines {
                lines.insert(line.uid, line.summary.clone());
            }
        }
        Ok(read
            .lines
            .into_iter()
            .map(|line| summary(account.id, place, line))
            .collect())
    }

    /// A mail from a folder (or found in the Inbox), opened: from this
    /// Mac's copy, else from its server, and marked read there when it
    /// isn't yet.
    pub async fn open_in(
        self: &Arc<Self>,
        workspace_id: Uuid,
        account_id: Uuid,
        key: &str,
        uid: i64,
    ) -> Result<Message, AppError> {
        let account = self.account(workspace_id, account_id).await?;
        let (place, raw) = self.source_of(&account, Some(key), uid).await?;
        let kept = self.kept_line(account.id, &place.path, uid).await?;
        let seen = kept.as_ref().is_some_and(|(line, _)| line.seen);
        let flagged = kept.as_ref().is_some_and(|(line, _)| line.flagged);
        let known = kept.map(|(line, _)| line.summary).or_else(|| {
            self.lock_lines()
                .get(&(account.id, place.path.clone()))
                .and_then(|(_, lines)| lines.get(&uid).cloned())
        });
        let line = Line {
            uid,
            summary: known.unwrap_or_else(|| imap::summarize(&raw, None)),
            seen: true,
            flagged,
        };
        if !seen {
            let mail = Arc::clone(self);
            let path = place.path.clone();
            tokio::spawn(async move { mail.mark_read(account, path, uid).await });
        }
        Ok(message::opened(&raw, summary(account_id, &place, line)))
    }
}

/// What marking a folder read marked, for Undo.
pub(crate) struct Marked {
    workspace_id: Uuid,
    marked: Vec<Read>,
}

/// An account's folder marked read: under which UIDVALIDITY, and which
/// of its mails its server marked; here only, while that waits for its
/// server (`queued`, `queued.rs`).
struct Read {
    account: MailAccount,
    place: Place,
    validity: u32,
    uids: Vec<u32>,
    queued: Option<ReadHere>,
}

impl Mail {
    /// A folder marked read while its server couldn't be reached, marked
    /// there now: Undo puts back what its server marked.
    pub(crate) fn marked_on_server(&self, id: Uuid, validity: u32, uids: Vec<u32>) {
        for marked in self.lock_marked().values_mut() {
            for read in &mut marked.marked {
                if read.queued.as_ref().is_some_and(|here| here.id == id) {
                    read.queued = None;
                    read.validity = validity;
                    read.uids.clone_from(&uids);
                }
            }
        }
    }
}

/// What the sidebar calls a folder for its role, whatever its server does.
pub(crate) fn role_name(role: FolderRole) -> String {
    match role {
        FolderRole::Sent => "Sent",
        FolderRole::Archive => "Archive",
        FolderRole::Drafts => "Drafts",
        FolderRole::Trash => "Trash",
        FolderRole::Junk => "Junk",
        FolderRole::Newsletters => "Newsletters",
    }
    .into()
}

/// A line in a folder, as the page gets it. Sent mail names who it went to.
pub(crate) fn summary(account_id: Uuid, place: &Place, line: Line) -> MailSummary {
    let imap::Summary {
        from_name,
        from_address,
        to_name,
        subject,
        snippet,
        date,
        ..
    } = line.summary;
    MailSummary {
        account_id,
        folder: Some(place.key()),
        uid: line.uid,
        from_name,
        from_address,
        subject,
        snippet,
        date,
        seen: line.seen,
        flagged: line.flagged,
        to_name: (place.role == Some(FolderRole::Sent)).then_some(to_name),
    }
}

/// Whether a search asks the folder: not Drafts, Trash or Junk.
pub(crate) fn searched(place: &Place) -> bool {
    !matches!(
        place.role,
        Some(FolderRole::Drafts | FolderRole::Trash | FolderRole::Junk)
    )
}

/// Whether every word is in what a list shows of the mail, in any case:
/// who sent it, to whom, its subject and its start. The servers look in
/// all of it.
fn has_words(mail: &MailSummary, words: &[String]) -> bool {
    let shown = [
        &mail.from_name,
        &mail.from_address,
        &mail.subject,
        &mail.snippet,
    ]
    .into_iter()
    .chain(mail.to_name.as_ref())
    .map(|text| text.to_lowercase())
    .collect::<Vec<_>>()
    .join("\n");
    words
        .iter()
        .all(|word| shown.contains(&word.to_lowercase()))
}

/// Every account's mails as one list, newest first, and why any account
/// couldn't be read; with what this Mac has of it (`here`), if anything.
pub(crate) fn listing(
    accounts: &[MailAccount],
    read: Vec<Result<Vec<MailSummary>, Failure>>,
    mut here: HashMap<Uuid, Vec<MailSummary>>,
) -> Listing {
    let mut mails = Vec::new();
    let mut problems = Vec::new();
    for (account, read) in accounts.iter().zip(read) {
        match read {
            Ok(found) => mails.extend(found),
            Err(failure) => {
                mails.extend(here.remove(&account.id).unwrap_or_default());
                problems.push(AccountStatus {
                    account_id: account.id,
                    state: match failure {
                        Failure::Offline(_) => AccountState::Offline,
                        Failure::Refused(_) => AccountState::Refused,
                    },
                    message: Some(failure.message()),
                    waiting: 0,
                });
            }
        }
    }
    mails.sort_by(|a, b| b.date.cmp(&a.date).then(b.uid.cmp(&a.uid)));
    mails.truncate(SHOWN);
    Listing { mails, problems }
}

/// `GET /api/w/{workspace}/folders`: the folders, each once.
pub async fn folders(
    State(state): State<AppState>,
    member: Member,
) -> Result<Json<Vec<Folder>>, AppError> {
    Ok(Json(state.mail.folders(member.actor.workspace_id).await?))
}

/// `GET /api/w/{workspace}/folders/{key}`: a folder's newest mail.
pub async fn folder(
    State(state): State<AppState>,
    member: Member,
    Path((_, key)): Path<(Uuid, String)>,
) -> Result<Json<Listing>, AppError> {
    Ok(Json(
        state.mail.folder(member.actor.workspace_id, &key).await?,
    ))
}

/// `POST /api/w/{workspace}/folders/{key}/rename`: the folder renamed in
/// every account, with its rules; it answers with the folder's new key.
pub async fn rename(
    State(state): State<AppState>,
    member: Member,
    Path((_, key)): Path<(Uuid, String)>,
    Json(rename): Json<RenameFolder>,
) -> Result<Json<FolderLink>, AppError> {
    let renamed = state
        .mail
        .rename(member.actor.workspace_id, &key, &rename.name)
        .await?;
    let to = &renamed[0].to;
    Ok(Json(FolderLink {
        key: to.key(),
        name: to.name.clone(),
    }))
}

/// `POST /api/w/{workspace}/folders/{key}/read`: the folder's unread mail
/// marked read in every account; it answers with how many, and the id
/// Undo takes.
pub async fn mark_all_read(
    State(state): State<AppState>,
    member: Member,
    Path((_, key)): Path<(Uuid, String)>,
) -> Result<Json<MarkedRead>, AppError> {
    Ok(Json(
        state
            .mail
            .mark_all_read(member.actor.workspace_id, &key)
            .await?,
    ))
}

/// `POST /api/w/{workspace}/marked/{id}/undo`: that mail unread again.
pub async fn undo_read(
    State(state): State<AppState>,
    member: Member,
    Path((_, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.mail.undo_read(member.actor.workspace_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/w/{workspace}/folders/{key}`: the folder gone from every
/// account, its mail in the Trash.
pub async fn delete(
    State(state): State<AppState>,
    member: Member,
    Path((_, key)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    state.mail.delete(member.actor.workspace_id, &key).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/w/{workspace}/folders/{key}/empty`: the Trash or Junk
/// emptied, in every account.
pub async fn empty(
    State(state): State<AppState>,
    member: Member,
    Path((_, key)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    state.mail.empty(member.actor.workspace_id, &key).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SearchParams {
    q: String,
    /// Only what this Mac has, without asking the servers.
    #[serde(default)]
    here: bool,
}

/// `GET /api/w/{workspace}/search?q=`: the mail with every word in it;
/// with `here=true`, what this Mac has of it, at once.
pub async fn search(
    State(state): State<AppState>,
    member: Member,
    Query(params): Query<SearchParams>,
) -> Result<Json<Listing>, AppError> {
    let workspace_id = member.actor.workspace_id;
    Ok(Json(if params.here {
        state.mail.search_here(workspace_id, &params.q).await?
    } else {
        state.mail.search(workspace_id, &params.q).await?
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoice() -> MailSummary {
        MailSummary {
            account_id: Uuid::nil(),
            folder: Some("Sent".into()),
            uid: 4,
            from_name: "Netcup".into(),
            from_address: "noreply@netcup.test".into(),
            subject: "Ihre Rechnung für November".into(),
            snippet: "Den Betrag buchen wir am 5. November ab.".into(),
            date: Utc::now(),
            seen: true,
            flagged: false,
            to_name: Some("Kristina".into()),
        }
    }

    #[test]
    fn a_mail_here_has_every_word_in_any_case() {
        let words = |text: &str| {
            text.split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert!(has_words(&invoice(), &words("netcup RECHNUNG")));
        assert!(has_words(&invoice(), &words("für betrag")));
        assert!(has_words(&invoice(), &words("kristina")));
        assert!(!has_words(&invoice(), &words("netcup Oktober")));
    }
}
