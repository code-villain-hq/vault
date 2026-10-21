// The sync protocol (from `@code-villain/sync-client`), named with Glyph's models.
import type { Sync } from "@code-villain/sync-client";
import type { ModelKind } from "./generated/ModelKind";

export type Operation = Sync.Operation<ModelKind>;
export type Transaction = Sync.Transaction<ModelKind>;
export type TransactionResult = Sync.TransactionResult;
export type Bootstrap = Sync.Bootstrap<ModelKind>;
export type SyncAction = Sync.SyncAction;
export type SyncEntry = Sync.SyncEntry<ModelKind>;
export type ServerMessage = Sync.ServerMessage<ModelKind>;
export type ApiError = Sync.ApiError;
