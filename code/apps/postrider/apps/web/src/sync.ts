import { HttpTransport, IndexedDbStore, SyncClient } from "@code-villain/sync-client";
import { databaseName } from "@code-villain/web";
import type {
  Calendar,
  CalendarPatch,
  MailAccount,
  MailAccountPatch,
  User,
} from "@postrider/types";
import { authHeaders, credentialsFor, type Place, socketProtocols } from "./server";

// Only Postrider's own records sync: the accounts and the calendar server.
// They're added and removed through the server, which signs in to them
// first (`/accounts`, `/calendar`); here they only change an account's
// name and hue, and whose colour a calendar wears. Mail and events stay on
// their servers and are read live.
export type Schema = {
  MailAccount: { record: MailAccount; create: never; patch: MailAccountPatch };
  Calendar: { record: Calendar; create: never; patch: CalendarPatch };
  User: { record: User; create: never; patch: never };
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
    onReject: (_transaction, error) => options.onRefused(error.message),
  });
}
