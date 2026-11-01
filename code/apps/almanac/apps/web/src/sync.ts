import type {
  Block,
  BlockCreate,
  BlockPatch,
  Message,
  MessagePatch,
  Point,
  PointCreate,
  PointPatch,
  Question,
  QuestionPatch,
  Reminder,
  ReminderCreate,
  ReminderPatch,
  Talk,
  Topic,
  TopicCreate,
  TopicPatch,
  User,
} from "@almanac/types";
import { HttpTransport, IndexedDbStore, SyncClient } from "@code-villain/sync-client";
import { databaseName } from "@code-villain/web";
import { authHeaders, credentialsFor, type Place, socketProtocols } from "./server";

// Everything a workspace keeps syncs: its topics, their blocks, what they keep,
// the talks it was kept from, and its members. The server starts topics
// and talks and writes the answers; the app reads them from its copy.
export type Schema = {
  Topic: { record: Topic; create: TopicCreate; patch: TopicPatch };
  Block: { record: Block; create: BlockCreate; patch: BlockPatch };
  Talk: { record: Talk; create: never; patch: never };
  Message: { record: Message; create: never; patch: MessagePatch };
  Point: { record: Point; create: PointCreate; patch: PointPatch };
  Question: { record: Question; create: never; patch: QuestionPatch };
  Reminder: { record: Reminder; create: ReminderCreate; patch: ReminderPatch };
  User: { record: User; create: never; patch: never };
};

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
    // biome-ignore-start lint/correctness/noComponentHookFactories: named after models, and they make records, not components
    defaults: {
      Block: () => ({
        title: "",
        position: Number.MAX_SAFE_INTEGER,
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
      Point: () => ({
        theme: "",
        source: null,
        source_url: null,
        talk_id: null,
        seen: true,
        position: Number.MAX_SAFE_INTEGER,
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
      Reminder: () => ({
        theme: null,
        at: null,
        every_days: null,
        notify: true,
        status: "on",
        message_id: null,
        replaces: [],
        last_done_on: null,
        notified_for: null,
        point_id: null,
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
    },
    // biome-ignore-end lint/correctness/noComponentHookFactories: the defaults end here
    onReject: (_transaction, error) => options.onRefused(error.message),
  });
}
