// The sync protocol as the server speaks it (`code/kit/sync/src/protocol.rs`).
// `M` is the app's model names; each app's types name it once (`ModelKind`).

export type Operation<M extends string = string> =
  | { action: "create"; model: M; id: string; data: Record<string, unknown> }
  | { action: "update"; model: M; id: string; data: Record<string, unknown> }
  | { action: "delete"; model: M; id: string };

/**
 * A group of changes that succeed or fail together. The client chooses the
 * id; sending the same transaction twice applies it once.
 */
export type Transaction<M extends string = string> = { id: string; operations: Operation<M>[] };

export type TransactionResult = {
  /**
   * The sync id of the transaction's last change, or null if it changed
   * nothing. Once the client has seen this sync id, the change is confirmed.
   */
  sync_id: number | null;
};

/** A snapshot of every synced record in the workspace. */
export type Bootstrap<M extends string = string> = {
  epoch: string;
  /** The snapshot includes every change up to and including this sync id. */
  sync_id: number;
  /** Records by model. */
  records: { [K in M]?: unknown[] };
};

export type SyncAction = "insert" | "update" | "delete";

/** One change from the log. */
export type SyncEntry<M extends string = string> = {
  sync_id: number;
  transaction_id: string;
  model: M;
  model_id: string;
  action: SyncAction;
  /** The whole record after the change; null for deletes. */
  data: Record<string, unknown> | null;
};

export type ServerMessage<M extends string = string> =
  | { type: "delta"; entries: SyncEntry<M>[]; sync_id: number }
  | { type: "resync" };

/** The body of every error response. */
export type ApiError = { code: string; message: string };
