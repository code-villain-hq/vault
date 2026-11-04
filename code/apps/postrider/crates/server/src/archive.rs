//! Filing a mail away: E moves it to its account's Archive, at
//! `POST /api/w/{workspace}/mails/{account}/{uid}/archive[?folder=<key>]`,
//! ⌫ to its Trash, at `…/trash`, and V to a folder you pick, at `…/move`.
//! An account without the folder gets one, beside its other folders.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use postrider_domain::{FolderRole, MailAccount, MoveMail};
use uuid::Uuid;

use crate::auth::Member;
use crate::error::AppError;
use crate::folders::role_name;
use crate::imap::{self, Failure, Place};
use crate::mail::{Mail, OpenParams};
use crate::state::AppState;

impl Mail {
    /// Moves a mail from the Inbox, or the folder `key` names, to the folder
    /// `to` names: the Inbox, a role's like Archive, or one of the account's
    /// own, made when it has none. One there already stays.
    pub async fn move_mail(
        self: &Arc<Self>,
        workspace_id: Uuid,
        account_id: Uuid,
        key: Option<&str>,
        uid: i64,
        to: &str,
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
        let (to, _) = self.destination(&account, to).await.map_err(refused)?;
        if from.path == to.path {
            return Ok(());
        }
        self.on_server(&account, |session| {
            let (from, to) = (from.path.clone(), to.path.clone());
            Box::pin(async move { imap::move_mail(session, &from, uid, &to).await })
        })
        .await
        .map_err(refused)?;
        if from.path == "INBOX" {
            self.forget_inbox_line(account.id, uid).await?;
        } else {
            self.forget_kept(account.id, &from.path, &[uid]).await?;
        }
        if let Some((_, lines)) = self.lock_lines().get_mut(&(account.id, from.path)) {
            lines.remove(&uid);
        }
        // What came to the folder it went to.
        self.keep_now();
        Ok(())
    }

    /// The account's folder `key` names, made when it has none; and
    /// whether a folder of its own was.
    pub(crate) async fn destination(
        &self,
        account: &MailAccount,
        key: &str,
    ) -> Result<(Place, bool), Failure> {
        if key.eq_ignore_ascii_case("inbox") {
            return Ok((Place::inbox(), false));
        }
        match FolderRole::ALL
            .into_iter()
            .find(|role| role.key().eq_ignore_ascii_case(key))
        {
            Some(role) => Ok((self.folder_for(account, role).await?, false)),
            None => self.folder_named(account, key).await,
        }
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
                Box::pin(async move { imap::make_folder(session, &name, Some(role)).await })
            })
            .await?;
        self.lock_places()
            .entry(account.id)
            .or_default()
            .push(made.clone());
        if let Err(e) = self.keep_made(account.id, &made).await {
            tracing::warn!(address = %account.address, "couldn't keep a folder made: {e}");
        }
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
        .move_mail(
            member.actor.workspace_id,
            account_id,
            params.folder.as_deref(),
            uid,
            FolderRole::Archive.key(),
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
        .move_mail(
            member.actor.workspace_id,
            account_id,
            params.folder.as_deref(),
            uid,
            FolderRole::Trash.key(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/w/{workspace}/mails/{account}/{uid}/move[?folder=<key>]`: to
/// the folder `to` names, or a new one by that name.
pub async fn move_to(
    State(state): State<AppState>,
    member: Member,
    Path((_, account_id, uid)): Path<(Uuid, Uuid, i64)>,
    Query(params): Query<OpenParams>,
    Json(moving): Json<MoveMail>,
) -> Result<StatusCode, AppError> {
    let to = moving.to.trim();
    if to.is_empty() || to.contains('/') {
        return Err(AppError::invalid("Pick a folder, or name a new one."));
    }
    state
        .mail
        .move_mail(
            member.actor.workspace_id,
            account_id,
            params.folder.as_deref(),
            uid,
            to,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
