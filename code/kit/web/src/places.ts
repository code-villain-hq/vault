// Where the API is, and how requests prove who's asking.
//
// Workspaces live in places. In the browser, and in the desktop app during
// development, the app comes from its own server: that's "here", and
// requests go to the same origin with a session cookie. The desktop app also
// runs the app's server for the workspaces on this Mac ("mac"), and the
// packaged app signs in to servers chosen at sign-in, keeping each one's
// session token in the system keychain.
//
// Every app is the same here but for its name, so each configures it once
// (`configureApp`, in its `server.ts`) before anything asks for a place.

/** What tells one app from another in its storage, headers and words. */
export type AppConfig = {
  /** The app's name in storage keys, headers and socket protocols: `glyph`. */
  key: string;
  /** The app's name in words: `Glyph`. */
  name: string;
  /** Calls a command of the desktop app (Tauri's `invoke`). */
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
};

let config: AppConfig | undefined;

/** Names the app. Its `server.ts` does this once, before any request. */
export function configureApp(app: AppConfig) {
  config = app;
}

export function appConfig(): AppConfig {
  if (!config) throw new Error("configureApp hasn't run: import the app's server.ts first.");
  return config;
}

/** True inside the Tauri desktop app. */
export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** True in the packaged desktop app, which isn't served by the app's server. */
export const bundled =
  isDesktop &&
  typeof location !== "undefined" &&
  (location.protocol === "tauri:" || location.hostname === "tauri.localhost");

export type Place = {
  /** "here", "mac", or the server's origin. */
  id: string;
  kind: "here" | "mac" | "server";
  /** Empty for "here": the app's own origin. */
  origin: string;
  token?: string;
};

/** The servers the packaged app has signed in to, by origin. */
const placesKey = () => `${appConfig().key}:places`;
/** The one server the packaged app knew before it knew several. */
const oldServerKey = () => `${appConfig().key}:server`;

const HERE: Place = { id: "here", kind: "here", origin: "" };

let places: Place[] = bundled ? [] : [HERE];
/** The place of the workspace on screen. */
let current: Place | undefined;
/** The server being signed in to, chosen on the sign-in screen. */
let signingIn: Place | undefined;
const byWorkspace = new Map<string, Place>();

function readServers(): string[] {
  try {
    const listed = JSON.parse(localStorage.getItem(placesKey()) ?? "null") as string[] | null;
    if (listed) return listed;
    const old = localStorage.getItem(oldServerKey());
    return old ? [old] : [];
  } catch {
    return [];
  }
}

function writeServers() {
  try {
    const servers = places.filter((p) => p.kind === "server").map((p) => p.origin);
    localStorage.setItem(placesKey(), JSON.stringify(servers));
    localStorage.removeItem(oldServerKey());
  } catch {}
}

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return appConfig().invoke<T>(command, args);
}

function macPlace(found: { origin: string; token: string }): Place {
  return { id: "mac", kind: "mac", origin: found.origin, token: found.token };
}

/**
 * Finds every place this app has workspaces in: here, this Mac once
 * someone has started on it (waiting while it starts), and the servers
 * the packaged app has a session for. This Mac comes first.
 */
export async function loadPlaces(): Promise<Place[]> {
  const found: Place[] = [];
  if (isDesktop) {
    const mac = await invoke<{ origin: string; token: string } | null>("this_mac").catch(
      () => null,
    );
    if (mac) found.push(macPlace(mac));
  }
  if (!bundled) found.push(HERE);
  if (bundled) {
    const servers = await Promise.all(
      readServers().map(async (origin): Promise<Place | null> => {
        const token = await invoke<string | null>("session_token", { server: origin }).catch(
          () => null,
        );
        return token ? { id: origin, kind: "server", origin, token } : null;
      }),
    );
    found.push(...servers.filter((p) => p !== null));
  }
  // The same place as before stays the same object, so what's open on it stays open.
  places = found.map(
    (place) =>
      places.find(
        (p) => p.id === place.id && p.origin === place.origin && p.token === place.token,
      ) ?? place,
  );
  if (bundled) writeServers();
  return places;
}

export function getPlaces(): Place[] {
  return places;
}

/**
 * "Start on this Mac": names the person the first time, and makes a
 * workspace there.
 */
export async function startOnThisMac(name: string, workspaceName: string): Promise<Place> {
  const started = await invoke<{ origin: string; token: string }>("start_on_this_mac", {
    name,
    workspaceName,
  }).catch((problem: unknown) => {
    // The app's commands fail with a message, not an Error.
    throw new Error(String(problem));
  });
  const place = macPlace(started);
  places = [place, ...places.filter((p) => p.kind !== "mac")];
  return place;
}

/** The Mac account's name, to suggest as yours. */
export function macUserName(): Promise<string> {
  return invoke<string>("mac_user_name").catch(() => "");
}

/** The workspace on screen is in this place. */
export function setCurrentPlace(place: Place) {
  current = place;
}

/** Which place a workspace is in, so its requests go there. */
export function placeWorkspaces(place: Place, workspaceIds: string[]) {
  for (const id of workspaceIds) byWorkspace.set(id, place);
}

/** Where signing in goes: the chosen server, or here. */
export function authPlace(): Place {
  return signingIn ?? (bundled ? { id: "", kind: "server", origin: "" } : HERE);
}

/** Where a request goes: its workspace's place, the sign-in's, or the one on screen. */
export function placeFor(path: string): Place {
  const workspace = /^\/api\/w\/([^/?]+)/.exec(path)?.[1];
  const known = workspace ? byWorkspace.get(workspace) : undefined;
  if (known) return known;
  if (path.startsWith("/api/auth/") || path.startsWith("/api/invites/")) return authPlace();
  return current ?? places[0] ?? authPlace();
}

/** The server being signed in to; empty until chosen, and in the browser. */
export function serverUrl(): string {
  return signingIn?.origin ?? "";
}

/** `https://forge.example.com`, from whatever someone typed. */
export function normalizeServer(input: string): string {
  const trimmed = input.trim().replace(/\/+$/, "");
  // Another scheme ("ftp://", "mailto:") isn't a web address; "host:8080" is.
  if (/^[a-z][a-z0-9+.-]*:(?!\d)/i.test(trimmed) && !/^https?:\/\//i.test(trimmed)) {
    throw new TypeError("not a web address");
  }
  const withScheme = /^https?:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`;
  return new URL(withScheme).origin;
}

/** Chooses the server to sign in to (packaged desktop app only). */
export function setServerUrl(url: string) {
  signingIn = places.find((p) => p.id === url) ?? { id: url, kind: "server", origin: url };
}

/** An API path in a place. */
export function apiUrl(path: string, place = placeFor(path)): string {
  return `${place.origin}${path}`;
}

/** A link to give people, which opens the app in its place rather than in this app. */
export function publicUrl(path: string, place: Place): string {
  return `${place.origin || location.origin}${path}`;
}

/** Cookies here; a bearer token everywhere else. */
export function credentialsFor(place: Place): RequestCredentials {
  return place.kind === "here" ? "include" : "omit";
}

export function authHeaders(place: Place): Record<string, string> {
  if (place.kind === "here") return {};
  const headers: Record<string, string> = { [`x-${appConfig().key}-client`]: "desktop" };
  if (place.token) headers.authorization = `Bearer ${place.token}`;
  return headers;
}

/** WebSocket subprotocols that carry the token, since sockets can't have headers. */
export function socketProtocols(place: Place): string[] | undefined {
  return place.token ? [appConfig().key, `bearer.${place.token}`] : undefined;
}

/** Keeps the token a server sent when signing in, if it sent one. */
export async function rememberToken(response: Response, place: Place): Promise<void> {
  const sent = response.headers.get(`x-${appConfig().key}-session`);
  if (place.kind !== "server" || !place.origin || !sent) return;
  place.token = sent;
  if (!places.includes(place)) places = [...places.filter((p) => p.id !== place.id), place];
  writeServers();
  await invoke("save_session_token", { server: place.origin, token: sent }).catch(() => {});
}

/** The session in a place has ended: a server is forgotten; here and this Mac stay. */
export async function forgetPlace(place: Place): Promise<void> {
  if (place.kind !== "server") return;
  place.token = undefined;
  places = places.filter((p) => p.id !== place.id);
  writeServers();
  if (signingIn?.id === place.id) signingIn = undefined;
  await invoke("forget_session_token", { server: place.origin }).catch(() => {});
}

/** The words for a place in the workspace menu. */
export function placeName(place: Place): string {
  if (place.kind === "mac") return "On this Mac";
  if (place.kind === "here") return location.host;
  return new URL(place.origin).host;
}

/** Each person's copy of each workspace lives in its own database. */
export function databaseName(userId: string, workspaceId: string) {
  return `${appConfig().key}-${userId}-${workspaceId}`;
}
