//! Each worktree's `git status`, asked several at a time, and how long a
//! task's may be reused before git is asked again (TASK-10).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

use crate::git;
use crate::pty::{Activity, PaneInfo, PaneKind};

/// How long a cold checkout may keep its last status before we ask git
/// again, while its task has a pane running: a shell, or an agent between
/// turns, can still change it.
const COLD_STATUS_TTL: Duration = Duration::from_secs(90);

/// The same for a task with nothing running at all. Nothing the app runs is
/// changing it: what is left is the user's own editor, and selecting the
/// task asks at once. At 90 seconds every worktree of every task kept, done
/// ones included, was a `git status` a minute and a half, for as long as
/// the app was open.
const QUIET_STATUS_TTL: Duration = Duration::from_secs(5 * 60);

/// How many worktrees to ask git about at once.
///
/// One `git status` after another is the whole cost of a cold `list_tasks`,
/// and at launch nothing is cached — so it is every worktree the user has ever
/// opened, in series, before the sidebar can draw. They are independent
/// processes mostly waiting on the disk, so they overlap well. The cap is
/// there because each one is a real fork: somebody with forty worktrees
/// should not start forty at once.
const STATUS_FANOUT: usize = 8;

/// What `git status` says about one worktree, that there is no worktree, or
/// why git cannot read the one that is there.
#[derive(Clone, Default)]
pub(super) struct Fresh {
    pub status: Option<git::WorktreeStatus>,
    pub changed: u32,
    pub exists: bool,
    pub broken: Option<String>,
}

fn fresh_status(dir: &Path) -> Fresh {
    if !dir.is_dir() {
        return Fresh::default();
    }
    if let Some(why) = git::unlinked(dir) {
        return Fresh { exists: true, broken: Some(why), ..Fresh::default() };
    }
    match git::status(dir) {
        // One `git status` already enumerated every dirty path. A second
        // `diff`/`ls-files` pass per checkout on every poll is what made the
        // app feel busy just for sitting open.
        Ok(s) => Fresh { changed: s.dirty_files, status: Some(s), exists: true, broken: None },
        Err(e) => Fresh { exists: true, broken: Some(e.to_string()), ..Fresh::default() },
    }
}

/// `fresh_status` for several worktrees, in the same order as the input.
///
/// Order matters more than it looks: the results are matched back to their
/// checkouts by position, so a worker that panics contributes placeholders
/// rather than a shorter list that would shift every status after it onto the
/// wrong repository.
pub(super) fn fresh_statuses(dirs: &[PathBuf]) -> Vec<Fresh> {
    if dirs.len() < 2 {
        return dirs.iter().map(|d| fresh_status(d)).collect();
    }
    let per = dirs.len().div_ceil(dirs.len().min(STATUS_FANOUT));
    std::thread::scope(|scope| {
        let handles: Vec<_> = dirs
            .chunks(per)
            .map(|chunk| {
                (
                    chunk.len(),
                    scope.spawn(move || chunk.iter().map(|d| fresh_status(d)).collect::<Vec<_>>()),
                )
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|(len, h)| h.join().unwrap_or_else(|_| vec![Fresh::default(); len]))
            .collect()
    })
}

/// How long after an agent last changed what it was doing its task stays
/// fresh: the edits of a turn's last seconds land after the poll before it
/// ended. Longer than the slowest poll (60 s, off the Work view): at 30 s, the
/// first poll after a turn usually found the task cold already, and showed
/// the status from before its last edits for another 90 s.
const SETTLE: TimeDelta = TimeDelta::seconds(75);

/// The tasks whose worktrees may be changing under the user right now: an
/// agent working, or one that stopped working a moment ago.
///
/// Any running pane used to count. A shell left open, or an agent finished
/// hours ago, kept its task re-running `git status` in every worktree on
/// every poll, and panes pile up over a session: the longer the app ran, the
/// more git it ran for nothing.
pub(super) fn busy_tasks(panes: &[PaneInfo], now: DateTime<Utc>) -> HashSet<&str> {
    panes
        .iter()
        .filter(|p| p.running && p.kind == PaneKind::Agent)
        .filter(|p| p.activity == Activity::Working || now - p.activity_since < SETTLE)
        .map(|p| p.task_id.as_str())
        .collect()
}

/// How long a task's checkouts may keep their last status (TASK-10): not at
/// all for the selected task or one an agent is changing, 90 seconds while
/// anything else runs in it, and 5 minutes once nothing does.
pub(super) fn status_ttl(task: &str, focus: Option<&str>, busy: &HashSet<&str>, running: &HashSet<&str>) -> Duration {
    if focus == Some(task) || busy.contains(task) {
        Duration::ZERO
    } else if running.contains(task) {
        COLD_STATUS_TTL
    } else {
        QUIET_STATUS_TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(task: &str, kind: PaneKind, activity: Activity, quiet_secs: i64) -> PaneInfo {
        let now = Utc::now();
        PaneInfo {
            id: format!("{task}-pane"),
            task_id: task.into(),
            checkout_id: None,
            kind,
            title: "pane".into(),
            agent_id: (kind == PaneKind::Agent).then(|| "claude".into()),
            cwd: "/tmp".into(),
            running: true,
            exit_code: None,
            started_at: now - TimeDelta::hours(3),
            last_output_at: now,
            notice: None,
            activity,
            activity_since: now - TimeDelta::seconds(quiet_secs),
            topic: None,
            acp: false,
            loop_said: None,
        }
    }

    #[test]
    fn only_tasks_whose_agents_are_at_work_rerun_git_status_every_poll() {
        use Activity::{Done, Idle, Working};
        let panes = vec![
            pane("open-shell", PaneKind::Shell, Idle, 7200),
            pane("idle-agent", PaneKind::Agent, Idle, 7200),
            pane("long-done", PaneKind::Agent, Done, 600),
            pane("working", PaneKind::Agent, Working, 600),
            pane("just-done", PaneKind::Agent, Done, 5),
        ];

        let busy = busy_tasks(&panes, Utc::now());

        let mut busy: Vec<&str> = busy.into_iter().collect();
        busy.sort();
        assert_eq!(busy, ["just-done", "working"]);
    }

    #[test]
    fn a_task_with_nothing_running_reuses_its_status_longest() {
        let busy: HashSet<&str> = ["working"].into();
        let running: HashSet<&str> = ["working", "open-shell"].into();
        let ttl = |task| status_ttl(task, Some("selected"), &busy, &running);
        assert_eq!(ttl("selected"), Duration::ZERO, "the task on screen is always asked");
        assert_eq!(ttl("working"), Duration::ZERO);
        assert_eq!(ttl("open-shell"), COLD_STATUS_TTL);
        assert_eq!(ttl("finished"), QUIET_STATUS_TTL);
    }
}
