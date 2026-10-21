/**
 * The shared setup's way to the workspace and back (§25). The backend keeps
 * reading `config.json` and `notes.json`; what changes there is queued, and
 * this sync client takes it to the workspace (PROF-13). What the workspace
 * keeps comes back through `apply_setup` (PROF-14). Offline included: the
 * client keeps its outbox in IndexedDB and sends it when it can.
 *
 * Each queued change is a transaction of its own, so one the workspace
 * refuses rolls back alone.
 */
import { HttpTransport, IndexedDbStore, SyncClient } from "@code-villain/sync-client";
import type { Schema } from "@layer/types";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { SetupSnapshot } from "./types-profiles";

type Lists = SetupSnapshot[keyof SetupSnapshot][];

/**
 * Starts once the workspace's server has (it may never: then nothing does).
 * `onSetup` reads the repositories and settings again; `onRefused` says why
 * a change was not taken. Returns what stops it.
 */
export async function startWorkspaceSync(hooks: {
  onSetup: () => void;
  onRefused: (message: string) => void;
}): Promise<() => void> {
  const place = await api.workspacePlace();
  if (!place) return () => {};
  const client = new SyncClient<Schema>({
    store: new IndexedDbStore(`layer-${place.user_id}-${place.workspace_id}`),
    transport: new HttpTransport({
      workspaceId: place.workspace_id,
      baseUrl: place.origin,
      credentials: "omit",
      headers: () => ({ authorization: `Bearer ${place.token}`, "x-layer-client": "desktop" }),
      protocols: () => ["layer", `bearer.${place.token}`],
    }),
    onReject: (_transaction, error) => hooks.onRefused(error.message),
  });

  const take = async () => {
    for (const op of await api.takeSetupWrites()) client.transact([op]);
  };

  // One snapshot at a time, and only a new one: `subscribe` also fires for
  // the connection coming and going, and `query` keeps an unchanged list.
  let last: Lists = [];
  let busy = false;
  let again = false;
  const push = async () => {
    if (busy) {
      again = true;
      return;
    }
    busy = true;
    try {
      do {
        again = false;
        if (!client.status().ready) return;
        const snapshot: SetupSnapshot = {
          repositories: client.query("Repository"),
          connections: client.query("Connection"),
          log_sources: client.query("LogSource"),
          agent_models: client.query("AgentModel"),
          notes: client.query("Note"),
        };
        const lists = Object.values(snapshot);
        if (lists.length === last.length && lists.every((list, i) => list === last[i])) continue;
        const applied = await api.applySetup(snapshot);
        if (!applied.applied) {
          // Changes made here first; taking them changes the snapshot.
          await take();
          continue;
        }
        last = lists;
        if (applied.setup) hooks.onSetup();
      } while (again);
    } catch (e) {
      hooks.onRefused(e instanceof Error ? e.message : String(e));
    } finally {
      busy = false;
    }
  };

  await client.start();
  await take();
  const unsubscribe = client.subscribe(() => void push());
  const unlisten = listen("setup:write", () => void take());
  void push();
  return () => {
    unsubscribe();
    void unlisten.then((un) => un());
    client.stop();
  };
}
