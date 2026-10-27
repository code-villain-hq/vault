//! Persisted, non-secret application state.
//!
//! A **task** is one unit of work — usually one Jira ticket. It owns one
//! **checkout** per repository it touches, all on the same branch name, laid
//! out as sibling directories under the task root so an agent started there
//! can see every repo at once. Tokens live in `secrets`, never here.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

mod integrations;
mod browser;
mod phone;
mod project;
pub mod shared;
pub use integrations::{
    AthenaTable, FlowStatus, GithubConfig, GrafanaConfig, JiraConfig, LogSource, ModelChoice, SlackBotConfig, SlackConfig, SlackThread,
    StartTo, TicketFlow,
};
pub use browser::{BrowserConfig, SavedSignIn, SavedTabs};
pub use phone::{PhoneConfig, PhoneDevice};
pub use project::Project;
pub use shared::Shared;

/// 3: the shared setup is kept in a workspace (§25), `shared` says which.
pub const SCHEMA_VERSION: u32 = 3;

/// One ticket's worth of work, spanning one or more repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    /// Directory the checkouts live under, and the cwd for a task-wide agent.
    pub root: String,
    /// Shared across every repo, so sibling PRs are findable by branch alone.
    pub branch: String,
    #[serde(default)]
    pub issue_key: Option<String>,
    #[serde(default)]
    pub issue_url: Option<String>,
    pub created_at: DateTime<Utc>,
    /// The last stage its ticket was moved for, "review" or "merged" (TKT-8),
    /// so each is done once and a ticket moved by hand afterwards stays put.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_stage: Option<String>,
    /// The room folder of the chat whose agent created this task (CHAT-3).
    /// That agent works on it from the chat, where the task cannot see it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    /// Someone else's pull request, checked out to review it by 0.5.0,
    /// which could review pull requests here. Nothing makes one now, but
    /// the checkouts made then stay until deleted: such a task is never
    /// pushed, opens no pull request and moves no ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewOf>,
    /// Its browser tabs' pages and which was active (BRW-6), opened again
    /// the next time its browser is needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<SavedTabs>,
    /// The one page its browser had before tabs (BRW-6): read, as one tab,
    /// and never written again.
    #[serde(default, skip_serializing)]
    pub browser_url: Option<String>,
    /// An epic (EPIC-1): its worktrees hold its spec, and its work is done
    /// in its sub-tasks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub epic: bool,
    /// The epic this is a sub-task of (EPIC-7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_task: Option<SubTaskOf>,
}

/// Which epic a sub-task was filed from, and which of its sub-tasks it is:
/// its title, as the epic's tasks name it (EPIC-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubTaskOf {
    pub epic: String,
    pub title: String,
}

impl Task {
    /// Its tabs as kept, whether written before tabs or since.
    pub fn saved_tabs(&self) -> SavedTabs {
        match (&self.browser, &self.browser_url) {
            (Some(tabs), _) => tabs.clone(),
            (None, Some(url)) => SavedTabs { urls: vec![url.clone()], active: 0 },
            (None, None) => SavedTabs::default(),
        }
    }
}

/// The pull request a review task left from 0.5.0 is of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewOf {
    /// `owner/name` of the repository it merges into.
    pub repo: String,
    pub number: u64,
    pub url: String,
    #[serde(default)]
    pub author: String,
    /// The commit its worktree was last moved to: what the review is of,
    /// and what a line comment posted to GitHub is pinned to.
    pub head_sha: String,
}

/// One repository's worktree within a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkout {
    pub id: String,
    pub task_id: String,
    pub project_id: String,
    pub path: String,
    /// The branch this worktree was cut from, usually the repo's default.
    pub base: String,
    /// The commit this worktree was branched from.
    ///
    /// The diff is "what this branch did", and that only has a fixed meaning
    /// against a fixed point. Comparing to the base *branch* moves as the base
    /// moves, and credits this branch with everything its history picked up
    /// from anywhere else.
    #[serde(default)]
    pub base_commit: Option<String>,
    /// What the remote branch was when this worktree last rebased, until the
    /// next push. A rebased branch can only be pushed by replacing the remote
    /// one, and this is the only commit it may replace: anything else there
    /// was pushed by someone else since.
    #[serde(default)]
    pub push_lease: Option<String>,
    /// `base_commit` as it was before an update that stopped on conflicts,
    /// for abandoning it. The target is recorded as the point straight away;
    /// abandoned, the branch never reaches it, and without this the diff fell
    /// back to the merge base and counted the base's merged-in work as the
    /// branch's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_before_update: Option<String>,
    /// The commit the worktree was last seen on. What a folder cut off from
    /// its repository is linked back at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_head: Option<String>,
}

fn yes() -> bool {
    true
}

/// The v1 shape: one workspace was one worktree in one repo. Migrated on load.
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyWorkspace {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub base: String,
    #[serde(default)]
    pub issue_key: Option<String>,
    #[serde(default)]
    pub issue_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A pane that was open when the app last closed, so it can be put back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPane {
    pub id: String,
    pub task_id: String,
    #[serde(default)]
    pub checkout_id: Option<String>,
    /// "agent" or "shell".
    pub kind: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Where it was running. Task panes derive this from their checkout, but a
    /// chat belongs to no task and its folder is the only thing that ties it
    /// back to its saved conversation.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Launches this has failed to come back on. A pane that cannot return —
    /// its repo removed from the task, its CLI uninstalled — was otherwise
    /// tried at every launch for good, ahead of panes that really were open.
    #[serde(default)]
    pub failed: u8,
    /// Over the limit at a launch, so not put back (PANE-7): kept and listed
    /// for the user to reopen or forget, and never reopened by a launch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub waiting: bool,
    /// Run over ACP (§20), and so put back that way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub acp: bool,
    /// The conversation an ACP agent was in, by the id it gave: what a
    /// restore picks up (ACP-9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The loop it was on, to put back on it (LOOP-11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_loop: Option<SavedLoop>,
}

/// A loop as kept with its pane between launches (LOOP-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedLoop {
    pub rounds: u32,
    pub round: u32,
    /// What the worktrees were when its checks last failed (LOOP-4).
    #[serde(default)]
    pub failed_at: Option<String>,
    /// Waiting on you, after a turn that changed nothing.
    #[serde(default)]
    pub held: bool,
}

/// Presentation preferences. Terminal text scales separately from the chrome,
/// because xterm measures its own cell grid and a page zoom would fight it.
///
/// Every field defaults, the struct as a whole: one missing field without
/// its own default made the whole config unreadable, and the app started
/// with no tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub scale: f32,
    /// The app's colours (SET-5): a palette `THEMES` names, or "system" for
    /// the Mac's own appearance, followed as it changes. Nightshade's night
    /// by default, the studio's theme. "dark" and "light", the names before
    /// Nightshade, read as its night and day.
    #[serde(default = "crate::theme::night", deserialize_with = "crate::theme::saved_name")]
    pub theme: String,
    pub terminal_font_size: u16,
    /// The text of an agent's conversation over ACP (ACP-15), which is the
    /// app's own page and not a terminal, so neither of the others.
    pub conversation_font_size: u16,
    /// Put back the panes that were open when the app last closed, resuming
    /// each agent's conversation where its CLI can.
    #[serde(default = "yes")]
    pub restore_panes: bool,
    /// Answer Claude Code's workspace-trust dialog for folders this app made,
    /// so a new task or chat starts working instead of waiting on a question.
    #[serde(default = "yes")]
    pub trust_agent_dirs: bool,
    /// Move the ticket into progress when a task is started for it, so the
    /// board and this app do not disagree about what is being worked on.
    #[serde(default = "yes")]
    pub sync_jira_status: bool,
    /// A banner when a review is requested or a ticket is assigned, and only
    /// while the window is in the background. On by default: the first sweep
    /// after launch is silent, so turning the app on does not announce the
    /// queue that was already waiting.
    #[serde(default = "yes")]
    pub system_notifications: bool,
    /// A banner when an agent stops to wait on you — gone quiet, asking to
    /// trust its folder, out of budget, or exited — while the window is in the
    /// background, and the count of those waiting on the dock icon.
    #[serde(default = "yes")]
    pub notify_waiting_agents: bool,
    /// Let agents read the output of terminals in this app.
    ///
    /// Off by default, and deliberately: a shell's scrollback holds whatever
    /// has been typed and whatever printed, which can include tokens echoed by
    /// a failing script or a remote URL with credentials in it.
    #[serde(default)]
    pub agents_read_panes: bool,
    /// Keep the Mac from idle sleep while an agent is working (STATE-7).
    /// Off by default: on battery, an agent left working overnight would
    /// keep the machine up until it ran flat.
    #[serde(default)]
    pub keep_awake: bool,
}


impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            scale: 1.0,
            theme: crate::theme::night(),
            terminal_font_size: 13,
            conversation_font_size: 14,
            restore_panes: true,
            trust_agent_dirs: true,
            sync_jira_status: true,
            system_notifications: true,
            notify_waiting_agents: true,
            agents_read_panes: false,
            keep_awake: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub checkouts: Vec<Checkout>,
    /// Read-only remnant of v1; drained by `migrate`.
    #[serde(default, skip_serializing)]
    pub workspaces: Vec<LegacyWorkspace>,
    /// Where task directories are created. Defaults to ~/.villain-worktrees.
    #[serde(default)]
    pub worktree_root: Option<String>,
    /// Repos last used, keyed "epic:ACME-12" / "project:ACME". Prefills the picker.
    #[serde(default)]
    pub last_repos: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub ui: UiPrefs,
    /// Messages the app posted to Slack, so it can retract its own litter.
    /// Only a bot can delete a bot's messages, so nobody else can clear these.
    #[serde(default)]
    pub slack_posted: Vec<crate::integrations::slack::Posted>,
    #[serde(default)]
    pub saved_panes: Vec<SavedPane>,
    #[serde(default)]
    pub jira: Option<JiraConfig>,
    #[serde(default)]
    pub github: Option<GithubConfig>,
    #[serde(default)]
    pub slack: Option<SlackConfig>,
    #[serde(default)]
    pub grafana: Option<GrafanaConfig>,
    /// Logs in S3 agents read (OBS-9).
    #[serde(default)]
    pub log_sources: Vec<LogSource>,
    #[serde(default)]
    pub slack_bot: Option<SlackBotConfig>,
    /// The models each ACP agent offered, by agent id, as its last
    /// conversation opened: the choices for the bot's model (BOT-17).
    #[serde(default)]
    pub agent_models: HashMap<String, Vec<ModelChoice>>,
    /// Slack threads handed to chats (BOT-4), newest last.
    #[serde(default)]
    pub slack_threads: Vec<SlackThread>,
    #[serde(default)]
    pub phone: PhoneConfig,
    #[serde(default)]
    pub browser: BrowserConfig,
    /// The workspace that keeps the shared setup, and what is on its way
    /// there (§25).
    #[serde(default)]
    pub shared: Shared,
}

impl AppConfig {
    /// Fold v1 workspaces into single-checkout tasks, keeping their ids so
    /// selection and any running panes survive the upgrade.
    fn migrate(&mut self) -> bool {
        // Written by a newer build. Nothing here knows how to read it back
        // down; the version is left as it is, so going back to that build
        // does not take this one's word for what the file holds.
        if self.version > SCHEMA_VERSION {
            return false;
        }
        if self.workspaces.is_empty() {
            let bumped = self.version != SCHEMA_VERSION;
            self.version = SCHEMA_VERSION;
            return bumped;
        }

        for w in std::mem::take(&mut self.workspaces) {
            if self.tasks.iter().any(|t| t.id == w.id) {
                continue;
            }
            self.tasks.push(Task {
                id: w.id.clone(),
                name: w.name,
                // The old layout had no task directory; the lone checkout is it.
                root: w.path.clone(),
                branch: w.branch,
                issue_key: w.issue_key,
                issue_url: w.issue_url,
                created_at: w.created_at,
                ticket_stage: None,
                chat: None,
                review: None,
                browser_url: None,
                epic: false,
                sub_task: None,
                browser: None,
            });
            self.checkouts.push(Checkout {
                id: uuid::Uuid::new_v4().to_string(),
                task_id: w.id,
                project_id: w.project_id,
                path: w.path,
                base: w.base,
                // Nothing recorded the branch point back then; the diff falls
                // back to the merge base for these.
                base_commit: None,
                push_lease: None,
                point_before_update: None,
                last_head: None,
            });
        }
        self.version = SCHEMA_VERSION;
        true
    }
}

/// Default location for task directories when the user has not chosen one.
pub fn default_worktree_root() -> PathBuf {
    #[allow(deprecated)]
    let home = std::env::home_dir().unwrap_or_else(std::env::temp_dir);
    home.join(".villain-worktrees")
}

/// Replace the file at `path` with `bytes`, whole: through a temp file
/// beside it and a rename, so a crash leaves the old one or the new one and
/// never half of either.
pub(crate) fn write_whole(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(bytes)?;
    // On disk before it replaces the old one, or a crash straight after
    // the rename can leave a config with nothing in it. Plain fsync, not
    // `sync_all`: on macOS that is F_FULLFSYNC, a drive-cache flush that
    // costs tens of milliseconds, and some writes come from the main
    // thread — every step of a settings slider is one.
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: the descriptor belongs to `file`, which is open here.
        unsafe { libc::fsync(file.as_raw_fd()) };
    }
    drop(file);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub struct ConfigStore {
    path: PathBuf,
    /// The config folder of the Default profile, where every other profile's
    /// folder is too (PROF-4): what `other_configs` looks through.
    base: PathBuf,
    /// Where this profile's task folders go when the user has not chosen
    /// (PROF-5). Each profile has its own, so clearing the setting never
    /// lands one profile in another's.
    default_root: PathBuf,
    inner: RwLock<AppConfig>,
    /// Held from a change until it is on disk.
    ///
    /// The change used to be made under `inner` and written after letting go
    /// of it, through one shared temp file. Commands run in parallel now — the
    /// blocking pool, the restore thread, MCP calls — so two updates could
    /// finish in the wrong order and leave the older copy on disk, or write
    /// the temp file at once and leave it truncated: on the next launch that
    /// reads as unreadable, and every task is gone from the app.
    disk: parking_lot::Mutex<()>,
}

impl ConfigStore {
    /// The config in `profile`'s folder (§25).
    pub fn load(profile: &crate::profiles::Profile) -> Result<Self> {
        let dir = profile.folder.clone();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("config.json");

        let mut inner: AppConfig = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            match serde_json::from_str(&raw) {
                Ok(cfg) => cfg,
                Err(e) => {
                    // Starting empty is the only way to start at all, but the
                    // migration below writes straight back over this file —
                    // so the original is kept beside it rather than lost to a
                    // half-written save or a field this build cannot read.
                    let kept = path.with_extension("json.unreadable");
                    let _ = std::fs::copy(&path, &kept);
                    eprintln!(
                        "config.json could not be read ({e}); starting fresh, the old one is at {}",
                        kept.display()
                    );
                    AppConfig::default()
                }
            }
        } else {
            AppConfig::default()
        };

        // An older build running over a newer build's config drops whatever
        // it does not know on its first save; and a migration rewrites the
        // file for good (version 3 moves the setup into a workspace,
        // PROF-12). A copy first, once per version.
        if inner.version != SCHEMA_VERSION && path.exists() {
            let kept = path.with_extension(format!("json.v{}", inner.version));
            if !kept.exists() {
                let _ = std::fs::copy(&path, &kept);
            }
        }
        if inner.version > SCHEMA_VERSION {
            let kept = path.with_extension(format!("json.v{}", inner.version));
            eprintln!(
                "config.json is from a newer build (version {}); a copy is at {}",
                inner.version,
                kept.display()
            );
        }
        let changed = inner.migrate();

        let store = Self {
            path,
            base: profile.base.clone(),
            default_root: profile.default_root(),
            inner: RwLock::new(inner),
            disk: parking_lot::Mutex::new(()),
        };
        if changed {
            let snapshot = store.read();
            store.persist(&snapshot)?;
        }
        Ok(store)
    }

    pub fn read(&self) -> AppConfig {
        self.inner.read().clone()
    }

    /// What `f` reads from the config, without copying all of it as `read`
    /// does: every task, checkout and project, every few seconds, for the
    /// few fields a background loop or a poll looks at.
    ///
    /// The read lock is held while `f` runs, so `f` must not call `update`,
    /// `read` or any other accessor here: the lock is not reentrant, and a
    /// writer waiting between the two reads deadlocks. Nor may it wait.
    pub fn with<T>(&self, f: impl FnOnce(&AppConfig) -> T) -> T {
        f(&self.inner.read())
    }

    /// A store over `cfg`, saving to `path`, for tests that need an
    /// `AppState` without a running app.
    #[cfg(test)]
    pub fn for_tests(path: PathBuf, cfg: AppConfig) -> Self {
        let base = path.parent().map(PathBuf::from).unwrap_or_default();
        Self { path, base, default_root: default_worktree_root(), inner: RwLock::new(cfg), disk: parking_lot::Mutex::new(()) }
    }

    /// Mutate the config and write it back to disk atomically.
    ///
    /// Readers are not held up by the write: `inner` is released before it,
    /// and only other updates wait on `disk`.
    ///
    /// A change to the shared setup is queued for the workspace on the way
    /// (PROF-13): see `shared.rs`.
    pub fn update<T>(&self, f: impl FnOnce(&mut AppConfig) -> T) -> Result<T> {
        let _disk = self.disk.lock();
        let (out, snapshot, sent) = {
            let mut guard = self.inner.write();
            let before = shared::records(&guard);
            let out = f(&mut guard);
            let sent = shared::record_changes(&mut guard, &before);
            (out, guard.clone(), sent)
        };
        self.persist(&snapshot)?;
        crate::workspace::wrote(sent);
        Ok(out)
    }

    /// `update` for what the workspace sent (PROF-14): nothing is queued
    /// back, and nothing is written unless `f` says it changed something.
    pub fn mirror(&self, f: impl FnOnce(&mut AppConfig) -> bool) -> Result<bool> {
        let _disk = self.disk.lock();
        let mut snapshot = self.inner.read().clone();
        if !f(&mut snapshot) {
            return Ok(false);
        }
        // An acknowledgement is effective only after it is durable. Keep
        // the in-memory queue too when disk refuses the replacement.
        self.persist(&snapshot)?;
        *self.inner.write() = snapshot;
        Ok(true)
    }

    fn persist(&self, cfg: &AppConfig) -> Result<()> {
        write_whole(&self.path, &serde_json::to_vec_pretty(cfg)?)
    }

    /// The folder `config.json` is in, where the app keeps its other files.
    pub fn folder(&self) -> &std::path::Path {
        self.path.parent().unwrap_or(std::path::Path::new("."))
    }

    pub fn worktree_root(&self) -> PathBuf {
        self.inner
            .read()
            .worktree_root
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_root.clone())
    }

    /// What the app's other builds and profiles on this machine have saved,
    /// read only: the dev build beside the installed one, whose identifier
    /// is this one's with `.dev` on the end, and every profile of either
    /// (§25). They can share a task folder location, the two builds' by
    /// default, so a folder or a copy this profile does not use may be
    /// another's. Plain JSON, since the other build may be older or newer.
    pub fn other_configs(&self) -> Vec<serde_json::Value> {
        let ours = self.folder().to_path_buf();
        let (Some(name), Some(all)) = (
            self.base.file_name().and_then(|n| n.to_str()),
            self.base.parent().and_then(|d| std::fs::read_dir(d).ok()),
        ) else {
            return Vec::new();
        };
        let builds = all.flatten().map(|e| e.path()).filter(|p| {
            let other = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            other == name || other.starts_with(&format!("{name}.")) || name.starts_with(&format!("{other}."))
        });
        builds
            .flat_map(|build| {
                let profiles = std::fs::read_dir(build.join("profiles")).into_iter().flatten().flatten().map(|e| e.path());
                std::iter::once(build.clone()).chain(profiles).collect::<Vec<_>>()
            })
            .filter(|folder| *folder != ours)
            .filter_map(|folder| std::fs::read_to_string(folder.join("config.json")).ok())
            .filter_map(|raw| serde_json::from_str(&raw).ok())
            .collect()
    }

    pub fn project(&self, id: &str) -> Result<Project> {
        self.inner
            .read()
            .projects
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("project {id}")))
    }

    pub fn task(&self, id: &str) -> Result<Task> {
        self.inner
            .read()
            .tasks
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    pub fn checkout(&self, id: &str) -> Result<Checkout> {
        self.inner
            .read()
            .checkouts
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("checkout {id}")))
    }

    pub fn checkouts_of(&self, task_id: &str) -> Vec<Checkout> {
        self.inner
            .read()
            .checkouts
            .iter()
            .filter(|c| c.task_id == task_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config written by v1, before tasks could span repositories.
    const V1: &str = r#"{
      "projects": [
        {"id": "p1", "name": "api", "path": "/repos/api", "default_branch": "main"}
      ],
      "workspaces": [
        {
          "id": "w1", "project_id": "p1", "name": "ACME-1 fix login",
          "branch": "acme-1-fix-login", "path": "/wt/acme-1-fix-login",
          "base": "main", "issue_key": "ACME-1",
          "issue_url": "https://x.atlassian.net/browse/ACME-1",
          "created_at": "2026-09-01T10:00:00Z"
        }
      ]
    }"#;

    #[test]
    fn migrates_v1_workspaces_into_single_repo_tasks() {
        let mut cfg: AppConfig = serde_json::from_str(V1).unwrap();
        assert!(cfg.migrate());

        assert_eq!(cfg.version, SCHEMA_VERSION);
        assert!(cfg.workspaces.is_empty());
        assert_eq!(cfg.tasks.len(), 1);
        assert_eq!(cfg.checkouts.len(), 1);

        let task = &cfg.tasks[0];
        // The id carries over so running panes and the current selection survive.
        assert_eq!(task.id, "w1");
        assert_eq!(task.branch, "acme-1-fix-login");
        assert_eq!(task.issue_key.as_deref(), Some("ACME-1"));

        let checkout = &cfg.checkouts[0];
        assert_eq!(checkout.task_id, "w1");
        assert_eq!(checkout.project_id, "p1");
        assert_eq!(checkout.path, "/wt/acme-1-fix-login");
        assert_eq!(checkout.base, "main");
    }

    #[test]
    fn a_config_missing_a_preference_still_loads() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"version":2,"tasks":[{"id":"t","name":"n","root":"/r","branch":"b","created_at":"2026-01-01T00:00:00Z"}],"ui":{"restore_panes":false}}"#,
        )
        .unwrap();
        assert_eq!(cfg.tasks.len(), 1);
        assert!(!cfg.ui.restore_panes);
        assert_eq!(cfg.ui.terminal_font_size, UiPrefs::default().terminal_font_size);
        // Saved before conversations had a text size of their own (ACP-15).
        assert_eq!(cfg.ui.conversation_font_size, 14);
        // Saved before there was a choice of theme: the studio's (SET-5).
        assert_eq!(cfg.ui.theme, "night");
    }

    #[test]
    fn a_theme_saved_as_dark_or_light_opens_in_nightshades_night_or_day() {
        for (saved, now) in [("dark", "night"), ("light", "day"), ("ayu-light", "ayu-light")] {
            let cfg: AppConfig = serde_json::from_str(&format!(r#"{{"version":2,"ui":{{"theme":"{saved}"}}}}"#)).unwrap();
            assert_eq!(cfg.ui.theme, now);
        }
    }

    #[test]
    fn a_newer_builds_config_keeps_its_version() {
        let mut cfg: AppConfig = serde_json::from_str(r#"{"version":99}"#).unwrap();
        assert!(!cfg.migrate());
        assert_eq!(cfg.version, 99);
    }

    #[test]
    fn migration_is_idempotent_and_drops_legacy_on_write() {
        let mut cfg: AppConfig = serde_json::from_str(V1).unwrap();
        cfg.migrate();
        let after_first = serde_json::to_string(&cfg).unwrap();

        // A second pass must not duplicate the task.
        assert!(!cfg.migrate());
        assert_eq!(cfg.tasks.len(), 1);

        // Legacy workspaces are never written back out.
        assert!(!after_first.contains("workspaces"));

        // And re-reading what we wrote is a no-op.
        let mut round_tripped: AppConfig = serde_json::from_str(&after_first).unwrap();
        assert!(!round_tripped.migrate());
        assert_eq!(round_tripped.tasks.len(), 1);
        assert_eq!(round_tripped.checkouts.len(), 1);
    }

    #[test]
    fn empty_config_is_stamped_with_the_current_version() {
        let mut cfg = AppConfig::default();
        assert!(cfg.migrate());
        assert_eq!(cfg.version, SCHEMA_VERSION);
        assert!(!cfg.migrate());
    }

    #[test]
    fn a_borrowed_read_sees_the_latest_update_and_lets_the_next_one_through() {
        let dir = std::env::temp_dir().join(format!("vl-config-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ConfigStore::for_tests(dir.join("config.json"), AppConfig::default());
        assert!(store.with(|c| c.ui.notify_waiting_agents));

        store.update(|c| c.ui.notify_waiting_agents = false).unwrap();
        assert!(!store.with(|c| c.ui.notify_waiting_agents));

        // The lock is let go when the closure returns, so an update from
        // another thread right after it is not held up by it.
        let tasks = store.with(|c| c.tasks.len());
        std::thread::scope(|s| {
            s.spawn(|| store.update(|c| c.worktree_root = Some("/elsewhere".into())).unwrap()).join().unwrap();
        });
        assert_eq!(tasks, 0);
        assert_eq!(store.with(|c| c.worktree_root.clone()), Some("/elsewhere".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
