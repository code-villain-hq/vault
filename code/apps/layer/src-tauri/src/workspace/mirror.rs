//! What the workspace keeps, brought into `config.json` and `notes.json`
//! (PROF-14), so agents, startup and every reader of the config see it
//! without the window.
//!
//! Only the shared half is written; what is this Mac's stays as it is: where
//! a repository's clone is, the Jira email, Slack's switches, a log source's
//! AWS profile. Nothing is taken in while changes made here are still on
//! their way, since the workspace has not seen them yet: the window sends
//! them first, then the snapshot again.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::config::shared::{take_record, Elsewhere};
use crate::config::{AppConfig, AthenaTable, LogSource, ModelChoice, TicketFlow};
use crate::commands::AppState;
use crate::error::Result;
use crate::git::UpdateBy;
use crate::notes::Note;

/// The workspace's records, as the window's sync client holds them. Fields
/// this build does not merge are ignored, so a newer server's are no harm.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    repositories: Vec<RepositoryRecord>,
    connections: Vec<ConnectionRecord>,
    log_sources: Vec<LogSourceRecord>,
    agent_models: Vec<AgentModelRecord>,
    notes: Vec<NoteRecord>,
    /// The members, for who added a repository not on this Mac.
    users: Vec<UserRecord>,
}

#[derive(Debug, Deserialize)]
struct UserRecord {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct RepositoryRecord {
    id: String,
    remote_key: String,
    name: String,
    default_branch: String,
    #[serde(default)]
    group_name: Option<String>,
    #[serde(default)]
    update_by: Option<UpdateBy>,
    #[serde(default)]
    check_command: Option<String>,
    #[serde(default)]
    spec_folder: Option<String>,
    #[serde(default)]
    specs_in_app: bool,
    #[serde(default)]
    added_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConnectionRecord {
    id: String,
    kind: String,
    #[serde(default)]
    settings: Value,
}

#[derive(Debug, Deserialize)]
struct LogSourceRecord {
    id: String,
    name: String,
    #[serde(default)]
    region: Option<String>,
    bucket: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    athena: Option<AthenaTable>,
}

#[derive(Debug, Deserialize)]
struct AgentModelRecord {
    id: String,
    agent_id: String,
    models: Vec<ModelChoice>,
}

#[derive(Debug, Deserialize)]
struct NoteRecord {
    id: String,
    repo: String,
    text: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    source: String,
    #[serde(default)]
    written_commit: Option<String>,
    written_at: DateTime<Utc>,
    checked_at: DateTime<Utc>,
    #[serde(default)]
    checked_commit: Option<String>,
}

/// Settings of every kind, read leniently: what a kind has not is None.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Settings {
    base_url: Option<String>,
    project_key: Option<String>,
    jql: Option<String>,
    epic_field: Option<String>,
    flow: Option<std::collections::HashMap<String, TicketFlow>>,
    api_url: Option<String>,
    web_url: Option<String>,
    review_team: Option<String>,
    channel: Option<String>,
    datasource: Option<String>,
}

/// Bring `snapshot` in. `Ok(None)` when changes made here are still on
/// their way; otherwise whether the setup and whether the notes changed.
pub fn apply(state: &AppState, snapshot: Snapshot) -> Result<Option<(bool, bool)>> {
    let Snapshot { repositories, connections, log_sources, agent_models, notes, users } = snapshot;
    let mut waiting = false;
    let setup = state.config.mirror(|c| {
        if c.shared.workspace.is_none() || !c.shared.outbox.is_empty() {
            waiting = true;
            return false;
        }
        let before = shared_view(c);
        merge(c, repositories, &users, connections, log_sources, agent_models);
        shared_view(c) != before
    })?;
    if waiting {
        return Ok(None);
    }
    let notes = notes.into_iter().map(note).collect();
    match state.notes.replace(notes)? {
        Some(changed) => Ok(Some((setup, changed))),
        None => Ok(None),
    }
}

fn merge(
    c: &mut AppConfig,
    repositories: Vec<RepositoryRecord>,
    users: &[UserRecord],
    connections: Vec<ConnectionRecord>,
    log_sources: Vec<LogSourceRecord>,
    agent_models: Vec<AgentModelRecord>,
) {
    // Every clone of a repository takes its record. One with no clone here
    // is not a project, since a clone is what a project is: it waits in
    // `elsewhere` for one (REPO-9).
    c.shared.elsewhere.clear();
    for r in repositories {
        let added_by = r.added_by.as_ref().and_then(|id| users.iter().find(|u| &u.id == id)).map(|u| u.name.clone());
        let record = Elsewhere {
            id: r.id,
            remote_key: r.remote_key,
            name: r.name,
            default_branch: r.default_branch,
            group: r.group_name,
            update_by: r.update_by,
            check: r.check_command,
            spec_folder: r.spec_folder,
            specs_in_app: r.specs_in_app,
            added_by,
        };
        let mut here = false;
        for p in c.projects.iter_mut().filter(|p| {
            p.repository_id.as_deref() == Some(record.id.as_str())
                || (p.repository_id.is_none() && p.remote.as_deref() == Some(record.remote_key.as_str()))
        }) {
            take_record(p, record.clone());
            here = true;
        }
        if !here {
            c.shared.elsewhere.push(record);
        }
    }

    // A connection needs this Mac's half (a token, the Jira email) before it
    // is one, so only those set up here take the workspace's settings.
    for record in connections {
        let s: Settings = serde_json::from_value(record.settings).unwrap_or_default();
        let here = match record.kind.as_str() {
            "jira" => c.jira.as_mut().map(|j| {
                j.base_url = s.base_url.unwrap_or_else(|| j.base_url.clone());
                j.project_key = s.project_key;
                j.jql = s.jql;
                j.epic_field = s.epic_field;
                j.flow = s.flow.unwrap_or_default();
            }),
            "github" => c.github.as_mut().map(|g| {
                g.api_url = s.api_url.unwrap_or_else(|| g.api_url.clone());
                g.web_url = s.web_url.unwrap_or_else(|| g.web_url.clone());
                g.review_team = s.review_team;
            }),
            "slack" => c.slack.as_mut().map(|sl| sl.channel = s.channel.unwrap_or_else(|| sl.channel.clone())),
            "grafana" => c.grafana.as_mut().map(|g| {
                g.base_url = s.base_url.unwrap_or_else(|| g.base_url.clone());
                g.datasource = s.datasource;
            }),
            _ => None,
        };
        if here.is_some() {
            c.shared.connections.insert(record.kind, record.id);
        }
    }

    // The workspace's log sources, each keeping the AWS profile it has here.
    let profiles: BTreeMap<String, String> = c.log_sources.iter().map(|s| (s.id.clone(), s.profile.clone())).collect();
    c.log_sources = log_sources
        .into_iter()
        .map(|s| LogSource {
            profile: profiles.get(&s.id).cloned().unwrap_or_default(),
            id: s.id,
            name: s.name,
            region: s.region,
            bucket: s.bucket,
            prefix: s.prefix,
            athena: s.athena,
        })
        .collect();

    c.agent_models.clear();
    c.shared.agent_models.clear();
    for m in agent_models {
        c.shared.agent_models.insert(m.agent_id.clone(), m.id);
        c.agent_models.insert(m.agent_id, m.models);
    }
}

/// What the mirror may change, to tell whether it did.
fn shared_view(c: &AppConfig) -> Value {
    serde_json::json!({
        "projects": c.projects,
        "jira": c.jira,
        "github": c.github,
        "slack": c.slack,
        "grafana": c.grafana,
        "log_sources": c.log_sources,
        "agent_models": c.agent_models,
        "shared": c.shared,
    })
}

fn note(n: NoteRecord) -> Note {
    Note {
        id: n.id,
        repo: n.repo,
        text: n.text,
        paths: n.paths,
        written_at: n.written_at.timestamp_millis(),
        source: n.source,
        written_commit: n.written_commit,
        checked_at: n.checked_at.timestamp_millis(),
        checked_commit: n.checked_commit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::moving::tests::{ops, temp_dir, v2_profile};
    use serde_json::json;

    /// A profile just moved in, its creates taken by the window, and what
    /// the workspace holds after them.
    fn moved() -> (AppState, Value) {
        let root = temp_dir();
        let state = v2_profile(&root);
        super::super::moving::move_in(&state, uuid::Uuid::now_v7()).unwrap();
        let sent = ops(&state);
        state.config.mirror(|c| {
            c.shared.outbox.clear();
            true
        })
        .unwrap();
        let mut held = json!({ "repositories": [], "connections": [], "log_sources": [], "agent_models": [], "notes": [], "users": [] });
        for op in sent {
            let mut record = op["data"].clone();
            record["id"] = op["id"].clone();
            let table = match op["model"].as_str() {
                Some("Repository") => "repositories",
                Some("Connection") => "connections",
                Some("LogSource") => "log_sources",
                Some("AgentModel") => "agent_models",
                _ => "notes",
            };
            held[table].as_array_mut().unwrap().push(record);
        }
        (state, held)
    }

    fn snapshot(held: &Value) -> Snapshot {
        serde_json::from_value(held.clone()).unwrap()
    }

    #[test]
    fn what_the_workspace_sent_back_unchanged_changes_nothing_here() {
        let (state, held) = moved();
        assert_eq!(apply(&state, snapshot(&held)).unwrap(), Some((false, false)));
    }

    #[test]
    fn the_workspaces_changes_come_in_and_this_macs_half_stays() {
        let (state, mut held) = moved();
        held["repositories"][0]["name"] = json!("API");
        held["connections"][0]["settings"]["project_key"] = json!("PAY");
        held["log_sources"][0]["name"] = json!("Payments logs");
        held["notes"][0]["text"] = json!("Tests need bun run db, then bun run seed.");

        assert_eq!(apply(&state, snapshot(&held)).unwrap(), Some((true, true)));
        let cfg = state.config.read();
        assert_eq!(cfg.projects[0].name, "API");
        assert!(cfg.projects[0].path.ends_with("/api"), "the clone stays where it is");
        let jira = cfg.jira.as_ref().unwrap();
        assert_eq!((jira.project_key.as_deref(), jira.email.as_str()), (Some("PAY"), "me@acme.example"));
        assert_eq!((cfg.log_sources[0].name.as_str(), cfg.log_sources[0].profile.as_str()), ("Payments logs", "acme-prod"));
        assert!(cfg.shared.outbox.is_empty(), "what came from the workspace is not sent back");

        let notes = state.notes.of("github.com/acme/api");
        assert_eq!(notes[0].text, "Tests need bun run db, then bun run seed.");
        let scratch = cfg.projects[1].path.clone();
        assert_eq!(state.notes.of(&scratch).len(), 1, "a note kept by a clone's path is this Mac's");
    }

    #[test]
    fn a_repository_with_no_clone_here_waits_for_one_and_its_clone_sends_nothing() {
        let (state, mut held) = moved();
        let ada = uuid::Uuid::now_v7().to_string();
        held["users"] = json!([{ "id": ada, "name": "Ada" }]);
        held["repositories"].as_array_mut().unwrap().push(json!({
            "id": uuid::Uuid::now_v7().to_string(),
            "remote_key": "github.com/acme/billing",
            "name": "billing",
            "default_branch": "trunk",
            "check_command": "make test",
            "added_by": ada,
        }));

        assert_eq!(apply(&state, snapshot(&held)).unwrap(), Some((true, false)));
        let cfg = state.config.read();
        assert_eq!(cfg.projects.len(), 2, "no project without a clone");
        let billing = &cfg.shared.elsewhere[0];
        assert_eq!((billing.name.as_str(), billing.added_by.as_deref()), ("billing", Some("Ada")));
        drop(cfg);

        let mut clone = state.config.read().projects[0].clone();
        clone.id = uuid::Uuid::new_v4().to_string();
        clone.path = "/elsewhere/billing".into();
        clone.name = "billing-clone".into();
        clone.remote = Some("github.com/acme/billing".into());
        clone.repository_id = None;
        state.config.update(|c| c.projects.push(clone)).unwrap();
        let cfg = state.config.read();
        assert!(cfg.shared.outbox.is_empty(), "the workspace has it already");
        assert!(cfg.shared.elsewhere.is_empty());
        let billing = cfg.projects.iter().find(|p| p.path == "/elsewhere/billing").unwrap();
        assert_eq!((billing.name.as_str(), billing.check.as_deref()), ("billing", Some("make test")));
    }

    #[test]
    fn nothing_comes_in_while_changes_made_here_are_on_their_way() {
        let (state, mut held) = moved();
        state.config.update(|c| c.projects[0].check = Some("make test".into())).unwrap();
        held["repositories"][0]["check_command"] = Value::Null;

        assert_eq!(apply(&state, snapshot(&held)).unwrap(), None);
        assert_eq!(state.config.read().projects[0].check.as_deref(), Some("make test"));
    }
}
