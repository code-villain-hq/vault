//! Folders and searches, read live from the servers: every account's
//! folders, each once by its name or role, at `GET /api/w/{workspace}/folders`;
//! a folder's newest mail at `/folders/{key}`; what a search finds in all
//! of them at `/search?q=`; and a mail opened from either at
//! `/mails/{account}/{uid}?folder=<key>`.
//!
//! Only the Inbox is followed and kept in Postgres. The rest is asked for
//! when it's shown; what the lists showed is kept in memory, so a folder
//! opened again reads only its new mail. A search finds what this Mac has
//! of it first (`/search?q=&here=true`), at once, while the servers are
//! asked, and an account that can't be asked shows that.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use futures::{StreamExt, TryStreamExt, stream};
use postrider_domain::{
    AccountState, AccountStatus, Folder, FolderLink, FolderRole, Listing, MailAccount, MailSummary,
    Message, RenameFolder,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap::{self, Failure, Line, Place};
use crate::mail::{Mail, READERS};
use crate::message;
use crate::rules::{Renamed, leaf};
use crate::state::AppState;

/// How many of a folder's newest mails a list shows, from each account.
const NEWEST: u32 = 100;

/// How many of the mails a search finds it shows, from each folder.
const FOUND: usize = 50;

/// The most a list shows, every account together.
const SHOWN: usize = 200;

impl Mail {
    /// The workspace's folders with what's unread in them: Sent, Archive,
    /// Drafts, Trash and Junk, then the rest by name.
    pub async fn folders(self: &Arc<Self>, workspace_id: Uuid) -> Result<Vec<Folder>, AppError> {
        let accounts = self.accounts(workspace_id).await?;
        let listed = join_all(accounts.iter().map(|account| self.count_folders(account))).await;
        let mut folders: Vec<Folder> = Vec::new();
        for (place, unseen) in listed.into_iter().flatten() {
            match folders.iter_mut().find(|folder| place.is(&folder.key)) {
                Some(folder) => folder.unread += i64::from(unseen),
                None => folders.push(Folder {
                    key: place.key(),
                    name: place.role.map_or_else(|| place.name.clone(), role_name),
                    role: place.role,
                    unread: i64::from(unseen),
                }),
            }
        }
        folders.sort_by_key(|f| (f.role.map_or(5, |role| role as u8), f.name.to_lowercase()));
        Ok(folders)
    }

    /// An account's folders with what's unread in each, as its server says
    /// now: in Newsletters, what came since you last looked there
    /// ([`Mail::last_looked`]). Offline, the folders it had, with nothing
    /// counted.
    async fn count_folders(&self, account: &MailAccount) -> Vec<(Place, u32)> {
        let looked = self.last_looked(account).await;
        let counted = self
            .on_server(account, |session| {
                Box::pin(async move {
                    let mut counted = Vec::new();
                    for place in imap::folders(session).await? {
                        let unseen = if place.role == Some(FolderRole::Newsletters) {
                            imap::unseen_since(session, &place.path, looked).await?
                        } else {
                            imap::unseen(session, &place.path).await?
                        };
                        counted.push((place, unseen));
                    }
                    Ok(counted)
                })
            })
            .await;
        match counted {
            Ok(counted) => {
                let places = counted.iter().map(|(place, _)| place.clone()).collect();
                self.lock_places().insert(account.id, places);
                counted
            }
            Err(failure) => {
                tracing::info!(address = %account.address, "couldn't list folders: {}", failure.message());
                self.lock_places()
                    .get(&account.id)
                    .map(|places| places.iter().map(|place| (place.clone(), 0)).collect())
                    .unwrap_or_default()
            }
        }
    }

    /// An account's folders, as its server listed them last, else now.
    pub(crate) async fn places(&self, account: &MailAccount) -> Result<Vec<Place>, Failure> {
        if let Some(places) = self.lock_places().get(&account.id) {
            return Ok(places.clone());
        }
        let places = self
            .on_server(account, |session| Box::pin(imap::folders(session)))
            .await?;
        self.lock_places().insert(account.id, places.clone());
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

    /// A folder's newest mail, from every account that has it.
    pub async fn folder(
        self: &Arc<Self>,
        workspace_id: Uuid,
        key: &str,
    ) -> Result<Listing, AppError> {
        let accounts = self.accounts(workspace_id).await?;
        let read = join_all(accounts.iter().map(|account| async move {
            match self.place(account, key).await? {
                Some(place) => self.read(account, &place, None).await,
                None => Ok(Vec::new()),
            }
        }))
        .await;
        Ok(listing(&accounts, read, HashMap::new()))
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
        let query = &imap::words_query(&words);
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
    /// account's: in the Inbox's copy, and in the lines the lists and
    /// searches read from the folders a search asks. Their read state is
    /// the servers' to tell: a folder's line shows read until they answer.
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
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT account_id, uid, from_name, from_address, subject, snippet, date, seen \
             FROM inbox_mails WHERE account_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        let inbox = Place::inbox();
        for (account_id, uid, from_name, from_address, subject, snippet, date, seen) in rows {
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
                to_name: None,
            };
            if has_words(&mail, words) {
                found.entry(account_id).or_default().push(mail);
            }
        }
        let places = self.lock_places().clone();
        let kept = self.lock_lines();
        for ((account_id, path), (_, lines)) in kept.iter() {
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
                };
                let mail = summary(*account_id, &place, line);
                if has_words(&mail, words) {
                    mails.push(mail);
                }
            }
        }
        Ok(found)
    }

    /// A folder's newest mail, or what `query` (an IMAP search) finds in
    /// it, as the list shows it. Lines read before aren't read again.
    pub(crate) async fn read(
        &self,
        account: &MailAccount,
        place: &Place,
        query: Option<&str>,
    ) -> Result<Vec<MailSummary>, Failure> {
        let at = (account.id, place.path.clone());
        let (validity, known) = self.lock_lines().get(&at).cloned().unwrap_or_default();
        let read = self
            .on_server(account, |session| {
                let (path, known, query) =
                    (place.path.clone(), known.clone(), query.map(str::to_string));
                Box::pin(async move {
                    match query {
                        Some(query) => {
                            imap::search(session, &path, &query, FOUND, &known, validity).await
                        }
                        None => imap::newest(session, &path, NEWEST, &known, validity).await,
                    }
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

    /// A mail from a folder (or found in the Inbox), opened: read from its
    /// server each time, and marked read there.
    pub async fn open_in(
        self: &Arc<Self>,
        workspace_id: Uuid,
        account_id: Uuid,
        key: &str,
        uid: i64,
    ) -> Result<Message, AppError> {
        let account = self.account(workspace_id, account_id).await?;
        let (place, raw) = self.source_of(&account, Some(key), uid).await?;
        let known = self
            .lock_lines()
            .get(&(account.id, place.path.clone()))
            .and_then(|(_, lines)| lines.get(&uid).cloned());
        let line = Line {
            uid,
            summary: known.unwrap_or_else(|| imap::summarize(&raw, None)),
            seen: true,
        };
        let mail = Arc::clone(self);
        let path = place.path.clone();
        tokio::spawn(async move { mail.mark_read(account, path, uid).await });
        Ok(message::read(&raw, summary(account_id, &place, line)))
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
fn summary(account_id: Uuid, place: &Place, line: Line) -> MailSummary {
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
        to_name: (place.role == Some(FolderRole::Sent)).then_some(to_name),
    }
}

/// Whether a search asks the folder: not Drafts, Trash or Junk.
fn searched(place: &Place) -> bool {
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
