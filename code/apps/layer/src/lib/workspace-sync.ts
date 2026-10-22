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
import { useSyncExternalStore } from "react";
import { api } from "./api";
import type { SetupSnapshot } from "./types-profiles";

type Lists = SetupSnapshot[keyof SetupSnapshot][];

/**
 * Whether the server stopped taking this workspace's session, and how many
 * times it was signed in to again since launch (PROF-16): the sync starts
 * again on each.
 */
let session = { signedOut: false, turn: 0 };
const watching = new Set<() => void>();
function setSession(next: typeof session) {
  session = next;
  for (const tell of watching) tell();
}
export function useSession() {
  return useSyncExternalStore((tell) => {
    watching.add(tell);
    return () => void watching.delete(tell);
  }, () => session);
}
export function signedInAgain() {
  setSession({ signedOut: false, turn: session.turn + 1 });
}
function signedOut() {
  if (!session.signedOut) setSession({ ...session, signedOut: true });
}

/**
 * What the sidebar's sync status shows (PROF-19), in the kit's own terms
 * (`SyncIndicator`): connecting until the client starts, offline when there
 * is no workspace to reach (its server did not start, or it is signed out).
 */
export interface SyncState {
  connection: "online" | "offline" | "connecting";
  pending: number;
  ready: boolean;
}
const CONNECTING: SyncState = { connection: "connecting", pending: 0, ready: false };
const UNREACHABLE: SyncState = { connection: "offline", pending: 0, ready: false };
let sync = CONNECTING;
const watchingSync = new Set<() => void>();
function setSync(next: SyncState) {
  if (next.connection === sync.connection && next.pending === sync.pending && next.ready === sync.ready) return;
  sync = next;
  for (const tell of watchingSync) tell();
}
export function useSyncState() {
  return useSyncExternalStore((tell) => {
    watchingSync.add(tell);
    return () => void watchingSync.delete(tell);
  }, () => sync);
}
/** Raises "refused", with the workspace's reason, when it turns a change down. */
export const refusals = new EventTarget();

/**
 * Starts once the workspace's server has (it may never: then nothing does).
 * `onSetup` reads the repositories and settings again; `onRefused` says why
 * a change from the workspace could not be kept here (one the workspace
 * refuses goes to `refusals`). Returns what stops it.
 */
export async function startWorkspaceSync(hooks: {
  onSetup: () => void;
  onRefused: (message: string) => void;
}): Promise<() => void> {
  const place = await api.workspacePlace();
  if (!place) {
    // A server workspace with no session: the menu offers Sign in again.
    const me = (await api.listProfiles()).find((p) => p.current);
    if (me?.server) signedOut();
    setSync(UNREACHABLE);
    return () => setSync(CONNECTING);
  }
  // Stopped for good once the server stops taking the session: offline from
  // then on, with what waits still counted.
  let shut = false;
  const tellSync = () => setSync(shut ? { ...client.status(), connection: "offline" } : client.status());
  const client = new SyncClient<Schema>({
    store: new IndexedDbStore(`layer-${place.user_id}-${place.workspace_id}`),
    transport: new HttpTransport({
      workspaceId: place.workspace_id,
      baseUrl: place.origin,
      credentials: "omit",
      headers: () => ({ authorization: `Bearer ${place.token}`, "x-layer-client": "desktop" }),
      protocols: () => ["layer", `bearer.${place.token}`],
      onUnauthorized: () => {
        signedOut();
        client.stop();
        shut = true;
        tellSync();
      },
    }),
    // Said where the sync status is (PROF-19), and kept in the message center.
    onReject: (_transaction, error) => {
      refusals.dispatchEvent(new CustomEvent("refused", { detail: error.message }));
      void api
        .addMessage("error", "error", `The workspace did not take a change: ${error.message}`)
        .catch(() => {});
    },
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
          users: client.query("User"),
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
  tellSync();
  const unsubscribe = client.subscribe(() => {
    tellSync();
    void push();
  });
  const unlisten = listen("setup:write", () => void take());
  void push();
  return () => {
    unsubscribe();
    void unlisten.then((un) => un());
    client.stop();
    setSync(CONNECTING);
  };
}
