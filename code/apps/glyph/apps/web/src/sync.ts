import { HttpTransport, IndexedDbStore, SyncClient } from "@glyph/sync-client";
import type {
  GlossaryEntry,
  GlossaryEntryCreate,
  GlossaryEntryPatch,
  Language,
  LanguageCreate,
  LanguagePair,
  LanguagePairCreate,
  LanguagePairPatch,
  LanguagePatch,
  Suggestion,
  Translation,
  TranslationCreate,
  TranslationPatch,
  TranslationRun,
  User,
} from "@glyph/types";
import { authHeaders, credentials, serverUrl, socketProtocols } from "./server";

export type Schema = {
  Language: { record: Language; create: LanguageCreate; patch: LanguagePatch };
  Translation: { record: Translation; create: TranslationCreate; patch: TranslationPatch };
  GlossaryEntry: {
    record: GlossaryEntry;
    create: GlossaryEntryCreate;
    patch: GlossaryEntryPatch;
  };
  LanguagePair: { record: LanguagePair; create: LanguagePairCreate; patch: LanguagePairPatch };
  // Only the server writes these; clients delete a suggestion to reject it.
  Suggestion: { record: Suggestion; create: never; patch: never };
  TranslationRun: { record: TranslationRun; create: never; patch: never };
  User: { record: User; create: never; patch: never };
};

/** Each person's copy of each workspace lives in its own database. */
export function databaseName(userId: string, workspaceId: string) {
  return `glyph-${userId}-${workspaceId}`;
}

const timestamps = () => {
  const now = new Date().toISOString();
  return { created_at: now, updated_at: now };
};

export function createSyncClient(options: {
  userId: string;
  workspaceId: string;
  onSessionEnded: () => void;
  onRefused: (message: string) => void;
}) {
  return new SyncClient<Schema>({
    store: new IndexedDbStore(databaseName(options.userId, options.workspaceId)),
    transport: new HttpTransport({
      workspaceId: options.workspaceId,
      baseUrl: serverUrl(),
      onUnauthorized: options.onSessionEnded,
      credentials,
      headers: authHeaders,
      protocols: socketProtocols,
    }),
    // What the server would fill in, so new records look complete at once.
    defaults: {
      Language: () => ({ is_source: false, workspace_id: options.workspaceId, ...timestamps() }),
      Translation: () => ({
        confidence: null,
        translated_by: "human",
        context: "",
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
      GlossaryEntry: () => ({
        target_variants: [],
        note: "",
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
      LanguagePair: () => ({
        model: "",
        temperature: 0.2,
        confidence_threshold: 0.7,
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
    },
    onReject: (_transaction, error) => options.onRefused(error.message),
  });
}
