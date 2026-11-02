import type { Me, ModelKind, WorkspaceInfo } from "@almanac/types";
import type { SyncClient } from "@code-villain/sync-client";
import { isGuest } from "@code-villain/web";
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
import { holdCopy } from "./copies";
import type { Place } from "./server";
import type { Schema } from "./sync";

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
 * Holds the workspace's local copy, which keeps it in sync, while mounted.
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
    /** Who it's open for, where: after a switch it serves nothing. */
    opened: string;
  } | null>(null);
  const userId = me.user.id;
  const workspaceId = workspace.id;
  const opens = `${place.id} ${userId} ${workspaceId}`;

  useEffect(() => {
    const { sync, refusals, release } = holdCopy(place, userId, workspaceId);
    setConnection({ sync, refusals, opened: `${place.id} ${userId} ${workspaceId}` });
    return release;
  }, [place, userId, workspaceId]);

  // Until the switched-to workspace opens, nothing shows the last one's.
  if (!connection || connection.opened !== opens) return null;
  const { sync, refusals } = connection;
  return (
    <Context.Provider value={{ place, me, workspace, sync, refusals }}>{children}</Context.Provider>
  );
}

export function useWorkspace(): WorkspaceContext {
  const context = useContext(Context);
  if (!context) throw new Error("useWorkspace needs a WorkspaceProvider");
  return context;
}

/**
 * Whether you're an admin of the workspace. Only admins set who answers and
 * with what (the web, the key); the server refuses the rest, so the app
 * doesn't offer it.
 */
export function useIsAdmin(): boolean {
  return useWorkspace().workspace.role === "admin";
}

/**
 * Whether you're a guest: you see none of the topics, and of the settings
 * only your account.
 */
export function useIsGuest(): boolean {
  return isGuest(useWorkspace().workspace.role);
}

/**
 * Its children as a guest sees them: to look at, with nothing to tick,
 * move or remove. Settings → Blocks draws its samples so.
 */
export function LookOnly({ children }: { children: ReactNode }) {
  const context = useWorkspace();
  const value = useMemo(
    () => ({ ...context, workspace: { ...context.workspace, role: "guest" as const } }),
    [context],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
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
