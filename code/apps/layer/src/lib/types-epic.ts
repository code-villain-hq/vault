// Epics (§26), mirrored from `commands/epics.rs`.

import type { PaneInfo } from "./types";

export interface SubTaskOf {
  /** The epic's task id. */
  epic: string;
  title: string;
}

/** What an epic's Sub-tasks tab shows (EPIC-7, EPIC-8). */
export interface EpicView {
  /** Some repository's tasks are approved: there is a plan to file. */
  planned: boolean;
  sub_tasks: SubTaskView[];
}

export interface SubTaskView {
  title: string;
  /** Its ticket: filed for it, or named by its heading. */
  key: string | null;
  /** Its heading named a ticket that was there before. */
  existing: boolean;
  /** Its repositories, by folder. */
  repos: string[];
  /** The requirements its steps serve. */
  serves: string[];
  steps: number;
  done: number;
  /** The titles of the sub-tasks it waits for (EPIC-5). */
  waits_for: string[];
  /** Its task, once filed. */
  task_id: string | null;
}

export interface SubTaskFailure {
  title: string;
  error: string;
}

export interface Filed {
  epic: EpicView;
  failed: SubTaskFailure[];
}

export interface StartedSubTask {
  pane: PaneInfo;
  moved: string | null;
}
