//! GitHub's shared definitions and this Mac's verified credential bindings.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::secrets;

use super::{shared, AppConfig, ConfigStore, GithubConfig};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GithubSources {
    pub shared: Vec<GithubConfig>,
    pub local: Vec<GithubConfig>,
    pub credentials: BTreeMap<String, GithubCredential>,
    pub legacy_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GithubCredential {
    pub origin: String,
    /// No slot means the credential is the v4 token, to be moved on first use.
    #[serde(default)]
    pub slot: Option<uuid::Uuid>,
}

impl GithubCredential {
    pub fn new(config: &GithubConfig) -> Result<Self> {
        Ok(Self { origin: config.api_origin()?, slot: Some(uuid::Uuid::new_v4()) })
    }

    fn key(&self, source: &str) -> String {
        let slot = self.slot.map_or_else(|| "legacy".into(), |id| id.to_string());
        format!("github-token:{source}:{}:{slot}", self.origin)
    }

    pub fn read(&self, source: &str) -> Result<Option<String>> {
        let key = self.key(source);
        match self.slot {
            Some(_) => secrets::get(&key),
            None => secrets::migrate_github(&key),
        }
    }

    pub fn save(&self, source: &str, token: &str) -> Result<()> {
        secrets::set(&self.key(source), token)
    }

    pub fn delete(&self, source: &str) -> Result<()> {
        secrets::forget_github(&self.key(source), self.slot.is_none())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GithubScope {
    Legacy,
    Shared,
    Local,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GithubSource {
    pub key: String,
    #[serde(flatten)]
    pub config: GithubConfig,
    pub shared: bool,
    pub status: &'static str,
    /// Changes on token-only reconnects, with no secret material from the keychain.
    pub revision: Option<uuid::Uuid>,
    #[serde(skip)]
    pub(crate) scope: GithubScope,
}

impl GithubConfig {
    pub(crate) fn source_key(&self) -> Option<String> {
        layer_domain::github_host(&self.web_url).ok().filter(|key| !key.is_empty())
    }

    pub(crate) fn api_origin(&self) -> Result<String> {
        layer_domain::github_address(&self.api_url)
            .map(|url| url.origin().ascii_serialization())
            .map_err(|e| Error::Other(e.0))
    }
}

impl AppConfig {
    pub(crate) fn migrate_github(&mut self) {
        let legacy = self.github.as_ref().and_then(GithubConfig::source_key);
        self.github_sources.legacy_host = legacy.or_else(|| {
            self.shared
                .held
                .get("github")
                .and_then(|held| held.settings["web_url"].as_str())
                .and_then(|url| layer_domain::github_host(url).ok())
        });
        if let Some((key, origin)) = self.github.as_ref().and_then(|g| Some((g.source_key()?, g.api_origin().ok()?))) {
            self.github_sources.credentials.entry(key).or_insert(GithubCredential { origin, slot: None });
        }
        if self.shared.workspace.as_ref().is_some_and(|w| !w.is_admin()) && !self.shared.held.contains_key("github") {
            self.github_sources.local.extend(self.github.take());
        }
    }

    pub(crate) fn github_source_list(&self) -> Vec<GithubSource> {
        let legacy = self
            .github
            .clone()
            .or_else(|| self.shared.held.get("github").and_then(|h| serde_json::from_value(h.settings.clone()).ok()));
        let mut all = BTreeMap::new();
        for (config, scope) in self
            .github_sources
            .local
            .iter()
            .map(|g| (g, GithubScope::Local))
            .chain(self.github_sources.shared.iter().map(|g| (g, GithubScope::Shared)))
            .chain(legacy.iter().map(|g| (g, GithubScope::Legacy)))
        {
            let Some(key) = config.source_key() else { continue };
            let binding = self.github_sources.credentials.get(&key);
            let status = match binding {
                Some(b) if config.api_origin().is_ok_and(|origin| origin == b.origin) => "connected",
                Some(_) => "reconnect_required",
                None => "disconnected",
            };
            all.insert(
                key.clone(),
                GithubSource {
                    key,
                    config: config.clone(),
                    shared: scope != GithubScope::Local,
                    status,
                    revision: binding.and_then(|b| b.slot),
                    scope,
                },
            );
        }
        all.into_values().collect()
    }

    pub(crate) fn github_source(&self, key: &str) -> Option<GithubSource> {
        self.github_source_list().into_iter().find(|g| g.key == key)
    }

    pub(crate) fn github_any_connected(&self) -> bool {
        self.github_source_list().iter().any(|g| g.status == "connected")
    }

    pub(crate) fn github_issue_host(&self) -> Option<&str> {
        self.github_issue_source.as_deref().or(self.github_sources.legacy_host.as_deref())
    }

    pub(crate) fn set_github_source(&mut self, was: Option<&GithubSource>, next: GithubConfig) {
        let scope = was.map(|g| g.scope).unwrap_or_else(|| {
            if self.shared.workspace.as_ref().is_some_and(|w| !w.is_admin()) {
                GithubScope::Local
            } else {
                GithubScope::Shared
            }
        });
        let list = match scope {
            GithubScope::Legacy => {
                self.github = Some(next);
                return;
            }
            GithubScope::Shared => &mut self.github_sources.shared,
            GithubScope::Local => &mut self.github_sources.local,
        };
        if let Some(was) = was {
            list.retain(|g| g.source_key().as_deref() != Some(&was.key));
        }
        list.push(next);
        list.sort_by_key(GithubConfig::source_key);
    }
}

impl ConfigStore {
    /// The caller may already have written a new keychain slot. Do not publish
    /// its binding or sync settings until the config write succeeded.
    pub(crate) fn update_github<T>(&self, f: impl FnOnce(&mut AppConfig) -> Result<T>) -> Result<T> {
        let _disk = self.disk.lock();
        let mut snapshot = self.inner.read().clone();
        let before = shared::records(&snapshot);
        let out = f(&mut snapshot)?;
        let sent = shared::record_changes(&mut snapshot, &before);
        self.persist(&snapshot)?;
        *self.inner.write() = snapshot;
        crate::workspace::wrote(sent);
        Ok(out)
    }
}
