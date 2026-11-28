import { describe, expect, test } from "bun:test";
import {
  facts,
  isHenchman,
  ran,
  shortCommand,
  sortTask,
  waiting,
  waitingCount,
} from "../src/lib/lair";
import type { CheckoutPr, PaneInfo, TaskView } from "../src/lib/types";

function pane(over: Partial<PaneInfo> = {}): PaneInfo {
  return {
    id: "p1",
    task_id: "t1",
    checkout_id: null,
    kind: "agent",
    title: "Claude Code",
    agent_id: "claude",
    cwd: "/w/t1",
    running: true,
    exit_code: null,
    started_at: "",
    last_output_at: "",
    notice: null,
    activity: "idle",
    activity_since: "",
    topic: null,
    acp: false,
    ...over,
  };
}

function pr(
  over: { merged?: boolean; closed?: boolean } = {},
  row: Partial<CheckoutPr> = {},
): CheckoutPr {
  return {
    checkout_id: "c",
    repo: "acme/api",
    checks: [],
    checks_state: over.merged || over.closed ? "unavailable" : "complete",
    reviews: [],
    past: [],
    verdict: "none",
    base: "main",
    changed: 1,
    to_reply: 0,
    error: null,
    pr: {
      number: 1,
      title: "",
      state: over.merged || over.closed ? "closed" : "open",
      merged: !!over.merged,
      draft: false,
      author: "",
      head: "",
      head_sha: "",
      base: "main",
      url: "",
      mergeable_state: null,
      comments: 0,
      review_comments: 0,
    },
    ...row,
  };
}

function task(id: string, changed = 0): TaskView {
  return {
    id,
    name: id,
    checkouts: [{ changed, exists: true, broken: false, status: null }],
    pane_count: 0,
  } as unknown as TaskView;
}

const why = (panes: PaneInfo[], rows: CheckoutPr[] = [], changed = 0, subTasks: boolean[] = []) =>
  sortTask(facts(panes, rows, changed, subTasks));

describe("the lair's groups (HEN-5)", () => {
  test("an agent that asks comes before everything a pull request says", () => {
    const red = pr({}, { verdict: "changes_requested" });
    expect(why([pane({ activity: "asking" })], [red])).toEqual({
      group: "needs_you",
      rank: 0,
      why: "its agent asks",
    });
  });

  test("an agent out of budget needs you, even while it shows a question", () => {
    expect(why([pane({ activity: "asking", notice: "usage_limit" })]).rank).toBe(1);
  });

  test("changes asked for need you only while no agent is working on them", () => {
    const red = pr({}, { verdict: "changes_requested" });
    expect(why([], [red]).why).toBe("changes asked for");
    expect(why([pane({ activity: "working" })], [red]).group).toBe("moving");
  });

  test("failed checks need you; a check still running does not", () => {
    const failed = pr(
      {},
      { checks: [{ name: "ci", status: "completed", conclusion: "failure", url: null }] },
    );
    const running = pr(
      {},
      { checks: [{ name: "ci", status: "in_progress", conclusion: null, url: null }] },
    );
    expect(why([], [failed]).why).toBe("checks failed");
    expect(why([], [running]).why).toBe("in review");
  });

  test("a task whose every pull request merged is to finish; a closed one does not count", () => {
    expect(why([], [pr({ merged: true }), pr({ closed: true })]).group).toBe("to_finish");
    expect(why([], [pr({ merged: true }), pr()]).group).toBe("moving");
  });

  test("an epic is to finish only once every sub-task has merged, whatever its own PR says", () => {
    const own = [pr({ merged: true })];
    expect(why([], own, 0, [true, false])).toEqual({
      group: "moving",
      rank: 0,
      why: "its sub-tasks are not all merged",
    });
    expect(why([], own, 0, [true, true]).group).toBe("to_finish");
    expect(why([], [], 0, [true]).group).toBe("to_finish");
  });

  test("a task with no agent, no PR and no change is not started; with changes it is quiet", () => {
    expect(why([]).group).toBe("not_started");
    expect(why([], [], 2)).toEqual({ group: "quiet", rank: 0, why: "nothing running" });
    expect(why([pane({ running: false })]).group).toBe("not_started");
  });

  test("a shell does not count as an agent", () => {
    expect(why([pane({ kind: "shell" })]).group).toBe("not_started");
  });
});

describe("the lair's Needs you (HEN-5)", () => {
  const tasks = [task("a"), task("b"), task("m"), task("c", 3)];
  const prs = { m: [pr({ merged: true })] };
  const panes = [
    pane({ task_id: "a", activity: "done" }),
    pane({ id: "p2", task_id: "b", activity: "asking" }),
    pane({ id: "c1", task_id: "chat", cwd: "/w/_chat/x", activity: "asking", topic: "Why" }),
  ];
  const henchman = pane({ id: "h", task_id: "chat", cwd: "/w/_chat/henchman", activity: "asking" });

  test("questions come first, then a chat that asks, then a finished turn, then merged tasks", () => {
    const w = waiting(tasks, panes, prs);
    expect(w.map((x) => x.id)).toEqual(["b", "c1", "a", "m"]);
    expect(w[1].key).toBe("Chat");
  });

  test("while the Henchman runs, a finished turn is its to handle, not the user's", () => {
    expect(waiting(tasks, [...panes, henchman], prs).map((x) => x.id)).toEqual(["b", "c1", "m"]);
  });

  test("Open goes to the agent that waits, and a merged task is to finish", () => {
    const w = waiting(tasks, panes, prs);
    expect(w[0].pane?.id).toBe("p2");
    expect(w[3]).toMatchObject({ finish: true, why: "merged: finish the task" });
  });

  test("an epic waits for its sub-tasks: no Finish while one is still in review", () => {
    const epic = { ...task("e"), epic: true } as TaskView;
    const sub = { ...task("s"), sub_task: { epic: "e", title: "Sub" } } as TaskView;
    const merged = { e: [pr({ merged: true })], s: [pr({ merged: true })] };
    expect(waiting([epic, sub], [], { ...merged, s: [pr()] }).map((x) => x.id)).toEqual([]);
    expect(waiting([epic, sub], [], merged).map((x) => x.id)).toEqual(["e", "s"]);
  });

  test("the Henchman is not one of its own chats", () => {
    expect(isHenchman(henchman)).toBe(true);
    expect(isHenchman(pane({ task_id: "chat", cwd: "/w/_chat/henchmen" }))).toBe(false);
    expect(waiting([], [henchman], {})).toEqual([]);
  });

  test("the top bar's count follows the panes, though it is kept between changes", () => {
    const one = [task("a")];
    const none = {};
    const asking = [pane({ task_id: "a", activity: "asking" })];
    expect(waitingCount(one, asking, none)).toBe(1);
    expect(waitingCount(one, asking, none)).toBe(1);
    expect(waitingCount(one, [pane({ task_id: "a", activity: "working" })], none)).toBe(0);
  });
});

describe("the At work rows (HEN-6)", () => {
  test("how long a process has run reads at a glance", () => {
    expect(ran(12)).toBe("12s");
    expect(ran(250)).toBe("4m");
    expect(ran(7200)).toBe("2h");
    expect(ran(7500)).toBe("2h 5m");
    expect(ran(3 * 86400 + 5)).toBe("3d");
  });

  test("a path in a command is cut to its last part, and words stay as they are", () => {
    expect(shortCommand("node /w/ACME-123/web/node_modules/.bin/vite --port 5173")).toBe(
      "node …/vite --port 5173",
    );
    expect(shortCommand("cargo watch -x test")).toBe("cargo watch -x test");
    expect(shortCommand("python3 -m http.server 8000")).toBe("python3 -m http.server 8000");
  });
});
