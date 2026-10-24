import { afterAll, afterEach, expect, test } from "bun:test";
import { configureApp } from "./places";
import { createSession } from "./session";

const realFetch = globalThis.fetch;
const realStorage = globalThis.localStorage;
const stored = new Map<string, string>();
const me = { user: { id: "ada" }, workspaces: [{ id: "acme", url_key: "acme" }] };
configureApp({ key: "session-test", name: "Test", invoke: async <T>() => null as T });
globalThis.localStorage = {
  getItem: (key: string) => stored.get(key) ?? null,
  setItem: (key: string, value: string) => void stored.set(key, value),
  removeItem: (key: string) => void stored.delete(key),
} as Storage;

afterEach(() => {
  globalThis.fetch = realFetch;
  stored.clear();
});
afterAll(() => {
  globalThis.localStorage = realStorage;
});

function stall() {
  let finish!: (response: Response) => void;
  let signal: AbortSignal | null | undefined;
  const called = Promise.withResolvers<void>();
  globalThis.fetch = ((_url, options) => {
    signal = options?.signal;
    called.resolve();
    return new Promise<Response>((resolve) => {
      finish = resolve;
    });
  }) as typeof fetch;
  return {
    called: called.promise,
    finish: (response: Response) => finish(response),
    signal: () => signal,
  };
}

test("cached workspaces open before a stalled request completes", async () => {
  stored.set("session-test:me", JSON.stringify(me));
  const request = stall();
  const session = createSession();
  const loading = session.loadSession();
  await request.called;
  expect(session.getSession().status).toBe("signed-in");
  expect(request.signal()).toBeInstanceOf(AbortSignal);
  request.finish(new Response(JSON.stringify(me)));
  await loading;
});

test("a refresh that explicitly ends the session removes the cached place", async () => {
  stored.set("session-test:me", JSON.stringify(me));
  const request = stall();
  const session = createSession();
  const loading = session.loadSession();
  await request.called;
  expect(session.getSession().status).toBe("signed-in");
  request.finish(new Response("{}", { status: 401 }));
  await loading;
  expect(session.getSession().status).toBe("signed-out");
  expect(stored.has("session-test:me")).toBe(false);
});
