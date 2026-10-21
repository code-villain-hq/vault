//! The move (PROF-12): the first time a profile's own server starts, the
//! profile becomes a workspace on it, named as the profile is, and its
//! shared setup and notes go in. `config.json` was kept aside as
//! `config.json.v2` when it was read (`ConfigStore::load`); `notes.json` is
//! kept as `notes.json.v2` here.
//!
//! Nothing is written to the workspace from here. The move only records the
//! workspace in `config.json`, and the change that makes queues every record
//! as a create (`config/shared.rs`), which the window's sync client sends.

use std::path::Path;

use layer_server::local;
use sqlx::PgPool;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::commands::AppState;
use crate::config::shared::{Shared, WorkspaceAt, THIS_MAC};

/// The person on this Mac and the workspace this profile is, moving it in
/// the first time.
pub async fn ensure(app: &AppHandle, pool: &PgPool) -> Result<(Uuid, Uuid), String> {
    let (person_name, workspace_name) = tauri::async_runtime::spawn_blocking(names).await.map_err(|e| e.to_string())?;
    let user = match local::person(pool).await.map_err(|e| e.to_string())? {
        Some(user) => user,
        None => local::start(pool, &person_name, &workspace_name).await.map_err(|e| e.to_string())?,
    };
    let theirs = local::workspaces(pool, user).await.map_err(|e| e.to_string())?;

    let state = app.state::<AppState>();
    let recorded = state.config.with(|c| c.shared.workspace.clone());
    if let Some(at) = &recorded {
        if let Some(id) = Uuid::parse_str(&at.id).ok().filter(|id| at.place == THIS_MAC && theirs.contains(id)) {
            return Ok((user, id));
        }
    }

    let workspace = match theirs.first() {
        Some(id) => *id,
        None => local::new_workspace(pool, user, &workspace_name).await.map_err(|e| e.to_string())?,
    };
    let app = app.clone();
    let gone = recorded.is_some();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if gone {
            forget(&state)?;
        }
        move_in(&state, workspace)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    Ok((user, workspace))
}

/// The person, as macOS names them, and the profile's name.
fn names() -> (String, String) {
    let profile = crate::profiles::current();
    let workspace = crate::profiles::read(&profile.base)
        .get(&profile.id)
        .map(|entry| entry.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Default".into());
    (account_name(), workspace)
}

/// The workspace this profile was in is not here any more (its Postgres
/// folder was removed): start again, as before the move.
fn forget(state: &AppState) -> crate::error::Result<()> {
    state.config.mirror(|c| {
        c.shared = Shared::default();
        for p in &mut c.projects {
            p.repository_id = None;
        }
        true
    })?;
    state.notes.unshare()
}

pub(super) fn move_in(state: &AppState, workspace: Uuid) -> crate::error::Result<()> {
    let notes = state.config.folder().join("notes.json");
    keep_aside(&notes, &state.config.folder().join("notes.json.v2"));

    // A repository is shared by where it fetches from; one added before
    // this build has none recorded yet.
    let unread: Vec<(String, String)> = state
        .config
        .with(|c| c.projects.iter().filter(|p| p.remote.is_none()).map(|p| (p.id.clone(), p.path.clone())).collect());
    let remotes: Vec<(String, String)> = unread
        .into_iter()
        .filter_map(|(id, path)| crate::git::origin_url(Path::new(&path)).map(|url| (id, crate::git::remote_key(&url))))
        .collect();

    state.config.update(|c| {
        for (id, remote) in remotes {
            if let Some(p) = c.projects.iter_mut().find(|p| p.id == id) {
                p.remote = Some(remote);
            }
        }
        c.shared.workspace = Some(WorkspaceAt { place: THIS_MAC.into(), id: workspace.to_string(), user_id: None });
    })?;
    state.notes.share()
}

fn keep_aside(file: &Path, kept: &Path) {
    if file.exists() && !kept.exists() {
        let _ = std::fs::copy(file, kept);
    }
}

/// The person's name as macOS has it: the workspace names them by it.
fn account_name() -> String {
    std::process::Command::new("id")
        .arg("-F")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "Me".into())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::config::{AppConfig, ConfigStore};
    use serde_json::{json, Value};
    use std::path::PathBuf;

    pub(in crate::workspace) fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vl-move-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn clone_at(dir: &Path, origin: Option<&str>) {
        std::fs::create_dir_all(dir).unwrap();
        crate::git::run_for_tests(dir, &["init", "-q", "-b", "main"]).unwrap();
        if let Some(url) = origin {
            crate::git::run_for_tests(dir, &["remote", "add", "origin", url]).unwrap();
        }
    }

    /// A profile as 0.10 left it: `config.json` at version 2, `notes.json`
    /// with the short ids it gave notes.
    pub(in crate::workspace) fn v2_profile(root: &Path) -> AppState {
        let api = root.join("api");
        let scratch = root.join("scratch");
        clone_at(&api, Some("git@github.com:acme/api.git"));
        clone_at(&scratch, None);
        let mut cfg = serde_json::to_value(AppConfig::default()).unwrap();
        let fields = json!({
            "version": 2,
            "projects": [
                { "id": "p1", "name": "api", "path": api, "default_branch": "main", "check": "bun run check" },
                { "id": "p2", "name": "scratch", "path": scratch, "default_branch": "main" },
            ],
            "jira": { "base_url": "https://acme.atlassian.net", "email": "me@acme.example", "project_key": "ACME" },
            "log_sources": [{ "id": "src-1", "name": "API logs", "profile": "acme-prod", "bucket": "acme-logs", "prefix": "api/" }],
            "agent_models": { "claude": [{ "value": "opus", "name": "Opus" }] },
        });
        for (k, v) in fields.as_object().unwrap() {
            cfg[k] = v.clone();
        }
        std::fs::write(root.join("config.json"), serde_json::to_vec(&cfg).unwrap()).unwrap();
        let notes = json!({ "version": 1, "notes": [
            { "id": "n1", "repo": "github.com/acme/api", "text": "Tests need bun run db first.", "source": "you", "written_at": 1, "checked_at": 2 },
            { "id": "n2", "repo": scratch, "text": "A scratch repository.", "source": "you", "written_at": 1, "checked_at": 2 },
        ]});
        std::fs::write(root.join("notes.json"), serde_json::to_vec(&notes).unwrap()).unwrap();

        let cfg: AppConfig = serde_json::from_value(cfg).unwrap();
        AppState {
            slack_bot: Default::default(),
            config: ConfigStore::for_tests(root.join("config.json"), cfg),
            ptys: Default::default(),
            jira_types: Default::default(),
            epic_field_missing: Default::default(),
            pending_notices: Default::default(),
            status_cache: Default::default(),
            prs_settled: Default::default(),
            news: Default::default(),
            messages: crate::messages::Messages::for_tests(root.join("messages.json")),
            notes: crate::notes::Notes::load(root),
            browser: Default::default(),
            loops: Default::default(),
            runs: crate::runs::Runs::for_tests(root.join("runs.json")),
        }
    }

    pub(in crate::workspace) fn ops(state: &AppState) -> Vec<Value> {
        let mut ops = state.config.read().shared.outbox;
        ops.extend(state.notes.take_writes().unwrap());
        ops
    }

    #[test]
    fn a_v2_profile_moves_its_shared_setup_in_and_keeps_this_macs_half() {
        let root = temp_dir();
        let state = v2_profile(&root);
        let workspace = Uuid::now_v7();
        super::move_in(&state, workspace).unwrap();

        let cfg = state.config.read();
        assert_eq!(cfg.shared.workspace.as_ref().map(|w| w.id.clone()), Some(workspace.to_string()));
        assert_eq!(cfg.projects[0].remote.as_deref(), Some("github.com/acme/api"));
        assert!(cfg.projects[0].repository_id.is_some());
        assert_eq!(cfg.projects[1].repository_id, None, "a repository with no origin stays on this Mac");
        assert_eq!(cfg.projects[0].path, root.join("api").to_string_lossy());
        assert_eq!(cfg.log_sources[0].profile, "acme-prod");
        assert!(Uuid::parse_str(&cfg.log_sources[0].id).is_ok());

        let ops = ops(&state);
        let models: Vec<&str> = ops.iter().map(|op| op["model"].as_str().unwrap()).collect();
        assert_eq!(models, ["AgentModel", "Connection", "LogSource", "Repository", "Note"]);
        assert!(ops.iter().all(|op| op["action"] == "create"));
        let everything = serde_json::to_string(&ops).unwrap();
        assert!(!everything.contains("me@acme.example"), "the Jira email is this Mac's");
        assert!(!everything.contains("acme-prod"), "the AWS profile is this Mac's");
        assert!(!everything.contains("scratch"), "nor is a repository with no origin, or its notes");
        assert_eq!(ops[1]["data"]["settings"]["project_key"], "ACME");
        assert_eq!(ops[4]["data"]["text"], "Tests need bun run db first.");
        assert!(Uuid::parse_str(ops[4]["id"].as_str().unwrap()).is_ok());

        assert!(root.join("notes.json.v2").exists());
    }

    #[test]
    fn a_profile_whose_workspace_is_gone_moves_into_the_new_one_whole_again() {
        let root = temp_dir();
        let state = v2_profile(&root);
        move_in(&state, Uuid::now_v7()).unwrap();
        let first = ops(&state).len();
        state.config.mirror(|c| { c.shared.outbox.clear(); true }).unwrap();

        forget(&state).unwrap();
        move_in(&state, Uuid::now_v7()).unwrap();
        assert_eq!(ops(&state).len(), first);
    }
}
