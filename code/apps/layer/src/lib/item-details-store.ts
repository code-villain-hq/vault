import type { State } from "../store";
import { api } from "./api";
import { sameJson } from "./same";
import type { DetailsEvent, DetailsPatch, ItemDetails, PaneInfo } from "./types";

// Saves and event notifications invalidate even refreshAll's independent reads.
export const detailsVersions = { tasks: 0, panes: 0, waiting: 0 };

/**
 * Skip a React storm when a poll returns the same world we already have.
 * Pane by pane: one agent changing state keeps every other pane's object, so
 * a view of another task's panes is left alone (`usePanesOf`).
 */
export function samePane(x: PaneInfo, y: PaneInfo): boolean {
  return (
    x.id === y.id &&
    x.running === y.running &&
    x.exit_code === y.exit_code &&
    x.notice === y.notice &&
    x.activity === y.activity &&
    x.activity_since === y.activity_since &&
    x.title === y.title &&
    x.topic === y.topic &&
    x.task_id === y.task_id &&
    sameJson(x.details, y.details)
  );
}

export interface ItemDetailsState {
  tags: string[];
  refreshTags: () => Promise<void>;
  detailsChanged: (event: DetailsEvent) => Promise<void>;
  saveTaskDetails: (taskId: string, patch: DetailsPatch) => Promise<ItemDetails>;
  saveChatDetails: (room: string, patch: DetailsPatch) => Promise<ItemDetails>;
  addTag: (tag: string) => Promise<string>;
  renameTag: (tag: string, to: string) => Promise<string>;
  deleteTag: (tag: string) => Promise<void>;
}

// Only the newest catalog read may replace the list, so an older one cannot
// bring back a deleted tag. A read that a local save overtook only adds to it:
// it may have been served before that save, or after another writer's.
let tagReads = 0;
let tagMerges = 0;

export function itemDetailsActions(
  set: (patch: Partial<State> | ((s: State) => Partial<State>)) => void,
  get: () => State,
): ItemDetailsState {
  const catalog = (tags: string[]) => {
    ++tagMerges;
    merge(tags);
  };
  const merge = (tags: string[]) => {
    set((s) => {
      const next = [...s.tags];
      for (const tag of tags)
        if (!next.some((t) => t.toLowerCase() === tag.toLowerCase())) next.push(tag);
      return sameJson(s.tags, next) ? s : { tags: next };
    });
  };
  return {
    tags: [],
    refreshTags: async () => {
      const read = ++tagReads;
      const merges = tagMerges;
      const tags = await api.listTags();
      if (read !== tagReads) return;
      if (merges !== tagMerges) merge(tags);
      else set((s) => (sameJson(s.tags, tags) ? s : { tags }));
    },
    detailsChanged: async (event) => {
      const pending: Promise<void>[] = [];
      if (event.kind === "task") {
        ++detailsVersions.tasks;
        pending.push(get().refreshTasks());
      }
      if (event.kind === "chat") {
        ++detailsVersions.panes;
        ++detailsVersions.waiting;
        pending.push(get().refreshPanes(), get().refreshWaiting());
      }
      if (event.catalog_changed) pending.push(get().refreshTags());
      await Promise.all(pending);
    },
    saveTaskDetails: async (taskId, patch) => {
      const details = await api.setTaskDetails(taskId, patch);
      catalog(details.tags);
      // A reply can arrive after a later writer's notification. Never publish
      // its snapshot: invalidate older reads and queue an authoritative one.
      // A failed reconciliation is a background failure, not a failed save.
      await get().detailsChanged({ kind: "task", catalog_changed: false }).catch(get().fail);
      return details;
    },
    saveChatDetails: async (room, patch) => {
      const details = await api.setChatDetails(room, patch);
      catalog(details.tags);
      await get().detailsChanged({ kind: "chat", catalog_changed: false }).catch(get().fail);
      return details;
    },
    addTag: async (tag) => {
      const canonical = await api.addTag(tag);
      catalog([canonical]);
      return canonical;
    },
    // A rename or delete reaches every chat and task with the tag. Reload them
    // here as well as on the event, so the caller sees it done when this ends.
    renameTag: async (tag, to) => {
      const canonical = await api.renameTag(tag, to);
      await Promise.all([
        get().detailsChanged({ kind: "task", catalog_changed: true }),
        get().detailsChanged({ kind: "chat", catalog_changed: false }),
      ]);
      return canonical;
    },
    deleteTag: async (tag) => {
      await api.deleteTag(tag);
      await Promise.all([
        get().detailsChanged({ kind: "task", catalog_changed: true }),
        get().detailsChanged({ kind: "chat", catalog_changed: false }),
      ]);
    },
  };
}
