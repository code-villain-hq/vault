import { HttpTransport, IndexedDbStore, SyncClient } from "@code-villain/sync-client";
import type { App, AppCreate, AppPatch, User } from "@raven/types";
import { authHeaders, credentialsFor, type Place, socketProtocols } from "./server";

// Only a workspace's settings sync. What Raven hears from the apps (calls,
// logs, checks) stays in its place and is read live (`useApi`).
export type Schema = {
  App: { record: App; create: AppCreate; patch: AppPatch };
  User: { record: User; create: never; patch: never };
};

/** Each person's copy of each workspace lives in its own database. */
export function databaseName(userId: string, workspaceId: string) {
  return `raven-${userId}-${workspaceId}`;
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
      App: () => ({
        health_url: null,
        is_self: false,
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
    },
    onReject: (_transaction, error) => options.onRefused(error.message),
  });
}
