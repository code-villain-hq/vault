//! A task's pull request between draft and ready for review, from its card
//! (PR-15). Before, that meant opening GitHub: Open PRs opens drafts by
//! default, and nothing here could take one out of draft.

use tauri::State;

use crate::error::Result;

use super::AppState;

/// Make pull request `number` of a checkout's repository a draft, or ready
/// for review. Returns whether it is a draft now.
#[tauri::command]
pub async fn github_set_pr_draft(
    state: State<'_, AppState>,
    checkout_id: String,
    number: u64,
    draft: bool,
    expected_url: String,
) -> Result<bool> {
    let checkout = state.config.checkout(&checkout_id)?;
    let (origin, client) = super::github_sources::checkout_client(&state, &checkout.path).await?;
    let (owner, name) = origin.slug();
    client.set_pull_draft(owner, name, number, draft, &expected_url).await
}
