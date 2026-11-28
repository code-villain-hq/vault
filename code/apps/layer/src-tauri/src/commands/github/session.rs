use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Instant;

use tokio::sync::Mutex;

use crate::error::Result;
use crate::integrations::github::GhTeam;

use super::AppState;

/// Orders credential changes, client snapshots, and publication of results.
/// Keychain, config writes, and review publication use the blocking pool;
/// async readers wait for the same gate without occupying a worker.
#[derive(Default)]
pub(crate) struct NativeGitHub {
    pub generation: u64,
    pub reviews_generation: u64,
    pub settled: HashMap<String, Instant>,
    pub swept: Option<Instant>,
    /// Each source's review team as resolved, with the setting it came from.
    pub resolved_team: HashMap<String, (String, GhTeam)>,
    // Unlike the PR cache, an in-flight Jira move must survive reconnect.
    ticket_transitions: HashMap<String, Weak<Mutex<()>>>,
}

impl NativeGitHub {
    pub(crate) fn ticket_transition(&mut self, task_id: &str) -> Arc<Mutex<()>> {
        self.ticket_transitions.retain(|_, lock| lock.strong_count() != 0);
        if let Some(lock) = self.ticket_transitions.get(task_id).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        self.ticket_transitions.insert(task_id.into(), Arc::downgrade(&lock));
        lock
    }

    /// Every change to a GitHub source passes here: a connect, a disconnect,
    /// a URL edit, or the workspace sending its shared sources. A changed
    /// address or credential on any source is a reconnect for all of them,
    /// since a read in flight cannot tell which source answered it.
    pub(crate) fn update<T>(
        &mut self,
        state: &AppState,
        credentials_changed: bool,
        write: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let read = || {
            state.config.with(|c| {
                c.github_source_list()
                    .into_iter()
                    .map(|s| ((s.key, s.config.api_url, s.config.web_url, s.status, s.revision), s.config.review_team))
                    .unzip::<_, _, Vec<_>, Vec<_>>()
            })
        };
        let (before, team_before) = read();
        let result = write();
        let (after, team_after) = read();
        if credentials_changed || before != after {
            self.generation += 1;
            self.reviews_generation += 1;
            self.settled.clear();
            self.swept = None;
            self.resolved_team.clear();
            state.prs_seen.lock().clear();
            state.news.forget_reviews();
        } else if team_before != team_after {
            // Editing a preference is not a reconnect: existing personal
            // requests still count, and the new team's first list is quiet.
            self.reviews_generation += 1;
            self.resolved_team.clear();
            state.news.forget_team();
        }
        result
    }
}

/// The session's generations when a read started. A read publishes what it
/// found (settled tasks, review banners, ticket moves) only if no source
/// changed in the meantime.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GitHubGate {
    pub generation: u64,
    pub reviews_generation: u64,
}

pub(crate) async fn github_gate(state: &AppState) -> GitHubGate {
    let session = state.github_session.lock().await;
    GitHubGate { generation: session.generation, reviews_generation: session.reviews_generation }
}
