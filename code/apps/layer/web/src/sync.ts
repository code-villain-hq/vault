import { HttpTransport, IndexedDbStore, SyncClient } from "@code-villain/sync-client";
import type {
  AgentModel,
  AgentModelCreate,
  AgentModelPatch,
  Connection,
  ConnectionCreate,
  ConnectionPatch,
  LogSource,
  LogSourceCreate,
  LogSourcePatch,
  Note,
  NoteCreate,
  NotePatch,
  Repository,
  RepositoryCreate,
  RepositoryPatch,
  User,
} from "@layer/types";
import { authHeaders, credentialsFor, type Place, socketProtocols } from "./server";

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

/** Each person's copy of each workspace lives in its own database. */
export function databaseName(userId: string, workspaceId: string) {
  return `layer-${userId}-${workspaceId}`;
}

const timestamps = () => {
  const now = new Date().toISOString();
  return { created_at: now, updated_at: now };
};

export function createSyncClient(options: {
  place: Place;
  userId: string;
  workspaceId: string;
  onSessionEnded: () => void;
  onRefused: (message: string) => void;
}) {
  const workspace_id = options.workspaceId;
  return new SyncClient<Schema>({
    store: new IndexedDbStore(databaseName(options.userId, options.workspaceId)),
    transport: new HttpTransport({
      workspaceId: options.workspaceId,
      baseUrl: options.place.origin,
      onUnauthorized: options.onSessionEnded,
      credentials: credentialsFor(options.place),
      headers: () => authHeaders(options.place),
      protocols: () => socketProtocols(options.place),
    }),
    // What the server would fill in, so new records look complete at once.
    defaults: {
      Repository: () => ({
        workspace_id,
        group_name: null,
        update_by: null,
        check_command: null,
        spec_folder: null,
        specs_in_app: false,
        added_by: options.userId,
        ...timestamps(),
      }),
      Connection: () => ({ workspace_id, ...timestamps() }),
      LogSource: () => ({ workspace_id, region: null, prefix: "", athena: null, ...timestamps() }),
      AgentModel: () => ({ workspace_id, ...timestamps() }),
      Note: () => {
        const now = new Date().toISOString();
        return {
          workspace_id,
          paths: [],
          source: "",
          written_commit: null,
          written_at: now,
          checked_at: now,
          checked_commit: null,
          ...timestamps(),
        };
      },
    },
    onReject: (_transaction, error) => options.onRefused(error.message),
  });
}
