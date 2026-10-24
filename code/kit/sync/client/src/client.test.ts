import { afterEach, describe, expect, test } from "bun:test";
import { SyncClient } from "./client";
import { FakeServer } from "./fake-server";
import type { Transaction } from "./protocol";
import { MemoryStore } from "./store";
import type { RejectedError } from "./transport";

// The client doesn't care what models look like; tests use a small task.
type Task = { id: string; title: string; number: number; status: string; priority: number };
type Schema = {
  Task: {
    record: Task;
    create: { title: string };
    patch: Partial<Omit<Task, "id" | "number">>;
  };
  Note: { record: { id: string }; create: object; patch: object };
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
    defaults: { Task: () => ({ number: 0, status: "backlog", priority: 0 }) },
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
  test("a new task shows at once, then gets its number from the server", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();

    const id = client.create("Task", { title: "First" });

    expect(client.get("Task", id)).toMatchObject({ title: "First", number: 0 });
    expect(client.isPending("Task", id)).toBe(true);
    await until(() => client.status().pending === 0, "confirmation");
    expect(client.get("Task", id)).toMatchObject({ title: "First", number: 1 });
    expect(client.isPending("Task", id)).toBe(false);
  });

  test("a refused change is rolled back and reported", async () => {
    const server = new FakeServer();
    const refused: string[] = [];
    const client = makeClient(server, new MemoryStore(), (_, error) => refused.push(error.code));
    await client.start();

    const id = client.create("Task", { title: "  " });
    expect(client.get("Task", id)).toBeDefined();

    await until(() => refused.length > 0, "refusal");
    expect(refused).toEqual(["invalid"]);
    expect(client.get("Task", id)).toBeUndefined();
    expect(client.status().pending).toBe(0);
  });

  test("changes made offline are sent in order when the connection returns", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();
    server.goOffline();
    await until(() => client.status().connection === "offline", "offline");

    const id = client.create("Task", { title: "Draft" });
    client.update("Task", id, { title: "Final", status: "todo" });

    expect(client.get("Task", id)).toMatchObject({ title: "Final", status: "todo" });
    expect(client.status().pending).toBe(2);

    server.online = true;
    await until(() => client.status().pending === 0, "outbox to empty");
    expect(server.records.get("Task")?.get(id)).toMatchObject({ title: "Final", status: "todo" });
    expect(client.status().connection).toBe("online");
  });

  test("changes from others arrive live", async () => {
    const server = new FakeServer();
    const alice = makeClient(server);
    const bob = makeClient(server);
    await Promise.all([alice.start(), bob.start()]);

    const id = alice.create("Task", { title: "Shared" });
    await until(() => bob.get("Task", id) !== undefined, "bob to see the task");
    bob.update("Task", id, { status: "done" });

    await until(() => alice.get("Task", id)?.status === "done", "alice to see bob's change");
  });

  test("an unconfirmed local edit stays on top of a remote change to another field", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();
    const id = client.create("Task", { title: "Original" });
    await until(() => client.status().pending === 0, "create to confirm");

    let release = () => {};
    server.hold = new Promise((resolve) => {
      release = resolve;
    });
    client.update("Task", id, { title: "Mine" });
    server.change("Task", id, { status: "in_progress" });

    await until(() => client.get("Task", id)?.status === "in_progress", "remote change");
    expect(client.get("Task", id)?.title).toBe("Mine");

    server.hold = undefined;
    release();
    await until(() => client.status().pending === 0, "edit to confirm");
    expect(client.get("Task", id)).toMatchObject({ title: "Mine", status: "in_progress" });
  });

  test("unsent changes survive a restart", async () => {
    const server = new FakeServer();
    const store = new MemoryStore();
    const first = makeClient(server, store);
    await first.start();
    server.goOffline();
    const id = first.create("Task", { title: "Written offline" });
    await first.saved();
    first.stop();

    server.online = true;
    const second = makeClient(server, store);
    await second.start();

    expect(second.get("Task", id)?.title).toBe("Written offline");
    await until(() => second.status().pending === 0, "outbox to empty");
    expect(server.records.get("Task")?.get(id)?.title).toBe("Written offline");
  });

  test("a restarted client catches up from where it left off", async () => {
    const server = new FakeServer();
    const store = new MemoryStore();
    const first = makeClient(server, store);
    await first.start();
    const id = first.create("Task", { title: "Before" });
    await until(() => first.status().pending === 0, "create to confirm");
    await first.saved();
    first.stop();

    server.change("Task", id, { title: "While away" });
    const second = makeClient(server, store);
    await second.start();

    await until(() => second.get("Task", id)?.title === "While away", "catch-up");
    expect(server.bootstraps).toBe(1);
  });

  test("when the server's history starts over, the client downloads a fresh copy", async () => {
    const server = new FakeServer();
    const store = new MemoryStore();
    const first = makeClient(server, store);
    await first.start();
    first.create("Task", { title: "Old world" });
    await until(() => first.status().pending === 0, "create to confirm");
    await first.saved();
    first.stop();

    server.reset();
    const id = server.apply({
      id: crypto.randomUUID(),
      operations: [
        { action: "create", model: "Task", id: crypto.randomUUID(), data: { title: "New world" } },
      ],
    });
    expect(id.sync_id).toBe(1);

    const second = makeClient(server, store);
    await second.start();
    await until(() => second.query("Task")[0]?.title === "New world", "fresh copy");
    expect(second.query("Task")).toHaveLength(1);
  });

  test("query returns the same array until something changes", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();

    const before = client.query("Task");
    expect(client.query("Task")).toBe(before);
    client.create("Task", { title: "New" });
    expect(client.query("Task")).not.toBe(before);
    expect(client.query("Task")).toHaveLength(1);
  });
});

describe("SyncClient across a server restart", () => {
  test("a connected client resyncs once when the server's history starts over", async () => {
    const server = new FakeServer();
    const client = makeClient(server);
    await client.start();
    client.create("Task", { title: "Before" });
    await until(() => client.status().pending === 0, "create to confirm");

    server.goOffline();
    await until(() => client.status().connection === "offline", "offline");
    server.reset();
    server.apply({
      id: crypto.randomUUID(),
      operations: [
        { action: "create", model: "Task", id: crypto.randomUUID(), data: { title: "After" } },
      ],
    });
    const bootstrapsBefore = server.bootstraps;
    server.online = true;

    await until(() => client.query("Task")[0]?.title === "After", "fresh copy");
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(server.bootstraps - bootstrapsBefore).toBe(1);
    expect(client.query("Task")).toHaveLength(1);
    expect(client.status().connection).toBe("online");
  });
});

test("failed local writes remain queued, report failure, and survive restart after storage recovers", async () => {
  class FullStore extends MemoryStore {
    full = true;
    async addToOutbox(transaction: Transaction) {
      if (this.full) throw new Error("quota exceeded");
      return super.addToOutbox(transaction);
    }
  }
  const server = new FakeServer();
  const store = new FullStore();
  const client = makeClient(server, store);
  await client.start();
  server.online = false;
  const id = client.create("Task", { title: "Offline work" });
  await expect(client.saved()).rejects.toThrow("Can't save on this device");
  expect(client.status().storageError).toBeDefined();
  expect((await store.load()).outbox).toHaveLength(0);
  store.full = false;
  client.retryNow();
  await until(() => !client.status().storageError, "storage recovery");
  await client.saved();
  expect((await store.load()).outbox).toHaveLength(1);
  client.stop();
  const reopened = makeClient(server, store);
  await reopened.start();
  expect(reopened.get("Task", id)?.title).toBe("Offline work");
});

test("bulk replay touches each record once and preserves operation order", async () => {
  const server = new FakeServer();
  server.online = false;
  let creates = 0;
  const client = new SyncClient<Schema>({
    store: new MemoryStore(),
    transport: server.transport(),
    defaults: {
      Task: () => {
        creates++;
        return { number: 0, status: "todo", priority: 0 };
      },
    },
  });
  try {
    client.transact(
      Array.from({ length: 1000 }, (_, i) => ({
        action: "create",
        model: "Task",
        id: String(i),
        data: { title: "original" },
      })),
    );
    const before = creates;
    client.transact([
      { action: "update", model: "Task", id: "0", data: { title: "first" } },
      { action: "update", model: "Task", id: "0", data: { title: "last" } },
    ]);
    expect(creates - before).toBe(1);
    expect(client.get("Task", "0")?.title).toBe("last");
    expect(client.get("Task", "1")?.title).toBe("original");
    await client.saved();
  } finally {
    client.stop();
  }
});
