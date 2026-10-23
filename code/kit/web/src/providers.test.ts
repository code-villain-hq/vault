import { expect, test } from "bun:test";
import { configureApp } from "./places";
import { discoverProviders, type SignInProviders } from "./providers";

test("a transient failure recovers sign-in discovery without reloading or changing servers", async () => {
  configureApp({ key: "test", name: "Test", invoke: async () => null as never });
  const realFetch = globalThis.fetch;
  const events = new EventTarget();
  const seen: SignInProviders[] = [];
  let calls = 0;
  globalThis.fetch = (async () => {
    if (++calls === 1) throw new TypeError("offline");
    return Response.json({ oidc: true, company: "Acme" });
  }) as unknown as typeof fetch;
  const stop = discoverProviders("https://example.test", (value) => seen.push(value), 5, events);
  try {
    const until = Date.now() + 1000;
    while (seen.length < 2 && Date.now() < until)
      await new Promise((resolve) => setTimeout(resolve, 2));
    expect(seen[0]?.error).toContain("Can't reach Test");
    expect(seen[1]).toEqual({ oidc: true, company: "Acme" });
  } finally {
    stop();
    globalThis.fetch = realFetch;
  }
});
