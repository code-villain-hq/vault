//! Where the integrations are, and how each is set: Jira, GitHub, Slack,
//! the Slack bot, Grafana.
//! Their tokens are in the keychain (`secrets`), never here.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JiraConfig {
    /// The custom field this site keeps the Epic Link in, discovered on
    /// connect. Only company-managed projects still use one; modern Cloud puts
    /// the epic on `parent`, so None is both common and fine.
    #[serde(default)]
    pub epic_field: Option<String>,
    /// e.g. https://your-site.atlassian.net
    pub base_url: String,
    pub email: String,
    #[serde(default)]
    pub project_key: Option<String>,
    /// JQL used for the task list; falls back to a sensible default.
    #[serde(default)]
    pub jql: Option<String>,
    /// Where tickets go as the work moves, per Jira project (TKT-8). Chosen
    /// in Settings: "Review" and "In Progress" share a status category, and
    /// a status's name is not the app's to assume.
    #[serde(default)]
    pub flow: HashMap<String, TicketFlow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TicketFlow {
    /// Where a ticket goes when work on it starts (TKT-1). None until chosen,
    /// and then as `UiPrefs::sync_jira_status` says: the switch that was all
    /// there was before, in General, out of sight of the rest of this.
    #[serde(default)]
    pub started: Option<StartTo>,
    /// Where a ticket goes when its task's pull request is ready for review.
    #[serde(default)]
    pub review: Option<FlowStatus>,
    /// Where it goes once every pull request of its task has merged.
    #[serde(default)]
    pub merged: Option<FlowStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowStatus {
    pub id: String,
    pub name: String,
}

/// Where a ticket goes when work on it starts (TKT-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum StartTo {
    /// The first status in Jira's "in progress" category its workflow
    /// allows from where it is, unless it is in that category already.
    FirstInProgress,
    /// Nowhere: the ticket stays where it is.
    Leave,
    /// This status, unless the ticket is there already.
    Status { id: String, name: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubConfig {
    /// https://api.github.com, or https://ghe.example.com/api/v3 for Enterprise.
    pub api_url: String,
    /// Web base, used for building links.
    pub web_url: String,
    /// Team whose review queue is listed beside your own. `@fe` or `org/fe`.
    /// Absent means only reviews requested of you. A name, not a constant:
    /// which team matters is a property of the install.
    #[serde(default)]
    pub review_team: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub channel: String,
    /// Master switch. Off means the app posts nothing at all, without having to
    /// disconnect and re-enter the token.
    #[serde(default = "super::yes")]
    pub enabled: bool,
    /// An agent pane exiting.
    #[serde(default = "super::yes")]
    pub notify_on_done: bool,
    /// Pull requests opened for a task.
    #[serde(default = "super::yes")]
    pub notify_on_prs: bool,
    /// The `slack_post` MCP tool. Agents post unattended, so this is separate
    /// from the app's own notifications.
    #[serde(default = "super::yes")]
    pub allow_agent_posts: bool,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            channel: String::new(),
            enabled: true,
            notify_on_done: true,
            notify_on_prs: true,
            allow_agent_posts: true,
        }
    }
}

/// The Slack bot (§24): a Slack app of the user's own, connected over
/// Socket Mode while the app runs. Both its tokens are in the keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackBotConfig {
    /// The one Slack member the bot takes requests from (BOT-2), `U…`.
    pub owner: String,
    /// The agent a thread is handed to, over ACP (BOT-3).
    pub agent_id: String,
    /// The bot's own member id, as Slack said when it was connected: what a
    /// mention of it looks like, and whose messages are its own.
    #[serde(default)]
    pub bot_user: String,
    /// Its handle, for showing.
    #[serde(default)]
    pub bot_name: String,
    /// Off keeps the tokens and the threads, and closes the connection.
    #[serde(default = "super::yes")]
    pub enabled: bool,
    /// Chats opened from Slack start in the agent's `auto` mode (BOT-12):
    /// a permission question waits for someone who is not at the Mac.
    #[serde(default = "super::yes")]
    pub auto: bool,
    /// Who the bot is, in the owner's words: the voice its chats answer in
    /// (BOT-13). Empty is the agent's own.
    #[serde(default)]
    pub personality: String,
    /// The model its chats run on, as the agent names it (BOT-17). Empty is
    /// the agent's own choice.
    #[serde(default)]
    pub model: String,
}

/// A model an agent offered when one of its conversations opened (BOT-17).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelChoice {
    pub value: String,
    pub name: String,
}

/// A Slack thread handed to a chat (BOT-4). Tied by the chat's room folder,
/// which outlives the pane's id across restarts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SlackThread {
    pub channel: String,
    /// The thread's first message, which names the thread.
    pub ts: String,
    pub room: String,
    /// `#channel`, or "a direct message".
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub link: String,
    /// The newest message of the thread its chat was given (BOT-5): a
    /// mention brings the chat what was said after it. Empty for a thread
    /// handed over before this was kept; `event::gap` then reads it from
    /// the owner's newest message.
    #[serde(default)]
    pub seen: String,
}

/// Logs kept in S3 that agents read (OBS-8, OBS-9): where they are, and the
/// AWS profile that reads them. No AWS key: the profile is the user's own,
/// signed in by the `aws` CLI. Every field defaults, so a source written by
/// a later version still loads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogSource {
    pub id: String,
    /// What the user calls it, and agents name it by: "Production request logs".
    pub name: String,
    pub profile: String,
    /// The profile's own when None.
    pub region: Option<String>,
    pub bucket: String,
    /// Agents read only under it (OBS-10). Empty, or ending in `/`.
    pub prefix: String,
    pub athena: Option<AthenaTable>,
}

/// The Athena table over a log source's files, for querying them (OBS-12).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AthenaTable {
    pub workgroup: String,
    pub database: String,
    pub table: String,
    /// The most data its queries may scan in a day, in GB (OBS-14). None is
    /// the default, `DEFAULT_DAILY_GB`.
    pub daily_limit_gb: Option<u64>,
}

/// The Grafana site agents read logs from (OBS-1). Its token is in the
/// keychain; the datasources are asked of the site each time, not kept.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GrafanaConfig {
    /// e.g. https://acme.grafana.net
    pub base_url: String,
    /// The Loki datasource's uid agents query when they name none (OBS-5).
    /// A site often has several (Grafana Cloud adds its own usage logs),
    /// and only the user knows which one their services write to.
    #[serde(default)]
    pub datasource: Option<String>,
}
