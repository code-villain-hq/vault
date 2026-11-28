//! A task's pull request between draft and ready for review, from its card
//! (PR-15). Before, that meant opening GitHub: Open PRs opens drafts by
//! default, and nothing here could take one out of draft.

use std::path::PathBuf;

use tauri::State;

use crate::error::Result;
use crate::git;

use super::github::github_client;
use super::{off_runtime, AppState};

/// Make pull request `number` of a checkout's repository a draft, or ready
/// for review. Returns whether it is a draft now.
#[tauri::command]
pub async fn github_set_pr_draft(
    state: State<'_, AppState>,
    checkout_id: String,
    number: u64,
    draft: bool,
) -> Result<bool> {
    let checkout = state.config.checkout(&checkout_id)?;
    let (client, _) = github_client(&state).await?;
    let dir = PathBuf::from(&checkout.path);
    let (owner, name) = off_runtime(move || git::origin_slug(&dir)).await??;
    client.set_pull_draft(&owner, &name, number, draft).await
}
