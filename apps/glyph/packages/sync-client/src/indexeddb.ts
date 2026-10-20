import type { ModelKind, SyncEntry, Transaction } from "@glyph/types";
import { type DBSchema, type IDBPDatabase, openDB } from "idb";
import type { LocalStore, Row, StoredState } from "./store";

/** Bump when the stored shape changes; older local copies are then discarded and downloaded again. */
const VERSION = 1;

interface Schema extends DBSchema {
  records: { key: [string, string]; value: { model: ModelKind; id: string; data: Row } };
  meta: { key: string; value: string | number };
  outbox: {
    key: number;
    value: { seq?: number; transaction: Transaction };
    indexes: { by_transaction: string };
  };
}

/** Keeps the local copy of a workspace in IndexedDB. */
export class IndexedDbStore implements LocalStore {
  #db: Promise<IDBPDatabase<Schema>>;

  constructor(name: string) {
    this.#db = openDB<Schema>(name, VERSION, {
      // Another tab wants to delete or upgrade the database: let it.
      blocking: () => this.close(),
      upgrade(db) {
        for (const store of [...db.objectStoreNames]) db.deleteObjectStore(store);
        db.createObjectStore("records", { keyPath: ["model", "id"] });
        db.createObjectStore("meta");
        db.createObjectStore("outbox", { keyPath: "seq", autoIncrement: true }).createIndex(
          "by_transaction",
          "transaction.id",
        );
      },
    });
  }

  close(): void {
    void this.#db.then((db) => db.close());
  }

  async load(): Promise<StoredState> {
    const db = await this.#db;
    const tx = db.transaction(["records", "meta", "outbox"]);
    const [rows, epoch, syncId, outbox] = await Promise.all([
      tx.objectStore("records").getAll(),
      tx.objectStore("meta").get("epoch"),
      tx.objectStore("meta").get("syncId"),
      tx.objectStore("outbox").getAll(),
    ]);

    const records = new Map<ModelKind, Map<string, Row>>();
    for (const { model, id, data } of rows) {
      let byId = records.get(model);
      if (!byId) {
        byId = new Map();
        records.set(model, byId);
      }
      byId.set(id, data);
    }
    return {
      epoch: typeof epoch === "string" ? epoch : null,
      syncId: typeof syncId === "number" ? syncId : 0,
      records,
      outbox: outbox.map((item) => item.transaction),
    };
  }

  async reset(epoch: string, syncId: number, records: Map<ModelKind, Map<string, Row>>) {
    const db = await this.#db;
    const tx = db.transaction(["records", "meta"], "readwrite");
    const store = tx.objectStore("records");
    await store.clear();
    for (const [model, rows] of records) {
      for (const [id, data] of rows) void store.put({ model, id, data });
    }
    void tx.objectStore("meta").put(epoch, "epoch");
    void tx.objectStore("meta").put(syncId, "syncId");
    await tx.done;
  }

  async apply(entries: SyncEntry[], syncId: number) {
    const db = await this.#db;
    const tx = db.transaction(["records", "meta"], "readwrite");
    const store = tx.objectStore("records");
    for (const entry of entries) {
      if (entry.data) {
        void store.put({ model: entry.model, id: entry.model_id, data: entry.data as Row });
      } else {
        void store.delete([entry.model, entry.model_id]);
      }
    }
    void tx.objectStore("meta").put(syncId, "syncId");
    await tx.done;
  }

  async addToOutbox(transaction: Transaction) {
    const db = await this.#db;
    await db.add("outbox", { transaction });
  }

  async removeFromOutbox(transactionId: string) {
    const db = await this.#db;
    const tx = db.transaction("outbox", "readwrite");
    const key = await tx.store.index("by_transaction").getKey(transactionId);
    if (key !== undefined) await tx.store.delete(key);
    await tx.done;
  }
}
