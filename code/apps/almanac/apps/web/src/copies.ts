// A workspace's local copy, open while anything holds it: the workspace on
// screen, and in the desktop app the ringer, which keeps every server
// workspace's copy in sync so its reminders ring wherever you are. One copy
// per workspace: two clients on the same IndexedDB would both send its
// outbox.

import type { SyncClient } from "@code-villain/sync-client";
import { followPhotos } from "./sending";
import type { Place } from "./server";
import { loadSession } from "./session";
import { followSetUps } from "./starting";
import { createSyncClient, type Schema } from "./sync";

type Copy = {
  place: Place;
  sync: SyncClient<Schema>;
  /** Fires a "refused" event, with a message, when the server refuses a change. */
  refusals: EventTarget;
  holders: number;
  close: () => void;
};

export type Held = Pick<Copy, "sync" | "refusals"> & { release: () => void };

const copies = new Map<string, Copy>();

function open(place: Place, userId: string, workspaceId: string): Copy {
  const refusals = new EventTarget();
  const sync = createSyncClient({
    place,
    userId,
    workspaceId,
    // Ask again: maybe signed out, maybe just removed from this workspace.
    onSessionEnded: () => void loadSession(),
    onRefused: (message) => refusals.dispatchEvent(new CustomEvent("refused", { detail: message })),
  });
  void sync.start();
  const stopSetUps = followSetUps(sync, place, workspaceId);
  const stopPhotos = followPhotos(sync, place, workspaceId);
  const online = () => sync.retryNow();
  window.addEventListener("online", online);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    window.removeEventListener("online", online);
    stopSetUps();
    stopPhotos();
    sync.stop();
  };
  return { place, sync, refusals, holders: 0, close };
}

/** The workspace's copy, opened if nothing holds it yet; `release` when done with it. */
export function holdCopy(place: Place, userId: string, workspaceId: string): Held {
  const key = `${place.id} ${userId} ${workspaceId}`;
  let copy = copies.get(key);
  // Signed in again: the copy goes on with the new session's token.
  if (copy && copy.place !== place) {
    copy.close();
    copy = undefined;
  }
  if (!copy) {
    copy = open(place, userId, workspaceId);
    copies.set(key, copy);
  }
  const held = copy;
  held.holders += 1;
  let released = false;
  return {
    sync: held.sync,
    refusals: held.refusals,
    release: () => {
      if (released) return;
      released = true;
      held.holders -= 1;
      if (held.holders > 0) return;
      held.close();
      if (copies.get(key) === held) copies.delete(key);
    },
  };
}
