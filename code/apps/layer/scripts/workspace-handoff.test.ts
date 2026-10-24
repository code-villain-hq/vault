import { expect, test } from "bun:test";
import { SyncClient } from "@code-villain/sync-client";
import type { Schema } from "@layer/types";
import { FakeServer } from "../../../kit/sync/client/src/fake-server";
import { MemoryStore } from "../../../kit/sync/client/src/store";
import { transferSetupWrites, type SetupWrite } from "../src/lib/workspace-handoff";

const write: SetupWrite = { id: "handoff-1", operation: { action: "create", model: "Note", id: "note-1", data: { text: "Keep this note." } } };

test("a failed disk write keeps the backend queue until it can be saved", async () => {
  class Store extends MemoryStore {
    fail = true;
    override async addToOutbox(transaction: Parameters<MemoryStore["addToOutbox"]>[0]) {
      if (this.fail) throw new Error("disk full");
      return super.addToOutbox(transaction);
    }
  }
  const store = new Store();
  const server = new FakeServer();
  const client = new SyncClient<Schema>({ store, transport: server.transport() });
  await client.start();
  server.goOffline();
  let queued = [write];
  const read = async () => queued;
  const ack = async () => { queued = []; };
  try {
    await expect(transferSetupWrites(read, ack, client)).rejects.toThrow("Can't save");
    expect(queued).toHaveLength(1);
    expect((await store.load()).outbox).toHaveLength(0);
    store.fail = false;
    client.retryNow();
    await client.saved();
    await transferSetupWrites(read, ack, client);
    expect(queued).toHaveLength(0);
    expect((await store.load()).outbox.map((t) => t.id)).toEqual([write.id]);
    expect(client.status().pending).toBe(1);
  } finally { client.stop(); }
});

test("an interrupted acknowledgement reuses the transaction after a restart", async () => {
  const store = new MemoryStore();
  const server = new FakeServer();
  let client = new SyncClient<Schema>({ store, transport: server.transport() });
  await client.start();
  server.goOffline();
  let queued = [write];
  const read = async () => queued;
  try {
    await expect(transferSetupWrites(read, async () => { throw new Error("IPC interrupted"); }, client)).rejects.toThrow("IPC interrupted");
    client.stop();
    client = new SyncClient<Schema>({ store, transport: server.transport() });
    await client.start();
    await transferSetupWrites(read, async () => { queued = []; }, client);
    expect(queued).toHaveLength(0);
    expect(client.status().pending).toBe(1);
    expect((await store.load()).outbox.map((t) => t.id)).toEqual([write.id]);
  } finally { client.stop(); }
});
