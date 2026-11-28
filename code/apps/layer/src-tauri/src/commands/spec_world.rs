//! The fixture Commit spec's and the spec folder's tests share: a task of
//! repositories in a temp dir that is also the config folder. Each item says
//! `#[cfg(test)]` itself, which is what the guard reads to know a file's
//! unwraps are test code.

#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::commands::AppState;
#[cfg(test)]
use crate::config::{AppConfig, Checkout, ConfigStore, Project, Task};

#[cfg(test)]
/// A task `T-1` of repositories `folders`, in a temp dir that is also the
/// config folder.
pub(crate) fn world(folders: &[&str]) -> (PathBuf, AppState, Task) {
    let root = std::env::temp_dir().join(format!("vl-spec-copy-{}", uuid::Uuid::new_v4()));
    let task_root = root.join("tasks/T-1");
    let mut cfg = AppConfig::default();
    for folder in folders {
        let worktree = task_root.join(folder);
        std::fs::create_dir_all(&worktree).unwrap();
        cfg.projects.push(Project {
            id: format!("p-{folder}"),
            name: (*folder).into(),
            path: String::new(),
            default_branch: "main".into(),
            group: None,
            store: None,
            update_by: None,
            check: None,
            check_approved: None,
            spec_folder: None,
            spec_toolset: None,
            spec_commit: Default::default(),
            remote: None,
            repository_id: None,
        });
        cfg.checkouts.push(Checkout {
            id: format!("c-{folder}"),
            task_id: "t1".into(),
            project_id: format!("p-{folder}"),
            path: worktree.to_string_lossy().to_string(),
            base: "main".into(),
            base_commit: None,
            push_lease: None,
            point_before_update: None,
            last_head: None,
        });
    }
    std::fs::create_dir_all(&task_root).unwrap();
    let task = Task {
        toolset: None,
        id: "t1".into(),
        name: "T-1 Refunds".into(),
        root: task_root.to_string_lossy().to_string(),
        branch: "T-1".into(),
        issue_key: None,
        issue_url: None,
        created_at: chrono::Utc::now(),
        ticket_stage: None,
        chat: None,
        review: None,
        browser: None,
        browser_url: None,
        epic: false,
        sub_task: None,
    };
    cfg.tasks.push(task.clone());
    let state = AppState {
        slack_bot: Default::default(),
        config: ConfigStore::for_tests(root.join("config.json"), cfg),
        ptys: crate::pty::PtyManager::default(),
        ticket_types: Default::default(),
        epic_field_missing: Default::default(),
        pending_notices: Default::default(),
        status_cache: Default::default(),
        github_session: Default::default(),
        prs_seen: Default::default(),
        news: Default::default(),
        messages: crate::messages::Messages::for_tests(root.join("messages.json")),
        notes: crate::notes::Notes::load(&root),
        browser: Default::default(),
        loops: Default::default(),
        runs: crate::runs::Runs::for_tests(root.join("runs.json")),
    };
    (root, state, task)
}

#[cfg(test)]
pub(crate) fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[cfg(test)]
pub(crate) fn get(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
