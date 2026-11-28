import { externalHttpUrl } from "./external-url";
import type { CheckRun, PrChecks } from "./types";

const RANK = { skipped: 0, passing: 1, cancelled: 2, unknown: 3, pending: 4, failing: 5 };
type Outcome = keyof typeof RANK;
type Result = { outcome: Outcome; label: string };
type Run = Result & Pick<CheckRun, "name" | "url">;

const COMPLETED = new Map<string, Result>([
  ["success", { outcome: "passing", label: "Passed" }],
  ["skipped", { outcome: "skipped", label: "Skipped" }],
  ["neutral", { outcome: "skipped", label: "Neutral" }],
  ["cancelled", { outcome: "cancelled", label: "Cancelled" }],
  ["failure", { outcome: "failing", label: "Failed" }],
  ["timed_out", { outcome: "failing", label: "Timed out" }],
  ["action_required", { outcome: "failing", label: "Action required" }],
  ["startup_failure", { outcome: "failing", label: "Failed to start" }],
  ["stale", { outcome: "failing", label: "Stale" }],
]);

const PENDING = new Map([
  ["queued", "Queued"],
  ["in_progress", "In progress"],
  ["pending", "Pending"],
  ["requested", "Requested"],
  ["waiting", "Waiting"],
]);

function result(run: CheckRun): Result {
  if (run.status === "completed") {
    const complete = COMPLETED.get(run.conclusion ?? "");
    if (complete) return complete;
  } else if (run.conclusion === null) {
    const label = PENDING.get(run.status);
    if (label) return { outcome: "pending", label };
  }
  return { outcome: "unknown", label: "Unknown" };
}

/**
 * A push and its PR can run the same suite twice. Keep the worst result per
 * name so the compact headline and expanded rows cannot disagree.
 */
export function checkRunsView(read: PrChecks) {
  const byName = new Map<string, Run>();
  for (const check of read.checks) {
    const run = { ...result(check), name: check.name, url: externalHttpUrl(check.url) };
    const prev = byName.get(run.name);
    if (
      !prev ||
      RANK[run.outcome] > RANK[prev.outcome] ||
      (run.label === prev.label && !prev.url && run.url)
    ) {
      byName.set(run.name, run);
    }
  }
  const runs = [...byName.values()];
  let worst: Outcome | undefined;
  for (const { outcome } of runs) {
    if (!worst || RANK[outcome] > RANK[worst]) worst = outcome;
  }
  const state =
    read.checks_state === "complete" || read.checks_state === "incomplete"
      ? read.checks_state
      : "unavailable";
  const summary: Outcome | "empty" | "incomplete" | "unavailable" =
    state === "complete"
      ? (worst ?? "empty")
      : state === "incomplete" && (worst === "failing" || worst === "pending")
        ? worst
        : state;
  return { summary, state, runs };
}

export type CheckRunsView = ReturnType<typeof checkRunsView>;
