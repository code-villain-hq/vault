import { sameJson } from "./same";
import type { CheckoutPr, TaskPrs } from "./types";

export interface TaskPrSnapshots {
  /** PR rows per task id, refreshed by the background watch. */
  prs: Record<string, CheckoutPr[]>;
  /** Last accepted PR read per task, including reads whose rows did not change. */
  prReadRevision: Record<string, number>;
}

export function acceptTaskPrReads(
  current: TaskPrSnapshots,
  reads: readonly Pick<TaskPrs, "task_id" | "rows">[],
  revision: number,
): TaskPrSnapshots {
  const changed = reads.filter((t) => !sameJson(current.prs[t.task_id], t.rows));
  return {
    prs:
      changed.length === 0
        ? current.prs
        : { ...current.prs, ...Object.fromEntries(changed.map((t) => [t.task_id, t.rows])) },
    prReadRevision: {
      ...current.prReadRevision,
      ...Object.fromEntries(reads.map((t) => [t.task_id, revision])),
    },
  };
}
