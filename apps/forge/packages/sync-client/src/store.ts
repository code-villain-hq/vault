import type { ModelKind, SyncEntry, Transaction } from "@forge/types";

/** A synced record as stored: every record has an id; the rest depends on the model. */
export type Row = { id: string } & Record<string, unknown>;

/** Everything a client keeps between sessions. */
export interface StoredState {
  /** Null until the first snapshot has been downloaded. */
  epoch: string | null;
  syncId: number;
  records: Map<ModelKind, Map<string, Row>>;
  /** Transactions not yet confirmed by the server, oldest first. */
  outbox: Transaction[];
}

/** Where the client keeps its copy of the workspace: IndexedDB in the app, memory in tests. */
export interface LocalStore {
  load(): Promise<StoredState>;
  /** Replaces every record with a downloaded snapshot. */
  reset(epoch: string, syncId: number, records: Map<ModelKind, Map<string, Row>>): Promise<void>;
  /** Applies changes from the log and moves to `syncId`, all or nothing. */
  apply(entries: SyncEntry[], syncId: number): Promise<void>;
  addToOutbox(transaction: Transaction): Promise<void>;
  removeFromOutbox(transactionId: string): Promise<void>;
  /** Releases the storage, e.g. so it can be deleted on sign-out. */
  close?(): void;
}

export class MemoryStore implements LocalStore {
  #state: StoredState = { epoch: null, syncId: 0, records: new Map(), outbox: [] };

  async load(): Promise<StoredState> {
    const state = this.#state;
    return {
      epoch: state.epoch,
      syncId: state.syncId,
      records: new Map([...state.records].map(([model, rows]) => [model, new Map(rows)])),
      outbox: [...state.outbox],
    };
  }

  async reset(epoch: string, syncId: number, records: Map<ModelKind, Map<string, Row>>) {
    this.#state.epoch = epoch;
    this.#state.syncId = syncId;
    this.#state.records = new Map([...records].map(([model, rows]) => [model, new Map(rows)]));
  }

  async apply(entries: SyncEntry[], syncId: number) {
    for (const entry of entries) {
      let rows = this.#state.records.get(entry.model);
      if (!rows) {
        rows = new Map();
        this.#state.records.set(entry.model, rows);
      }
      if (entry.data) rows.set(entry.model_id, entry.data as Row);
      else rows.delete(entry.model_id);
    }
    this.#state.syncId = syncId;
  }

  async addToOutbox(transaction: Transaction) {
    this.#state.outbox.push(transaction);
  }

  async removeFromOutbox(transactionId: string) {
    this.#state.outbox = this.#state.outbox.filter((t) => t.id !== transactionId);
  }
}
