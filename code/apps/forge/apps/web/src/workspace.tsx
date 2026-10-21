import type { SyncClient } from "@code-villain/sync-client";
import type { Me, ModelKind, WorkspaceInfo } from "@forge/types";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
} from "react";
import { loadSession } from "./session";
import { createSyncClient, type Schema } from "./sync";

interface WorkspaceContext {
  me: Me;
  workspace: WorkspaceInfo;
  sync: SyncClient<Schema>;
  /** Fires a "refused" event, with a message, when the server refuses a change. */
  refusals: EventTarget;
}

const Context = createContext<WorkspaceContext | null>(null);

/** Opens the workspace's local copy and keeps it in sync while mounted. */
export function WorkspaceProvider({
  me,
  workspace,
  children,
}: {
  me: Me;
  workspace: WorkspaceInfo;
  children: ReactNode;
}) {
  const [connection, setConnection] = useState<{
    sync: SyncClient<Schema>;
    refusals: EventTarget;
  } | null>(null);
  const userId = me.user.id;
  const workspaceId = workspace.id;

  useEffect(() => {
    const refusals = new EventTarget();
    const sync = createSyncClient({
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
  }, [userId, workspaceId]);

  if (!connection) return null;
  return <Context.Provider value={{ me, workspace, ...connection }}>{children}</Context.Provider>;
}

export function useWorkspace(): WorkspaceContext {
  const context = useContext(Context);
  if (!context) throw new Error("useWorkspace needs a WorkspaceProvider");
  return context;
}

/**
 * Whether you're an admin of the workspace. Only admins delete specs,
 * decisions, projects, services, labels and initiatives, delete issues they
 * didn't create, change team settings, and handle agents' keys and runners;
 * the server refuses the rest, so the app doesn't offer it.
 */
export function useIsAdmin(): boolean {
  return useWorkspace().workspace.role === "admin";
}

/** Whether you may delete an issue: admins any, others the ones they created. */
export function useMayDeleteIssue(): (issue: { created_by: string }) => boolean {
  const { me } = useWorkspace();
  const isAdmin = useIsAdmin();
  return useCallback(
    (issue: { created_by: string }) => isAdmin || issue.created_by === me.user.id,
    [isAdmin, me.user.id],
  );
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
