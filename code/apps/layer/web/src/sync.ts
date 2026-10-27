import { HttpTransport, IndexedDbStore, SyncClient } from "@code-villain/sync-client";
import { databaseName } from "@code-villain/web";
import type { Schema } from "@layer/types";
import { authHeaders, credentialsFor, type Place, socketProtocols } from "./server";

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
    // biome-ignore-start lint/correctness/noComponentHookFactories: named after models, and they make records, not components
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
    // biome-ignore-end lint/correctness/noComponentHookFactories: the defaults end here
    onReject: (_transaction, error) => options.onRefused(error.message),
  });
}
