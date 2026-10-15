import { describe, expect, test } from "bun:test";
import { byRepo, periodStart, sortRows, taskRows, usageDays, usageStats, type LiveTask } from "../src/lib/usage";
import type { Run, TaskDone } from "../src/lib/types-runs";

const now = new Date(2026, 9, 9, 15, 0).getTime();
const daysAgo = (d: number, minutes = 0) => new Date(now - d * 86_400_000 - minutes * 60_000).toISOString();
const api = { id: "api", name: "ACME API" };
const web = { id: "web", name: "ACME Web" };
const tokens = (n: number) => ({ input: n, output: 0, cached_read: 0, cached_write: 0 });

function run(over: Partial<Run> = {}): Run {
  return {
    id: "p",
    agent: "claude",
    task_id: "t1",
    task: "Fix the login",
    repos: [api],
    acp: false,
    started_at: daysAgo(0, 10),
    ended_at: daysAgo(0),
    end: "stopped",
    code: null,
    loop: null,
    tokens: null,
    turns: 1,
    asks: 0,
    waited_secs: 0,
    worked_secs: null,
    limited: false,
    tools: null,
    ...over,
  };
}

function done(over: Partial<TaskDone> = {}): TaskDone {
  return {
    id: "t1",
    name: "Fix the login",
    issue_key: "ACME-1",
    repos: [api],
    created_at: daysAgo(3),
    ended_at: daysAgo(1),
    finished: true,
    ...over,
  };
}

function live(over: Partial<LiveTask> = {}): LiveTask {
  return { id: "t2", name: "Audit log page", issue_key: "ACME-2", created_at: daysAgo(2), repos: [web], ...over };
}

const week = { since: periodStart(7, now), now, repo: null, byRuns: false };

describe("a period", () => {
  test("starts at midnight, today counting as its first day", () => {
    expect(new Date(periodStart(1, now))).toEqual(new Date(2026, 9, 9));
    expect(new Date(periodStart(7, now))).toEqual(new Date(2026, 9, 3));
  });
});

describe("a task's row (RUN-9)", () => {
  test("says how long a finished task took, start to finish, and what its agents did", () => {
    const [row] = taskRows(
      [run({ worked_secs: 1200, tokens: tokens(500), asks: 2, waited_secs: 60 }), run({ worked_secs: 300, tokens: tokens(100) })],
      [done()],
      [],
      week,
    );
    expect(row).toMatchObject({ name: "Fix the login", key: "ACME-1", state: "finished", tookMs: 2 * 86_400_000 });
    expect([row.runs, row.workedSecs, row.tokens, row.asks, row.waitedSecs]).toEqual([2, 1500, 600, 2, 60]);
  });

  test("says how long an open task has been open, and leaves out one started before the period with no work in it", () => {
    const rows = taskRows([], [], [live(), live({ id: "old", created_at: daysAgo(30) })], week);
    expect(rows.map((r) => [r.id, r.state, r.tookMs])).toEqual([["t2", "open", 2 * 86_400_000]]);
  });

  test("keeps an old open task that was worked on in the period", () => {
    const rows = taskRows([run({ task_id: "old" })], [], [live({ id: "old", created_at: daysAgo(30) })], week);
    expect(rows.map((r) => [r.id, r.state])).toEqual([["old", "open"]]);
  });

  test("tells a deleted task from a finished one, and leaves out one that ended before the period", () => {
    const rows = taskRows([], [done({ id: "d", finished: false }), done({ id: "gone", ended_at: daysAgo(20) })], [], week);
    expect(rows.map((r) => [r.id, r.state])).toEqual([["d", "deleted"]]);
  });

  test("of a task a run in the period was for is named by its record, though it ended before", () => {
    const [row] = taskRows([run()], [done({ ended_at: daysAgo(20), created_at: daysAgo(25) })], [], week);
    expect(row).toMatchObject({ state: "finished", key: "ACME-1", tookMs: 5 * 86_400_000 });
  });

  test("of a task removed before tasks were kept is its runs alone, with no length", () => {
    const [row] = taskRows([run({ task_id: "x", task: "" })], [], [], week);
    expect(row).toMatchObject({ name: "A task since removed", state: "removed", tookMs: null, runs: 1 });
  });

  test("puts every chat in one row", () => {
    const rows = taskRows([run({ task_id: "chat", task: "Chat", repos: [] }), run({ task_id: "chat", task: "Chat", repos: [] })], [], [], week);
    expect(rows.map((r) => [r.name, r.state, r.runs])).toEqual([["Chats", "chat", 2]]);
  });

  test("with a repository chosen, leaves out tasks in other repositories", () => {
    const rows = taskRows([], [done()], [live()], { ...week, repo: "web" });
    expect(rows.map((r) => r.id)).toEqual(["t2"]);
  });

  test("with an agent chosen, keeps only tasks one of its runs was for", () => {
    const rows = taskRows([run({ task_id: "t1" })], [done(), done({ id: "t3" })], [live()], { ...week, byRuns: true });
    expect(rows.map((r) => r.id)).toEqual(["t1"]);
  });
});

describe("sorting the tasks", () => {
  const rows = taskRows(
    [run({ task_id: "t1", tokens: tokens(10) }), run({ task_id: "x", task: "Zed", tokens: tokens(99) })],
    [done()],
    [live({ id: "t2" })],
    week,
  );

  test("puts the biggest first", () => {
    expect(sortRows(rows, "tokens").map((r) => r.id)).toEqual(["x", "t1", "t2"]);
  });

  test("puts a length that is not known last", () => {
    expect(sortRows(rows, "took").map((r) => r.id)).toEqual(["t1", "t2", "x"]);
  });

  test("by name, A to Z", () => {
    expect(sortRows(rows, "name").map((r) => r.name)).toEqual(["Audit log page", "Fix the login", "Zed"]);
  });
});

describe("the tiles", () => {
  test("add up tokens and working time over the runs that said, and give the typical task's length", () => {
    const runs = [run({ tokens: tokens(100), worked_secs: 60 }), run({ worked_secs: null }), run({ tokens: tokens(50) })];
    const rows = taskRows(runs, [done(), done({ id: "b", created_at: daysAgo(5) }), done({ id: "c", finished: false })], [live()], week);
    const s = usageStats(runs, rows);
    expect([s.tokens, s.withTokens, s.workedSecs, s.withWorked, s.runs]).toEqual([150, 2, 60, 1, 3]);
    expect([s.finished, s.deleted, s.open]).toEqual([2, 1, 1]);
    expect(s.leadMedianMs).toBe(2 * 86_400_000);
    expect(s.leadP90Ms).toBe(4 * 86_400_000);
  });

  test("of nothing are nothing, not NaN", () => {
    expect(usageStats([], [])).toMatchObject({ tokens: 0, workedSecs: 0, finished: 0, leadMedianMs: 0 });
  });
});

describe("the chart", () => {
  test("has a bar for each day up to today, empty ones too, with tokens, working time and tasks finished", () => {
    const rows = taskRows([], [done({ ended_at: daysAgo(0) })], [], week);
    const days = usageDays(
      [run({ tokens: tokens(5), worked_secs: 30 }), run({ ended_at: daysAgo(2), worked_secs: 10 }), run({ ended_at: daysAgo(20), tokens: tokens(9) })],
      rows,
      7,
      now,
    );
    expect(days).toHaveLength(7);
    expect(days[6]).toEqual({ day: "2026-10-09", tokens: 5, workedSecs: 30, finished: 1 });
    expect(days[4]).toEqual({ day: "2026-10-07", tokens: 0, workedSecs: 10, finished: 0 });
    expect(days.reduce((n, d) => n + d.tokens, 0)).toBe(5);
  });
});

describe("the repositories", () => {
  test("count a run at a task's root in full for each, and the tasks in each, the most tokens first", () => {
    const runs = [run({ repos: [api, web], tokens: tokens(10), worked_secs: 60 }), run({ task_id: "t2", repos: [web], tokens: tokens(5) })];
    const rows = taskRows(runs, [done({ repos: [api, web] })], [live()], week);
    expect(byRepo(runs, rows).map((r) => [r.name, r.tokens, r.workedSecs, r.tasks, r.finished])).toEqual([
      ["ACME Web", 15, 60, 2, 1],
      ["ACME API", 10, 60, 1, 1],
    ]);
  });
});
