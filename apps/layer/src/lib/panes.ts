import { useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import { useStore } from "../store";
import { fleetAgents, taskAgents } from "./derive";
import type { PaneInfo } from "./types";

/**
 * The panes of one task, or the chats, and their ids in bar order.
 *
 * Every pane's activity changes the store's pane list, and a view that
 * filtered it with `useMemo` got a new list each time: an agent working in
 * another task re-rendered this one, re-ran its "keep a pane selected"
 * effect, wrote its selection to localStorage again and re-bound Ctrl+Tab,
 * many times a minute for as long as the app ran.
 */
export function usePanesOf(taskId: string): { panes: PaneInfo[]; ids: string[] } {
  // The store keeps each pane that did not change (`refreshPanes`), so
  // compared item by item this is the same list until one of these moves.
  const panes = useStore(useShallow((s) => s.panes.filter((p) => p.task_id === taskId)));
  // What depends on which panes there are, not on what they are doing,
  // follows the ids alone.
  const key = panes.map((p) => p.id).join("\n");
  const ids = useMemo(() => (key ? key.split("\n") : []), [key]);
  return { panes, ids };
}

/**
 * A task's agents that are running, for the views that offer to hand them
 * something (PR feedback, a diff note, a review). The same list until one of
 * them changes, not on every pane's activity anywhere.
 */
export function useRunningAgentsOf(taskId: string): PaneInfo[] {
  return useStore(
    useShallow((s) => s.panes.filter((p) => p.task_id === taskId && p.kind === "agent" && p.running)),
  );
}

/**
 * What one task's sidebar row shows about its agents. A new value only when
 * one of those four does, so an agent in another task leaves the row alone.
 */
export function useTaskAgents(taskId: string) {
  return useStore(useShallow((s) => taskAgents(s.panes, taskId)));
}

/** "All agents": how many run and how many need you. */
export function useFleetAgents() {
  return useStore(useShallow((s) => fleetAgents(s.panes)));
}
