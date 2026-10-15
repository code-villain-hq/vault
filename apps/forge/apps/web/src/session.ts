import type { Me } from "@forge/types";
import { useSyncExternalStore } from "react";
import { api, RequestError, setSessionEndedHandler } from "./api";
import { forgetToken, hasSession, loadToken } from "./server";
import { databaseName } from "./sync";

export type Session =
  | { status: "loading" }
  | { status: "signed-out" }
  | { status: "signed-in"; me: Me };

/** The last known signed-in user, so the app opens offline too. */
const CACHE_KEY = "forge:me";

let session: Session = { status: "loading" };
const listeners = new Set<() => void>();

function set(next: Session) {
  session = next;
  for (const listener of listeners) listener();
}

export function useSession(): Session {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => session,
  );
}

/** Asks the server who is signed in. Offline, trusts the last answer. */
export async function loadSession(): Promise<void> {
  await loadToken();
  if (!hasSession()) {
    // The packaged app hasn't signed in to a server yet.
    set({ status: "signed-out" });
    return;
  }
  try {
    signedIn(await api<Me>("GET", "/api/me"));
  } catch (error) {
    if (error instanceof RequestError && error.status === 401) {
      ended();
      return;
    }
    // Offline, or the server is having trouble (a restart, a proxy error):
    // carry on with what we knew, and keep the session.
    const cached = readCache();
    set(cached ? { status: "signed-in", me: cached } : { status: "signed-out" });
  }
}

export function signedIn(me: Me) {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(me));
  } catch {
    // Storage unavailable; the app still works online.
  }
  set({ status: "signed-in", me });
}

/** The session ended on the server. Local data stays, in case the same person signs back in. */
function ended() {
  void forgetToken();
  try {
    localStorage.removeItem(CACHE_KEY);
  } catch {}
  set({ status: "signed-out" });
}

setSessionEndedHandler(ended);

/** Signs out and deletes this device's copies of the user's workspaces. */
export async function signOut(): Promise<void> {
  const me = session.status === "signed-in" ? session.me : readCache();
  await api("POST", "/api/auth/logout").catch(() => {});
  // Signing out first closes the open sync clients, so their databases can go.
  ended();
  for (const workspace of me?.workspaces ?? []) {
    indexedDB.deleteDatabase(databaseName(me?.user.id ?? "", workspace.id));
  }
}

function readCache(): Me | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    return raw ? (JSON.parse(raw) as Me) : null;
  } catch {
    return null;
  }
}

const LAST_WORKSPACE_KEY = "forge:last-workspace";

/** The workspace to open: the last one used here, or the first. */
export function defaultWorkspace(me: Me) {
  let last: string | null = null;
  try {
    last = localStorage.getItem(LAST_WORKSPACE_KEY);
  } catch {}
  return me.workspaces.find((w) => w.url_key === last) ?? me.workspaces[0];
}

export function rememberWorkspace(urlKey: string) {
  try {
    localStorage.setItem(LAST_WORKSPACE_KEY, urlKey);
  } catch {}
}
