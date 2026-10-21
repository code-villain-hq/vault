import type { Me, WorkspaceInfo } from "@layer/types";
import { useSyncExternalStore } from "react";
import { api, RequestError, setSessionEndedHandler } from "./api";
import {
  authPlace,
  forgetPlace,
  getPlaces,
  loadPlaces,
  type Place,
  placeWorkspaces,
} from "./server";
import { databaseName } from "./sync";

/** Who you are in one place. */
export type SignedIn = { place: Place; me: Me };

export type Session =
  | { status: "loading" }
  | { status: "signed-out" }
  | { status: "signed-in"; places: SignedIn[] };

let session: Session = { status: "loading" };
const listeners = new Set<() => void>();

function set(places: SignedIn[]) {
  // In the order of the places: this Mac, here, then the servers.
  const order = getPlaces().map((p) => p.id);
  const sorted = [...places].sort((a, b) => order.indexOf(a.place.id) - order.indexOf(b.place.id));
  for (const { place, me } of sorted) {
    placeWorkspaces(
      place,
      me.workspaces.map((w) => w.id),
    );
  }
  session = sorted.length ? { status: "signed-in", places: sorted } : { status: "signed-out" };
  for (const listener of listeners) listener();
}

function signedInPlaces(): SignedIn[] {
  return session.status === "signed-in" ? session.places : [];
}

export function getSession(): Session {
  return session;
}

export function useSession(): Session {
  return useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }, getSession);
}

/** Asks every place who is signed in. A place that can't answer gets its last answer. */
export async function loadSession(): Promise<void> {
  const places = await loadPlaces();
  const answers = await Promise.all(
    places.map(async (place): Promise<SignedIn | null> => {
      try {
        const me = await api<Me>("GET", "/api/me", undefined, place);
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
export function signedIn(me: Me, place = authPlace()) {
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
export async function signOut(place: Place): Promise<void> {
  const me = signedInPlaces().find((s) => s.place.id === place.id)?.me ?? readCache(place);
  await api("POST", "/api/auth/logout", undefined, place).catch(() => {});
  // Signing out first closes the open sync clients, so their databases can go.
  ended(place);
  for (const workspace of me?.workspaces ?? []) {
    indexedDB.deleteDatabase(databaseName(me?.user.id ?? "", workspace.id));
  }
}

/** The last known answer for each place, so the app opens offline too. */
function cacheKey(place: Place) {
  // "layer:me" is here's, as it was before there were places.
  return place.kind === "here" ? "layer:me" : `layer:me:${place.id}`;
}

function writeCache(place: Place, me: Me) {
  try {
    localStorage.setItem(cacheKey(place), JSON.stringify(me));
  } catch {
    // Storage unavailable; the app still works online.
  }
}

function readCache(place: Place): Me | null {
  try {
    const raw = localStorage.getItem(cacheKey(place));
    return raw ? (JSON.parse(raw) as Me) : null;
  } catch {
    return null;
  }
}

/** A workspace, where it is, and its key in the app's addresses. */
export type WorkspaceEntry = SignedIn & { workspace: WorkspaceInfo; key: string };

/**
 * Every workspace in every place, in order. The key is the workspace's own,
 * unless one before it has it: then the key names the server too
 * (`studio@layer.example.com`).
 */
export function workspaceEntries(places: SignedIn[]): WorkspaceEntry[] {
  const taken = new Set<string>();
  const entries: WorkspaceEntry[] = [];
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

const LAST_WORKSPACE_KEY = "layer:last-workspace";

/** The workspace to open: the last one used here, or the first. */
export function defaultWorkspace(entries: WorkspaceEntry[]): WorkspaceEntry | undefined {
  let last: string | null = null;
  try {
    last = localStorage.getItem(LAST_WORKSPACE_KEY);
  } catch {}
  return entries.find((e) => e.key === last) ?? entries[0];
}

export function rememberWorkspace(key: string) {
  try {
    localStorage.setItem(LAST_WORKSPACE_KEY, key);
  } catch {}
}

/** The key of a workspace in a place, as the session has it now. */
export function workspaceKey(place: Place, workspaceId: string): string | undefined {
  return workspaceEntries(signedInPlaces()).find(
    (e) => e.place.id === place.id && e.workspace.id === workspaceId,
  )?.key;
}
