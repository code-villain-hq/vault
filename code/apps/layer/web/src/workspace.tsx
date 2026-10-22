import type { SyncClient } from "@code-villain/sync-client";
import { isGuest } from "@code-villain/web";
import type { Me, ModelKind, Schema, WorkspaceInfo } from "@layer/types";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";
import type { Place } from "./server";
import { loadSession } from "./session";
import { createSyncClient } from "./sync";

interface WorkspaceContext {
  /** Where the workspace lives: this Mac, or a server. */
  place: Place;
  me: Me;
  workspace: WorkspaceInfo;
  sync: SyncClient<Schema>;
  /** Fires a "refused" event, with a message, when the server refuses a change. */
  refusals: EventTarget;
}

const Context = createContext<WorkspaceContext | null>(null);

/**
 * Opens the workspace's local copy and keeps it in sync while mounted.
 * Inside, the workspace's `url_key` is its key in the app's addresses,
 * which names its server when another place has the same key.
 */
export function WorkspaceProvider({
  place,
  me,
  workspace: info,
  routeKey,
  children,
}: {
  place: Place;
  me: Me;
  workspace: WorkspaceInfo;
  routeKey: string;
  children: ReactNode;
}) {
  const workspace = useMemo(() => ({ ...info, url_key: routeKey }), [info, routeKey]);
  const [connection, setConnection] = useState<{
    sync: SyncClient<Schema>;
    refusals: EventTarget;
  } | null>(null);
  const userId = me.user.id;
  const workspaceId = workspace.id;

  useEffect(() => {
    const refusals = new EventTarget();
    const sync = createSyncClient({
      place,
      userId,
      workspaceId,
      // Ask again: maybe signed out, maybe just removed from this workspace.
      onSessionEnded: () => void loadSession(),
      onRefused: (message) =>
        refusals.dispatchEvent(new CustomEvent("refused", { detail: message })),
    });
    setConnection({ sync, refusals });
    void sync.start();
    const online = () => sync.retryNow();
    window.addEventListener("online", online);
    return () => {
      window.removeEventListener("online", online);
      sync.stop();
    };
  }, [place, userId, workspaceId]);

  if (!connection) return null;
  return (
    <Context.Provider value={{ place, me, workspace, ...connection }}>{children}</Context.Provider>
  );
}

export function useWorkspace(): WorkspaceContext {
  const context = useContext(Context);
  if (!context) throw new Error("useWorkspace needs a WorkspaceProvider");
  return context;
}

/**
 * Whether you're an admin of the workspace. Only admins change its
 * connections; the server refuses the rest, so the app doesn't offer it.
 */
export function useIsAdmin(): boolean {
  return useWorkspace().workspace.role === "admin";
}

/**
 * Whether you're a guest: you work only in the repositories you're added
 * to, add none, and of the settings see only your account.
 */
export function useIsGuest(): boolean {
  return isGuest(useWorkspace().workspace.role);
}

function useSyncStore<T>(read: (sync: SyncClient<Schema>) => T): T {
  const { sync } = useWorkspace();
  const subscribe = useCallback((listener: () => void) => sync.subscribe(listener), [sync]);
  return useSyncExternalStore(subscribe, () => read(sync));
}

/** Every record of a model, updated live. */
export function useRecords<K extends ModelKind>(model: K): Schema[K]["record"][] {
  return useSyncStore((sync) => sync.query(model));
}

export function useSyncStatus() {
  return useSyncStore((sync) => sync.status());
}

/** True while the record has local changes the server hasn't confirmed. */
export function useIsPending(model: ModelKind, id: string) {
  return useSyncStore((sync) => sync.isPending(model, id));
}
