import type { RepoResult, Synced } from "./types";

/**
 * Say what went wrong in a multi-repo operation, through `fail`, which is
 * where the user acted (NOTE-6).
 *
 * Each repo answers on its own, so a failure in one must not swallow successes
 * in the others. What worked says nothing more (NOTE-7): its row, or the
 * dialog closing, shows it. An empty reply is worth saying out loud rather
 * than leaving the click looking like it did nothing.
 */
export function reportRepoResults(fail: (text: string) => void, results: RepoResult[]) {
  const bad = results.filter((r) => !r.ok);
  if (bad.length) {
    fail(bad.map((r) => `${r.repo}: ${r.detail}`).join("\n"));
  }
  if (results.length === 0) {
    fail("Nothing to do. No repository had changes.");
  }
}

/** How many failed repos a Sync summary names before it only counts them. */
const NAMED = 5;

/**
 * One line for a Sync of several repos, naming the ones that failed.
 *
 * "Synced 52 of 53. Each repo says what happened." left the one that failed
 * to be found by scrolling fifty rows, and a banner or the message center
 * had no rows at all. The reasons stay on the rows: git's are several lines.
 */
export function syncSummary(rows: Synced[]): string {
  const failed = rows.filter((r) => !r.ok).map((r) => r.repo);
  if (failed.length === 0) return `Synced ${rows.length} repositories.`;
  const named = failed.slice(0, NAMED).join(", ");
  const rest = failed.length - NAMED;
  const which = rest > 0 ? `${named} and ${rest} more` : named;
  return `Synced ${rows.length - failed.length} of ${rows.length}. Failed: ${which}. Each says why on its row.`;
}
