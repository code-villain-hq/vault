// The sync protocol (from `@code-villain/sync-client`), named with Layer's
// models, and the schema both of Layer's sync clients use: the web app's and
// the desktop window's.
import type { Sync } from "@code-villain/sync-client";
import type { AgentModel } from "./generated/AgentModel";
import type { AgentModelCreate } from "./generated/AgentModelCreate";
import type { AgentModelPatch } from "./generated/AgentModelPatch";
import type { Connection } from "./generated/Connection";
import type { ConnectionCreate } from "./generated/ConnectionCreate";
import type { ConnectionPatch } from "./generated/ConnectionPatch";
import type { LogSource } from "./generated/LogSource";
import type { LogSourceCreate } from "./generated/LogSourceCreate";
import type { LogSourcePatch } from "./generated/LogSourcePatch";
import type { ModelKind } from "./generated/ModelKind";
import type { Note } from "./generated/Note";
import type { NoteCreate } from "./generated/NoteCreate";
import type { NotePatch } from "./generated/NotePatch";
import type { Repository } from "./generated/Repository";
import type { RepositoryCreate } from "./generated/RepositoryCreate";
import type { RepositoryPatch } from "./generated/RepositoryPatch";
import type { User } from "./generated/User";

export type Operation = Sync.Operation<ModelKind>;
export type Transaction = Sync.Transaction<ModelKind>;
export type TransactionResult = Sync.TransactionResult;
export type Bootstrap = Sync.Bootstrap<ModelKind>;
export type SyncAction = Sync.SyncAction;
export type SyncEntry = Sync.SyncEntry<ModelKind>;
export type ServerMessage = Sync.ServerMessage<ModelKind>;
export type ApiError = Sync.ApiError;

// What a team shares syncs. Tasks, panes, runs and messages stay on the
// Mac that runs them, and never reach a server.
export type Schema = {
  Repository: { record: Repository; create: RepositoryCreate; patch: RepositoryPatch };
  Connection: { record: Connection; create: ConnectionCreate; patch: ConnectionPatch };
  LogSource: { record: LogSource; create: LogSourceCreate; patch: LogSourcePatch };
  AgentModel: { record: AgentModel; create: AgentModelCreate; patch: AgentModelPatch };
  Note: { record: Note; create: NoteCreate; patch: NotePatch };
  User: { record: User; create: never; patch: never };
};
