import { CHAT_TASK_ID, needsYou } from "./derive";
import type { PaneInfo, Task } from "./types";

/** How the All agents view orders its cards (PANE-17). */
export const AGENT_SORTS = ["needs", "activity", "task", "started"] as const;
export type AgentSort = (typeof AGENT_SORTS)[number];

export const AGENT_SORT_LABEL: Record<AgentSort, string> = {
  needs: "Needs you first",
  activity: "Latest activity",
  task: "By task",
  started: "Newest first",
};

const time = (iso: string) => Date.parse(iso) || 0;

/**
 * When an agent last did something. One at work is at it now, so every
 * working agent comes before the rest. Otherwise its `activity_since`:
 * when it finished, asked, or last worked before it exited. Looking at a
 * finished agent does not move it, so opening one does not reorder the list.
 */
function lastActive(p: PaneInfo): [number, number] {
  const working = p.running && p.activity === "working";
  return [working ? 1 : 0, time(p.activity_since)];
}

/**
 * The agent panes' ids in `by`'s order. Ties keep the order the panes came
 * in, so a list that has not changed does not shuffle.
 */
export function sortAgents(panes: PaneInfo[], tasks: Task[], by: AgentSort): string[] {
  const agents = panes.filter((p) => p.kind === "agent");
  const taskName = (p: PaneInfo) =>
    p.task_id === CHAT_TASK_ID ? null : (tasks.find((t) => t.id === p.task_id)?.name ?? "");
  const order: Record<AgentSort, (a: PaneInfo, b: PaneInfo) => number> = {
    // What needs you first, so the dock's count is the top of this list.
    needs: (a, b) => Number(needsYou(b)) - Number(needsYou(a)),
    activity: (a, b) => {
      const [aw, at] = lastActive(a);
      const [bw, bt] = lastActive(b);
      return bw - aw || bt - at;
    },
    // Tasks by name, chats after them; within one, the newest agent first.
    task: (a, b) => {
      const an = taskName(a);
      const bn = taskName(b);
      if (an !== bn) {
        if (an === null) return 1;
        if (bn === null) return -1;
        return an.localeCompare(bn, undefined, { numeric: true, sensitivity: "base" });
      }
      return time(b.started_at) - time(a.started_at);
    },
    started: (a, b) => time(b.started_at) - time(a.started_at),
  };
  return agents.sort(order[by]).map((p) => p.id);
}
