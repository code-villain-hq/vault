import "fake-indexeddb/auto";
import { expect, test } from "bun:test";
import type { ModelKind, Transaction } from "@forge/types";
import { IndexedDbStore } from "./indexeddb";
import type { Row } from "./store";

const issue = (id: string, title: string): Row => ({ id, title });

test("IndexedDbStore keeps records, position and outbox between sessions", async () => {
  const name = `test-${crypto.randomUUID()}`;
  const store = new IndexedDbStore(name);
  expect(await store.load()).toMatchObject({ epoch: null, syncId: 0, outbox: [] });

  const records = new Map<ModelKind, Map<string, Row>>([
    ["Issue", new Map([["a", issue("a", "A")]])],
  ]);
  await store.reset("epoch-1", 3, records);
  await store.apply(
    [
      {
        sync_id: 4,
        transaction_id: "t",
        model: "Issue",
        model_id: "b",
        action: "insert",
        data: issue("b", "B"),
      },
      {
        sync_id: 5,
        transaction_id: "t",
        model: "Issue",
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
  expect([...(reopened.records.get("Issue")?.values() ?? [])]).toEqual([issue("b", "B")]);
  expect(reopened.outbox).toEqual([second]);
});
