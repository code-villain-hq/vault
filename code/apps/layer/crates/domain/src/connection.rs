use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use url::Url;
use uuid::Uuid;

use crate::model::{Validate, ValidationError, invalid, parse, text, web_address};

/// Where the workspace's Jira, GitHub, Slack or Grafana is, and how it is
/// used: one of each kind. The `tracker` kind is no service of its own: it
/// says which one the workspace's tickets come from (TKT-15). What is one person's own never syncs: their
/// token (in their keychain), their Jira email, and Slack's notify switches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Connection {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// One connection per kind in a workspace. It can't change.
    pub kind: ConnectionKind,
    /// The kind's settings: [`JiraSettings`], [`GithubSettings`],
    /// [`SlackSettings`], [`GrafanaSettings`] or [`TrackerSettings`].
    #[ts(as = "ConnectionSettings")]
    pub settings: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A connection's settings, for the clients: which kind they are is the
/// record's `kind`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum ConnectionSettings {
    Jira(JiraSettings),
    Github(GithubSettings),
    GithubSources(GithubSourcesSettings),
    Slack(SlackSettings),
    Grafana(GrafanaSettings),
    Tracker(TrackerSettings),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ConnectionKind {
    Jira,
    Github,
    GithubSources,
    Slack,
    Grafana,
    Tracker,
}

impl ConnectionKind {
    /// Checks `settings` are this kind's, with nothing else in them.
    pub fn check_settings(self, settings: &Value) -> Result<(), ValidationError> {
        let settings = settings.clone();
        match self {
            ConnectionKind::Jira => parse::<JiraSettings>(settings)?.validate(),
            ConnectionKind::Github => parse::<GithubSettings>(settings)?.validate(),
            ConnectionKind::GithubSources => parse::<GithubSourcesSettings>(settings)?.validate(),
            ConnectionKind::Slack => parse::<SlackSettings>(settings)?.validate(),
            ConnectionKind::Grafana => parse::<GrafanaSettings>(settings)?.validate(),
            ConnectionKind::Tracker => parse::<TrackerSettings>(settings)?.validate(),
        }
    }

    pub fn github_hosts(self, settings: &Value) -> Result<Vec<String>, ValidationError> {
        match self {
            Self::Github => Ok(vec![github_host(
                &parse::<GithubSettings>(settings.clone())?.web_url,
            )?]),
            Self::GithubSources => parse::<GithubSourcesSettings>(settings.clone())?
                .sources
                .iter()
                .map(|source| github_host(&source.web_url))
                .collect(),
            _ => Ok(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct JiraSettings {
    /// e.g. https://acme.atlassian.net
    pub base_url: String,
    pub project_key: Option<String>,
    /// JQL for the task list; the app's own default when left out.
    pub jql: Option<String>,
    /// The custom field this site keeps the Epic Link in, when it has one.
    pub epic_field: Option<String>,
    /// Where tickets go as the work moves, per Jira project.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub flow: HashMap<String, TicketFlow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TicketFlow {
    /// Where a ticket goes when work on it starts.
    pub started: Option<StartTo>,
    /// When its task's pull request is ready for review.
    pub review: Option<FlowStatus>,
    /// Once every pull request of its task has merged.
    pub merged: Option<FlowStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct FlowStatus {
    pub id: String,
    pub name: String,
}

/// Where a ticket goes when work on it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "to", rename_all = "snake_case")]
#[ts(export)]
pub enum StartTo {
    /// The first "in progress" status its workflow allows.
    FirstInProgress,
    /// Nowhere: the ticket stays where it is.
    Leave,
    Status {
        id: String,
        name: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct GithubSettings {
    /// https://api.github.com, or https://ghe.example.com/api/v3.
    pub api_url: String,
    /// Web base, for links.
    pub web_url: String,
    /// The team whose review queue is listed beside your own: `@fe` or `org/fe`.
    pub review_team: Option<String>,
}

/// Additional GitHub hosts. Keep the old singleton's settings shape so an
/// older desktop never mistakes one of these for the connection it knows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct GithubSourcesSettings {
    pub sources: Vec<GithubSettings>,
}

/// Parse the address the desktop will use, without letting secrets into
/// shared settings through URL userinfo or query parameters.
pub fn github_address(value: &str) -> Result<Url, ValidationError> {
    let bad = || {
        ValidationError(
            "A GitHub address must be HTTPS, without credentials, a query or a fragment.".into(),
        )
    };
    if value.len() > 500 || value.contains(char::is_whitespace) {
        return Err(bad());
    }
    let url = Url::parse(value).map_err(|_| bad())?;
    let host = url.host_str().ok_or_else(bad)?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if !(url.scheme() == "https" || url.scheme() == "http" && loopback)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(bad());
    }
    Ok(url)
}

pub fn github_host(value: &str) -> Result<String, ValidationError> {
    let url = github_address(value)?;
    Ok(url
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_ascii_lowercase())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SlackSettings {
    /// Where the app posts: `#dev` or a channel id.
    pub channel: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct GrafanaSettings {
    /// e.g. https://acme.grafana.net
    pub base_url: String,
    /// The Loki datasource agents query when they name none.
    pub datasource: Option<String>,
}

/// Which connection the workspace's tickets come from (TKT-15). A
/// workspace without one reads as Jira while it has a Jira connection, so
/// those connected before there was a choice keep it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct TrackerSettings {
    pub uses: TrackerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_source: Option<String>,
}

impl Validate for TrackerSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        if let Some(host) = &self.github_source
            && (self.uses != TrackerKind::Github
                || host.is_empty()
                || github_host(&format!("https://{host}")).ok().as_ref() != Some(host))
        {
            return invalid("Choose a GitHub host for GitHub Issues.");
        }
        Ok(())
    }
}

/// The issue trackers a workspace can use. Forge joins once it is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrackerKind {
    Jira,
    /// GitHub Issues, in the workspace's repositories, with Code's token.
    Github,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ConnectionCreate {
    pub kind: ConnectionKind,
    #[ts(as = "ConnectionSettings")]
    pub settings: Value,
}

/// The settings, whole. Which kind they must be is the record's, so the
/// server checks them against it (`layer-sync`'s rules).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ConnectionPatch {
    #[ts(as = "ConnectionSettings")]
    pub settings: Value,
}

impl Validate for ConnectionCreate {
    fn validate(&self) -> Result<(), ValidationError> {
        self.kind.check_settings(&self.settings)
    }
}

impl Validate for ConnectionPatch {
    fn validate(&self) -> Result<(), ValidationError> {
        if !self.settings.is_object() {
            return invalid("A connection's settings are an object.");
        }
        Ok(())
    }
}

impl Validate for JiraSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        web_address(&self.base_url, "A Jira site")?;
        if let Some(key) = &self.project_key {
            let valid = !key.is_empty()
                && key.len() <= 20
                && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                return invalid("A Jira project key is letters and digits, like ACME.");
            }
        }
        if let Some(jql) = &self.jql {
            text(jql, "JQL", 2000)?;
        }
        Ok(())
    }
}

impl Validate for GithubSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        github_address(&self.api_url)?;
        github_address(&self.web_url)?;
        if let Some(team) = &self.review_team {
            text(team, "A review team", 100)?;
        }
        Ok(())
    }
}

impl Validate for GithubSourcesSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        let mut hosts = HashSet::new();
        for source in &self.sources {
            source.validate()?;
            let host = github_host(&source.web_url)?;
            if !hosts.insert(host) {
                return invalid("The workspace already has a GitHub source for that host.");
            }
        }
        Ok(())
    }
}

impl Validate for SlackSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        text(&self.channel, "A Slack channel", 80)
    }
}

impl Validate for GrafanaSettings {
    fn validate(&self) -> Result<(), ValidationError> {
        web_address(&self.base_url, "A Grafana site")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_are_their_kinds() {
        let jira = json!({ "base_url": "https://acme.atlassian.net", "project_key": "ACME" });
        assert!(ConnectionKind::Jira.check_settings(&jira).is_ok());
        assert!(ConnectionKind::Grafana.check_settings(&jira).is_err());
        let slack = json!({ "channel": "#dev" });
        assert!(ConnectionKind::Slack.check_settings(&slack).is_ok());
    }

    #[test]
    fn nobody_puts_a_token_or_an_email_in_shared_settings() {
        for settings in [
            json!({ "base_url": "https://acme.atlassian.net", "email": "ada@acme.test" }),
            json!({ "base_url": "https://acme.atlassian.net", "token": "secret" }),
        ] {
            assert!(ConnectionKind::Jira.check_settings(&settings).is_err());
        }
        let slack = json!({ "channel": "#dev", "notify_on_done": false });
        assert!(ConnectionKind::Slack.check_settings(&slack).is_err());
    }

    #[test]
    fn a_ticket_flow_reads_as_the_app_writes_it() {
        let jira = json!({
            "base_url": "https://acme.atlassian.net",
            "flow": { "ACME": {
                "started": { "to": "status", "id": "3", "name": "Doing" },
                "review": { "id": "4", "name": "Review" }
            } }
        });
        assert!(ConnectionKind::Jira.check_settings(&jira).is_ok());
    }

    #[test]
    fn a_tracker_names_one_the_app_knows_and_nothing_else() {
        assert!(
            ConnectionKind::Tracker
                .check_settings(&json!({ "uses": "jira" }))
                .is_ok()
        );
        assert!(
            ConnectionKind::Tracker
                .check_settings(&json!({ "uses": "redmine" }))
                .is_err()
        );
        assert!(
            ConnectionKind::Tracker
                .check_settings(&json!({ "uses": "jira", "token": "secret" }))
                .is_err()
        );
        assert!(ConnectionKind::Tracker.check_settings(&json!({})).is_err());
    }

    #[test]
    fn additional_github_hosts_are_token_free_and_unique() {
        let check = |settings| {
            let create: ConnectionCreate = serde_json::from_value(json!({
                "kind": "github_sources", "settings": settings,
            }))?;
            Ok::<_, serde_json::Error>(create.validate())
        };
        let public = json!({"api_url": "https://api.github.com", "web_url": "https://github.com"});
        let enterprise = json!({"api_url": "https://git.example.com/api/v3", "web_url": "https://git.example.com"});
        let settings = json!({"sources": [public, enterprise]});
        assert!(check(settings.clone()).is_ok_and(|result| result.is_ok()));
        let mut duplicate = settings.clone();
        duplicate["sources"][1]["web_url"] = json!("https://GITHUB.COM:443/");
        assert!(check(duplicate).is_ok_and(|result| result.is_err()));
        let mut secret = settings.clone();
        secret["sources"][1]["token"] = json!("do not share");
        assert!(check(secret).is_ok_and(|result| result.is_err()));
        let mut userinfo = settings;
        userinfo["sources"][1]["api_url"] = json!("https://secret@git.example.com/api/v3");
        assert!(check(userinfo).is_ok_and(|result| result.is_err()));
    }
}
