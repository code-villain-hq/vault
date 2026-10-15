//! A consistent view of a workspace for the MCP tools, with lookups by the
//! names agents use: issue identifiers, team keys, people's names, titles.
//!
//! It loads the same snapshot clients download. That keeps tools simple and
//! exactly in step with the app; targeted queries can replace it when
//! workspaces grow large.

use std::collections::HashMap;

use forge_domain::{
    Actor, Comment, Decision, Domain, Initiative, Issue, IssueRelation, Label, ModelKind, Project,
    Service, Spec, SpecChange, Team, User, WorkflowState, queue_positions,
};
use serde::de::DeserializeOwned;
use sqlx::PgPool;

pub struct Snapshot {
    pub actor: Actor,
    pub teams: Vec<Team>,
    pub states: Vec<WorkflowState>,
    pub labels: Vec<Label>,
    pub issues: Vec<Issue>,
    pub relations: Vec<IssueRelation>,
    pub comments: Vec<Comment>,
    pub projects: Vec<Project>,
    pub initiatives: Vec<Initiative>,
    pub users: Vec<User>,
    pub services: Vec<Service>,
    pub domains: Vec<Domain>,
    pub specs: Vec<Spec>,
    pub decisions: Vec<Decision>,
    pub spec_changes: Vec<SpecChange>,
    /// Each issue waiting in an agent's queue: its place there, from 1.
    pub queue: HashMap<uuid::Uuid, usize>,
}

pub type ToolResult<T> = Result<T, String>;

impl Snapshot {
    pub async fn load(pool: &PgPool, actor: Actor) -> ToolResult<Snapshot> {
        let mut bootstrap = forge_sync::bootstrap(pool, actor)
            .await
            .map_err(|e| e.to_string())?;
        let mut take = |model: ModelKind| bootstrap.records.remove(&model).unwrap_or_default();
        let mut snapshot = Snapshot {
            actor,
            teams: parse(take(ModelKind::Team))?,
            states: parse(take(ModelKind::WorkflowState))?,
            labels: parse(take(ModelKind::Label))?,
            issues: parse(take(ModelKind::Issue))?,
            relations: parse(take(ModelKind::IssueRelation))?,
            comments: parse(take(ModelKind::Comment))?,
            projects: parse(take(ModelKind::Project))?,
            initiatives: parse(take(ModelKind::Initiative))?,
            users: parse(take(ModelKind::User))?,
            services: parse(take(ModelKind::Service))?,
            domains: parse(take(ModelKind::Domain))?,
            specs: parse(take(ModelKind::Spec))?,
            decisions: parse(take(ModelKind::Decision))?,
            spec_changes: parse(take(ModelKind::SpecChange))?,
            queue: HashMap::new(),
        };
        snapshot.queue = queue_positions(&snapshot.issues, &snapshot.states, &snapshot.users);
        Ok(snapshot)
    }

    pub fn team(&self, key_or_name: &str) -> ToolResult<&Team> {
        let wanted = key_or_name.trim();
        self.teams
            .iter()
            .find(|t| t.key.eq_ignore_ascii_case(wanted) || t.name.eq_ignore_ascii_case(wanted))
            .ok_or_else(|| {
                let keys: Vec<&str> = self.teams.iter().map(|t| t.key.as_str()).collect();
                format!("no team {wanted:?}; teams: {}", keys.join(", "))
            })
    }

    pub fn team_of(&self, issue: &Issue) -> Option<&Team> {
        self.teams.iter().find(|t| t.id == issue.team_id)
    }

    /// `FOR-12`, or the issue id.
    pub fn identifier(&self, issue: &Issue) -> String {
        let key = self.team_of(issue).map_or("?", |t| t.key.as_str());
        format!("{key}-{}", issue.number)
    }

    /// An issue by identifier (`FOR-12`) or id.
    pub fn issue(&self, identifier: &str) -> ToolResult<&Issue> {
        let wanted = identifier.trim();
        let found = match wanted.rsplit_once('-') {
            Some((key, number)) if number.chars().all(|c| c.is_ascii_digit()) => {
                let team = self.teams.iter().find(|t| t.key.eq_ignore_ascii_case(key));
                let number: i32 = number.parse().unwrap_or(-1);
                team.and_then(|team| {
                    self.issues
                        .iter()
                        .find(|i| i.team_id == team.id && i.number == number)
                })
            }
            _ => None,
        };
        found
            .or_else(|| self.issues.iter().find(|i| i.id.to_string() == wanted))
            .ok_or_else(|| format!("no issue {wanted:?}"))
    }

    pub fn state_of(&self, issue: &Issue) -> Option<&WorkflowState> {
        self.states.iter().find(|s| s.id == issue.state_id)
    }

    /// A team's workflow state by name, e.g. "In Progress".
    pub fn state(&self, team: &Team, name: &str) -> ToolResult<&WorkflowState> {
        let mut states: Vec<&WorkflowState> = self
            .states
            .iter()
            .filter(|s| s.team_id == team.id)
            .collect();
        states.sort_by(|a, b| a.position.total_cmp(&b.position));
        states
            .iter()
            .copied()
            .find(|s| s.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| {
                let names: Vec<&str> = states.iter().map(|s| s.name.as_str()).collect();
                format!(
                    "{} has no status {name:?}; its statuses: {}",
                    team.key,
                    names.join(", ")
                )
            })
    }

    /// A member by name or email; "me" is the agent itself.
    pub fn user(&self, who: &str) -> ToolResult<&User> {
        let wanted = who.trim();
        if wanted.eq_ignore_ascii_case("me") {
            return self
                .users
                .iter()
                .find(|u| u.id == self.actor.user_id)
                .ok_or_else(|| "you aren't a member".into());
        }
        self.users
            .iter()
            .find(|u| u.email.eq_ignore_ascii_case(wanted) || u.name.eq_ignore_ascii_case(wanted))
            .ok_or_else(|| {
                let names: Vec<&str> = self.users.iter().map(|u| u.name.as_str()).collect();
                format!("no member {wanted:?}; members: {}", names.join(", "))
            })
    }

    pub fn user_name(&self, id: Option<uuid::Uuid>) -> Option<String> {
        id.and_then(|id| self.users.iter().find(|u| u.id == id))
            .map(|u| u.name.clone())
    }

    /// A label usable by the team, by name.
    pub fn label(&self, team: &Team, name: &str) -> ToolResult<&Label> {
        self.labels
            .iter()
            .find(|l| {
                l.name.eq_ignore_ascii_case(name.trim()) && l.team_id.is_none_or(|t| t == team.id)
            })
            .ok_or_else(|| format!("no label {name:?}"))
    }

    pub fn project(&self, id_or_name: &str) -> ToolResult<&Project> {
        find_named(&self.projects, id_or_name, |p| (p.id, &p.name), "project")
    }

    pub fn service(&self, id_or_name: &str) -> ToolResult<&Service> {
        find_named(&self.services, id_or_name, |s| (s.id, &s.name), "service")
    }

    pub fn domain(&self, id_or_name: &str) -> ToolResult<&Domain> {
        find_named(&self.domains, id_or_name, |d| (d.id, &d.name), "domain")
    }

    /// The name of a spec's domain, if it has one.
    pub fn domain_of(&self, spec: &Spec) -> Option<&str> {
        let id = spec.domain_id?;
        self.domains
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.as_str())
    }

    pub fn spec(&self, id_or_title: &str) -> ToolResult<&Spec> {
        find_named(&self.specs, id_or_title, |s| (s.id, &s.title), "spec")
    }

    pub fn decision(&self, id_or_title: &str) -> ToolResult<&Decision> {
        find_named(
            &self.decisions,
            id_or_title,
            |d| (d.id, &d.title),
            "decision",
        )
    }

    pub fn initiative(&self, id_or_name: &str) -> ToolResult<&Initiative> {
        find_named(
            &self.initiatives,
            id_or_name,
            |i| (i.id, &i.name),
            "initiative",
        )
    }
}

fn parse<T: DeserializeOwned>(rows: Vec<serde_json::Value>) -> ToolResult<Vec<T>> {
    rows.into_iter()
        .map(|row| serde_json::from_value(row).map_err(|e| e.to_string()))
        .collect()
}

/// By id, then exact name (ignoring case), then a unique partial match.
fn find_named<'a, T>(
    records: &'a [T],
    wanted: &str,
    key: impl Fn(&T) -> (uuid::Uuid, &String),
    what: &str,
) -> ToolResult<&'a T> {
    let wanted = wanted.trim();
    if let Some(found) = records.iter().find(|r| key(r).0.to_string() == wanted) {
        return Ok(found);
    }
    if let Some(found) = records
        .iter()
        .find(|r| key(r).1.eq_ignore_ascii_case(wanted))
    {
        return Ok(found);
    }
    let lower = wanted.to_lowercase();
    let partial: Vec<&T> = records
        .iter()
        .filter(|r| key(r).1.to_lowercase().contains(&lower))
        .collect();
    match partial.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no {what} {wanted:?}")),
        many => {
            let names: Vec<&str> = many.iter().map(|r| key(r).1.as_str()).collect();
            Err(format!(
                "{wanted:?} matches several {what}s: {}; be more specific",
                names.join(", ")
            ))
        }
    }
}
