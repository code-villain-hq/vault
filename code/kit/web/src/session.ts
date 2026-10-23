// Who is signed in, in every place, kept for the app to open offline too.
import { useSyncExternalStore } from "react";
import { api, RequestError, setSessionEndedHandler } from "./api";
import {
  appConfig,
  authPlace,
  databaseName,
  forgetPlace,
  getPlaces,
  loadPlaces,
  type Place,
  placeWorkspaces,
} from "./places";
import { messageOf } from "./problems";

let signOutError: string | null = null;
const errorListeners = new Set<() => void>();
function setSignOutError(message: string | null) {
  signOutError = message;
  for (const notify of errorListeners) notify();
}
export function useSignOutError() {
  return useSyncExternalStore(
    (notify) => {
      errorListeners.add(notify);
      return () => {
        errorListeners.delete(notify);
      };
    },
    () => signOutError,
  );
}

/** What every app's `/api/me` says, at the least. */
export type SessionMe = {
  user: { id: string };
  workspaces: { id: string; url_key: string }[];
};

/** Who you are in one place. */
export type SignedIn<M extends SessionMe> = { place: Place; me: M };

export type Session<M extends SessionMe> =
  | { status: "loading" }
  | { status: "signed-out" }
  | { status: "signed-in"; places: SignedIn<M>[] };

/** A workspace, where it is, and its key in the app's addresses. */
export type WorkspaceEntry<M extends SessionMe> = SignedIn<M> & {
  workspace: M["workspaces"][number];
  key: string;
};

/**
 * Every workspace in every place, in order. The key is the workspace's own,
 * unless one before it has it: then the key names the server too
 * (`studio@forge.example.com`).
 */
export function workspaceEntries<M extends SessionMe>(places: SignedIn<M>[]): WorkspaceEntry<M>[] {
  const taken = new Set<string>();
  const entries: WorkspaceEntry<M>[] = [];
  for (const { place, me } of places) {
    for (const workspace of me.workspaces) {
      let key = workspace.url_key;
      if (taken.has(key)) key = `${key}@${place.origin ? new URL(place.origin).host : place.id}`;
      taken.add(key);
      entries.push({ place, me, workspace, key });
    }
  }
  return entries;
}

/** The session, typed with the app's own `Me`. An app makes it once, in its `session.ts`. */
export function createSession<M extends SessionMe>() {
  let session: Session<M> = { status: "loading" };
  const listeners = new Set<() => void>();

  function set(places: SignedIn<M>[]) {
    // In the order of the places: this Mac, here, then the servers.
    const order = getPlaces().map((p) => p.id);
    const sorted = [...places].sort(
      (a, b) => order.indexOf(a.place.id) - order.indexOf(b.place.id),
    );
    for (const { place, me } of sorted) {
      placeWorkspaces(
        place,
        me.workspaces.map((w) => w.id),
      );
    }
    session = sorted.length ? { status: "signed-in", places: sorted } : { status: "signed-out" };
    for (const listener of listeners) listener();
  }

  function signedInPlaces(): SignedIn<M>[] {
    return session.status === "signed-in" ? session.places : [];
  }

  function getSession(): Session<M> {
    return session;
  }

  function useSession(): Session<M> {
    return useSyncExternalStore((listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    }, getSession);
  }

  /** Asks every place who is signed in. A place that can't answer gets its last answer. */
  async function loadSession(): Promise<void> {
    const places = await loadPlaces();
    const answers = await Promise.all(
      places.map(async (place): Promise<SignedIn<M> | null> => {
        try {
          const me = await api<M>("GET", "/api/me", undefined, place);
          writeCache(place, me);
          return { place, me };
        } catch (error) {
          // Signed out there: the handler has forgotten it.
          if (error instanceof RequestError && error.status === 401) return null;
          // Offline, or the server is having trouble (a restart, a proxy error):
          // carry on with what we knew, and keep the session.
          const cached = readCache(place);
          return cached ? { place, me: cached } : null;
        }
      }),
    );
    set(answers.filter((a) => a !== null));
  }

  /** Signed in, or the answer changed (a new name, a new workspace). */
  function signedIn(me: M, place = authPlace()) {
    writeCache(place, me);
    set([...signedInPlaces().filter((s) => s.place.id !== place.id), { place, me }]);
  }

  /** The session ended in a place. Local data stays, in case the same person signs back in. */
  function ended(place: Place) {
    void forgetPlace(place);
    try {
      localStorage.removeItem(cacheKey(place));
    } catch {}
    // While loading, `loadSession` sets the answer once every place has given one.
    if (session.status === "signed-in") set(session.places.filter((s) => s.place.id !== place.id));
  }

  setSessionEndedHandler(ended);

  /** Signs out of a place and deletes this device's copies of its workspaces. */
  async function signOut(place: Place): Promise<boolean> {
    const me = signedInPlaces().find((s) => s.place.id === place.id)?.me ?? readCache(place);
    setSignOutError(null);
    try {
      await api("POST", "/api/auth/logout", undefined, place);
    } catch (error) {
      setSignOutError(`Couldn't sign out. ${messageOf(error)}`);
      return false;
    }
    // Signing out first closes the open sync clients, so their databases can go.
    ended(place);
    for (const workspace of me?.workspaces ?? []) {
      indexedDB.deleteDatabase(databaseName(me?.user.id ?? "", workspace.id));
    }
    return true;
  }

  /** The last known answer for each place, so the app opens offline too. */
  function cacheKey(place: Place) {
    // "<app>:me" is here's, as it was before there were places.
    const key = appConfig().key;
    return place.kind === "here" ? `${key}:me` : `${key}:me:${place.id}`;
  }

  function writeCache(place: Place, me: M) {
    try {
      localStorage.setItem(cacheKey(place), JSON.stringify(me));
    } catch {
      // Storage unavailable; the app still works online.
    }
  }

  function readCache(place: Place): M | null {
    try {
      const raw = localStorage.getItem(cacheKey(place));
      return raw ? (JSON.parse(raw) as M) : null;
    } catch {
      return null;
    }
  }

  const lastWorkspaceKey = () => `${appConfig().key}:last-workspace`;

  /** The workspace to open: the last one used here, or the first. */
  function defaultWorkspace(entries: WorkspaceEntry<M>[]): WorkspaceEntry<M> | undefined {
    let last: string | null = null;
    try {
      last = localStorage.getItem(lastWorkspaceKey());
    } catch {}
    return entries.find((e) => e.key === last) ?? entries[0];
  }

  function rememberWorkspace(key: string) {
    try {
      localStorage.setItem(lastWorkspaceKey(), key);
    } catch {}
  }

  /** The key of a workspace in a place, as the session has it now. */
  function workspaceKey(place: Place, workspaceId: string): string | undefined {
    return workspaceEntries(signedInPlaces()).find(
      (e) => e.place.id === place.id && e.workspace.id === workspaceId,
    )?.key;
  }

  return {
    getSession,
    useSession,
    loadSession,
    signedIn,
    signOut,
    defaultWorkspace,
    rememberWorkspace,
    workspaceKey,
  };
}
