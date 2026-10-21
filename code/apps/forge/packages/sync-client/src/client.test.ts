import { afterEach, describe, expect, test } from "bun:test";
import type { ModelKind, Transaction } from "@forge/types";
import { SyncClient } from "./client";
import { FakeServer } from "./fake-server";
import { MemoryStore } from "./store";
import type { RejectedError } from "./transport";

// The client doesn't care what models look like; tests use a small issue.
type TestIssue = { id: string; title: string; number: number; status: string; priority: number };
type Other = { record: { id: string }; create: object; patch: object };
type Schema = Record<Exclude<ModelKind, "Issue">, Other> & {
  Issue: {
    record: TestIssue;
    create: { title: string };
    patch: Partial<Omit<TestIssue, "id" | "number">>;
  };
};

const clients: SyncClient<Schema>[] = [];

function makeClient(
  server: FakeServer,
  store = new MemoryStore(),
  onReject?: (t: Transaction, e: RejectedError) => void,
) {
  const client = new SyncClient<Schema>({
    store,
    transport: server.transport(),
    defaults: { Issue: () => ({ number: 0, status: "backlog", priority: 0 }) },
    backoff: { min: 5, max: 20 },
    onReject,
  });
  clients.push(client);
  return client;
}

async function until(condition: () => boolean, what = "condition") {
  const deadline = Date.now() + 2000;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 2));
  }
}

afterEach(() => {
  for (const client of clients.splice(0)) client.stop();
});

describe("SyncClient", () => {
  test("a new issue shows at once, then gets its number from the server", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();

    const id = client.create("Issue", { title: "First" });

    expect(client.get("Issue", id)).toMatchObject({ title: "First", number: 0 });
    expect(client.isPending("Issue", id)).toBe(true);
    await until(() => client.status().pending === 0, "confirmation");
    expect(client.get("Issue", id)).toMatchObject({ title: "First", number: 1 });
    expect(client.isPending("Issue", id)).toBe(false);
  });

  test("a refused change is rolled back and reported", async () => {
    const server = new FakeServer();
    const refused: string[] = [];
    const client = makeClient(server, new MemoryStore(), (_, error) => refused.push(error.code));
    await client.start();

    const id = client.create("Issue", { title: "  " });
    expect(client.get("Issue", id)).toBeDefined();

    await until(() => refused.length > 0, "refusal");
    expect(refused).toEqual(["invalid"]);
    expect(client.get("Issue", id)).toBeUndefined();
    expect(client.status().pending).toBe(0);
  });

  test("changes made offline are sent in order when the connection returns", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();
    server.goOffline();
    await until(() => client.status().connection === "offline", "offline");

    const id = client.create("Issue", { title: "Draft" });
    client.update("Issue", id, { title: "Final", status: "todo" });

    expect(client.get("Issue", id)).toMatchObject({ title: "Final", status: "todo" });
    expect(client.status().pending).toBe(2);

    server.online = true;
    await until(() => client.status().pending === 0, "outbox to empty");
    expect(server.records.get("Issue")?.get(id)).toMatchObject({ title: "Final", status: "todo" });
    expect(client.status().connection).toBe("online");
  });

  test("changes from others arrive live", async () => {
    const server = new FakeServer();
    const alice = makeClient(server);
    const bob = makeClient(server);
    await Promise.all([alice.start(), bob.start()]);

    const id = alice.create("Issue", { title: "Shared" });
    await until(() => bob.get("Issue", id) !== undefined, "bob to see the issue");
    bob.update("Issue", id, { status: "done" });

    await until(() => alice.get("Issue", id)?.status === "done", "alice to see bob's change");
  });

  test("an unconfirmed local edit stays on top of a remote change to another field", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();
    const id = client.create("Issue", { title: "Original" });
    await until(() => client.status().pending === 0, "create to confirm");

    let release = () => {};
    server.hold = new Promise((resolve) => {
      release = resolve;
    });
    client.update("Issue", id, { title: "Mine" });
    server.change("Issue", id, { status: "in_progress" });

    await until(() => client.get("Issue", id)?.status === "in_progress", "remote change");
    expect(client.get("Issue", id)?.title).toBe("Mine");

    server.hold = undefined;
    release();
    await until(() => client.status().pending === 0, "edit to confirm");
    expect(client.get("Issue", id)).toMatchObject({ title: "Mine", status: "in_progress" });
  });

  test("unsent changes survive a restart", async () => {
    const server = new FakeServer();
    const store = new MemoryStore();
    const first = makeClient(server, store);
    await first.start();
    server.goOffline();
    const id = first.create("Issue", { title: "Written offline" });
    await first.saved();
    first.stop();

    server.online = true;
    const second = makeClient(server, store);
    await second.start();

    expect(second.get("Issue", id)?.title).toBe("Written offline");
    await until(() => second.status().pending === 0, "outbox to empty");
    expect(server.records.get("Issue")?.get(id)?.title).toBe("Written offline");
  });

  test("a restarted client catches up from where it left off", async () => {
    const server = new FakeServer();
    const store = new MemoryStore();
    const first = makeClient(server, store);
    await first.start();
    const id = first.create("Issue", { title: "Before" });
    await until(() => first.status().pending === 0, "create to confirm");
    await first.saved();
    first.stop();

    server.change("Issue", id, { title: "While away" });
    const second = makeClient(server, store);
    await second.start();

    await until(() => second.get("Issue", id)?.title === "While away", "catch-up");
    expect(server.bootstraps).toBe(1);
  });

  test("when the server's history starts over, the client downloads a fresh copy", async () => {
    const server = new FakeServer();
    const store = new MemoryStore();
    const first = makeClient(server, store);
    await first.start();
    first.create("Issue", { title: "Old world" });
    await until(() => first.status().pending === 0, "create to confirm");
    await first.saved();
    first.stop();

    server.reset();
    const id = server.apply({
      id: crypto.randomUUID(),
      operations: [
        { action: "create", model: "Issue", id: crypto.randomUUID(), data: { title: "New world" } },
      ],
    });
    expect(id.sync_id).toBe(1);

    const second = makeClient(server, store);
    await second.start();
    await until(() => second.query("Issue")[0]?.title === "New world", "fresh copy");
    expect(second.query("Issue")).toHaveLength(1);
  });

  test("query returns the same array until something changes", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();

    const before = client.query("Issue");
    expect(client.query("Issue")).toBe(before);
    client.create("Issue", { title: "New" });
    expect(client.query("Issue")).not.toBe(before);
    expect(client.query("Issue")).toHaveLength(1);
  });
});

describe("SyncClient across a server restart", () => {
  test("a connected client resyncs once when the server's history starts over", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();
    client.create("Issue", { title: "Before" });
    await until(() => client.status().pending === 0, "create to confirm");

    server.goOffline();
    await until(() => client.status().connection === "offline", "offline");
    server.reset();
    server.apply({
      id: crypto.randomUUID(),
      operations: [
        { action: "create", model: "Issue", id: crypto.randomUUID(), data: { title: "After" } },
      ],
    });
    const bootstrapsBefore = server.bootstraps;
    server.online = true;

    await until(() => client.query("Issue")[0]?.title === "After", "fresh copy");
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(server.bootstraps - bootstrapsBefore).toBe(1);
    expect(client.query("Issue")).toHaveLength(1);
    expect(client.status().connection).toBe("online");
  });
});
