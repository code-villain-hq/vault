//! Filing a mail away: E moves it to its account's Archive, at
//! `POST /api/w/{workspace}/mails/{account}/{uid}/archive[?folder=<key>]`,
//! and ⌫ to its Trash, at `…/trash`. An account without the folder gets
//! one, beside its other folders.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use postrider_domain::{FolderRole, MailAccount};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::folders::role_name;
use crate::imap::{self, Failure, Place};
use crate::mail::{Mail, OpenParams};
use crate::state::AppState;

impl Mail {
    /// Moves a mail from the Inbox, or the folder `key` names, to its
    /// account's folder for `role`: Archive or Trash. One there already stays.
    pub async fn file_away(
        self: &Arc<Self>,
        workspace_id: Uuid,
        account_id: Uuid,
        key: Option<&str>,
        uid: i64,
        role: FolderRole,
    ) -> Result<(), AppError> {
        let refused = |failure: Failure| AppError::refused(failure.message());
        let account = self.account(workspace_id, account_id).await?;
        let from = match key {
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
        if from.role == Some(role) {
            return Ok(());
        }
        let to = self.folder_for(&account, role).await.map_err(refused)?;
        self.on_server(&account, |session| {
            let (from, to) = (from.path.clone(), to.path.clone());
            Box::pin(async move { imap::move_mail(session, &from, uid, &to).await })
        })
        .await
        .map_err(refused)?;
        if from.path == "INBOX" {
            self.forget_inbox_line(account.id, uid).await?;
        }
        if let Some((_, lines)) = self.lock_lines().get_mut(&(account.id, from.path)) {
            lines.remove(&uid);
        }
        Ok(())
    }

    /// The account's folder for `role`, made when it has none.
    pub(crate) async fn folder_for(
        &self,
        account: &MailAccount,
        role: FolderRole,
    ) -> Result<Place, Failure> {
        let places = self.places(account).await?;
        if let Some(place) = places.into_iter().find(|place| place.role == Some(role)) {
            return Ok(place);
        }
        let name = role_name(role);
        let made = self
            .on_server(account, |session| {
                let name = name.clone();
                Box::pin(async move { imap::make_folder(session, &name, role).await })
            })
            .await?;
        self.lock_places()
            .entry(account.id)
            .or_default()
            .push(made.clone());
        Ok(made)
    }
}

/// `POST /api/w/{workspace}/mails/{account}/{uid}/archive[?folder=<key>]`.
pub async fn archive(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid)): Path<(Uuid, Uuid, i64)>,
    Query(params): Query<OpenParams>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .file_away(
            member.actor.workspace_id,
            account_id,
            params.folder.as_deref(),
            uid,
            FolderRole::Archive,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/w/{workspace}/mails/{account}/{uid}/trash[?folder=<key>]`.
pub async fn trash(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid)): Path<(Uuid, Uuid, i64)>,
    Query(params): Query<OpenParams>,
) -> Result<StatusCode, AppError> {
    state
        .mail
        .file_away(
            member.actor.workspace_id,
            account_id,
            params.folder.as_deref(),
            uid,
            FolderRole::Trash,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
