import { create } from "zustand";
import { api } from "./api";
import { taskReview, type TaskReview } from "./derive";
import type { CheckoutPr, EpicView, SubTaskView } from "./types";

/** Where a sub-task stands, in the order EPIC-10 asks the questions. */
export type SubTaskState =
  | "merged" | "changes" | "approved" | "in review" | "asking" | "working" | "waits" | "ready" | "to file";

/**
 * The first that holds (EPIC-10): its pull requests, as the task list reads
 * them; then its agents; then what it waits for. A sub-task not filed yet
 * has no task to ask about.
 */
export function subTaskState(
  filed: boolean,
  review: TaskReview,
  agents: { waiting: boolean; live: number },
  waits: boolean,
): SubTaskState {
  if (!filed) return "to file";
  if (review === "merged") return "merged";
  if (review === "changes_requested") return "changes";
  if (review === "approved") return "approved";
  if (review !== "none") return "in review";
  if (agents.waiting) return "asking";
  if (agents.live > 0) return "working";
  if (waits) return "waits";
  return "ready";
}

/** Its word's colour: calm for done and waiting, a status where it needs someone. */
export const STATE_COLOR: Partial<Record<SubTaskState, string>> = {
  changes: "var(--status-danger-ink)",
  approved: "var(--status-ok-ink)",
  asking: "var(--status-warn-ink)",
};

/**
 * The titles `sub` waits for that have not merged. One not filed has not;
 * read from the pull requests the task list already has.
 */
export function openWaits(sub: SubTaskView, all: SubTaskView[], prs: Record<string, CheckoutPr[]>): string[] {
  return sub.waits_for.filter((title) => {
    const other = all.find((s) => s.title.toLowerCase() === title.toLowerCase());
    return !other?.task_id || taskReview(prs[other.task_id] ?? []) !== "merged";
  });
}

/**
 * Each epic's Sub-tasks tab, as last read. Read when its row or page is
 * drawn and again after anything that changes it (filing, approving its
 * tasks); its sub-tasks' own progress comes from the task list's events.
 */
export const useEpicViews = create<{
  byEpic: Record<string, EpicView>;
  load: (epicId: string) => Promise<void>;
  put: (epicId: string, view: EpicView) => void;
}>((set) => ({
  byEpic: {},
  load: async (epicId) => {
    const view = await api.epicView(epicId);
    set((s) => ({ byEpic: { ...s.byEpic, [epicId]: view } }));
  },
  put: (epicId, view) => set((s) => ({ byEpic: { ...s.byEpic, [epicId]: view } })),
}));
