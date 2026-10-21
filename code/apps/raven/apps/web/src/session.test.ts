import { afterEach, expect, test } from "bun:test";
import type { Me } from "@raven/types";
import { api } from "./api";
import { authPlace, type Place, placeFor, setCurrentPlace } from "./server";
import { getSession, loadSession, signedIn, workspaceEntries } from "./session";

// Bun has no localStorage; the session keeps its last answers there.
const stored = new Map<string, string>();
globalThis.localStorage = {
  getItem: (key: string) => stored.get(key) ?? null,
  setItem: (key: string, value: string) => void stored.set(key, value),
  removeItem: (key: string) => void stored.delete(key),
} as Storage;

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

function me(id: string, keys: string[]): Me {
  return {
    user: { id, name: id, email: `${id}@example.com` },
    workspaces: keys.map((key) => ({ id: `${id}-${key}`, name: key, url_key: key, role: "admin" })),
  } as unknown as Me;
}

function server(origin: string): Place {
  return { id: origin, kind: "server", origin, token: "t" };
}

const mac: Place = { id: "mac", kind: "mac", origin: "http://127.0.0.1:7585", token: "m" };

test("a workspace keeps its key unless one before it has it", () => {
  const keys = workspaceEntries([
    { place: mac, me: me("ada", ["villain", "notes"]) },
    { place: server("https://a.example"), me: me("ada", ["villain", "studio"]) },
    { place: server("http://b.example:7580"), me: me("ada", ["studio"]) },
  ]).map((e) => e.key);
  expect(keys).toEqual([
    "villain",
    "notes",
    "villain@a.example",
    "studio",
    "studio@b.example:7580",
  ]);
});

test("a request goes to its workspace's place, and signing in to the chosen one", () => {
  const a = server("https://a.example");
  signedIn(me("ada", ["studio"]), a);
  expect(placeFor("/api/w/ada-studio/sync/log")).toBe(a);
  expect(placeFor("/api/auth/login")).toBe(authPlace());
});

test("a workspace's pages go to its place, and the rest to the open one's", () => {
  const a = server("https://a.example");
  signedIn(me("ada", ["studio"]), a);
  setCurrentPlace(mac);
  expect(placeFor("/api/w/ada-studio/apps/forge/health")).toBe(a);
  expect(placeFor("/api/me")).toBe(mac);
  setCurrentPlace(a);
  expect(placeFor("/api/me")).toBe(a);
});

test("one server ending its session leaves the other places signed in", async () => {
  const a = server("https://a.example");
  signedIn(me("ada", ["studio"]), a);
  signedIn(me("grace", ["villain"]));
  globalThis.fetch = (async (url: string) =>
    new Response("{}", {
      status: String(url).startsWith("https://a.example") ? 401 : 200,
    })) as unknown as typeof fetch;

  await expect(api("GET", "/api/w/ada-studio/members")).rejects.toThrow();

  const now = getSession();
  expect(now.status).toBe("signed-in");
  if (now.status === "signed-in") {
    expect(now.places.map((s) => s.me.user.id)).toEqual(["grace"]);
  }
});

test("a place that can't be reached opens with its last answer", async () => {
  signedIn(me("grace", ["villain"]));
  globalThis.fetch = (async () => {
    throw new TypeError("offline");
  }) as unknown as typeof fetch;

  await loadSession();

  const now = getSession();
  expect(now.status === "signed-in" && now.places.map((s) => s.me.user.id)).toEqual(["grace"]);
});
