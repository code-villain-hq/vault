//! Folders and searches, read live from the servers: every account's
//! folders, each once by its name or role, at `GET /api/w/{workspace}/folders`;
//! a folder's newest mail at `/folders/{key}`; what a search finds in all
//! of them at `/search?q=`; and a mail opened from either at
//! `/mails/{account}/{uid}?folder=<key>`.
//!
//! Only the Inbox is followed and kept in Postgres. The rest is asked for
//! when it's shown; what the lists showed is kept in memory, so a folder
//! opened again reads only its new mail.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use futures::future::join_all;
use futures::{StreamExt, TryStreamExt, stream};
use postrider_domain::{
    AccountState, AccountStatus, Folder, FolderRole, Listing, MailAccount, MailSummary, Message,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::imap::{self, Failure, Line, Place};
use crate::mail::{Mail, READERS};
use crate::message;
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
    /// now. Offline, the folders it had, with nothing counted.
    async fn count_folders(&self, account: &MailAccount) -> Vec<(Place, u32)> {
        let counted = self
            .on_server(account, |session| {
                Box::pin(async move {
                    let mut counted = Vec::new();
                    for place in imap::folders(session).await? {
                        let unseen = imap::unseen(session, &place.path).await?;
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
        Ok(listing(&accounts, read))
    }

    /// The mails with every word of `query` in them, in the Inbox and the
    /// folders of every account: not in Drafts, Trash or Junk.
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
        let words = &words;
        let read = join_all(accounts.iter().map(|account| async move {
            let mut places = vec![Place::inbox()];
            places.extend(self.places(account).await?.into_iter().filter(|place| {
                !matches!(
                    place.role,
                    Some(FolderRole::Drafts | FolderRole::Trash | FolderRole::Junk)
                )
            }));
            // A few folders at once, each over a connection of its own.
            let found: Vec<Vec<MailSummary>> = stream::iter(places)
                .map(|place| async move { self.read(account, &place, Some(words)).await })
                .buffer_unordered(READERS)
                .try_collect()
                .await?;
            Ok(found.concat())
        }))
        .await;
        Ok(listing(&accounts, read))
    }

    /// A folder's newest mail, or what `words` find in it, as the list
    /// shows it. Lines read before aren't read again.
    async fn read(
        &self,
        account: &MailAccount,
        place: &Place,
        words: Option<&[String]>,
    ) -> Result<Vec<MailSummary>, Failure> {
        let at = (account.id, place.path.clone());
        let (validity, known) = self.lock_lines().get(&at).cloned().unwrap_or_default();
        let read = self
            .on_server(account, |session| {
                let (path, known, words) = (
                    place.path.clone(),
                    known.clone(),
                    words.map(<[String]>::to_vec),
                );
                Box::pin(async move {
                    match words {
                        Some(words) => {
                            imap::search(session, &path, &words, FOUND, &known, validity).await
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
        message_id: _,
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

/// Every account's mails as one list, newest first, and why any account
/// couldn't be read.
fn listing(accounts: &[MailAccount], read: Vec<Result<Vec<MailSummary>, Failure>>) -> Listing {
    let mut mails = Vec::new();
    let mut problems = Vec::new();
    for (account, read) in accounts.iter().zip(read) {
        match read {
            Ok(found) => mails.extend(found),
            Err(failure) => problems.push(AccountStatus {
                account_id: account.id,
                state: match failure {
                    Failure::Offline(_) => AccountState::Offline,
                    Failure::Refused(_) => AccountState::Refused,
                },
                message: Some(failure.message()),
            }),
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

#[derive(Deserialize)]
pub struct SearchParams {
    q: String,
}

/// `GET /api/w/{workspace}/search?q=`: the mail with every word in it.
pub async fn search(
    State(state): State<AppState>,
    member: Member,
    Query(params): Query<SearchParams>,
) -> Result<Json<Listing>, AppError> {
    Ok(Json(
        state
            .mail
            .search(member.actor.workspace_id, &params.q)
            .await?,
    ))
}
