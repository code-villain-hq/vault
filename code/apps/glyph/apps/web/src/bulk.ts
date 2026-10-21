// Changes too big for one transaction, such as an import or renaming a
// namespace, go to the server in batches.
import type { SyncClient } from "@code-villain/sync-client";
import type { Operation, Suggestion, Translation } from "@glyph/types";
import type { Schema } from "./sync";

/** How many operations one transaction carries at most. */
export const BATCH = 500;

export function transactInBatches(sync: SyncClient<Schema>, operations: Operation[]): number {
  for (let i = 0; i < operations.length; i += BATCH) {
    sync.transact(operations.slice(i, i + BATCH));
  }
  return Math.ceil(operations.length / BATCH);
}

/**
 * Moves every key in a namespace to another, in every language. Waiting
 * suggestions go: they were made for the old name. Refuses to merge into a
 * namespace that has keys, so nothing is overwritten.
 */
export function renameNamespace(
  translations: readonly Translation[],
  suggestions: readonly Suggestion[],
  from: string,
  to: string,
): Operation[] {
  if (translations.some((t) => t.namespace === to)) {
    throw new Error(`“${to}” already has keys. Pick a namespace that doesn't exist yet.`);
  }
  return [
    ...translations
      .filter((t) => t.namespace === from)
      .map(
        (t): Operation => ({
          action: "update",
          model: "Translation",
          id: t.id,
          data: { namespace: to },
        }),
      ),
    ...suggestions
      .filter((s) => s.namespace === from)
      .map((s): Operation => ({ action: "delete", model: "Suggestion", id: s.id })),
  ];
}

/** Deletes a namespace's keys in every language, and their suggestions. */
export function deleteNamespace(
  translations: readonly Translation[],
  suggestions: readonly Suggestion[],
  namespace: string,
): Operation[] {
  return deleteKeys(translations, suggestions, (ns) => ns === namespace);
}

/** Deletes one key in every language, and its suggestions. */
export function deleteKey(
  translations: readonly Translation[],
  suggestions: readonly Suggestion[],
  namespace: string,
  key: string,
): Operation[] {
  return deleteKeys(translations, suggestions, (ns, k) => ns === namespace && k === key);
}

function deleteKeys(
  translations: readonly Translation[],
  suggestions: readonly Suggestion[],
  match: (namespace: string, key: string) => boolean,
): Operation[] {
  return [
    ...suggestions
      .filter((s) => match(s.namespace, s.key))
      .map((s): Operation => ({ action: "delete", model: "Suggestion", id: s.id })),
    ...translations
      .filter((t) => match(t.namespace, t.key))
      .map((t): Operation => ({ action: "delete", model: "Translation", id: t.id })),
  ];
}

/**
 * Takes a suggestion: its text becomes the translation, written by its
 * model, and the suggestion goes. `value` is the text as edited, if it was.
 */
export function acceptSuggestion(
  suggestion: Suggestion,
  existing: Translation | undefined,
  value: string = suggestion.value,
  newId: () => string,
): Operation[] {
  const edited = value !== suggestion.value;
  const fields = {
    value,
    confidence: edited ? null : suggestion.confidence,
    translated_by: edited ? "human" : suggestion.model,
  };
  return [
    existing
      ? { action: "update", model: "Translation", id: existing.id, data: fields }
      : {
          action: "create",
          model: "Translation",
          id: newId(),
          data: {
            lang: suggestion.lang,
            namespace: suggestion.namespace,
            key: suggestion.key,
            ...fields,
          },
        },
    { action: "delete", model: "Suggestion", id: suggestion.id },
  ];
}
