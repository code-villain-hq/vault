import type {
  Bootstrap,
  ServerMessage,
  SyncEntry,
  Transaction,
  TransactionResult,
} from "./protocol";
import type { Row } from "./store";
import { type FeedHandlers, RejectedError, type Transport } from "./transport";
import { uuidv7 } from "./uuid";

/**
 * An in-memory stand-in for an app's server, for tests. It follows the same
 * rules as the real one: transactions are all-or-nothing and idempotent, every
 * change goes into a log, and connected clients get each change live.
 */
export class FakeServer {
  epoch = uuidv7();
  records = new Map<string, Map<string, Row>>();
  log: SyncEntry[] = [];
  /** While false, sends fail and connections drop, as if the network were down. */
  online = true;
  /** While set, sends wait until it resolves. */
  hold?: Promise<void>;
  /** How many snapshots clients have downloaded. */
  bootstraps = 0;

  #applied = new Map<string, number | null>();
  #feeds = new Set<FeedHandlers & { after: number }>();
  #nextNumber = 1;

  /** A transport for one client. */
  transport(): Transport {
    return {
      bootstrap: async () => {
        this.#ensureOnline();
        this.bootstraps++;
        return this.bootstrap();
      },
      send: async (transaction) => {
        await this.hold;
        this.#ensureOnline();
        return this.apply(transaction);
      },
      connect: (after, epoch, handlers) => {
        const feed = { ...handlers, after };
        queueMicrotask(() => {
          if (!this.online) return handlers.close();
          this.#feeds.add(feed);
          handlers.open();
          const current = this.log.at(-1)?.sync_id ?? 0;
          if (epoch !== this.epoch || after > current) return this.#send(feed, { type: "resync" });
          this.#deliver(feed);
        });
        return () => this.#feeds.delete(feed);
      },
    };
  }

  bootstrap(): Bootstrap {
    const records: Bootstrap["records"] = {};
    for (const [model, rows] of this.records) records[model] = [...rows.values()];
    return { epoch: this.epoch, sync_id: this.log.at(-1)?.sync_id ?? 0, records };
  }

  apply(transaction: Transaction): TransactionResult {
    const previous = this.#applied.get(transaction.id);
    if (previous !== undefined) return { sync_id: previous };

    // Work on copies so a failing operation leaves nothing behind.
    const records = new Map([...this.records].map(([model, rows]) => [model, new Map(rows)]));
    const entries: SyncEntry[] = [];
    let nextSyncId = (this.log.at(-1)?.sync_id ?? 0) + 1;
    let numbers = this.#nextNumber;

    for (const op of transaction.operations) {
      let rows = records.get(op.model);
      if (!rows) {
        rows = new Map();
        records.set(op.model, rows);
      }
      const existing = rows.get(op.id);
      let data: Row | null;
      if (op.action === "create") {
        if (existing) throw new RejectedError(409, "conflict", "id already used");
        if (!String(op.data.title ?? "").trim()) {
          throw new RejectedError(422, "invalid", "title can't be empty");
        }
        data = { status: "backlog", priority: 0, description: "", ...op.data, id: op.id };
        data.number = numbers++;
      } else if (op.action === "update") {
        if (!existing) throw new RejectedError(404, "not_found", `${op.model} not found`);
        data = { ...existing, ...op.data };
      } else {
        if (!existing) continue;
        data = null;
      }
      if (data) rows.set(op.id, data);
      else rows.delete(op.id);
      entries.push({
        sync_id: nextSyncId++,
        transaction_id: transaction.id,
        model: op.model,
        model_id: op.id,
        action: op.action === "create" ? "insert" : op.action,
        data,
      });
    }

    this.records = records;
    this.#nextNumber = numbers;
    this.log.push(...entries);
    const syncId = entries.at(-1)?.sync_id ?? null;
    this.#applied.set(transaction.id, syncId);
    for (const feed of this.#feeds) this.#deliver(feed);
    return { sync_id: syncId };
  }

  /** Changes a record directly, as another client would. */
  change(model: string, id: string, data: Record<string, unknown>): void {
    this.apply({ id: uuidv7(), operations: [{ action: "update", model, id, data }] });
  }

  /** Starts the history over, like a database reset. */
  reset(): void {
    this.epoch = uuidv7();
    this.records = new Map();
    this.log = [];
    this.#applied.clear();
    this.#nextNumber = 1;
  }

  goOffline(): void {
    this.online = false;
    for (const feed of this.#feeds) feed.close();
    this.#feeds.clear();
  }

  #ensureOnline(): void {
    if (!this.online) throw new Error("network down");
  }

  #deliver(feed: FeedHandlers & { after: number }): void {
    const entries = this.log.filter((entry) => entry.sync_id > feed.after);
    const last = entries.at(-1);
    if (!last) return;
    feed.after = last.sync_id;
    this.#send(feed, { type: "delta", entries, sync_id: last.sync_id });
  }

  #send(feed: FeedHandlers, message: ServerMessage): void {
    // Deliver asynchronously, like a real socket.
    const copy = JSON.parse(JSON.stringify(message)) as ServerMessage;
    queueMicrotask(() => {
      if (this.#feeds.has(feed as FeedHandlers & { after: number })) feed.message(copy);
    });
  }
}
