//! Whether Claude Code can answer on this Mac, as Settings → Claude shows it.

use serde::{Deserialize, Serialize};

use crate::acp::{Adapter, Failure};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeCodeInfo {
    /// `ready`, `not_installed`, `signed_out` or `trouble`.
    pub state: String,
    /// The ACP adapter's version, when it's ready.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// How to install it, as a Terminal command.
    pub install: String,
    /// What went wrong, in its words, with `trouble`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Whether it can answer: starts it and opens a session, asking nothing.
pub async fn how_it_is(adapter: &Adapter) -> ClaudeCodeInfo {
    let (state, version, problem) = match adapter.check().await {
        Ok(version) => ("ready", version, None),
        Err(Failure::NotInstalled) => ("not_installed", None, None),
        Err(Failure::SignedOut) => ("signed_out", None, None),
        Err(Failure::Trouble(said)) => ("trouble", None, Some(said)),
    };
    ClaudeCodeInfo {
        state: state.into(),
        version,
        install: adapter.kind().install().into(),
        problem,
    }
}
