use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::model::{Validate, ValidationError, invalid, parse, text, web_address};

/// Where the workspace's Jira, GitHub, Slack or Grafana is, and how it is
/// used: one of each kind. What is one person's own never syncs: their
/// token (in their keychain), their Jira email, and Slack's notify switches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Connection {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// One connection per kind in a workspace. It can't change.
    pub kind: ConnectionKind,
    /// The kind's settings: [`JiraSettings`], [`GithubSettings`],
    /// [`SlackSettings`] or [`GrafanaSettings`].
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
    Slack(SlackSettings),
    Grafana(GrafanaSettings),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ConnectionKind {
    Jira,
    Github,
    Slack,
    Grafana,
}

impl ConnectionKind {
    /// Checks `settings` are this kind's, with nothing else in them.
    pub fn check_settings(self, settings: &Value) -> Result<(), ValidationError> {
        let settings = settings.clone();
        match self {
            ConnectionKind::Jira => parse::<JiraSettings>(settings)?.validate(),
            ConnectionKind::Github => parse::<GithubSettings>(settings)?.validate(),
            ConnectionKind::Slack => parse::<SlackSettings>(settings)?.validate(),
            ConnectionKind::Grafana => parse::<GrafanaSettings>(settings)?.validate(),
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
        web_address(&self.api_url, "GitHub's API address")?;
        web_address(&self.web_url, "GitHub's web address")?;
        if let Some(team) = &self.review_team {
            text(team, "A review team", 100)?;
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
}
