//! A repository registered here: the user's clone of it, and what the app
//! knows about it. Its shared half is the workspace's (`shared.rs`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Absolute path to the user's own clone: where the repository was found,
    /// and what the app copies. Task worktrees do not come from it.
    pub path: String,
    pub default_branch: String,
    /// "frontend", "backend", ... One group per repo; None means ungrouped.
    #[serde(default)]
    pub group: Option<String>,
    /// The app's own bare copy of the repository, which task worktrees
    /// belong to (`git/store.rs`). None until it has been made.
    #[serde(default)]
    pub store: Option<String>,
    /// How Update from base brings the base into a branch here: set in the
    /// Repos view, or the way this repo was last updated. None: guessed from
    /// its history (UPD-7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_by: Option<crate::git::UpdateBy>,
    /// What says the work here is done (`bun run check`), run in a
    /// worktree by a loop (LOOP-1). Set only by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// Where specs are committed here (SPEC-1), from its root. None: `specs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_folder: Option<String>,
    /// Specs are kept by the app instead of committed here (SPEC-3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub specs_in_app: bool,
    /// Where it fetches from, as `git::remote_key` spells it
    /// (`github.com/acme/api`): read from `origin` when it was added, or
    /// when its profile became a workspace. None: it has no `origin`, and
    /// stays on this Mac only (PROF-12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// The workspace's record of the repository (PROF-13), shared by every
    /// clone of it here. None until it is in a workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
}

impl Project {
    /// The repository new task worktrees are cut from: the app's copy once
    /// there is one, the user's clone until then. For a worktree that
    /// already exists, ask it (`commands::owner_of`) instead.
    pub fn repo(&self) -> PathBuf {
        PathBuf::from(self.store.as_deref().unwrap_or(&self.path))
    }
}
