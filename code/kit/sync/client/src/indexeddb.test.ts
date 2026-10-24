import "fake-indexeddb/auto";
import { expect, test } from "bun:test";
import { IndexedDbStore } from "./indexeddb";
import type { Transaction } from "./protocol";
import type { Row } from "./store";

const task = (id: string, title: string): Row => ({ id, title });

test("IndexedDbStore keeps records, position and outbox between sessions", async () => {
  const name = `test-${crypto.randomUUID()}`;
  const store = new IndexedDbStore(name);
  expect(await store.load()).toMatchObject({ epoch: null, syncId: 0, outbox: [] });

  const records = new Map<string, Map<string, Row>>([["Task", new Map([["a", task("a", "A")]])]]);
  await store.reset("epoch-1", 3, records);
  await store.apply(
    [
      {
        sync_id: 4,
        transaction_id: "t",
        model: "Task",
        model_id: "b",
        action: "insert",
        data: task("b", "B"),
      },
      {
        sync_id: 5,
        transaction_id: "t",
        model: "Task",
        model_id: "a",
        action: "delete",
        data: null,
      },
    ],
    5,
  );
  const first: Transaction = { id: "tx-1", operations: [] };
  const second: Transaction = { id: "tx-2", operations: [] };
  await store.addToOutbox(first);
  await store.addToOutbox(second);
  await store.removeFromOutbox("tx-1");

  const reopened = await new IndexedDbStore(name).load();
  expect(reopened.epoch).toBe("epoch-1");
  expect(reopened.syncId).toBe(5);
  expect([...(reopened.records.get("Task")?.values() ?? [])]).toEqual([task("b", "B")]);
  expect(reopened.outbox).toEqual([second]);
});

test("an unavailable database can be opened again without dropping its saved outbox", async () => {
  const name = `test-${crypto.randomUUID()}`;
  const original = new IndexedDbStore(name);
  await original.addToOutbox({ id: "offline", operations: [] });
  original.close();
  const store = new IndexedDbStore(name);
  const open = indexedDB.open;
  indexedDB.open = () => {
    throw new Error("storage unavailable");
  };
  try {
    await expect(store.load()).rejects.toThrow("storage unavailable");
  } finally {
    indexedDB.open = open;
  }
  expect((await store.load()).outbox.map((t) => t.id)).toEqual(["offline"]);
  store.close();
});
