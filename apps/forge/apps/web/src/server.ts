// Where the API is, and how requests prove who's asking.
//
// In the browser, and in the desktop app during development, the app comes
// from the Forge server itself: requests go to the same origin with a
// session cookie. The packaged desktop app carries its own copy of the app,
// so it talks to a server chosen at sign-in and keeps its session token in
// the system keychain (see the "Desktop app" spec in Forge).
import { isDesktop } from "./platform";

/** True in the packaged desktop app, which isn't served by a Forge server. */
export const bundled =
  isDesktop &&
  typeof location !== "undefined" &&
  (location.protocol === "tauri:" || location.hostname === "tauri.localhost");

const SERVER_KEY = "forge:server";
export const DEFAULT_SERVER = "http://localhost:7390";

let server = bundled ? readServer() : "";
let token: string | undefined;

function readServer(): string {
  try {
    return localStorage.getItem(SERVER_KEY) ?? "";
  } catch {
    return "";
  }
}

/** The server the packaged app talks to; empty until chosen, and in the browser. */
export function serverUrl(): string {
  return server;
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
  server = url;
  token = undefined;
  try {
    localStorage.setItem(SERVER_KEY, url);
  } catch {}
}

/** An API path on the server. */
export function apiUrl(path: string): string {
  return `${server}${path}`;
}

/** A link to give people, which opens Forge on the server rather than in this app. */
export function publicUrl(path: string): string {
  return `${server || location.origin}${path}`;
}

/** Cookies in the browser; a bearer token in the packaged app. */
export const credentials: RequestCredentials = bundled ? "omit" : "include";

export function authHeaders(): Record<string, string> {
  if (!bundled) return {};
  const headers: Record<string, string> = { "x-forge-client": "desktop" };
  if (token) headers.authorization = `Bearer ${token}`;
  return headers;
}

/** WebSocket subprotocols that carry the token, since sockets can't have headers. */
export function socketProtocols(): string[] | undefined {
  return bundled && token ? ["forge", `bearer.${token}`] : undefined;
}

async function invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

/** Loads the token for the chosen server from the keychain. */
export async function loadToken(): Promise<void> {
  if (!bundled || !server) return;
  token = (await invoke<string | null>("session_token", { server }).catch(() => null)) ?? undefined;
}

/** Keeps the token the server sent when signing in, if it sent one. */
export async function rememberToken(response: Response): Promise<void> {
  const sent = response.headers.get("x-forge-session");
  if (!bundled || !sent) return;
  token = sent;
  await invoke("save_session_token", { server, token: sent }).catch(() => {});
}

export async function forgetToken(): Promise<void> {
  if (!bundled) return;
  token = undefined;
  await invoke("forget_session_token", { server }).catch(() => {});
}

/** Whether the packaged app knows which server to use and has a session for it. */
export function hasSession(): boolean {
  return !bundled || Boolean(server && token);
}
