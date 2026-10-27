/**
 * Epics (§26) for the mock harness: ACME-100 Sign-in overhaul, whose
 * approved spec makes four sub-tasks. One merged, one asking you with
 * changes requested, one waiting for it, and one not filed yet, so both the
 * filing lead and every row's state are on screen. `epics.rs` is the real
 * thing; this answers from a fixed plan.
 */
import type { CheckoutView, EpicView, PaneInfo, SubTaskView, TaskPrs, TaskView } from "../lib/types";
import { ago, pane, type World } from "./world";

type Args = Record<string, unknown>;

const EPIC = "t-epic";
const ROOT = "/Users/you/.villain-worktrees";
const clean = { ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, conflicted: 0, dirty_files: 0 };

/** What the epic's approved tasks group their steps into. */
const PLAN: Omit<SubTaskView, "task_id">[] = [
  { title: "Lock session refresh", key: "ACME-160", existing: false, repos: ["api"], serves: ["R-1"], steps: 2, done: 2, waits_for: [] },
  { title: "Retry a lost race, then 409", key: "ACME-161", existing: false, repos: ["api"], serves: ["R-2", "R-3"], steps: 3, done: 2, waits_for: ["Lock session refresh"] },
  { title: "Say so on the sign-in form", key: "ACME-162", existing: false, repos: ["web"], serves: ["R-3", "R-4"], steps: 3, done: 0, waits_for: ["Retry a lost race, then 409"] },
  { title: "Rate-limit password resets", key: null, existing: false, repos: ["api", "web"], serves: ["R-5"], steps: 4, done: 0, waits_for: [] },
];

function checkout(task: string, key: string, repo: string): CheckoutView {
  return {
    id: `c-${task}-${repo}`, task_id: task, project_id: `p-${repo}`, project_name: repo,
    path: `${ROOT}/${key}/${repo}`, base: "main", exists: true, broken: null, changed: 0,
    status: { ...clean, branch: key },
  };
}

function task(id: string, key: string, name: string, repos: string[], extra: Partial<TaskView> = {}): TaskView {
  return {
    id, name: `${key} ${name}`, root: `${ROOT}/${key}`, branch: key, issue_key: key,
    issue_url: `https://acme.atlassian.net/browse/${key}`, created_at: ago(2 * 86400), pane_count: 0,
    checkouts: repos.map((r) => checkout(id, key, r)), ...extra,
  };
}

const subTask = (id: string, i: number) =>
  task(id, PLAN[i].key!, PLAN[i].title, PLAN[i].repos, { sub_task: { epic: EPIC, title: PLAN[i].title } });

function prs(taskId: string, repo: string, number: number, merged: boolean, verdict: string): TaskPrs {
  return {
    task_id: taskId, ticket: null,
    rows: [{
      checkout_id: `c-${taskId}-${repo}`, repo,
      pr: {
        number, title: "", state: merged ? "closed" : "open", draft: false, author: "you", head: taskId, head_sha: "9a8b7c6",
        base: "main", url: `https://github.com/acme/${repo}/pull/${number}`, mergeable_state: merged ? null : "clean",
        merged, comments: 0, review_comments: merged ? 0 : 2,
      },
      checks: [], reviews: [], past: [], verdict: verdict as TaskPrs["rows"][number]["verdict"], base: "main", changed: 0, error: null,
    }],
  };
}

/** Puts the epic and its filed sub-tasks into a world that has tasks. */
export function seedEpic(world: World) {
  if (world.tasks.length === 0) return;
  world.tasks.push(
    task(EPIC, "ACME-100", "Sign-in overhaul", ["api", "web"], { epic: true, created_at: ago(3 * 86400) }),
    subTask("t-lock", 0),
    subTask("t-retry", 1),
    subTask("t-form", 2),
  );
  world.prs.push(prs("t-lock", "api", 51, true, "approved"), prs("t-retry", "api", 53, false, "changes_requested"));
  world.panes.push(pane("pane-retry", "t-retry", "agent", "Claude Code · api", "claude", "asking", 90, { topic: "Retry a lost race" }));
  world.output["pane-retry"] = "Ana asked for a test of the retry path. Add it to session.test.ts, or file it apart?\r\n";
  world.issues.push(
    { ...world.issues[0], key: "ACME-100", summary: "Sign-in overhaul", issue_type: "Epic", epic_key: null, epic_summary: null, url: "https://acme.atlassian.net/browse/ACME-100" },
    ...PLAN.filter((p) => p.key).map((p, i) => ({
      ...world.issues[0], key: p.key!, summary: p.title, status: i === 0 ? "Done" : "In Progress", status_category: i === 0 ? "done" : "indeterminate",
      epic_key: "ACME-100", url: `https://acme.atlassian.net/browse/${p.key}`,
    })),
  );
}

export function epicAnswers(world: World, spawn: (a: Args, taskId: string) => PaneInfo) {
  const view = (epicId: string): EpicView => ({
    planned: epicId === EPIC,
    sub_tasks: epicId !== EPIC ? [] : PLAN.map((p) => ({
      ...p,
      task_id: world.tasks.find((t) => t.sub_task?.epic === epicId && t.sub_task.title === p.title)?.id ?? null,
    })),
  });
  return {
    epic_view: (a: Args) => view(a.epicId as string),
    file_sub_tasks: (a: Args) => {
      const epicId = a.epicId as string;
      for (const p of view(epicId).sub_tasks.filter((s) => !s.task_id)) {
        const i = PLAN.findIndex((q) => q.title === p.title);
        PLAN[i] = { ...PLAN[i], key: PLAN[i].key ?? `ACME-${170 + i}` };
        world.tasks.push(subTask(`t-sub-${i}`, i));
      }
      return { epic: view(epicId), failed: [] };
    },
    start_sub_task: (a: Args) => ({ pane: spawn(a, a.taskId as string), moved: "In Progress" }),
  };
}
