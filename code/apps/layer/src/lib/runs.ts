/**
 * Reading the run log (§22): what a run came to, and the sums the Usage view
 * shows over the runs its filters leave (RUN-3, RUN-4). What they add up to
 * per task, day and repository is `usage.ts`. Pure, so they are tested on
 * their own.
 */
import type { Run, RunTokens } from "./types-runs";

/**
 * What a run came to (RUN-2). A loop's verdict says more than the process
 * ending, which is usually you stopping it once the loop is done; a process
 * that failed says most of all.
 */
export type RunResult = "passed" | "gave_up" | "failed" | "stopped" | "exited";

export function resultOf(run: Run): RunResult {
  if (run.end === "failed") return "failed";
  if (run.loop?.end === "passed") return "passed";
  if (run.loop?.end === "gave_up") return "gave_up";
  return run.end;
}

/** It went wrong: the agent failed, or its loop ran out of rounds with checks still failing. */
export function wentWrong(run: Run): boolean {
  const r = resultOf(run);
  return r === "failed" || r === "gave_up";
}

export function durationMs(run: Run): number {
  return Math.max(0, Date.parse(run.ended_at) - Date.parse(run.started_at)) || 0;
}

export interface RunFilter {
  agent: string | null;
  repo: string | null;
}

/**
 * How many runs the list draws at first, and adds with each "Show more"
 * (RUN-3). It drew all of them, up to the 1,000 the log keeps, and its
 * clock redrew every one every 30 seconds.
 */
export const RUN_PAGE = 100;

/** The first `pages` pages of `runs`, and how many are left after them. */
export function runPage(runs: Run[], pages: number): { rows: Run[]; left: number } {
  const n = Math.max(1, pages) * RUN_PAGE;
  return { rows: runs.length > n ? runs.slice(0, n) : runs, left: Math.max(0, runs.length - n) };
}

/** A run counts for a repository it could have changed: its own, or any of its task's. */
export function filterRuns(runs: Run[], f: RunFilter): Run[] {
  return runs.filter((r) => (!f.agent || r.agent === f.agent) && (!f.repo || r.repos.some((p) => p.id === f.repo)));
}

export interface RunStats {
  runs: number;
  wrong: number;
  /** Of `runs`, 0 with none. */
  wrongRate: number;
  /**
   * The typical run, and the slowest tenth's. Not the average: one agent
   * left open overnight made the average of a day's runs hours long.
   */
  medianMs: number;
  p90Ms: number;
  /** Times agents stopped on something only you could answer, and how long they waited (RUN-7). */
  asks: number;
  waitedSecs: number;
  /** Time spent working, over the runs that said, and how many did (RUN-7). */
  workedSecs: number;
  withWorked: number;
  /** Runs that hit a usage limit. */
  limited: number;
  /** Runs that were on a loop, and the rounds they used between them. */
  looped: number;
  rounds: number;
  /** Summed over the runs that said, and how many did. */
  tokens: RunTokens | null;
  withTokens: number;
  /** Tool calls over the runs that said, and how many did. */
  tools: { calls: number; failed: number } | null;
  withTools: number;
}

/** The value `p` of the way up `values` (0.5 the median), 0 for none. */
export function percentile(values: number[], p: number): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil(p * sorted.length) - 1))];
}

function addTokens(into: RunTokens, t: RunTokens) {
  into.input += t.input;
  into.output += t.output;
  into.cached_read += t.cached_read;
  into.cached_write += t.cached_write;
}

export function runStats(runs: Run[]): RunStats {
  let wrong = 0, asks = 0, waitedSecs = 0, workedSecs = 0, withWorked = 0;
  let limited = 0, looped = 0, rounds = 0, withTokens = 0, withTools = 0;
  const tokens: RunTokens = { input: 0, output: 0, cached_read: 0, cached_write: 0 };
  const tools = { calls: 0, failed: 0 };
  for (const r of runs) {
    if (wentWrong(r)) wrong++;
    asks += r.asks;
    waitedSecs += r.waited_secs;
    if (r.worked_secs !== null && r.worked_secs !== undefined) {
      withWorked++;
      workedSecs += r.worked_secs;
    }
    if (r.limited) limited++;
    if (r.loop) {
      looped++;
      rounds += r.loop.used;
    }
    if (r.tokens) {
      withTokens++;
      addTokens(tokens, r.tokens);
    }
    if (r.tools) {
      withTools++;
      tools.calls += r.tools.calls;
      tools.failed += r.tools.failed;
    }
  }
  const n = runs.length;
  const lengths = runs.map(durationMs);
  return {
    runs: n,
    wrong,
    wrongRate: n ? wrong / n : 0,
    medianMs: percentile(lengths, 0.5),
    p90Ms: percentile(lengths, 0.9),
    asks,
    waitedSecs,
    workedSecs,
    withWorked,
    limited,
    looped,
    rounds,
    tokens: withTokens ? tokens : null,
    withTokens,
    tools: withTools ? tools : null,
    withTools,
  };
}

export function totalTokens(t: RunTokens): number {
  return t.input + t.output + t.cached_read + t.cached_write;
}

/** One agent's runs, side by side with the others' (RUN-8). */
export interface AgentRow {
  agent: string;
  stats: RunStats;
}

/** A row per agent, the busiest first. */
export function byAgent(runs: Run[]): AgentRow[] {
  const of = new Map<string, Run[]>();
  for (const r of runs) of.set(r.agent, [...(of.get(r.agent) ?? []), r]);
  return [...of].map(([agent, rs]) => ({ agent, stats: runStats(rs) })).sort((a, b) => b.stats.runs - a.stats.runs);
}

/** The filters' choices: every agent and repository the log has seen, the latest name for each. */
export function runChoices(runs: Run[]): { agents: string[]; repos: { id: string; name: string }[] } {
  const agents = new Set<string>();
  const repos = new Map<string, string>();
  for (const r of runs) {
    if (r.agent) agents.add(r.agent);
    // Newest first, so the first name seen is the latest.
    for (const p of r.repos) if (!repos.has(p.id)) repos.set(p.id, p.name || p.id);
  }
  return {
    agents: [...agents].sort(),
    repos: [...repos].map(([id, name]) => ({ id, name })).sort((a, b) => a.name.localeCompare(b.name)),
  };
}

/** "45s", "3m 12s", "1h 5m". */
export function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

/** A task's length: "40m", "5h 20m", "3d 4h", "4d". */
export function formatSpan(ms: number): string {
  const m = Math.round(ms / 60_000);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return m % 60 ? `${h}h ${m % 60}m` : `${h}h`;
  return h % 24 ? `${Math.floor(h / 24)}d ${h % 24}h` : `${Math.floor(h / 24)}d`;
}

/** "950", "12.3k", "4.5M". */
export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}
