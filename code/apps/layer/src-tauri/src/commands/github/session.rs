use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Instant;

use tokio::sync::Mutex;

use crate::config::GithubConfig;
use crate::error::{Error, Result};
use crate::integrations::github::{GhTeam, GitHub};
use crate::secrets;

use super::{off_runtime, AppState};

/// Orders credential changes, client snapshots, and publication of results.
/// Keychain, config writes, and review publication use the blocking pool;
/// async readers wait for the same gate without occupying a worker.
#[derive(Default)]
pub(crate) struct NativeGitHub {
    pub generation: u64,
    pub reviews_generation: u64,
    pub settled: HashMap<String, Instant>,
    pub swept: Option<Instant>,
    pub resolved_team: Option<GhTeam>,
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

    pub(super) fn update<T>(
        &mut self,
        state: &AppState,
        credentials_changed: bool,
        write: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let read = || state.config.with(|c| c.github.clone());
        let before = read();
        let result = write();
        let after = read();
        let urls = |cfg: Option<GithubConfig>| cfg.map(|c| (c.api_url, c.web_url));
        let team = |cfg: &Option<GithubConfig>| cfg.as_ref().and_then(|c| c.review_team.clone());
        let team_changed = team(&before) != team(&after);
        if credentials_changed || urls(before) != urls(after) {
            self.generation += 1;
            self.reviews_generation += 1;
            self.settled.clear();
            self.swept = None;
            self.resolved_team = None;
            state.prs_seen.lock().clear();
            state.news.forget_reviews();
        } else if team_changed {
            // Editing a preference is not a reconnect: existing personal
            // requests still count, and the new team's first list is quiet.
            self.reviews_generation += 1;
            self.resolved_team = None;
            state.news.forget_team();
        }
        result
    }
}

pub(crate) struct ConnectedGitHub {
    pub client: GitHub,
    pub config: GithubConfig,
    pub generation: u64,
    pub reviews_generation: u64,
}

pub(crate) async fn connected_github(state: &AppState) -> Result<ConnectedGitHub> {
    let session = state.github_session.clone().lock_owned().await;
    let config = state.config.with(|c| c.github.clone()).ok_or(Error::NotConfigured("GitHub"))?;
    // Move the guard with the snapshot so reconnect cannot replace the token
    // between reading the configuration and taking the client.
    off_runtime(move || {
        let token = secrets::get(secrets::GITHUB)?.ok_or(Error::NotConfigured("GitHub"))?;
        Ok(ConnectedGitHub {
            client: GitHub::new(&config, &token)?,
            config,
            generation: session.generation,
            reviews_generation: session.reviews_generation,
        })
    })
    .await?
}
