//! A task's spec (§19): one per task, whatever number of repositories it
//! touches. Its requirements and design are the task's, and each repository
//! has its own tasks. Every repository gets a copy, committed on the task's
//! branch once approved.
//!
//! A spec used to be `SPEC.md` in the task folder: deleted with the task,
//! and never checked against the work it described. In the repository it
//! goes through review with the code and outlives the task, the branch and
//! the app. Then each repository had a spec of its own, and they drifted: a
//! requirement two repositories met was written in both and edited in one,
//! and each design described the endpoint between them in its own words.
//!
//! What was approved is kept in the task folder (`.spec-drafts/approved/`)
//! and written from there into every repository's spec folder. A copy that
//! is not what was approved was changed outside the app, by an agent, an
//! editor or a merge, and is shown as such (SPEC-22). Drafts, which file is
//! out of date, and the last checks are kept beside it, where no agent is
//! told to look (SPEC-6).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{Checkout, Project, Task};
use crate::error::{Error, Result};
use crate::git;
use crate::spec::{self, Kind, Part, Requirement, Step};

use super::panes::agent_file_dir;
use super::spec_view::{view, TaskSpec};
use super::task_context::write_task_context;
use super::AppState;

/// Where a spec was kept before it was kept in the repository (SPEC-17).
pub(crate) const LEGACY_FILE: &str = "SPEC.md";

/// The task folder's drafts and what was approved (SPEC-2, SPEC-6).
pub(crate) const DRAFTS: &str = ".spec-drafts";

/// Under `DRAFTS`: the spec as it was approved, which every repository's
/// copy is compared against (SPEC-22).
const APPROVED: &str = "approved";

/// One file of the spec: the task's requirements or design, or one
/// repository's tasks, by its folder in the task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub part: Part,
    #[serde(default)]
    pub repo: Option<String>,
}

impl Slot {
    pub fn shared(part: Part) -> Slot {
        Slot { part, repo: None }
    }

    pub fn tasks(folder: &str) -> Slot {
        Slot { part: Part::Tasks, repo: Some(folder.to_string()) }
    }
}

/// What the app keeps about a task's spec beside its drafts.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(super) struct Notes {
    #[serde(default)]
    kind: Kind,
    /// Approved files an earlier one was approved since (SPEC-7).
    #[serde(default)]
    pub stale: Vec<Slot>,
    /// Changes agents proposed, waiting for the user (SPEC-24).
    #[serde(default)]
    pub proposals: Vec<Proposal>,
}

/// A change an agent proposed with `spec_propose` (SPEC-24). Its text is
/// the slot's draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub slot: Slot,
    pub why: String,
    /// The agent's name, and the pane it is in, to tell it the answer.
    pub by: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    pub at: i64,
}

/// What the app keeps about one repository's share: its last check.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct RepoNotes {
    #[serde(default)]
    check: Option<Check>,
}

/// The last check against the spec (SPEC-15).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    /// The commit it was checked at.
    pub sha: String,
    pub at: i64,
    pub results: Vec<Verdict>,
}

/// One requirement's answer: `met`, `not met` or `unclear`, and what shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub id: String,
    pub verdict: String,
    pub evidence: String,
}

/// Where one repository's copy of the spec lives, for one task.
pub(crate) struct Home {
    pub checkout: Checkout,
    pub project: Project,
    /// The checkout's folder in the task: `api`.
    pub folder: String,
    /// Where its copy is.
    pub dir: PathBuf,
    /// The spec folder relative to the worktree, when it is committed there.
    pub rel: Option<String>,
}

impl Home {
    /// The spec folder as an agent or a person finds it.
    pub fn shown(&self) -> String {
        self.dir.display().to_string()
    }

    pub(super) fn copy(&self, part: Part, kind: Kind) -> Option<String> {
        std::fs::read_to_string(self.dir.join(part.file(kind))).ok()
    }
}

/// A task's spec: where it is drafted and recorded, and every repository's
/// copy.
pub(crate) struct Spec {
    /// The task folder's `.spec-drafts`: None for a task with no folder of
    /// its own, whose one repository's copy is all there is.
    pub drafts: Option<PathBuf>,
    pub homes: Vec<Home>,
}

impl Spec {
    pub(super) fn notes(&self) -> Notes {
        self.drafts
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.join("state.json")).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn keep(&self, notes: &Notes) -> Result<()> {
        write_json(self.drafts.as_ref().ok_or_else(no_folder)?, notes)
    }

    fn repo_notes(&self, home: &Home) -> RepoNotes {
        self.drafts
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.join(&home.folder).join("state.json")).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn set_check(&self, home: &Home, check: Check) -> Result<()> {
        let mut notes = self.repo_notes(home);
        notes.check = Some(check);
        write_json(&self.drafts.as_ref().ok_or_else(no_folder)?.join(&home.folder), &notes)
    }

    /// Feature or bug: which requirements file was approved, or else what
    /// was chosen when the spec was started.
    pub fn kind(&self) -> Kind {
        let has = |dir: &Path, kind: Kind| dir.join(Part::Requirements.file(kind)).is_file();
        let mut dirs: Vec<PathBuf> = self.drafts.iter().map(|d| d.join(APPROVED)).collect();
        dirs.extend(self.homes.iter().map(|h| h.dir.clone()));
        for dir in dirs {
            if has(&dir, Kind::Bugfix) {
                return Kind::Bugfix;
            }
            if has(&dir, Kind::Feature) {
                return Kind::Feature;
            }
        }
        self.notes().kind
    }

    pub fn set_kind(&self, kind: Kind) -> Result<()> {
        let mut notes = self.notes();
        notes.kind = kind;
        self.keep(&notes)
    }

    pub fn home(&self, folder: &str) -> Option<&Home> {
        self.homes.iter().find(|h| h.folder == folder)
    }

    /// Every file the spec has: requirements, design, each repository's tasks.
    pub fn slots(&self) -> Vec<Slot> {
        let mut all = vec![Slot::shared(Part::Requirements), Slot::shared(Part::Design)];
        all.extend(self.homes.iter().map(|h| Slot::tasks(&h.folder)));
        all
    }

    /// Where a slot's file is under `base`: drafts go by the part's plain
    /// name, since whether it is a bug is decided when it is approved.
    fn under(base: &Path, slot: &Slot, kind: Kind) -> PathBuf {
        match &slot.repo {
            Some(folder) => base.join(folder).join(Part::Tasks.file(kind)),
            None => base.join(slot.part.file(kind)),
        }
    }

    pub fn draft(&self, slot: &Slot) -> Option<String> {
        let text = std::fs::read_to_string(Self::under(self.drafts.as_ref()?, slot, Kind::Feature)).ok()?;
        (!text.trim().is_empty()).then_some(text)
    }

    pub fn save_draft(&self, slot: &Slot, text: Option<&str>) -> Result<()> {
        let path = Self::under(self.drafts.as_ref().ok_or_else(no_folder)?, slot, Kind::Feature);
        match text.filter(|t| !t.trim().is_empty()) {
            Some(text) => write(&path, text),
            None => remove(&path),
        }
    }

    /// What was approved, as the app recorded it.
    pub(super) fn record(&self, slot: &Slot, kind: Kind) -> Option<String> {
        let text = std::fs::read_to_string(Self::under(&self.drafts.as_ref()?.join(APPROVED), slot, kind)).ok()?;
        (!text.trim().is_empty()).then_some(text)
    }

    /// The text a repository's copy holds, for `slot` (a task's requirements
    /// and design are in every repository; tasks in their own).
    fn copies(&self, slot: &Slot, kind: Kind) -> Vec<(&Home, Option<String>)> {
        self.homes
            .iter()
            .filter(|h| slot.repo.as_ref().is_none_or(|r| *r == h.folder))
            .map(|h| (h, h.copy(slot.part, kind).filter(|t| !t.trim().is_empty())))
            .collect()
    }

    /// An approved file's text: what was recorded, or with no record (a
    /// spec from before, or found on the branch, SPEC-9) the first copy.
    /// Tasks are read from the repository's copy, where agents tick them,
    /// while it differs from the record in its ticks alone.
    pub fn approved(&self, slot: &Slot) -> Option<String> {
        let kind = self.kind();
        let record = self.record(slot, kind);
        let copies = self.copies(slot, kind);
        match (slot.part, record) {
            (Part::Tasks, Some(record)) => match copies.into_iter().next().and_then(|(_, c)| c) {
                Some(copy) if spec::same_text(Part::Tasks, &record, &copy) => Some(copy),
                _ => Some(record),
            },
            (_, Some(record)) => Some(record),
            (_, None) => copies.into_iter().find_map(|(_, c)| c),
        }
    }

    /// What a later file is drafted from: the draft where there is one, else
    /// what was approved.
    pub fn latest(&self, slot: &Slot) -> Option<String> {
        self.draft(slot).or_else(|| self.approved(slot))
    }

    /// The copies that are not what was approved (SPEC-22), with what they
    /// hold ("" for a copy that is gone). None without a record to compare.
    pub(super) fn outside(&self, slot: &Slot, kind: Kind) -> Vec<(&Home, String)> {
        let Some(record) = self.record(slot, kind) else { return Vec::new() };
        self.copies(slot, kind)
            .into_iter()
            .filter_map(|(h, copy)| {
                let copy = copy.unwrap_or_default();
                (!spec::same_text(slot.part, &record, &copy)).then_some((h, copy))
            })
            .collect()
    }

    /// Whether any copy of `slot` was changed outside the app (SPEC-22).
    pub fn has_outside(&self, slot: &Slot) -> bool {
        !self.outside(slot, self.kind()).is_empty()
    }

    /// Repositories with specs of their own from before (SPEC-25): nothing
    /// recorded, and their requirements differ.
    pub(super) fn split(&self) -> bool {
        let kind = self.kind();
        let slot = Slot::shared(Part::Requirements);
        if self.record(&slot, kind).is_some() {
            return false;
        }
        let texts: Vec<String> = self.copies(&slot, kind).into_iter().filter_map(|(_, c)| c).map(|c| c.trim().to_string()).collect();
        texts.len() > 1 && texts.iter().any(|t| *t != texts[0])
    }

    pub fn requirements(&self) -> Vec<Requirement> {
        self.approved(&Slot::shared(Part::Requirements)).map(|t| spec::requirements(&t)).unwrap_or_default()
    }

    pub fn steps(&self, home: &Home) -> Vec<Step> {
        self.approved(&Slot::tasks(&home.folder)).map(|t| spec::steps(&t)).unwrap_or_default()
    }

    pub fn check(&self, home: &Home) -> Option<Check> {
        self.repo_notes(home).check
    }

    /// Keep a proposal until the user answers it, in place of one before
    /// for the same file.
    pub(super) fn wait_on(&self, proposal: Proposal) -> Result<()> {
        let mut notes = self.notes();
        notes.proposals.retain(|p| p.slot != proposal.slot);
        notes.proposals.push(proposal);
        self.keep(&notes)
    }

    /// The pane a waiting proposal came from, and forget the proposal.
    pub(super) fn settle(&self, slot: &Slot) -> Result<Option<Proposal>> {
        let mut notes = self.notes();
        let Some(at) = notes.proposals.iter().position(|p| p.slot == *slot) else { return Ok(None) };
        let proposal = notes.proposals.remove(at);
        self.keep(&notes)?;
        Ok(Some(proposal))
    }
}

fn write_json<T: Serialize>(dir: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value).map_err(|e| Error::Other(e.to_string()))?;
    write(&dir.join("state.json"), &json)
}

pub(super) fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::agents::replace_file(path, text.as_bytes(), None)?;
    Ok(())
}

fn no_folder() -> Error {
    Error::Other("this task has no folder of its own to keep spec drafts in".into())
}

/// `task`'s spec: each repository's copy in checkout order.
pub(crate) fn spec_of(state: &AppState, task: &Task) -> Spec {
    let name = spec::folder_name(&task.branch);
    let homes = state
        .config
        .checkouts_of(&task.id)
        .into_iter()
        .filter_map(|checkout| {
            let project = state.config.project(&checkout.project_id).ok()?;
            let folder = Path::new(&checkout.path).file_name()?.to_string_lossy().to_string();
            let (dir, rel) = if project.specs_in_app {
                let kept = state.config.folder().join("specs").join(spec::folder_name(&project.name)).join(&name);
                (kept, None)
            } else {
                let base = project.spec_folder.clone().unwrap_or_else(|| "specs".into());
                let rel = format!("{base}/{name}");
                (Path::new(&checkout.path).join(&rel), Some(rel))
            };
            Some(Home { checkout, project, folder, dir, rel })
        })
        .collect();
    Spec { drafts: agent_file_dir(state, task).map(|d| d.join(DRAFTS)), homes }
}

/// The slot a file the frontend names is: a part, and for tasks the
/// checkout whose they are.
pub(super) fn slot_of(spec: &Spec, part: Part, checkout_id: Option<&str>) -> Result<Slot> {
    if part != Part::Tasks {
        return Ok(Slot::shared(part));
    }
    let id = checkout_id.ok_or_else(|| Error::Other("say whose tasks".into()))?;
    let home = spec.homes.iter().find(|h| h.checkout.id == id).ok_or_else(|| Error::NotFound(format!("checkout {id}")))?;
    Ok(Slot::tasks(&home.folder))
}

/// One file's approved text: requirements and design are the task's, tasks
/// one checkout's.
#[derive(Debug, Deserialize)]
pub struct PartText {
    pub part: Part,
    #[serde(default)]
    pub checkout_id: Option<String>,
    pub text: String,
}

/// What approving did: the spec as it is now, and each repository it could
/// not be written or committed in (SPEC-6).
#[derive(Debug, Serialize)]
pub struct Approved {
    pub spec: TaskSpec,
    pub failed: Vec<RepoFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoFailure {
    pub folder: String,
    pub error: String,
}

/// Approve files of the spec: recorded, written into every repository's
/// copy (tasks into their own) and, in a repository that keeps specs,
/// committed on the task branch with only those files in the commit
/// (SPEC-6). The files after them that were approved before are out of
/// date from now, in every repository (SPEC-7). An agent whose proposal it
/// was is told (SPEC-24). Empty text removes a file.
#[tauri::command]
pub async fn approve_spec(app: tauri::AppHandle, task_id: String, texts: Vec<PartText>, kind: Option<Kind>) -> Result<Approved> {
    super::blocking(app, move |state| {
        let task = state.config.task(&task_id)?;
        let spec = spec_of(state, &task);
        let texts: Vec<(Slot, String)> = texts
            .into_iter()
            .map(|t| Ok((slot_of(&spec, t.part, t.checkout_id.as_deref())?, t.text)))
            .collect::<Result<_>>()?;
        let failed = approve(&spec, &task, &texts, kind)?;
        for (slot, _) in &texts {
            if let Some(p) = spec.settle(slot)? {
                super::spec_outside::tell_proposer(state, &task, &p, "approved: work to it as it is now");
            }
        }
        // The requirements approved: the spec from before has been carried
        // over (SPEC-17).
        if spec.approved(&Slot::shared(Part::Requirements)).is_some() {
            if let Some(dir) = agent_file_dir(state, &task) {
                let _ = std::fs::remove_file(dir.join(LEGACY_FILE));
            }
        }
        write_task_context(state, &task)?;
        Ok(Approved { spec: view(state, &task), failed })
    })
    .await
}

fn approve(spec: &Spec, task: &Task, texts: &[(Slot, String)], kind: Option<Kind>) -> Result<Vec<RepoFailure>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let before = spec.kind();
    let kind = kind.unwrap_or(before);
    let switched = kind != before && texts.iter().any(|(s, _)| s.part == Part::Requirements);
    let body = |text: &str| {
        let text = text.trim();
        (!text.is_empty()).then(|| format!("{text}\n"))
    };

    if let Some(drafts) = &spec.drafts {
        let recorded = drafts.join(APPROVED);
        for (slot, text) in texts {
            let path = Spec::under(&recorded, slot, kind);
            match body(text) {
                Some(text) => write(&path, &text)?,
                None => remove(&path)?,
            }
        }
        // A bug that turned out to be a feature, or the other way round,
        // keeps one requirements file, not both.
        if switched {
            remove(&recorded.join(Part::Requirements.file(before)))?;
        }
    }

    let mut failed = Vec::new();
    for home in &spec.homes {
        let mine: Vec<&(Slot, String)> = texts.iter().filter(|(s, _)| s.repo.as_ref().is_none_or(|r| *r == home.folder)).collect();
        if mine.is_empty() {
            continue;
        }
        if let Err(e) = approve_in(home, task, &mine, kind, switched.then_some(before)) {
            failed.push(RepoFailure { folder: home.folder.clone(), error: e.to_string() });
        }
    }

    let mut notes = spec.notes();
    notes.kind = kind;
    let approved: Vec<&Slot> = texts.iter().map(|(s, _)| s).collect();
    let first = approved.iter().map(|s| s.part.index()).min().unwrap_or(0);
    notes.stale.retain(|s| !approved.contains(&s));
    for slot in spec.slots().into_iter().filter(|s| s.part.index() > first && !approved.contains(&s)) {
        if spec.approved(&slot).is_some() && !notes.stale.contains(&slot) {
            notes.stale.push(slot);
        }
    }
    if spec.drafts.is_some() {
        spec.keep(&notes)?;
        for (slot, _) in texts {
            spec.save_draft(slot, None)?;
        }
    }
    Ok(failed)
}

/// Write one repository's copy of what was approved, and commit it there.
fn approve_in(home: &Home, task: &Task, texts: &[&(Slot, String)], kind: Kind, was: Option<Kind>) -> Result<()> {
    let mut files: Vec<String> = Vec::new();
    for (slot, text) in texts {
        let file = slot.part.file(kind);
        let path = home.dir.join(file);
        let existed = path.exists();
        match text.trim() {
            "" => remove(&path)?,
            text => write(&path, &format!("{text}\n"))?,
        }
        if existed || path.exists() {
            files.push(file.to_string());
        }
    }
    if let Some(was) = was {
        let old = home.dir.join(Part::Requirements.file(was));
        if old.exists() {
            remove(&old)?;
            files.push(Part::Requirements.file(was).to_string());
        }
    }
    let Some(rel) = &home.rel else { return Ok(()) };
    let mut words: Vec<&str> = Vec::new();
    for (slot, _) in texts {
        let word = slot.part.word(kind);
        if !words.contains(&word) {
            words.push(word);
        }
    }
    let message = format!("Approve the {} for {}.", and_list(&words), task.issue_key.as_deref().unwrap_or(&task.branch));
    let paths: Vec<String> = files.iter().map(|f| format!("{rel}/{f}")).collect();
    git::commit_only(Path::new(&home.checkout.path), &paths.iter().map(String::as_str).collect::<Vec<_>>(), &message)?;
    Ok(())
}

fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

pub(crate) fn and_list(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(root: &Path, folder: &str, in_repo: bool) -> Home {
        let worktree = root.join(folder);
        std::fs::create_dir_all(&worktree).unwrap();
        let rel = in_repo.then(|| "specs/ACME-12".to_string());
        let dir = match &rel {
            Some(r) => worktree.join(r),
            None => root.join("kept").join(folder).join("ACME-12"),
        };
        Home {
            checkout: Checkout {
                id: format!("c-{folder}"),
                task_id: "t1".into(),
                project_id: format!("p-{folder}"),
                path: worktree.to_string_lossy().to_string(),
                base: "main".into(),
                base_commit: None,
                push_lease: None,
                point_before_update: None,
                last_head: None,
            },
            project: Project {
                id: format!("p-{folder}"),
                name: folder.into(),
                path: String::new(),
                default_branch: "main".into(),
                group: None,
                store: None,
                update_by: None,
                spec_folder: None,
                specs_in_app: !in_repo,
                remote: None,
                repository_id: None,
                check: None,
            },
            folder: folder.into(),
            dir,
            rel,
        }
    }

    fn spec(root: &Path, folders: &[&str], in_repo: bool) -> Spec {
        Spec { drafts: Some(root.join(DRAFTS)), homes: folders.iter().map(|f| home(root, f, in_repo)).collect() }
    }

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("vl-spec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn task() -> Task {
        Task {
            id: "t1".into(),
            name: "ACME-12 Refunds".into(),
            root: "/work/acme-12".into(),
            branch: "ACME-12".into(),
            issue_key: Some("ACME-12".into()),
            issue_url: None,
            created_at: chrono::Utc::now(),
            ticket_stage: None,
            chat: None,
            review: None,
            browser: None,
            browser_url: None,
        }
    }

    fn git_init(dir: &Path) {
        for args in [&["init", "-q", "-b", "main"][..], &["config", "user.email", "t@villain.local"], &["config", "user.name", "T"], &["commit", "-q", "--allow-empty", "-m", "init"]] {
            git::run_for_tests(dir, args).unwrap();
        }
    }

    const REQS: &str = "## Requirements\n- R-1 [api, web]: WHEN x THE SYSTEM SHALL y.";

    #[test]
    fn the_tasks_requirements_and_design_are_written_alike_into_every_repository_and_tasks_into_their_own() {
        let root = root();
        let s = spec(&root, &["api", "web"], false);
        let texts = vec![
            (Slot::shared(Part::Requirements), REQS.to_string()),
            (Slot::shared(Part::Design), "## Contracts\n- C-1: POST /refunds".to_string()),
            (Slot::tasks("api"), "- [ ] 1. Add the endpoint (R-1, C-1)".to_string()),
            (Slot::tasks("web"), "- [ ] 1. Call it (R-1, C-1; after api 1)".to_string()),
        ];
        assert!(approve(&s, &task(), &texts, None).unwrap().is_empty());
        for h in &s.homes {
            assert_eq!(std::fs::read_to_string(h.dir.join("requirements.md")).unwrap(), format!("{REQS}\n"));
            assert!(std::fs::read_to_string(h.dir.join("design.md")).unwrap().contains("C-1"));
        }
        assert!(s.approved(&Slot::tasks("web")).unwrap().contains("after api 1"));
        assert!(!std::fs::read_to_string(s.homes[0].dir.join("tasks.md")).unwrap().contains("Call it"));
        assert_eq!(s.requirements()[0].repos, vec!["api", "web"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn approving_the_requirements_puts_every_repositorys_design_and_tasks_out_of_date() {
        let root = root();
        let s = spec(&root, &["api", "web"], false);
        let all = vec![
            (Slot::shared(Part::Requirements), REQS.to_string()),
            (Slot::shared(Part::Design), "d".to_string()),
            (Slot::tasks("api"), "- [ ] 1. a".to_string()),
            (Slot::tasks("web"), "- [ ] 1. b".to_string()),
        ];
        approve(&s, &task(), &all, None).unwrap();
        assert!(s.notes().stale.is_empty(), "approved together, nothing is out of date");

        s.save_draft(&Slot::shared(Part::Requirements), Some("## Requirements\n- R-1: z")).unwrap();
        approve(&s, &task(), &[(Slot::shared(Part::Requirements), "## Requirements\n- R-1: z".into())], None).unwrap();
        assert_eq!(s.notes().stale, vec![Slot::shared(Part::Design), Slot::tasks("api"), Slot::tasks("web")]);
        assert!(s.draft(&Slot::shared(Part::Requirements)).is_none(), "an approved draft is let go");

        approve(&s, &task(), &[(Slot::tasks("api"), "- [ ] 1. a2".into())], None).unwrap();
        assert_eq!(s.notes().stale, vec![Slot::shared(Part::Design), Slot::tasks("web")], "one repository's tasks leave the other's out of date");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_copy_an_agent_edited_is_seen_as_changed_outside_the_app_and_its_ticks_are_not() {
        let root = root();
        let s = spec(&root, &["api", "web"], false);
        approve(&s, &task(), &[(Slot::shared(Part::Requirements), REQS.into()), (Slot::tasks("api"), "- [ ] 1. a\n- [ ] 2. b".into())], None).unwrap();
        let kind = s.kind();

        let tasks = s.homes[0].dir.join("tasks.md");
        std::fs::write(&tasks, spec::tick(&std::fs::read_to_string(&tasks).unwrap(), 1, true).unwrap()).unwrap();
        assert!(s.outside(&Slot::tasks("api"), kind).is_empty(), "a tick is no change");
        assert!(s.approved(&Slot::tasks("api")).unwrap().contains("- [x] 1. a"), "the tab shows the tick");

        std::fs::write(s.homes[1].dir.join("requirements.md"), "## Requirements\n- R-1: something nobody agreed").unwrap();
        let changed = s.outside(&Slot::shared(Part::Requirements), kind);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0.folder, "web");
        assert_eq!(s.approved(&Slot::shared(Part::Requirements)).unwrap().trim(), REQS, "what was approved still stands");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn repositories_whose_requirements_differ_with_nothing_recorded_have_specs_from_before() {
        let root = root();
        let s = spec(&root, &["api", "web"], false);
        for (h, text) in s.homes.iter().zip(["## Requirements\n- R-1: api's", "## Requirements\n- R-1: web's"]) {
            std::fs::create_dir_all(&h.dir).unwrap();
            std::fs::write(h.dir.join("requirements.md"), text).unwrap();
        }
        assert!(s.split());
        approve(&s, &task(), &[(Slot::shared(Part::Requirements), REQS.into())], None).unwrap();
        assert!(!s.split(), "merged and approved, it is one spec");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_bug_keeps_its_requirements_in_bugfix_md_and_only_there() {
        let root = root();
        let s = spec(&root, &["api"], false);
        approve(&s, &task(), &[(Slot::shared(Part::Requirements), "## Requirements\n- R-1: x".into())], None).unwrap();
        approve(&s, &task(), &[(Slot::shared(Part::Requirements), "## Expected behaviour\n- R-1: x".into())], Some(Kind::Bugfix)).unwrap();
        assert!(s.homes[0].dir.join("bugfix.md").is_file());
        assert!(!s.homes[0].dir.join("requirements.md").exists());
        assert!(!root.join(DRAFTS).join(APPROVED).join("requirements.md").exists());
        assert_eq!(s.kind(), Kind::Bugfix);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn approving_commits_only_the_files_approved_in_each_repository() {
        let root = root();
        let s = spec(&root, &["api", "web"], true);
        for h in &s.homes {
            git_init(Path::new(&h.checkout.path));
        }
        approve(&s, &task(), &[(Slot::shared(Part::Requirements), REQS.into())], None).unwrap();
        // An agent edits the design it was never given; approving the
        // requirements again must not commit that.
        std::fs::write(s.homes[0].dir.join("design.md"), "an agent's design").unwrap();
        approve(&s, &task(), &[(Slot::shared(Part::Requirements), format!("{REQS}\n- R-2: more"))], None).unwrap();
        for h in &s.homes {
            let log = git::run_for_tests(Path::new(&h.checkout.path), &["log", "--format=%s", "--name-only", "-1"]).unwrap();
            assert_eq!(log.trim(), "Approve the requirements for ACME-12.\n\nspecs/ACME-12/requirements.md", "{}", h.folder);
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_repository_that_cannot_commit_is_named_and_the_others_keep_theirs() {
        let root = root();
        let s = spec(&root, &["api", "web"], true);
        git_init(Path::new(&s.homes[0].checkout.path));
        // `web` is no repository at all.
        let failed = approve(&s, &task(), &[(Slot::shared(Part::Requirements), REQS.into())], None).unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].folder, "web");
        let log = git::run_for_tests(Path::new(&s.homes[0].checkout.path), &["log", "--format=%s", "-1"]).unwrap();
        assert_eq!(log.trim(), "Approve the requirements for ACME-12.");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn commit_messages_name_every_file_approved() {
        assert_eq!(and_list(&["requirements", "design", "tasks"]), "requirements, design and tasks");
        assert_eq!(and_list(&["design"]), "design");
    }
}
