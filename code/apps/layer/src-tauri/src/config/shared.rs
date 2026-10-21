//! The half of `config.json` a workspace shares (§25): the repositories, the
//! connections' shared settings, the log sources and the agents' models.
//!
//! The rest of the app reads and writes `config.json` as it always has.
//! What a change sends is worked out after it (PROF-13): the shared setup as
//! the workspace's records before, the same after, and the difference as the
//! sync engine's operations, queued in `outbox` until the window's sync
//! client takes them. So no write site has to know about the workspace.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{AppConfig, Project};
use crate::git::UpdateBy;

pub const REPOSITORY: &str = "Repository";
pub const CONNECTION: &str = "Connection";
pub const LOG_SOURCE: &str = "LogSource";
pub const AGENT_MODEL: &str = "AgentModel";
pub const NOTE: &str = "Note";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Shared {
    /// The workspace that keeps the shared setup. None before the move
    /// (PROF-12), or while it could not be made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceAt>,
    /// The workspace's record of each connection, by kind: `jira`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connections: BTreeMap<String, String>,
    /// The workspace's record of each agent's models, by agent id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_models: BTreeMap<String, String>,
    /// Changes the window's sync client has not taken yet, oldest first:
    /// kept here so none is lost to a quit before the window loaded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbox: Vec<Value>,
    /// The workspace's repositories with no clone on this Mac (REPO-9), as
    /// the workspace keeps them. A clone added later takes one's record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elsewhere: Vec<Elsewhere>,
}

/// A repository of the workspace's that is not on this Mac.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Elsewhere {
    pub id: String,
    pub remote_key: String,
    pub name: String,
    pub default_branch: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub update_by: Option<UpdateBy>,
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub spec_folder: Option<String>,
    #[serde(default)]
    pub specs_in_app: bool,
    /// Who added it, by name, when the workspace says.
    #[serde(default)]
    pub added_by: Option<String>,
}

/// Where the workspace is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceAt {
    /// `this_mac`: the app's own server, on the workspace's own Postgres.
    /// Otherwise a Layer server's origin (PROF-15).
    pub place: String,
    pub id: String,
    /// Who signed in to the server, so the window's offline copy is theirs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

pub const THIS_MAC: &str = "this_mac";

/// The shared setup as records: by model and id, what creating each sends.
pub type Records = BTreeMap<(&'static str, String), Map<String, Value>>;

/// What a change sent, for `workspace::wrote`.
#[derive(Debug, Default)]
pub struct Sent {
    pub queued: usize,
    /// Changes the workspace would refuse, in words: kept here, not sent.
    pub refused: Vec<String>,
}

/// The shared setup of `cfg`: nothing until it is in a workspace.
pub fn records(cfg: &AppConfig) -> Records {
    let mut out = Records::new();
    if cfg.shared.workspace.is_none() {
        return out;
    }
    for p in &cfg.projects {
        let (Some(id), Some(remote)) = (&p.repository_id, &p.remote) else { continue };
        // Two clones of one repository are one record: the first speaks for it.
        out.entry((REPOSITORY, id.clone())).or_insert_with(|| {
            object(json!({
                "remote_key": remote,
                "name": p.name,
                "default_branch": p.default_branch,
                "group_name": p.group,
                "update_by": p.update_by,
                "check_command": p.check,
                "spec_folder": p.spec_folder,
                "specs_in_app": p.specs_in_app,
            }))
        });
    }
    // Not on this Mac, but the workspace's all the same: kept as it came.
    for e in &cfg.shared.elsewhere {
        out.entry((REPOSITORY, e.id.clone())).or_insert_with(|| {
            object(json!({
                "remote_key": e.remote_key,
                "name": e.name,
                "default_branch": e.default_branch,
                "group_name": e.group,
                "update_by": e.update_by,
                "check_command": e.check,
                "spec_folder": e.spec_folder,
                "specs_in_app": e.specs_in_app,
            }))
        });
    }
    for (kind, settings) in connection_settings(cfg) {
        if let Some(id) = cfg.shared.connections.get(kind) {
            out.insert((CONNECTION, id.clone()), object(json!({ "kind": kind, "settings": settings })));
        }
    }
    for s in &cfg.log_sources {
        out.insert(
            (LOG_SOURCE, s.id.clone()),
            object(json!({
                "name": s.name,
                "region": s.region,
                "bucket": s.bucket,
                "prefix": s.prefix,
                "athena": s.athena,
            })),
        );
    }
    for (agent, models) in &cfg.agent_models {
        if let Some(id) = cfg.shared.agent_models.get(agent) {
            out.insert((AGENT_MODEL, id.clone()), object(json!({ "agent_id": agent, "models": models })));
        }
    }
    out
}

/// Each connection's settings that everyone shares, by kind. Not the Jira
/// email, Slack's switches or any token: those are one person's own.
fn connection_settings(cfg: &AppConfig) -> Vec<(&'static str, Value)> {
    let mut out = Vec::new();
    if let Some(j) = &cfg.jira {
        out.push((
            "jira",
            json!({
                "base_url": j.base_url,
                "project_key": j.project_key,
                "jql": j.jql,
                "epic_field": j.epic_field,
                "flow": j.flow,
            }),
        ));
    }
    if let Some(g) = &cfg.github {
        out.push(("github", json!({ "api_url": g.api_url, "web_url": g.web_url, "review_team": g.review_team })));
    }
    if let Some(s) = &cfg.slack {
        out.push(("slack", json!({ "channel": s.channel })));
    }
    if let Some(g) = &cfg.grafana {
        out.push(("grafana", json!({ "base_url": g.base_url, "datasource": g.datasource })));
    }
    out
}

/// After a change: give what is new its record's id, and queue what changed.
pub fn record_changes(cfg: &mut AppConfig, before: &Records) -> Sent {
    if cfg.shared.workspace.is_none() {
        return Sent::default();
    }
    stamp(cfg);
    // A record refused keeps its id, so it is not offered again with every
    // change after this one; only a change to it is.
    let (ops, refused) = diff(before, &records(cfg));
    let sent = Sent { queued: ops.len(), refused };
    cfg.shared.outbox.extend(ops);
    sent
}

/// Ids for what has none yet: a repository by its remote (a second clone
/// takes the first's, a first clone the workspace's record), a connection
/// and an agent's models by their kind.
fn stamp(cfg: &mut AppConfig) {
    for i in 0..cfg.projects.len() {
        let p = &cfg.projects[i];
        let Some(remote) = p.remote.clone() else { continue };
        if p.repository_id.is_some() {
            continue;
        }
        if let Some(at) = cfg.shared.elsewhere.iter().position(|e| e.remote_key == remote) {
            let e = cfg.shared.elsewhere.remove(at);
            take_record(&mut cfg.projects[i], e);
            continue;
        }
        let sibling = cfg
            .projects
            .iter()
            .find(|q| q.remote.as_deref() == Some(remote.as_str()) && q.repository_id.is_some())
            .and_then(|q| q.repository_id.clone());
        cfg.projects[i].repository_id = Some(sibling.unwrap_or_else(new_id));
    }
    let kinds: Vec<&str> = connection_settings(cfg).into_iter().map(|(kind, _)| kind).collect();
    cfg.shared.connections.retain(|kind, _| kinds.contains(&kind.as_str()));
    for kind in kinds {
        cfg.shared.connections.entry(kind.to_string()).or_insert_with(new_id);
    }
    // Records are named by uuids; a source saved by hand may have another id.
    for s in cfg.log_sources.iter_mut().filter(|s| uuid::Uuid::parse_str(&s.id).is_err()) {
        s.id = new_id();
    }
    let agents: Vec<String> = cfg.agent_models.keys().cloned().collect();
    cfg.shared.agent_models.retain(|agent, _| agents.contains(agent));
    for agent in agents {
        cfg.shared.agent_models.entry(agent).or_insert_with(new_id);
    }
}

/// A clone of a repository the workspace has: its settings are the
/// workspace's, so taking it here changes nothing there (REPO-9).
pub fn take_record(p: &mut Project, e: Elsewhere) {
    p.repository_id = Some(e.id);
    p.name = e.name;
    p.default_branch = e.default_branch;
    p.group = e.group;
    p.update_by = e.update_by;
    p.check = e.check;
    p.spec_folder = e.spec_folder;
    p.specs_in_app = e.specs_in_app;
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// The operations that take `before` to `after`, each checked as the
/// workspace will check it.
pub fn diff(before: &Records, after: &Records) -> (Vec<Value>, Vec<String>) {
    let mut ops = Vec::new();
    let mut refused = Vec::new();
    for (key @ (model, id), whole) in after {
        let Some(kind) = layer_domain::ModelKind::from_name(model) else { continue };
        let (action, data) = match before.get(key) {
            None => ("create", whole.clone()),
            Some(old) => {
                let patch: Map<String, Value> = patch_keys(model)
                    .iter()
                    .filter(|k| old.get(**k) != whole.get(**k))
                    .map(|k| (k.to_string(), whole.get(*k).cloned().unwrap_or(Value::Null)))
                    .collect();
                if patch.is_empty() {
                    continue;
                }
                ("update", patch)
            }
        };
        let checked = if action == "create" { kind.validate_create(&data) } else { kind.validate_patch(&data) };
        match checked {
            Ok(()) => ops.push(json!({ "action": action, "model": model, "id": id, "data": data })),
            Err(e) => refused.push(format!("{} {}: {}", describe(model), name_of(whole), e.0)),
        }
    }
    for (model, id) in before.keys() {
        // One never sent, under an id that was not yet a uuid (`stamp`).
        if uuid::Uuid::parse_str(id).is_err() {
            continue;
        }
        if !after.contains_key(&(*model, id.clone())) {
            ops.push(json!({ "action": "delete", "model": model, "id": id }));
        }
    }
    (ops, refused)
}

/// What names a record to the user.
fn name_of(data: &Map<String, Value>) -> String {
    ["name", "kind", "agent_id", "text"]
        .iter()
        .find_map(|k| data.get(*k).and_then(Value::as_str))
        .map(|s| s.chars().take(60).collect())
        .unwrap_or_default()
}

fn describe(model: &str) -> &'static str {
    match model {
        REPOSITORY => "The repository",
        CONNECTION => "The connection",
        LOG_SOURCE => "The log source",
        AGENT_MODEL => "The models of",
        _ => "The note",
    }
}

/// What an update may change: the rest is fixed when a record is made.
fn patch_keys(model: &str) -> &'static [&'static str] {
    match model {
        REPOSITORY => &["name", "default_branch", "group_name", "update_by", "check_command", "spec_folder", "specs_in_app"],
        CONNECTION => &["settings"],
        LOG_SOURCE => &["name", "region", "bucket", "prefix", "athena"],
        AGENT_MODEL => &["models"],
        NOTE => &["text", "paths", "checked_at", "checked_commit"],
        _ => &[],
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{JiraConfig, Project};

    fn project(id: &str, path: &str, remote: Option<&str>) -> Project {
        Project {
            id: id.into(),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            path: path.into(),
            default_branch: "main".into(),
            group: None,
            store: None,
            update_by: None,
            check: None,
            spec_folder: None,
            specs_in_app: false,
            remote: remote.map(String::from),
            repository_id: None,
        }
    }

    fn in_a_workspace(projects: Vec<Project>) -> AppConfig {
        let mut cfg = AppConfig { projects, ..Default::default() };
        cfg.shared.workspace = Some(WorkspaceAt { place: THIS_MAC.into(), id: new_id(), user_id: None });
        cfg
    }

    fn change(cfg: &mut AppConfig, f: impl FnOnce(&mut AppConfig)) -> Sent {
        let before = records(cfg);
        f(cfg);
        record_changes(cfg, &before)
    }

    #[test]
    fn nothing_is_queued_before_the_profile_is_a_workspace() {
        let mut cfg = AppConfig::default();
        let sent = change(&mut cfg, |c| c.projects.push(project("p1", "/code/api", Some("github.com/acme/api"))));
        assert_eq!(sent.queued, 0);
        assert!(cfg.shared.outbox.is_empty());
        assert_eq!(cfg.projects[0].repository_id, None);
    }

    #[test]
    fn two_clones_of_a_repository_are_one_record_and_one_without_origin_stays_here() {
        let mut cfg = in_a_workspace(vec![]);
        change(&mut cfg, |c| {
            c.projects.push(project("p1", "/code/api", Some("github.com/acme/api")));
            c.projects.push(project("p2", "/elsewhere/api", Some("github.com/acme/api")));
            c.projects.push(project("p3", "/code/scratch", None));
        });
        assert_eq!(cfg.projects[0].repository_id, cfg.projects[1].repository_id);
        assert_eq!(cfg.projects[2].repository_id, None);
        let creates: Vec<&Value> = cfg.shared.outbox.iter().filter(|op| op["action"] == "create").collect();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0]["model"], REPOSITORY);
        assert_eq!(creates[0]["data"]["remote_key"], "github.com/acme/api");
        assert!(creates[0]["data"].get("path").is_none(), "where the clone is stays on this Mac");
    }

    #[test]
    fn an_update_sends_only_what_changed_and_a_removal_a_delete() {
        let mut cfg = in_a_workspace(vec![]);
        change(&mut cfg, |c| c.projects.push(project("p1", "/code/api", Some("github.com/acme/api"))));
        cfg.shared.outbox.clear();

        change(&mut cfg, |c| c.projects[0].check = Some("bun run check".into()));
        assert_eq!(cfg.shared.outbox.len(), 1);
        assert_eq!(cfg.shared.outbox[0]["action"], "update");
        assert_eq!(cfg.shared.outbox[0]["data"], json!({ "check_command": "bun run check" }));

        cfg.shared.outbox.clear();
        let id = cfg.projects[0].repository_id.clone();
        change(&mut cfg, |c| c.projects.clear());
        assert_eq!(cfg.shared.outbox, vec![json!({ "action": "delete", "model": REPOSITORY, "id": id })]);
    }

    #[test]
    fn a_connection_shares_its_settings_but_never_the_jira_email() {
        let mut cfg = in_a_workspace(vec![]);
        change(&mut cfg, |c| {
            c.jira = Some(JiraConfig {
                base_url: "https://acme.atlassian.net".into(),
                email: "me@acme.example".into(),
                project_key: Some("ACME".into()),
                ..Default::default()
            })
        });
        let op = &cfg.shared.outbox[0];
        assert_eq!(op["model"], CONNECTION);
        assert_eq!(op["data"]["kind"], "jira");
        assert_eq!(op["data"]["settings"]["project_key"], "ACME");
        assert!(!op.to_string().contains("me@acme.example"));
    }

    #[test]
    fn a_record_the_workspace_would_refuse_is_said_and_not_offered_again() {
        let mut cfg = in_a_workspace(vec![]);
        let mut nameless = project("p1", "/code/api", Some("github.com/acme/api"));
        nameless.name = " ".into();
        let sent = change(&mut cfg, |c| c.projects.push(nameless));
        assert_eq!(sent.queued, 0);
        assert_eq!(sent.refused.len(), 1);
        assert!(sent.refused[0].starts_with("The repository"));

        let again = change(&mut cfg, |c| c.ui.theme = "day".into());
        assert_eq!((again.queued, again.refused.len()), (0, 0));
    }
}
