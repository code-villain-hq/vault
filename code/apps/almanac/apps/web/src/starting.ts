// Starting a topic: the app makes it through sync, offline too, with the
// blocks of the template picked. Claude sets it up once the server has it
// (`POST /api/w/{id}/topics/{topic}/set_up`): the brief, an icon, a group,
// the first questions and, with none picked, the template. Until then this
// device keeps what's still to set up.

import type { Field, SetUpTopicRequest, StartedTopic } from "@almanac/types";
import { type SyncClient, uuidv7 } from "@code-villain/sync-client";
import { api, RequestError } from "./api";
import { answerHere, asksHere } from "./here";
import type { Place } from "./server";
import type { Schema } from "./sync";

type Sync = SyncClient<Schema>;

/**
 * The blocks each template starts a page with, as `TEMPLATES` in
 * `crates/domain/src/topic.rs` has them: kind, title, and a checklist's
 * check.
 */
export const TEMPLATE_BLOCKS: Record<string, [kind: string, title: string, check: string][]> = {
  routine: [
    ["collection", "Routine", "Done"],
    ["collection", "Your shelf", ""],
    ["paragraph", "Advice", ""],
    ["you", "You", ""],
  ],
  collection: [
    ["collection", "To do", "Done"],
    ["collection", "Notes", ""],
    ["you", "You", ""],
  ],
  study: [
    ["collection", "Reading", "Done"],
    ["collection", "Terms", ""],
    ["paragraph", "What it says", ""],
    ["you", "You", ""],
  ],
};

export interface NewTopic {
  name: string;
  sticks_to?: string;
  /** One of `TEMPLATE_BLOCKS`; Almanac picks one when it's left out. */
  template?: string;
  parent_id?: string;
}

/** Makes a topic and its template's blocks at once, to set up later; returns its id. */
export function startTopic(sync: Sync, workspaceId: string, topic: NewTopic): string {
  const id = uuidv7();
  const position = Math.max(0, ...sync.query("Topic").map((t) => t.position)) + 1;
  const blocks = (topic.template && TEMPLATE_BLOCKS[topic.template]) || [];
  sync.transact([
    {
      action: "create",
      model: "Topic",
      id,
      data: {
        name: topic.name,
        sticks_to: topic.sticks_to ?? "",
        icon: "book",
        group_name: "",
        parent_id: topic.parent_id,
        starters: [],
        position,
        kind: "topic",
      },
    },
    ...blocks.map(([kind, title, check], position) => ({
      action: "create" as const,
      model: "Block" as const,
      id: uuidv7(),
      data: {
        topic_id: id,
        kind,
        title,
        facets: [],
        fields: check ? [checkField(check)] : [],
        width: "",
        view: "",
        position,
      },
    })),
  ]);
  setUps(workspaceId).push({ id, template: topic.template });
  save(workspaceId);
  return id;
}

const checkField = (name: string): Field => ({
  name,
  kind: "check",
  unit: "",
  goal: null,
  options: [],
});

/** A topic made on this device, still to set up. */
interface ToSetUp {
  id: string;
  template?: string;
}

const waiting = new Map<string, ToSetUp[]>();
const keyOf = (workspaceId: string) => `almanac.set-ups.${workspaceId}`;

function setUps(workspaceId: string): ToSetUp[] {
  let list = waiting.get(workspaceId);
  if (!list) {
    try {
      list = JSON.parse(localStorage.getItem(keyOf(workspaceId)) ?? "[]") as ToSetUp[];
    } catch {
      list = [];
    }
    waiting.set(workspaceId, list);
  }
  return list;
}

function save(workspaceId: string) {
  try {
    localStorage.setItem(keyOf(workspaceId), JSON.stringify(setUps(workspaceId)));
  } catch {
    // Kept for as long as the app is open.
  }
}

function done(workspaceId: string, id: string) {
  waiting.set(
    workspaceId,
    setUps(workspaceId).filter((t) => t.id !== id),
  );
  save(workspaceId);
}

/**
 * Sets up the topics made on this device once the server has them, each
 * once; returns what stops it. A server that can't be reached is asked
 * again later, one that refuses isn't.
 */
export function followSetUps(sync: Sync, place: Place, workspaceId: string): () => void {
  let busy = false;
  let again = false;
  let stopped = false;
  let later: ReturnType<typeof setTimeout> | undefined;
  const run = async (): Promise<void> => {
    const status = sync.status();
    if (stopped || !status.ready || status.connection !== "online") return;
    if (busy) {
      again = true;
      return;
    }
    busy = true;
    again = false;
    try {
      for (const topic of setUps(workspaceId)) {
        if (stopped || sync.isPending("Topic", topic.id)) continue;
        // Deleted before it was set up.
        if (!sync.get("Topic", topic.id)) {
          done(workspaceId, topic.id);
          continue;
        }
        const body: SetUpTopicRequest = { template: topic.template };
        if (asksHere(place)) body.here = true;
        try {
          const { set_up } = await api<StartedTopic>(
            "POST",
            `/api/w/${workspaceId}/topics/${topic.id}/set_up`,
            body,
            place,
          );
          done(workspaceId, topic.id);
          // Claude Code on this Mac sets it up.
          if (set_up) void answerHere(place, set_up);
        } catch (error) {
          if (error instanceof RequestError && !error.offline && error.status < 500) {
            done(workspaceId, topic.id);
            continue;
          }
          clearTimeout(later);
          later = setTimeout(() => void run(), 30_000);
          return;
        }
      }
    } finally {
      busy = false;
    }
    if (again) await run();
  };
  const unsubscribe = sync.subscribe(() => void run());
  void run();
  return () => {
    stopped = true;
    clearTimeout(later);
    unsubscribe();
  };
}
