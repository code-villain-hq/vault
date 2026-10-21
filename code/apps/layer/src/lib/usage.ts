/**
 * What the run log adds up to, for the Usage view (§22): per task, from its
 * start to its end (RUN-9), per day and per repository. The runs' own sums
 * are `runs.ts`. Pure, so they are tested on their own.
 */
import { CHAT_TASK_ID } from "./derive";
import { percentile, totalTokens } from "./runs";
import type { Run, TaskDone } from "./types-runs";

/** The spans the view looks back over, in days. */
export const PERIODS = [7, 30, 90] as const;
export type Period = (typeof PERIODS)[number];

/** Local midnight `days - 1` days before `now`: a period of 7 is today and the six before. */
export function periodStart(days: number, now: number): number {
  const d = new Date(now);
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - (days - 1));
  return d.getTime();
}

/** A task still in the app, as the view needs it. */
export interface LiveTask {
  id: string;
  name: string;
  issue_key: string | null;
  created_at: string;
  repos: { id: string; name: string }[];
}

/**
 * Where a task stands. `removed`: gone before tasks were kept as they ended
 * (RUN-9), so only its runs say anything. `chat`: every chat, as one row.
 */
export type TaskState = "open" | "finished" | "deleted" | "removed" | "chat";

export interface TaskRow {
  id: string;
  name: string;
  key: string | null;
  state: TaskState;
  /**
   * How long it took, start to end, or how long it has been open; null when
   * that is not known (a chat, or a task removed before RUN-9).
   */
  tookMs: number | null;
  /** When it ended, or was last worked on: for sorting by "latest". */
  at: number;
  /** Agent working time over the runs that said (RUN-7). */
  workedSecs: number;
  tokens: number;
  runs: number;
  asks: number;
  waitedSecs: number;
  repos: { id: string; name: string }[];
}

/**
 * A row per task the period touched: one that ended in it, one still open
 * that was started or worked on in it, and any other a run in it was for.
 * `runs` are the period's, already filtered; with `byRuns` (an agent filter)
 * only tasks with a run in them are kept.
 */
export function taskRows(
  runs: Run[],
  done: TaskDone[],
  live: LiveTask[],
  opts: { since: number; now: number; repo: string | null; byRuns: boolean },
): TaskRow[] {
  const rows = new Map<string, TaskRow>();
  const row = (id: string, init: () => Omit<TaskRow, "workedSecs" | "tokens" | "runs" | "asks" | "waitedSecs">) => {
    let r = rows.get(id);
    if (!r) {
      r = { ...init(), workedSecs: 0, tokens: 0, runs: 0, asks: 0, waitedSecs: 0 };
      rows.set(id, r);
    }
    return r;
  };
  const inRepo = (repos: { id: string }[]) => !opts.repo || repos.some((p) => p.id === opts.repo);

  // Newest first, so the first run seen for a task is its latest.
  for (const run of runs) {
    const r = row(run.task_id, () => ({
      id: run.task_id,
      name: run.task_id === CHAT_TASK_ID ? "Chats" : run.task || "A task since removed",
      key: null,
      state: run.task_id === CHAT_TASK_ID ? "chat" : "removed",
      tookMs: null,
      at: Date.parse(run.ended_at),
      repos: run.repos,
    }));
    r.runs++;
    r.workedSecs += run.worked_secs ?? 0;
    if (run.tokens) r.tokens += totalTokens(run.tokens);
    r.asks += run.asks;
    r.waitedSecs += run.waited_secs;
  }

  for (const t of done) {
    const ended = Date.parse(t.ended_at);
    // One a run in the period was for is named by its record wherever it ended.
    if (!rows.has(t.id) && (opts.byRuns || ended < opts.since || !inRepo(t.repos))) continue;
    const r = row(t.id, () => ({ id: t.id, name: t.name, key: null, state: "deleted", tookMs: null, at: ended, repos: t.repos }));
    Object.assign(r, {
      name: t.name,
      key: t.issue_key,
      state: t.finished ? "finished" : "deleted",
      tookMs: Math.max(0, ended - Date.parse(t.created_at)),
      at: ended,
      repos: t.repos,
    });
  }

  for (const t of live) {
    const created = Date.parse(t.created_at);
    const worked = rows.get(t.id);
    if (!worked && (opts.byRuns || created < opts.since || !inRepo(t.repos))) continue;
    const r = row(t.id, () => ({ id: t.id, name: t.name, key: null, state: "open", tookMs: null, at: created, repos: t.repos }));
    Object.assign(r, {
      name: t.name,
      key: t.issue_key,
      state: "open",
      tookMs: Math.max(0, opts.now - created),
      at: Math.max(r.at, created),
      repos: t.repos,
    });
  }
  return [...rows.values()];
}

export type SortKey = "name" | "took" | "worked" | "tokens" | "asks" | "at";

/** Biggest first, a name A to Z; what is not known goes last either way. */
export function sortRows(rows: TaskRow[], by: SortKey): TaskRow[] {
  const value = (r: TaskRow): number | null =>
    by === "took" ? r.tookMs : by === "worked" ? r.workedSecs : by === "tokens" ? r.tokens : by === "asks" ? r.asks : r.at;
  return [...rows].sort((a, b) => {
    if (by === "name") return a.name.localeCompare(b.name);
    const x = value(a), y = value(b);
    if (x === null || y === null) return x === null ? (y === null ? 0 : 1) : -1;
    return y - x;
  });
}

/** The tiles at the top of the view. */
export interface UsageStats {
  tokens: number;
  /** Runs that said what they used. */
  withTokens: number;
  workedSecs: number;
  /** Runs that said whether they were working. */
  withWorked: number;
  runs: number;
  asks: number;
  waitedSecs: number;
  finished: number;
  deleted: number;
  open: number;
  /** Start to finish, of the tasks finished in the period: the typical one and the slowest tenth. */
  leadMedianMs: number;
  leadP90Ms: number;
}

export function usageStats(runs: Run[], rows: TaskRow[]): UsageStats {
  let tokens = 0, withTokens = 0, workedSecs = 0, withWorked = 0, asks = 0, waitedSecs = 0;
  for (const r of runs) {
    if (r.tokens) {
      withTokens++;
      tokens += totalTokens(r.tokens);
    }
    if (r.worked_secs !== null && r.worked_secs !== undefined) {
      withWorked++;
      workedSecs += r.worked_secs;
    }
    asks += r.asks;
    waitedSecs += r.waited_secs;
  }
  const finished = rows.filter((r) => r.state === "finished");
  const leads = finished.map((r) => r.tookMs ?? 0);
  return {
    tokens,
    withTokens,
    workedSecs,
    withWorked,
    runs: runs.length,
    asks,
    waitedSecs,
    finished: finished.length,
    deleted: rows.filter((r) => r.state === "deleted").length,
    open: rows.filter((r) => r.state === "open").length,
    leadMedianMs: percentile(leads, 0.5),
    leadP90Ms: percentile(leads, 0.9),
  };
}

/** One day of the chart, by the Mac's own calendar. */
export interface UsageDay {
  /** `YYYY-MM-DD`. */
  day: string;
  tokens: number;
  workedSecs: number;
  finished: number;
}

function dayOf(t: number): string {
  const d = new Date(t);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** The last `days` days up to today, oldest first: what runs that ended on each used, and the tasks finished. */
export function usageDays(runs: Run[], finished: TaskRow[], days: number, now: number): UsageDay[] {
  const out: UsageDay[] = [];
  const at = new Map<string, UsageDay>();
  // Midday, so a day is still a day either side of a clock change.
  const today = new Date(now);
  today.setHours(12, 0, 0, 0);
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today);
    d.setDate(today.getDate() - i);
    const day: UsageDay = { day: dayOf(d.getTime()), tokens: 0, workedSecs: 0, finished: 0 };
    at.set(day.day, day);
    out.push(day);
  }
  for (const r of runs) {
    const day = at.get(dayOf(Date.parse(r.ended_at)));
    if (!day) continue;
    if (r.tokens) day.tokens += totalTokens(r.tokens);
    day.workedSecs += r.worked_secs ?? 0;
  }
  for (const t of finished) {
    if (t.state !== "finished") continue;
    const day = at.get(dayOf(t.at));
    if (day) day.finished++;
  }
  return out;
}

/** One repository's share. */
export interface RepoRow {
  id: string;
  name: string;
  /** Tasks in it, chats aside. */
  tasks: number;
  finished: number;
  workedSecs: number;
  tokens: number;
}

/**
 * Per repository, the most tokens first. A run at a task's root counts in
 * full for each of its repositories, since it could have changed any of them.
 */
export function byRepo(runs: Run[], rows: TaskRow[]): RepoRow[] {
  const at = new Map<string, RepoRow>();
  const of = (p: { id: string; name: string }) => {
    let r = at.get(p.id);
    if (!r) {
      r = { id: p.id, name: p.name || p.id, tasks: 0, finished: 0, workedSecs: 0, tokens: 0 };
      at.set(p.id, r);
    }
    return r;
  };
  for (const run of runs) {
    for (const p of run.repos) {
      const r = of(p);
      r.workedSecs += run.worked_secs ?? 0;
      if (run.tokens) r.tokens += totalTokens(run.tokens);
    }
  }
  for (const t of rows) {
    if (t.state === "chat") continue;
    for (const p of t.repos) {
      const r = of(p);
      r.tasks++;
      if (t.state === "finished") r.finished++;
    }
  }
  return [...at.values()].sort((a, b) => b.tokens - a.tokens || b.workedSecs - a.workedSecs);
}
