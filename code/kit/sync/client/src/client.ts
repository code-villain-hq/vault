import type { Operation, ServerMessage, SyncEntry, Transaction } from "./protocol";
import type { LocalStore, Row } from "./store";
import { RejectedError, type Transport } from "./transport";
import { uuidv7 } from "./uuid";

/** For each model: its record type, and what creating and updating one accept. */
export type SchemaShape = Record<string, { record: { id: string }; create: object; patch: object }>;

/** The model names of a schema. */
export type ModelOf<S extends SchemaShape> = Extract<keyof S, string>;

export type ConnectionState = "connecting" | "online" | "offline";

export interface SyncStatus {
  connection: ConnectionState;
  /** Local changes the server hasn't confirmed yet. */
  pending: number;
  /** True once there is a local copy to show, from disk or downloaded. */
  ready: boolean;
  /** Local writes are waiting for storage to recover. */
  storageError?: string;
}

export interface SyncClientOptions<S extends SchemaShape> {
  store: LocalStore;
  transport: Transport;
  /**
   * Values for fields the server fills in (numbers, timestamps, defaults), so
   * a record looks complete before the server has confirmed it.
   */
  defaults?: { [K in ModelOf<S>]?: (data: S[K]["create"]) => Partial<S[K]["record"]> };
  /** Called when the server refuses a transaction; its changes are rolled back. */
  onReject?: (transaction: Transaction<ModelOf<S>>, error: RejectedError) => void;
  /** Retry delays in milliseconds; they double from `min` up to `max`. */
  backoff?: { min: number; max: number };
}

interface Pending {
  transaction: Transaction;
  sent: boolean;
  /** The sync id that confirms this transaction, once the server has applied it. */
  confirmAt?: number;
}

/**
 * The client side of the sync engine.
 *
 * Reads come from a local copy of the workspace, so they are instant. Writes
 * apply locally at once, wait in an outbox that survives restarts, and are sent
 * in order. What you see is the server's state with your unconfirmed changes
 * on top; when the server confirms a change (its sync id arrives on the feed),
 * it drops out of the outbox. If the server refuses one, it is rolled back.
 */
export class SyncClient<S extends SchemaShape> {
  readonly #store: LocalStore;
  readonly #transport: Transport;
  readonly #options: SyncClientOptions<S>;

  /** The server's state as far as we know it. */
  #confirmed = new Map<string, Map<string, Row>>();
  /** Local transactions not yet confirmed, oldest first. */
  #pending: Pending[] = [];
  #pendingByRecord = new Map<string, { transaction: Transaction; op: Operation }[]>();
  /** Confirmed state with pending changes applied: what the app sees. */
  #view = new Map<string, Map<string, Row>>();
  /** Cached arrays for `query`, so React gets a stable value between changes. */
  #lists = new Map<string, Row[]>();

  #epoch: string | null = null;
  #syncId = 0;
  #status: SyncStatus = { connection: "connecting", pending: 0, ready: false };
  #listeners = new Set<() => void>();
  #notifyQueued = false;

  #closeFeed?: () => void;
  #flushing = false;
  #flushTimer?: ReturnType<typeof setTimeout>;
  #flushDelay: number;
  #reconnectTimer?: ReturnType<typeof setTimeout>;
  #reconnectDelay: number;
  #stopped = false;
  /** Store writes run one after another, in the order they were made. */
  #storeQueue: Promise<void> = Promise.resolve();
  #writes: ((store: LocalStore) => Promise<void>)[] = [];
  #saving = false;
  #storageTimer?: ReturnType<typeof setTimeout>;

  constructor(options: SyncClientOptions<S>) {
    this.#options = options;
    this.#store = options.store;
    this.#transport = options.transport;
    this.#flushDelay = this.#backoff.min;
    this.#reconnectDelay = this.#backoff.min;
  }

  /** Loads the local copy, then connects. Never throws for being offline. */
  async start(): Promise<void> {
    const saved = await this.#store.load();
    this.#epoch = saved.epoch;
    this.#syncId = saved.syncId;
    this.#confirmed = saved.records;
    this.#pending = saved.outbox.map((transaction) => ({ transaction, sent: false }));
    for (const pending of this.#pending) this.#indexPending(pending.transaction);
    this.#rebuildView();
    this.#setStatus({ ready: saved.epoch !== null, pending: this.#pending.length });
    await this.#open();
  }

  /** Disconnects for good. Unsaved local writes still finish. */
  stop(): void {
    this.#stopped = true;
    void this.#storeQueue.then(() => this.#store.close?.());
    this.#closeFeed?.();
    this.#closeFeed = undefined;
    clearTimeout(this.#flushTimer);
    clearTimeout(this.#reconnectTimer);
    clearTimeout(this.#storageTimer);
  }

  /** Try to reconnect and send now, for example when the browser comes back online. */
  retryNow(): void {
    if (this.#stopped) return;
    this.#saveWrites();
    if (!this.#closeFeed) {
      clearTimeout(this.#reconnectTimer);
      this.#reconnectDelay = this.#backoff.min;
      void this.#open();
    }
    clearTimeout(this.#flushTimer);
    void this.#flush();
  }

  // Reading

  query<K extends ModelOf<S>>(model: K): S[K]["record"][] {
    let list = this.#lists.get(model);
    if (!list) {
      list = [...(this.#view.get(model)?.values() ?? [])];
      this.#lists.set(model, list);
    }
    return list as unknown as S[K]["record"][];
  }

  get<K extends ModelOf<S>>(model: K, id: string): S[K]["record"] | undefined {
    return this.#view.get(model)?.get(id) as S[K]["record"] | undefined;
  }

  /** True if the record has local changes the server hasn't confirmed. */
  isPending(model: ModelOf<S>, id: string): boolean {
    return this.#pendingByRecord.has(JSON.stringify([model, id]));
  }

  status(): SyncStatus {
    return this.#status;
  }

  /** Calls `listener` after any change to records or status. */
  subscribe(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  // Writing

  /** Creates a record and returns its id. */
  create<K extends ModelOf<S>>(model: K, data: S[K]["create"], id: string = uuidv7()): string {
    this.transact([{ action: "create", model, id, data: data as Record<string, unknown> }]);
    return id;
  }

  update<K extends ModelOf<S>>(model: K, id: string, patch: S[K]["patch"]): void {
    this.transact([{ action: "update", model, id, data: patch as Record<string, unknown> }]);
  }

  delete(model: ModelOf<S>, id: string): void {
    this.transact([{ action: "delete", model, id }]);
  }

  /** Applies operations locally at once and queues them for the server as one transaction. */
  transact(operations: Operation<ModelOf<S>>[]): string {
    const transaction: Transaction = { id: uuidv7(), operations };
    this.#pending.push({ transaction, sent: false });
    this.#indexPending(transaction);
    this.#persist((store) => store.addToOutbox(transaction));
    this.#recompute(operations.map((op) => [op.model, op.id]));
    this.#setStatus({ pending: this.#pending.length });
    void this.#flush();
    return transaction.id;
  }

  // Connection

  get #backoff() {
    return this.#options.backoff ?? { min: 500, max: 15_000 };
  }

  async #open(): Promise<void> {
    if (this.#stopped || this.#closeFeed) return;
    this.#setStatus({ connection: "connecting" });
    try {
      if (this.#epoch === null) await this.#bootstrap();
    } catch {
      this.#scheduleReconnect();
      return;
    }
    if (this.#stopped || this.#epoch === null) return;

    this.#closeFeed = this.#transport.connect(this.#syncId, this.#epoch, {
      open: () => {
        this.#reconnectDelay = this.#backoff.min;
        this.#setStatus({ connection: "online" });
        void this.#flush();
      },
      message: (message) => this.#onMessage(message),
      close: () => {
        this.#closeFeed = undefined;
        this.#setStatus({ connection: "offline" });
        this.#scheduleReconnect();
      },
    });
  }

  #scheduleReconnect(): void {
    if (this.#stopped) return;
    this.#setStatus({ connection: "offline" });
    clearTimeout(this.#reconnectTimer);
    this.#reconnectTimer = setTimeout(() => void this.#open(), this.#reconnectDelay);
    this.#reconnectDelay = Math.min(this.#reconnectDelay * 2, this.#backoff.max);
  }

  #onMessage(message: ServerMessage): void {
    switch (message.type) {
      case "delta":
        this.#applyEntries(message.entries, message.sync_id);
        break;
      case "resync":
        // Our copy can't be caught up: download a fresh snapshot.
        this.#closeFeed?.();
        this.#closeFeed = undefined;
        void this.#resync();
        break;
    }
  }

  async #resync(): Promise<void> {
    this.#setStatus({ connection: "connecting" });
    try {
      await this.#bootstrap();
    } catch {
      // Reconnecting will ask for a resync again.
      this.#scheduleReconnect();
      return;
    }
    await this.#open();
  }

  async #bootstrap(): Promise<void> {
    const snapshot = await this.#transport.bootstrap();
    const records = new Map<string, Map<string, Row>>();
    for (const [model, rows] of Object.entries(snapshot.records)) {
      records.set(model, new Map((rows as Row[]).map((row) => [row.id, row])));
    }
    if (this.#epoch !== null && this.#epoch !== snapshot.epoch) {
      // The server's history started over, so anything it confirmed before
      // may be gone. Send every pending transaction again.
      for (const p of this.#pending) {
        p.sent = false;
        p.confirmAt = undefined;
      }
    }
    this.#confirmed = records;
    this.#epoch = snapshot.epoch;
    this.#syncId = snapshot.sync_id;
    this.#persist((store) => store.reset(snapshot.epoch, snapshot.sync_id, records));
    this.#confirmUpTo(snapshot.sync_id);
    this.#rebuildView();
    this.#setStatus({ ready: true });
  }

  #applyEntries(entries: SyncEntry[], syncId: number): void {
    const touched: [string, string][] = [];
    for (const entry of entries) {
      let rows = this.#confirmed.get(entry.model);
      if (!rows) {
        rows = new Map();
        this.#confirmed.set(entry.model, rows);
      }
      if (entry.data) rows.set(entry.model_id, entry.data as Row);
      else rows.delete(entry.model_id);
      touched.push([entry.model, entry.model_id]);
    }
    this.#syncId = syncId;
    this.#persist((store) => store.apply(entries, syncId));
    touched.push(...this.#confirmUpTo(syncId));
    this.#recompute(touched);
  }

  // Sending

  async #flush(): Promise<void> {
    if (this.#flushing || this.#stopped) return;
    this.#flushing = true;
    try {
      for (let next = this.#nextUnsent(); next; next = this.#nextUnsent()) {
        try {
          const result = await this.#transport.send(next.transaction);
          next.sent = true;
          this.#flushDelay = this.#backoff.min;
          if (result.sync_id === null || result.sync_id <= this.#syncId) {
            this.#settle([next]);
          } else {
            next.confirmAt = result.sync_id;
          }
        } catch (error) {
          if (error instanceof RejectedError) {
            this.#settle([next]);
            this.#options.onReject?.(next.transaction as Transaction<ModelOf<S>>, error);
            continue;
          }
          this.#scheduleFlush();
          return;
        }
      }
    } finally {
      this.#flushing = false;
    }
  }

  #nextUnsent(): Pending | undefined {
    return this.#stopped ? undefined : this.#pending.find((p) => !p.sent);
  }

  #scheduleFlush(): void {
    clearTimeout(this.#flushTimer);
    this.#flushTimer = setTimeout(() => void this.#flush(), this.#flushDelay);
    this.#flushDelay = Math.min(this.#flushDelay * 2, this.#backoff.max);
  }

  /** Drops transactions the feed has caught up with; returns the records they touched. */
  #confirmUpTo(syncId: number): [string, string][] {
    const done = this.#pending.filter((p) => p.confirmAt !== undefined && p.confirmAt <= syncId);
    return this.#settle(done, false);
  }

  /** Removes transactions from the outbox, confirmed or refused. */
  #settle(done: Pending[], recompute = true): [string, string][] {
    if (done.length === 0) return [];
    const settled = new Set(done);
    this.#pending = this.#pending.filter((p) => !settled.has(p));
    const removed = new Set(done.flatMap((p) => p.transaction.operations));
    const transactions = new Set(done.map((p) => p.transaction));
    const affected = new Set([...removed].map((op) => JSON.stringify([op.model, op.id])));
    for (const key of affected) {
      const remaining =
        this.#pendingByRecord.get(key)?.filter((entry) => !transactions.has(entry.transaction)) ??
        [];
      if (remaining.length) this.#pendingByRecord.set(key, remaining);
      else this.#pendingByRecord.delete(key);
    }
    for (const p of done) this.#persist((store) => store.removeFromOutbox(p.transaction.id));
    const touched = done.flatMap((p) =>
      p.transaction.operations.map((op): [string, string] => [op.model, op.id]),
    );
    if (recompute) this.#recompute(touched);
    this.#setStatus({ pending: this.#pending.length });
    return touched;
  }

  // The view

  #rebuildView(): void {
    this.#view = new Map([...this.#confirmed].map(([model, rows]) => [model, new Map(rows)]));
    this.#lists.clear();
    const touched = this.#pending.flatMap((p) =>
      p.transaction.operations.map((op): [string, string] => [op.model, op.id]),
    );
    this.#recompute(touched);
    this.#queueNotify();
  }

  #indexPending(transaction: Transaction): void {
    for (const op of transaction.operations) {
      const key = JSON.stringify([op.model, op.id]);
      const pending = this.#pendingByRecord.get(key) ?? [];
      pending.push({ transaction, op });
      this.#pendingByRecord.set(key, pending);
    }
  }

  /** Replays only this record's pending operations, in transaction order. */
  #recompute(keys: [string, string][]): void {
    const seen = new Set<string>();
    for (const [model, id] of keys) {
      const key = JSON.stringify([model, id]);
      if (seen.has(key)) continue;
      seen.add(key);
      let row = this.#confirmed.get(model)?.get(id);
      for (const { op } of this.#pendingByRecord.get(key) ?? []) {
        if (op.action === "create") row ??= this.#optimistic(model, id, op.data);
        else if (op.action === "update") row = row && { ...row, ...op.data };
        else row = undefined;
      }

      let rows = this.#view.get(model);
      if (!rows) {
        rows = new Map();
        this.#view.set(model, rows);
      }
      if (row) rows.set(id, row);
      else rows.delete(id);
      this.#lists.delete(model);
    }
    if (keys.length > 0) this.#queueNotify();
  }

  #optimistic(model: string, id: string, data: Record<string, unknown>): Row {
    const defaults = this.#options.defaults?.[model as ModelOf<S>]?.(data as never) ?? {};
    return { ...defaults, ...data, id };
  }

  // Notifying

  #setStatus(change: Partial<SyncStatus>): void {
    const next = { ...this.#status, ...change };
    const same = (Object.keys(next) as (keyof SyncStatus)[]).every(
      (key) => next[key] === this.#status[key],
    );
    if (same) return;
    this.#status = next;
    this.#queueNotify();
  }

  /** Listeners run once per tick, however many changes happened in it. */
  #queueNotify(): void {
    if (this.#notifyQueued) return;
    this.#notifyQueued = true;
    queueMicrotask(() => {
      this.#notifyQueued = false;
      for (const listener of this.#listeners) listener();
    });
  }

  #persist(write: (store: LocalStore) => Promise<void>): void {
    this.#writes.push(write);
    this.#saveWrites();
  }

  #saveWrites(): void {
    if (this.#saving || this.#stopped) return;
    clearTimeout(this.#storageTimer);
    this.#saving = true;
    this.#storeQueue = (async () => {
      try {
        while (this.#writes.length) {
          await this.#writes[0]?.(this.#store);
          this.#writes.shift();
        }
        this.#setStatus({ storageError: undefined });
      } catch {
        // Keep the failed write and everything behind it in order. In
        // particular, never remove an outbox entry before its add is saved.
        this.#setStatus({
          storageError:
            "Can't save on this device. Free up storage. Your changes are still open here.",
        });
        if (!this.#stopped)
          this.#storageTimer = setTimeout(() => this.#saveWrites(), this.#backoff.max);
      } finally {
        this.#saving = false;
      }
    })();
  }

  /** Resolves only when the queued writes are durable; rejects on storage failure. */
  async saved(): Promise<void> {
    await this.#storeQueue;
    if (this.#writes.length)
      throw new Error(this.#status.storageError ?? "Local changes haven't been saved.");
  }
}
