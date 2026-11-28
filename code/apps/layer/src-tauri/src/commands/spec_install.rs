//! Installing a task's spec toolset (§19): what its manifest says to run,
//! fetch and copy, done in the background when the task is made, into the
//! task folder and nowhere else. The files an install writes are recorded in
//! `.spec-install.json`, so deleting the task or Clean up takes exactly
//! those and leaves what the user or an agent added beside them.
//!
//! A failure is recorded, not raised: the task and its agents go on without
//! the toolset, and Retry runs it again. Nothing here may write inside a
//! repository's worktree, which would show up in Diff and in commits.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::config::Task;
use crate::error::{Error, Result};
use crate::spec::toolset::{self, InstallStep, RunIn, Toolset, Vars};
use crate::spec::{self, glob};

use super::panes::agent_file_dir;
use super::spec_copy::{spec_dir, toolset_of, MOST_DEPTH, MOST_FILES};
use super::spec_runner::{tail, Runner};
use super::spec_work::with_spec_moved;
use super::AppState;

/// What an install wrote, in the task folder: `{ toolset, version, ok,
/// error, log_tail, written }`.
pub(crate) const INSTALL_RECORD: &str = ".spec-install.json";

/// Where each agent CLI looks for skills, in the task folder. Claude Code
/// reads the first; Codex and OpenCode the second.
const SKILL_DIRS: [&str; 2] = [".claude/skills", ".agents/skills"];

/// A `run` step that names no timeout of its own.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// How much of what the steps printed is kept in the record.
const LOG_TAIL: usize = 4096;

/// What names the file beside a fetched copy's folder that says where it
/// came from. Never inside the copy, which is someone else's repository and
/// may hold a link of that name; and `@` is not in a version, so it can
/// never be another copy's folder.
const SOURCE: &str = "@source";

/// The file an install leaves in the task folder.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    toolset: String,
    version: Option<String>,
    ok: bool,
    error: Option<String>,
    log_tail: String,
    /// Paths in the task folder, `/`-separated; a folder ends in `/`. What
    /// the install made, which Clean up takes and nothing else.
    written: Vec<String>,
}

/// How an install stands: what the Spec tab shows (a line and a dot).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallState {
    pub toolset: String,
    pub version: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
    pub running: bool,
}

/// The `spec:install` event: whose install, and how it stands.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Announce<'a> {
    pub(super) task_id: &'a str,
    pub(super) state: &'a InstallState,
}

/// Tasks being installed now. A second install of one (Retry pressed twice,
/// or while the first still runs) would write over the first.
static RUNNING: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Busy(String);

impl Busy {
    fn take(task_id: &str) -> Option<Busy> {
        let mut running = RUNNING.lock();
        if running.iter().any(|t| t == task_id) {
            return None;
        }
        running.push(task_id.to_string());
        Some(Busy(task_id.to_string()))
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        RUNNING.lock().retain(|t| *t != self.0);
    }
}

/// Fetching takes this, so that two tasks made together fetch once.
static FETCHING: Mutex<()> = Mutex::new(());

fn refused(what: impl std::fmt::Display) -> Error {
    Error::Other(what.to_string())
}

/// The OpenSpec tool ids of the agent CLIs given, by Layer's ids for them.
/// Only those whose files OpenSpec writes into the task folder: it refuses a
/// `--tools` it does not know, and for codex it writes to `~/.codex/prompts`,
/// which every task and every project shares.
fn openspec_tools_for<'a>(agents: impl IntoIterator<Item = &'a str>) -> String {
    agents.into_iter().filter(|a| matches!(*a, "claude" | "opencode" | "gemini")).collect::<Vec<_>>().join(",")
}

/// Install `task`'s toolset: each step of its manifest in order, stopping at
/// the first that fails. Never an error: how it went is in the state and in
/// the record, and the task goes on either way.
pub(crate) fn install(state: &AppState, task: &Task, runner: &dyn Runner) -> InstallState {
    // First of all: an earlier build's spec is moved only while `spec/` is
    // absent (SPEC-17), and the pin below and the install itself would make it,
    // so a task given a repository (or its install retried) before its Spec
    // tab was opened would lose its approved spec for good. The move also pins
    // the task to Kiro, which is what it moves. A move that fails stops the
    // install: it would make `spec/`, and the old spec could never move after.
    let moved = with_spec_moved(state, task);
    let task = moved.as_ref().unwrap_or(task);
    let toolset = toolset_of(state, task);
    let mut out = InstallState {
        toolset: toolset.id.clone(),
        version: toolset.version.clone(),
        ok: false,
        error: None,
        running: false,
    };
    if let Err(e) = &moved {
        out.error = Some(format!(
            "the spec from before could not be moved into spec/ ({e}), so nothing is installed: Retry once it can be"
        ));
        return out;
    }
    let Some(_busy) = Busy::take(&task.id) else {
        out.running = true;
        return out;
    };
    // The task keeps the toolset it is installed with, whatever the app's
    // default becomes later.
    if task.toolset.is_none() {
        let (id, own) = (task.id.clone(), toolset.id.clone());
        let _ = state.config.update(move |c| {
            if let Some(t) = c.tasks.iter_mut().find(|t| t.id == id && t.toolset.is_none()) {
                t.toolset = Some(own);
            }
        });
    }
    let Some(dir) = agent_file_dir(state, task) else {
        out.error = Some("this task has no folder of its own, so there is nowhere to install into".into());
        return out;
    };
    let mut work = Work::begin(state, task, &dir);
    let result = toolset::validate(&toolset).map_err(Error::Other).and_then(|()| work.run(&toolset, runner));
    out.ok = result.is_ok();
    out.error = result.err().map(|e| e.to_string());
    if let Some(why) = &out.error {
        work.note(&format!("failed: {why}"));
    }
    let record = Record {
        toolset: out.toolset.clone(),
        version: out.version.clone(),
        ok: out.ok,
        error: out.error.clone(),
        log_tail: tail(&work.log, LOG_TAIL).to_string(),
        written: work.written.into_iter().collect(),
    };
    if let Err(e) = write_record(&dir, &record) {
        out.ok = false;
        out.error.get_or_insert_with(|| format!("the install could not be recorded: {e}"));
    }
    out
}

/// One install under way: where it writes, and what it has written.
struct Work<'a> {
    state: &'a AppState,
    task: &'a Task,
    /// The task folder.
    dir: PathBuf,
    /// What an earlier install of this task wrote.
    prior: BTreeSet<String>,
    written: BTreeSet<String>,
    log: String,
    /// The fetched copy a `skills` step copies from.
    fetched: Option<PathBuf>,
}

impl<'a> Work<'a> {
    fn begin(state: &'a AppState, task: &'a Task, dir: &Path) -> Self {
        // What an earlier install wrote stays on record: a second `init` that
        // finds its files there writes none, and they are still its own.
        let prior: BTreeSet<String> = read_record(dir).map(|r| r.written.into_iter().collect()).unwrap_or_default();
        Work { state, task, dir: dir.to_path_buf(), written: prior.clone(), prior, log: String::new(), fetched: None }
    }

    fn note(&mut self, line: &str) {
        self.log.push_str(line.trim_end());
        self.log.push('\n');
    }

    fn run(&mut self, toolset: &Toolset, runner: &dyn Runner) -> Result<()> {
        let spec = spec_dir(self.state, self.task);
        let spec_rel = spec
            .strip_prefix(&self.dir)
            .ok()
            .map(|r| r.to_string_lossy().to_string())
            .filter(|r| glob::is_safe_rel(r))
            .ok_or_else(|| {
                refused("this task's spec is not kept in its folder, so there is nowhere to install into")
            })?;
        let vars = Vars {
            version: toolset.version.clone().unwrap_or_default(),
            task: spec::folder_name(&self.task.branch),
            openspec_tools: openspec_tools_for(crate::agents::AGENTS.iter().map(|a| a.id)),
            skill_dirs: SKILL_DIRS
                .iter()
                .map(|d| self.dir.join(d).to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(","),
        };
        for step in &toolset.install {
            match step {
                InstallStep::Run { run, cwd, timeout_secs } => {
                    let argv: Vec<String> = run.iter().map(|a| toolset::expand(a, &vars)).collect();
                    let timeout = timeout_secs.map(Duration::from_secs).unwrap_or(DEFAULT_TIMEOUT);
                    let at = match cwd {
                        RunIn::Spec => {
                            self.make_dir(&spec_rel)?;
                            spec.clone()
                        }
                        RunIn::Task => self.dir.clone(),
                    };
                    self.run_step(runner, &argv, &at, timeout)?;
                }
                InstallStep::Fetch { fetch, r#ref, commit } => {
                    let (url, r#ref) = (toolset::expand(fetch, &vars), toolset::expand(r#ref, &vars));
                    self.fetched = Some(self.fetch(runner, toolset, &url, &r#ref, commit.as_deref())?);
                }
                InstallStep::Skills { skills } => {
                    let cache =
                        self.fetched.clone().ok_or_else(|| refused("a skills step needs a fetch step before it"))?;
                    self.copy_skills(&cache, skills)?;
                }
            }
        }
        Ok(())
    }

    /// One `run` step. What it left in its folder is recorded whether it
    /// succeeded or not: a step that died halfway still wrote something.
    fn run_step(&mut self, runner: &dyn Runner, argv: &[String], at: &Path, timeout: Duration) -> Result<()> {
        let skip: Vec<PathBuf> =
            self.state.config.checkouts_of(&self.task.id).iter().map(|c| PathBuf::from(&c.path)).collect();
        let before = snapshot(&self.dir, at, &skip);
        self.note(&format!("$ {}", argv.join(" ")));
        let ran = runner.run(argv, at, timeout);
        self.written.extend(snapshot(&self.dir, at, &skip).difference(&before).cloned());
        let printed = ran?;
        self.note(&printed);
        Ok(())
    }

    /// The fetched copy for `url` at `ref`: the one kept from an earlier
    /// install, or one made now. A copy is cloned beside its folder and moved
    /// into place, so one that stopped halfway is never taken for whole.
    /// With `commit`, the ref must be that commit: a tag moved since it was
    /// pinned is refused, and nothing it holds is kept.
    fn fetch(
        &mut self,
        runner: &dyn Runner,
        toolset: &Toolset,
        url: &str,
        r#ref: &str,
        commit: Option<&str>,
    ) -> Result<PathBuf> {
        let version = toolset
            .version
            .as_deref()
            .ok_or_else(|| refused(format!("{} fetches code but names no version; pin one", toolset.id)))?;
        let kept = self.state.config.folder().join("toolsets").join(&toolset.id);
        let cache = kept.join(version);
        // The URL and ref it came from: a copy fetched for another is not
        // this one, whatever its folder is called.
        let marker = format!("{url}\n{}\n{}\n", r#ref, commit.unwrap_or(""));
        let source = kept.join(format!("{version}{SOURCE}"));
        let _one_at_a_time = FETCHING.lock();
        if std::fs::read_to_string(&source).is_ok_and(|m| m == marker) && cache.is_dir() {
            self.note(&format!("using the copy of {url} at {} kept from before", r#ref));
            return Ok(cache);
        }
        self.note(&format!("fetching {url} at {}", r#ref));
        std::fs::create_dir_all(&kept)?;
        // Ours, under the config folder, and named by a validated id and version.
        let _ = std::fs::remove_file(&source);
        let _ = std::fs::remove_dir_all(&cache);
        let stage = kept.join(format!(".fetching-{}", uuid::Uuid::new_v4()));
        // The marker last: a copy that stopped halfway has none.
        let made = runner.fetch(url, r#ref, &stage).and_then(|got| {
            if let Some(pinned) = commit.filter(|c| !c.eq_ignore_ascii_case(&got)) {
                return Err(refused(format!(
                    "{url} at {} is commit {got}, not the pinned {pinned}; it was moved, so it is not used",
                    r#ref
                )));
            }
            std::fs::rename(&stage, &cache)?;
            Ok(std::fs::write(&source, &marker)?)
        });
        if made.is_err() {
            let _ = std::fs::remove_dir_all(&stage);
        }
        made?;
        Ok(cache)
    }

    /// Copy the skill folders `pattern` names in the fetched copy into every
    /// skill folder of the task. Links in the copy are never followed: a
    /// folder or file that is one is left out, and said.
    fn copy_skills(&mut self, cache: &Path, pattern: &str) -> Result<()> {
        let mut found = Vec::new();
        let mut left_out = Vec::new();
        find_skills(cache, "", 0, pattern, &mut 0, &mut found, &mut left_out)?;
        found.sort();
        for rel in &left_out {
            self.note(&format!("left out {rel}: a link, and a copy never goes through one"));
        }
        for skills in SKILL_DIRS {
            for (name, src) in &found {
                self.copy_tree(src, &format!("{skills}/{name}"), 0, &mut 0)?;
            }
        }
        Ok(())
    }

    /// The regular files under `src`, into `rel` in the task folder. A file
    /// the user already has there is theirs: kept, and not recorded, so
    /// Clean up keeps it too. A file this install wrote before is replaced.
    fn copy_tree(&mut self, src: &Path, rel: &str, depth: usize, total: &mut usize) -> Result<()> {
        self.make_dir(rel)?;
        for entry in std::fs::read_dir(src)?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let child = format!("{rel}/{name}");
            let kind = entry.file_type()?;
            if kind.is_dir() {
                if depth >= MOST_DEPTH {
                    return Err(refused(format!("{} is nested too deeply to copy", entry.path().display())));
                }
                self.copy_tree(&entry.path(), &child, depth + 1, total)?;
            } else if kind.is_file() {
                *total += 1;
                if *total > MOST_FILES {
                    return Err(refused(format!("{} holds more than {MOST_FILES} files to copy", src.display())));
                }
                let to = self.dir.join(&child);
                match std::fs::symlink_metadata(&to) {
                    Ok(m) if !m.is_file() || !self.prior.contains(&child) => {
                        self.note(&format!("kept your {child}, which is already there"));
                        continue;
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
                std::fs::copy(entry.path(), &to)?;
                self.written.insert(child);
            } else {
                self.note(&format!("left out {}: a link, and a copy never goes through one", entry.path().display()));
            }
        }
        Ok(())
    }

    /// Make the folder `rel` in the task folder and each one above it that is
    /// missing, recording those made. A link on the way is an error: writing
    /// through one would land wherever it points.
    fn make_dir(&mut self, rel: &str) -> Result<()> {
        let mut at = self.dir.clone();
        let mut so_far = String::new();
        for segment in rel.split('/') {
            at.push(segment);
            if !so_far.is_empty() {
                so_far.push('/');
            }
            so_far.push_str(segment);
            match std::fs::symlink_metadata(&at) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => {
                    return Err(refused(format!(
                        "{so_far} is a link or a file, and an install never writes through one"
                    )));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&at)?;
                    self.written.insert(format!("{so_far}/"));
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

/// The skill folders in the fetched copy `pattern` matches, as (name, path).
fn find_skills(
    cache: &Path,
    rel: &str,
    depth: usize,
    pattern: &str,
    seen: &mut usize,
    found: &mut Vec<(String, PathBuf)>,
    left_out: &mut Vec<String>,
) -> Result<()> {
    let at = if rel.is_empty() { cache.to_path_buf() } else { cache.join(rel) };
    for entry in std::fs::read_dir(&at)?.flatten() {
        *seen += 1;
        if *seen > MOST_FILES {
            return Err(refused(format!("the fetched copy holds more than {MOST_FILES} things to look through")));
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        let child = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let kind = entry.file_type()?;
        let hit = glob::matches(pattern, &child);
        if kind.is_symlink() {
            if hit {
                left_out.push(child);
            }
        } else if kind.is_dir() {
            if hit {
                found.push((name, entry.path()));
            } else if depth < MOST_DEPTH {
                find_skills(cache, &child, depth + 1, pattern, seen, found, left_out)?;
            }
        }
    }
    Ok(())
}

/// Every file and folder under `at`, by path in the task folder `dir` (a
/// folder ends in `/`), leaving out worktrees, `.git` and `node_modules`.
/// Bounded like the spec's own walks. What a step added is the difference of
/// two of these.
fn snapshot(dir: &Path, at: &Path, skip: &[PathBuf]) -> BTreeSet<String> {
    fn walk(dir: &Path, at: &Path, depth: usize, skip: &[PathBuf], seen: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(at) else { return };
        for entry in entries.flatten() {
            if seen.len() >= MOST_FILES {
                return;
            }
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(dir) else { continue };
            let rel = rel.to_string_lossy().to_string();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                let name = entry.file_name();
                if name == ".git" || name == "node_modules" || skip.contains(&path) {
                    continue;
                }
                seen.insert(format!("{rel}/"));
                if depth < MOST_DEPTH {
                    walk(dir, &path, depth + 1, skip, seen);
                }
            } else {
                seen.insert(rel);
            }
        }
    }
    let mut seen = BTreeSet::new();
    walk(dir, at, 0, skip, &mut seen);
    seen
}

/// The record in a task folder, when there is a readable one. A link is not
/// followed: an agent can plant one.
fn read_record(dir: &Path) -> Option<Record> {
    let path = dir.join(INSTALL_RECORD);
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// How `task`'s install stands now, for the Spec tab when it opens: under
/// way, or as its record says. None when none has run.
pub(crate) fn state_of(state: &AppState, task: &Task) -> Option<InstallState> {
    if RUNNING.lock().contains(&task.id) {
        let toolset = toolset_of(state, task);
        return Some(InstallState {
            toolset: toolset.id,
            version: toolset.version,
            ok: false,
            error: None,
            running: true,
        });
    }
    let record = read_record(&agent_file_dir(state, task)?)?;
    Some(InstallState {
        toolset: record.toolset,
        version: record.version,
        ok: record.ok,
        error: record.error,
        running: false,
    })
}

/// The text is written beside it, to a file nobody else can have made, and
/// renamed over it: a reader sees the old record or the new, never half.
fn write_record(dir: &Path, record: &Record) -> Result<()> {
    use std::io::Write;
    let temp = dir.join(format!(".spec-install-{}.tmp", uuid::Uuid::new_v4()));
    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?)?;
        file.sync_all()?;
        std::fs::rename(&temp, dir.join(INSTALL_RECORD))
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    Ok(written?)
}

/// Take what the install of the task folder `dir` wrote, and nothing else:
/// its files, then the folders it made that they left empty. The record is
/// a file an agent can edit, so each path in it is checked before it is
/// touched: inside the folder, through no link, and not inside a worktree.
pub(crate) fn remove_written(dir: &Path) {
    let Some(record) = read_record(dir) else { return };
    let (mut folders, files): (Vec<&str>, Vec<&str>) =
        record.written.iter().map(String::as_str).partition(|w| w.ends_with('/'));
    for rel in files {
        if ours(dir, rel) {
            let _ = std::fs::remove_file(dir.join(rel));
        }
    }
    // Deepest first, and only while empty: a folder the user put something in stays.
    folders.sort_by_key(|f| std::cmp::Reverse(f.matches('/').count()));
    for rel in folders {
        let rel = rel.trim_end_matches('/');
        if ours(dir, rel) {
            let _ = std::fs::remove_dir(dir.join(rel));
        }
    }
}

/// The files, by path in the task folder `dir`, that its install wrote and
/// that `remove_written` would take: for Clean up, to tell them from what
/// someone else put there.
pub(crate) fn written_files(dir: &Path) -> HashSet<String> {
    read_record(dir)
        .map(|r| r.written.into_iter().filter(|w| !w.ends_with('/') && ours(dir, w)).collect())
        .unwrap_or_default()
}

/// Whether `rel`, from a record, may be removed from `dir`.
fn ours(dir: &Path, rel: &str) -> bool {
    if !glob::is_safe_rel(rel) {
        return false;
    }
    let mut at = dir.to_path_buf();
    let segments: Vec<&str> = rel.split('/').collect();
    for segment in &segments[..segments.len() - 1] {
        at.push(segment);
        // A link, or a folder that is a repository's worktree.
        if !std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_dir())
            || std::fs::symlink_metadata(at.join(".git")).is_ok()
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::commands::panes::remove_generated;
    use crate::config::{AppConfig, Checkout, ConfigStore, Project};

    /// What a `Runner` does here: it records what it was asked and leaves the
    /// files a real install would, so nothing goes out to npm or GitHub.
    #[derive(Default)]
    struct Fake {
        ran: RefCell<Vec<(Vec<String>, PathBuf)>>,
        fetched: RefCell<Vec<(String, String)>>,
        /// Files an `init` leaves in the folder it ran in.
        leaves: Vec<(&'static str, &'static str)>,
        fails: Option<&'static str>,
        /// Files the fetched copy holds.
        cache: Vec<(&'static str, &'static str)>,
        /// Links the fetched copy holds, by name, and what they point at.
        links: Vec<(&'static str, PathBuf)>,
        /// The tag was moved: a fetch gets another commit than the pinned one.
        moved: bool,
    }

    impl Runner for Fake {
        fn run(&self, argv: &[String], cwd: &Path, _timeout: Duration) -> Result<String> {
            self.ran.borrow_mut().push((argv.to_vec(), cwd.to_path_buf()));
            if let Some(why) = self.fails {
                return Err(Error::Other(why.into()));
            }
            for (rel, text) in &self.leaves {
                put(&cwd.join(rel), text);
            }
            Ok("initialised".into())
        }

        fn fetch(&self, url: &str, r#ref: &str, into: &Path) -> Result<String> {
            self.fetched.borrow_mut().push((url.into(), r#ref.into()));
            for (rel, text) in &self.cache {
                put(&into.join(rel), text);
            }
            for (rel, target) in &self.links {
                let at = into.join(rel);
                std::fs::create_dir_all(at.parent().unwrap()).unwrap();
                std::os::unix::fs::symlink(target, at).unwrap();
            }
            Ok(if self.moved { "0".repeat(40) } else { "22f0b463e7a5e579904dd6211a520321035526f7".into() })
        }
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        crate::git::run_for_tests(dir, args).unwrap()
    }

    /// A task `T-1` of repositories `folders`, each a real repository with one
    /// commit, in a temp dir that is also the config folder.
    fn world(folders: &[&str], toolset: &str) -> (PathBuf, AppState, Task) {
        let root = std::env::temp_dir().join(format!("vl-spec-install-{}", uuid::Uuid::new_v4()));
        // Installs of one task are one at a time, so tests running side by
        // side must not share a task's id.
        let id = format!("t-{}", uuid::Uuid::new_v4());
        let task_root = root.join("tasks/T-1");
        let mut cfg = AppConfig::default();
        for folder in folders {
            let worktree = task_root.join(folder);
            std::fs::create_dir_all(&worktree).unwrap();
            git(&worktree, &["init", "-q", "-b", "main"]);
            put(&worktree.join("a.txt"), "one\n");
            git(&worktree, &["add", "-A"]);
            git(
                &worktree,
                &["-c", "user.email=a@b.c", "-c", "user.name=T", "-c", "commit.gpgsign=false", "commit", "-qm", "init"],
            );
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
                task_id: id.clone(),
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
            toolset: Some(toolset.into()),
            id,
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

    fn superpowers_cache() -> Fake {
        Fake {
            cache: vec![
                ("skills/brainstorming/SKILL.md", "brainstorm"),
                ("skills/brainstorming/notes/a.md", "a"),
                ("skills/debugging/SKILL.md", "debug"),
                ("README.md", "not a skill"),
            ],
            ..Default::default()
        }
    }

    fn record(task: &Task) -> Record {
        let text = std::fs::read_to_string(Path::new(&task.root).join(INSTALL_RECORD)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn an_old_tasks_approved_spec_is_moved_before_the_install_pins_a_toolset_and_makes_spec() {
        let (root, state, task) = world(&["api"], "openspec");
        // A task from before toolsets, while the app's default is another one.
        state
            .config
            .update(|c| {
                c.default_toolset = Some("openspec".into());
                c.tasks.iter_mut().for_each(|t| t.toolset = None);
            })
            .unwrap();
        let task = state.config.task(&task.id).unwrap();
        put(&Path::new(&task.root).join(".spec-drafts/approved/requirements.md"), "# Approved\n");
        let fake = Fake::default();

        let done = install(&state, &task, &fake);

        let dir = Path::new(&task.root);
        assert_eq!(std::fs::read_to_string(dir.join("spec/requirements.md")).unwrap(), "# Approved\n");
        assert_eq!(state.config.task(&task.id).unwrap().toolset.as_deref(), Some("kiro"), "what is moved is Kiro's");
        assert!(fake.ran.borrow().is_empty(), "Kiro installs nothing, so the default's init did not run");
        assert_eq!((done.toolset.as_str(), done.ok), ("kiro", true), "{done:?}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn installing_openspec_runs_its_pinned_init_in_the_spec_folder() {
        let (root, state, task) = world(&["api"], "openspec");
        let fake = Fake { leaves: vec![("openspec/project.md", "p"), ("AGENTS.md", "a")], ..Default::default() };

        let done = install(&state, &task, &fake);

        let tools = openspec_tools_for(crate::agents::AGENTS.iter().map(|a| a.id));
        let ran = fake.ran.borrow();
        assert_eq!(ran.len(), 1);
        let pinned = ["npx", "-y", "@fission-ai/openspec@1.14.3", "init", "--tools", tools.as_str(), "."];
        assert_eq!(ran[0].0, pinned);
        assert_eq!(ran[0].1, Path::new(&task.root).join("spec"));
        assert!(tools.split(',').any(|t| t == "claude"), "{tools}");
        assert_eq!((done.ok, done.error.clone(), done.running), (true, None, false), "{done:?}");
        assert_eq!((done.toolset.as_str(), done.version.as_deref()), ("openspec", Some("1.14.3")));
        let rec = record(&task);
        assert_eq!(rec.written, ["spec/", "spec/AGENTS.md", "spec/openspec/", "spec/openspec/project.md"]);
        assert!(rec.log_tail.contains("initialised"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn openspec_is_told_only_the_agent_clis_whose_files_it_keeps_in_the_task() {
        assert_eq!(openspec_tools_for(["claude", "codex", "copilot", "gemini", "opencode"]), "claude,gemini,opencode");
        assert_eq!(openspec_tools_for(["copilot"]), "");
    }

    #[test]
    fn installing_superpowers_copies_its_skills_into_every_skill_folder() {
        let (root, state, task) = world(&["api"], "superpowers");
        let fake = superpowers_cache();

        let done = install(&state, &task, &fake);

        assert!(done.ok, "{done:?}");
        let dir = Path::new(&task.root);
        for skills in [".claude/skills", ".agents/skills"] {
            let kept = |rel: &str| std::fs::read_to_string(dir.join(skills).join(rel)).unwrap();
            assert_eq!(kept("brainstorming/SKILL.md"), "brainstorm");
            assert_eq!(kept("brainstorming/notes/a.md"), "a");
            assert_eq!(kept("debugging/SKILL.md"), "debug");
            assert!(!dir.join(skills).join("README.md").exists(), "only skill folders are copied");
        }
        let rec = record(&task);
        for must in
            [".claude/skills/brainstorming/SKILL.md", ".agents/skills/brainstorming/SKILL.md", ".agents/skills/"]
        {
            assert!(rec.written.iter().any(|w| w == must), "{must} in {:?}", rec.written);
        }
        assert_eq!(*fake.fetched.borrow(), [("https://github.com/obra/superpowers".to_string(), "v7.0.2".to_string())]);
        assert!(
            root.join("toolsets/superpowers/7.0.2/skills/brainstorming/SKILL.md").is_file(),
            "cached for next time"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_fetched_copy_is_reused_for_the_next_task() {
        let (root, state, task) = world(&["api"], "superpowers");
        let fake = superpowers_cache();
        assert!(install(&state, &task, &fake).ok);

        let mut next = task.clone();
        next.id = format!("{}-next", task.id);
        next.root = root.join("tasks/T-2").to_string_lossy().to_string();
        std::fs::create_dir_all(&next.root).unwrap();
        state.config.update(|c| c.tasks.push(next.clone())).unwrap();
        let done = install(&state, &next, &fake);

        assert!(done.ok, "{done:?}");
        assert_eq!(fake.fetched.borrow().len(), 1, "fetched once for two tasks");
        assert!(Path::new(&next.root).join(".claude/skills/debugging/SKILL.md").is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_copy_fetched_from_somewhere_else_is_not_taken_for_this_one() {
        let (root, state, task) = world(&["api"], "superpowers");
        let fake = superpowers_cache();
        assert!(install(&state, &task, &fake).ok);
        let mut custom = toolset::find("superpowers", &[]).unwrap();
        custom.install[0] =
            InstallStep::Fetch { fetch: "https://example.com/fork".into(), r#ref: "v7.0.2".into(), commit: None };
        state.config.update(|c| c.toolsets.push(custom)).unwrap();

        assert!(install(&state, &task, &fake).ok);

        assert_eq!(fake.fetched.borrow().len(), 2, "a different URL under the same version is fetched again");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_tag_moved_since_it_was_pinned_is_refused_and_nothing_of_it_is_kept() {
        let (root, state, task) = world(&["api"], "superpowers");
        let fake = Fake { moved: true, ..superpowers_cache() };

        let done = install(&state, &task, &fake);

        assert!(!done.ok, "{done:?}");
        assert!(done.error.as_deref().is_some_and(|e| e.contains("not the pinned")), "{done:?}");
        assert!(!Path::new(&task.root).join(".claude/skills/debugging/SKILL.md").exists());
        let kept = root.join("toolsets/superpowers");
        assert!(!kept.join("7.0.2").exists(), "the moved copy is not cached");
        assert!(
            std::fs::read_dir(&kept)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with(".fetching")),
            "nor left half-way"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_old_spec_that_cannot_be_moved_stops_the_install_so_it_can_move_later() {
        use std::os::unix::fs::PermissionsExt;
        let (root, state, task) = world(&["api"], "openspec");
        let dir = PathBuf::from(&task.root);
        put(&dir.join(".spec-drafts/approved/requirements.md"), "# Approved\n");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let fake = Fake::default();

        let done = install(&state, &task, &fake);

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!done.ok, "{done:?}");
        assert!(done.error.as_deref().is_some_and(|e| e.contains("could not be moved")), "{done:?}");
        assert!(fake.ran.borrow().is_empty(), "nothing ran");
        assert!(!dir.join("spec").exists(), "and spec/ was not made, so the move can happen later");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_failed_step_is_recorded_and_does_not_error_the_task() {
        let (root, state, task) = world(&["api"], "openspec");
        let fake = Fake { fails: Some("npx exited with 1: network is down"), ..Default::default() };

        let done = install(&state, &task, &fake);

        assert!(!done.ok);
        assert!(done.error.as_deref().unwrap().contains("network is down"), "{done:?}");
        let rec = record(&task);
        assert!(!rec.ok);
        assert!(rec.error.unwrap().contains("network is down"));
        assert!(rec.log_tail.contains("network is down"));
        assert!(state.config.task(&task.id).is_ok(), "the task is untouched");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn what_a_failed_step_wrote_before_it_failed_is_still_on_record() {
        let (root, state, task) = world(&["api"], "openspec");
        let fake = Fake { fails: Some("died"), leaves: vec![("half.md", "h")], ..Default::default() };
        // The fake fails before it leaves anything: run it the other way round.
        struct Half(Fake);
        impl Runner for Half {
            fn run(&self, argv: &[String], cwd: &Path, timeout: Duration) -> Result<String> {
                put(&cwd.join("openspec/half.md"), "h");
                self.0.run(argv, cwd, timeout)
            }
            fn fetch(&self, url: &str, r#ref: &str, into: &Path) -> Result<String> {
                self.0.fetch(url, r#ref, into)
            }
        }

        assert!(!install(&state, &task, &Half(fake)).ok);

        assert!(record(&task).written.iter().any(|w| w == "spec/openspec/half.md"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_skill_folder_symlinked_out_of_the_cache_is_not_copied() {
        let (root, state, task) = world(&["api"], "superpowers");
        let outside = root.join("outside");
        put(&outside.join("SKILL.md"), "stolen");
        put(&root.join("secret.txt"), "secret");
        let mut fake = superpowers_cache();
        fake.links = vec![("skills/evil", outside.clone()), ("skills/brainstorming/leak.md", root.join("secret.txt"))];

        let done = install(&state, &task, &fake);

        assert!(done.ok, "the rest is installed: {done:?}");
        let dir = Path::new(&task.root);
        for skills in [".claude/skills", ".agents/skills"] {
            assert!(!dir.join(skills).join("evil").exists(), "a linked folder is not copied");
            assert!(dir.join(skills).join("brainstorming/SKILL.md").is_file());
            assert!(!dir.join(skills).join("brainstorming/leak.md").exists(), "nor is a linked file");
        }
        assert!(record(&task).log_tail.contains("skills/evil"), "what was left out is said");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_link_in_the_fetched_repository_cannot_make_the_install_write_outside_the_cache() {
        let (root, state, task) = world(&["api"], "superpowers");
        let precious = root.join("precious.txt");
        put(&precious, "keep me");
        // A repository that tracks the marker's old name as a link.
        let mut fake = superpowers_cache();
        fake.links = vec![(".layer-fetched", precious.clone()), (".fetched", precious.clone())];

        let done = install(&state, &task, &fake);

        assert!(done.ok, "{done:?}");
        assert_eq!(std::fs::read_to_string(&precious).unwrap(), "keep me");
        assert!(install(&state, &task, &fake).ok);
        assert_eq!(fake.fetched.borrow().len(), 1, "the kept copy is still taken for the next install");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_skill_folder_that_is_already_a_link_in_the_task_folder_is_not_written_through() {
        let (root, state, task) = world(&["api"], "superpowers");
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, Path::new(&task.root).join(".claude")).unwrap();

        let done = install(&state, &task, &superpowers_cache());

        assert!(!done.ok, "{done:?}");
        assert!(done.error.unwrap().contains("link"));
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0, "nothing went through the link");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_skill_file_the_user_already_has_is_left_alone_and_not_taken_for_the_installs() {
        let (root, state, task) = world(&["api"], "superpowers");
        let mine = Path::new(&task.root).join(".claude/skills/brainstorming/SKILL.md");
        put(&mine, "mine");

        assert!(install(&state, &task, &superpowers_cache()).ok);

        assert_eq!(std::fs::read_to_string(&mine).unwrap(), "mine");
        assert!(!record(&task).written.iter().any(|w| w == ".claude/skills/brainstorming/SKILL.md"));
        remove_generated(Path::new(&task.root));
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), "mine", "Clean up leaves it too");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn nothing_is_written_inside_a_repository_worktree() {
        for id in ["openspec", "superpowers"] {
            let (root, state, task) = world(&["api", "web"], id);
            let fake = Fake { leaves: vec![("openspec/project.md", "p")], ..superpowers_cache() };

            let done = install(&state, &task, &fake);

            assert!(done.ok, "{id}: {done:?}");
            for checkout in state.config.checkouts_of(&task.id) {
                let at = Path::new(&checkout.path);
                assert_eq!(
                    git(at, &["status", "--porcelain", "--untracked-files=all"]),
                    "",
                    "{id} in {}",
                    at.display()
                );
                assert_eq!(std::fs::read_dir(at).unwrap().flatten().filter(|e| e.file_name() != ".git").count(), 1);
            }
            std::fs::remove_dir_all(&root).ok();
        }
    }

    #[test]
    fn a_run_in_the_task_folder_does_not_record_what_is_in_its_worktrees() {
        let (root, state, task) = world(&["api"], "openspec");
        let mut custom = toolset::find("openspec", &[]).unwrap();
        custom.install = vec![InstallStep::Run { run: vec!["tool".into()], cwd: RunIn::Task, timeout_secs: None }];
        state.config.update(|c| c.toolsets.push(custom)).unwrap();
        let fake = Fake { leaves: vec![("notes.md", "n")], ..Default::default() };

        assert!(install(&state, &task, &fake).ok);

        assert_eq!(record(&task).written, ["notes.md"], "the worktree's own files are not the install's");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cleanup_removes_exactly_what_the_install_wrote() {
        let (root, state, task) = world(&["api"], "superpowers");
        let dir = Path::new(&task.root);
        put(&dir.join(".claude/notes.md"), "mine");
        put(&dir.join(".claude/skills/mine/SKILL.md"), "mine");
        assert!(install(&state, &task, &superpowers_cache()).ok);
        assert!(dir.join(".agents/skills/brainstorming/SKILL.md").is_file());

        remove_generated(dir);

        assert!(!dir.join(INSTALL_RECORD).exists());
        assert!(!dir.join(".claude/skills/brainstorming").exists());
        assert!(!dir.join(".claude/skills/debugging").exists());
        assert!(!dir.join(".agents").exists(), "folders only the install made go once empty");
        assert_eq!(std::fs::read_to_string(dir.join(".claude/notes.md")).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(dir.join(".claude/skills/mine/SKILL.md")).unwrap(), "mine");
        assert!(dir.join("api/a.txt").is_file(), "a worktree is not touched");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_record_naming_paths_outside_the_task_folder_removes_none_of_them() {
        let (root, _, task) = world(&["api"], "superpowers");
        let dir = Path::new(&task.root);
        put(&root.join("outside.txt"), "keep");
        put(&root.join("tasks/T-1/api/a.txt"), "one\n");
        let elsewhere = root.join("elsewhere");
        put(&elsewhere.join("x.txt"), "keep");
        std::os::unix::fs::symlink(&elsewhere, dir.join("linked")).unwrap();
        let record = Record {
            written: [
                "../../outside.txt",
                "../outside.txt",
                root.join("outside.txt").to_str().unwrap(),
                "linked/x.txt",
                "api/a.txt",
                "a/../../outside.txt",
            ]
            .map(String::from)
            .to_vec(),
            ..Default::default()
        };
        put(&dir.join(INSTALL_RECORD), &serde_json::to_string(&record).unwrap());

        remove_generated(dir);

        assert_eq!(std::fs::read_to_string(root.join("outside.txt")).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(elsewhere.join("x.txt")).unwrap(), "keep", "not through a link");
        assert!(dir.join("api/a.txt").is_file(), "not inside a worktree, which has a .git of its own");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn installing_again_keeps_what_the_first_install_wrote_on_record() {
        let (root, state, task) = world(&["api"], "openspec");
        let fake = Fake { leaves: vec![("openspec/project.md", "p")], ..Default::default() };
        assert!(install(&state, &task, &fake).ok);

        // The second `init` finds its files there and writes nothing new.
        let again = Fake::default();
        assert!(install(&state, &task, &again).ok);

        assert!(record(&task).written.iter().any(|w| w == "spec/openspec/project.md"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_toolset_is_fixed_to_the_task_by_its_first_install() {
        let (root, state, mut task) = world(&["api"], "openspec");
        task.toolset = None;
        state.config.update(|c| c.tasks[0].toolset = None).unwrap();
        state.config.update(|c| c.default_toolset = Some("openspec".into())).unwrap();

        assert!(install(&state, &task, &Fake::default()).ok);
        state.config.update(|c| c.default_toolset = Some("kiro".into())).unwrap();

        assert_eq!(state.config.task(&task.id).unwrap().toolset.as_deref(), Some("openspec"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_toolset_without_install_steps_still_leaves_a_record() {
        let (root, state, task) = world(&["api"], "kiro");

        let done = install(&state, &task, &Fake::default());

        assert!(done.ok);
        let rec = record(&task);
        assert_eq!((rec.toolset.as_str(), rec.ok, rec.written.len()), ("kiro", true, 0));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_task_whose_root_is_its_checkout_has_nowhere_to_install() {
        let (root, state, mut task) = world(&["api"], "openspec");
        task.root = root.join("tasks/T-1/api").to_string_lossy().to_string();
        let fake = Fake::default();

        let done = install(&state, &task, &fake);

        assert!(!done.ok);
        assert!(done.error.unwrap().contains("no folder of its own"));
        assert!(fake.ran.borrow().is_empty());
        assert_eq!(git(&root.join("tasks/T-1/api"), &["status", "--porcelain"]), "");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_task_being_installed_is_not_installed_twice_at_once() {
        let (root, state, task) = world(&["api"], "openspec");
        let _busy = Busy::take(&task.id).unwrap();
        let fake = Fake::default();

        let done = install(&state, &task, &fake);

        assert!(done.running);
        assert!(fake.ran.borrow().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_manifest_that_would_not_validate_is_not_run() {
        let (root, state, task) = world(&["api"], "openspec");
        let mut custom = toolset::find("openspec", &[]).unwrap();
        custom.install = vec![InstallStep::Run { run: vec![], cwd: RunIn::Spec, timeout_secs: None }];
        state.config.update(|c| c.toolsets.push(custom)).unwrap();
        let fake = Fake::default();

        let done = install(&state, &task, &fake);

        assert!(!done.ok);
        assert!(fake.ran.borrow().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_fetch_needs_a_pinned_version_and_skills_need_a_fetch() {
        let (root, state, task) = world(&["api"], "superpowers");
        let mut custom = toolset::find("superpowers", &[]).unwrap();
        custom.version = None;
        state.config.update(|c| c.toolsets.push(custom)).unwrap();
        let fake = superpowers_cache();
        let done = install(&state, &task, &fake);
        assert!(done.error.unwrap().contains("version"));
        assert!(fake.fetched.borrow().is_empty());

        let mut custom = toolset::find("superpowers", &[]).unwrap();
        custom.install = vec![InstallStep::Skills { skills: "skills/*".into() }];
        state.config.update(|c| c.toolsets = vec![custom]).unwrap();
        assert!(install(&state, &task, &fake).error.unwrap().contains("fetch"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_event_carries_the_task_and_its_state_in_camel_case() {
        let state = InstallState {
            toolset: "openspec".into(),
            version: Some("1.14.3".into()),
            ok: true,
            error: None,
            running: false,
        };
        let json = serde_json::to_value(Announce { task_id: "t1", state: &state }).unwrap();
        assert_eq!(json["taskId"], "t1");
        assert_eq!(json["state"]["toolset"], "openspec");
        assert_eq!(json["state"]["running"], false);
    }
}
