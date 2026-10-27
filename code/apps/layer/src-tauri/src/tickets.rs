//! Where the workspace's tickets come from (TKT-15): one tracker, chosen per
//! workspace. The app asks `Tracker` and gets the same shapes back whichever
//! it is: Jira or GitHub Issues. What a tracker does in its own way, like
//! Jira's transitions, stays with its client.

pub mod github;
mod jira;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{AppConfig, JiraConfig, TrackerKind};
use crate::error::{Error, Result};
use crate::integrations::github::GitHub;
use crate::integrations::jira::Jira;
use crate::secrets;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Ticket {
    pub key: String,
    pub summary: String,
    pub description: String,
    pub status: String,
    pub status_id: String,
    /// "new", "indeterminate" or "done": what the status means, whatever the
    /// tracker calls it.
    pub status_category: String,
    pub issue_type: String,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    /// The epic (or other parent) this ticket sits under, when it has one.
    pub epic_key: Option<String>,
    pub epic_summary: Option<String>,
    pub url: String,
    /// When it was filed, as RFC 3339 (TKT-11). None if the tracker left it out.
    pub created: Option<String>,
    /// When it last changed, as RFC 3339, for sorting by it (TKT-13).
    pub updated: Option<String>,
}

/// A type of ticket exactly as the tracker defines it. Nothing about types is
/// assumed: the names, the icons and the hierarchy all come from the tracker,
/// so a project with custom types renders as correctly as a stock one.
#[derive(Debug, Clone, Serialize)]
pub struct TicketType {
    pub id: String,
    pub name: String,
    pub subtask: bool,
    /// 1 and above is epic-level, 0 is a standard ticket, -1 is a sub-task.
    pub hierarchy_level: i32,
    /// The tracker's own icon as a data URI. Avatar URLs need authentication,
    /// so the image is fetched here rather than by an <img> tag in the web view.
    pub icon: Option<String>,
}

/// A page of search results, and whether the tracker had more to give.
#[derive(Debug, Clone, Serialize)]
pub struct TicketPage {
    pub issues: Vec<Ticket>,
    /// True when the cap was reached and the tracker still had results.
    pub more: bool,
}

/// A field a project insists on before it will accept a new ticket.
#[derive(Debug, Clone, Serialize)]
pub struct RequiredField {
    pub id: String,
    pub name: String,
    pub required: bool,
    /// "array" when the tracker expects several values, otherwise the scalar type.
    pub kind: String,
    /// The values this field will accept, when it is a closed set.
    pub allowed: Vec<AllowedValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowedValue {
    pub id: String,
    pub name: String,
}

/// A search past your own queue: unassigned work, or anyone else's.
#[derive(Debug, Default)]
pub struct Find {
    pub text: Option<String>,
    pub whose: String,
    pub include_done: bool,
    pub types: Vec<String>,
    pub order: String,
}

#[derive(Debug, Deserialize)]
pub struct NewTicket {
    pub summary: String,
    #[serde(default)]
    pub description: String,
    pub issue_type: String,
    /// Falls back to the parent's project, then to the one in Settings.
    #[serde(default)]
    pub project_key: Option<String>,
    #[serde(default)]
    pub parent_key: Option<String>,
    /// Anything else this project requires, already shaped the way the
    /// tracker wants it by the caller, which is the side that has the metadata.
    #[serde(default)]
    pub fields: Option<Value>,
}

/// The workspace's tracker, connected.
pub enum Tracker {
    Jira(Jira, JiraConfig),
    Github(GitHub, github::Issues),
}

impl Tracker {
    /// The tracker this Mac's workspace uses, with its token from the keychain.
    pub fn connect(cfg: &AppConfig) -> Result<Self> {
        match cfg.tracker_in_use() {
            Some(TrackerKind::Jira) | None => {
                let jira = cfg.jira.clone().ok_or(Error::NotConfigured("Jira"))?;
                let token = secrets::get(secrets::JIRA)?.ok_or(Error::NotConfigured("Jira"))?;
                Ok(Self::Jira(Jira::new(&jira, &token)?, jira))
            }
            // Code's own connection: nothing more to connect (TKT-17).
            Some(TrackerKind::Github) => {
                let gh = cfg.github.clone().ok_or(Error::NotConfigured("GitHub"))?;
                let token = secrets::get(secrets::GITHUB)?.ok_or(Error::NotConfigured("GitHub"))?;
                Ok(Self::Github(GitHub::new(&gh, &token)?, github::Issues::of(cfg, &gh.web_url)))
            }
        }
    }

    pub fn kind(&self) -> TrackerKind {
        match self {
            Self::Jira(..) => TrackerKind::Jira,
            Self::Github(..) => TrackerKind::Github,
        }
    }

    pub async fn ticket(&self, key: &str) -> Result<Ticket> {
        match self {
            Self::Jira(client, _) => client.issue(key).await,
            Self::Github(client, _) => github::ticket(client, key).await,
        }
    }

    /// Your own queue, all of it.
    pub async fn mine(&self) -> Result<TicketPage> {
        match self {
            Self::Jira(client, cfg) => jira::mine(client, cfg).await,
            Self::Github(client, issues) => github::mine(client, issues).await,
        }
    }

    pub async fn find(&self, find: Find) -> Result<TicketPage> {
        match self {
            Self::Jira(client, cfg) => jira::find(client, cfg, find).await,
            Self::Github(client, issues) => github::find(client, issues, find).await,
        }
    }

    /// These tickets, wherever they are; the ones the tracker doesn't have
    /// are left out.
    pub async fn by_key(&self, keys: &[String]) -> Result<Vec<Ticket>> {
        match self {
            Self::Jira(client, _) => jira::by_key(client, keys).await,
            Self::Github(client, _) => github::by_key(client, keys).await,
        }
    }

    pub async fn types(&self) -> Result<Vec<TicketType>> {
        match self {
            Self::Jira(client, _) => client.issue_types().await,
            Self::Github(..) => Ok(github::types()),
        }
    }

    /// The tracker's priorities, highest first.
    pub async fn priorities(&self) -> Result<Vec<String>> {
        match self {
            Self::Jira(client, _) => client.priorities().await,
            // Issues have none.
            Self::Github(..) => Ok(Vec::new()),
        }
    }

    pub async fn required_fields(&self, project_key: &str, type_id: &str) -> Result<Vec<RequiredField>> {
        match self {
            Self::Jira(client, _) => client.create_fields(project_key, type_id).await,
            // A title and a body, and nothing a repository can insist on.
            Self::Github(..) => Ok(Vec::new()),
        }
    }

    /// The id the project gives a type of this name, when it is not `not`.
    pub async fn type_in_project(&self, project_key: &str, name: &str, not: &str) -> Option<String> {
        match self {
            Self::Jira(client, _) => client
                .project_issue_types(project_key)
                .await
                .ok()?
                .into_iter()
                .find(|(id, n)| n.eq_ignore_ascii_case(name) && id != not)
                .map(|(id, _)| id),
            Self::Github(..) => None,
        }
    }

    /// File a ticket, and say its key.
    pub async fn file(&self, new: &NewTicket) -> Result<String> {
        match self {
            Self::Jira(client, cfg) => jira::file(client, cfg, new).await,
            Self::Github(client, issues) => github::file(client, issues, new).await,
        }
    }

    /// Put a ticket under another, as a sub-task under its epic.
    pub async fn put_under(&self, key: &str, parent: &str) -> Result<()> {
        match self {
            Self::Jira(client, _) => client.set_parent(key, parent).await,
            Self::Github(client, _) => client.put_under(&github::issue(key)?, &github::issue(parent)?).await,
        }
    }

    /// What starting work does to a ticket beyond the task, when the
    /// tracker does not move it by itself: Jira's move is `ticket_flow`'s,
    /// and a GitHub issue is given to you if nobody has it (TKT-18).
    pub async fn take(&self, ticket: &Ticket) -> Result<()> {
        match self {
            Self::Jira(..) => Ok(()),
            Self::Github(client, _) => github::take(client, ticket).await,
        }
    }

    pub async fn comment(&self, key: &str, text: &str) -> Result<()> {
        match self {
            Self::Jira(client, _) => client.comment(key, text).await,
            Self::Github(client, _) => client.comment_issue(&github::issue(key)?, text).await,
        }
    }
}

/// Where a ticket is seen in its tracker, without asking it.
pub fn url_of(cfg: &AppConfig, key: &str) -> Option<String> {
    match cfg.tracker_in_use()? {
        TrackerKind::Jira => Some(format!("{}/browse/{key}", cfg.jira.as_ref()?.base_url.trim_end_matches('/'))),
        TrackerKind::Github => github::url_of(&cfg.github.as_ref()?.web_url, key),
    }
}
