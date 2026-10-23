// Reading i18n JSON files into keys, and writing keys back out. Pure, so the
// tests can hold them to the rules.
import type { Operation, Translation } from "@glyph/types";

export interface ParsedKey {
  namespace: string;
  key: string;
  value: string;
}

/** A plausible namespace: letters, digits, dot, dash and underscore. */
const NAMESPACE = /^[A-Za-z0-9._-]+$/;

/**
 * Peels a locize-style `namespace:` off a key. Only a plausible namespace
 * counts, and not a URL (`https://…`), so other keys with a colon stay whole.
 */
export function splitKey(raw: string, fallback: string): { namespace: string; key: string } {
  const at = raw.indexOf(":");
  if (at <= 0) return { namespace: fallback, key: raw };
  const prefix = raw.slice(0, at);
  const rest = raw.slice(at + 1);
  if (!NAMESPACE.test(prefix) || rest === "" || rest.startsWith("/")) {
    return { namespace: fallback, key: raw };
  }
  return { namespace: prefix, key: rest };
}

/**
 * The keys in a file: a flat `{ "key": "text" }` object, or i18next's
 * nested one, whose levels join with dots. Keys may carry their namespace.
 */
export function parseFile(text: string, fallback: string): ParsedKey[] {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    throw new Error("That isn't a JSON file.");
  }
  if (typeof data !== "object" || data === null || Array.isArray(data)) {
    throw new Error("The file should hold one object of keys and texts.");
  }
  const keys: ParsedKey[] = [];
  const walk = (value: unknown, path: string) => {
    if (typeof value === "string") {
      keys.push({ ...splitKey(path, fallback), value });
    } else if (typeof value === "number" || typeof value === "boolean") {
      keys.push({ ...splitKey(path, fallback), value: String(value) });
    } else if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      for (const [k, v] of Object.entries(value)) walk(v, path ? `${path}.${k}` : k);
    } else {
      throw new Error(`“${path}” isn't text.`);
    }
  };
  walk(data, "");
  return keys;
}

export interface ImportPlan {
  operations: Operation[];
  added: number;
  changed: number;
  same: number;
  /** Texts that differ but stay, when existing texts are kept. */
  kept: number;
  namespaces: { namespace: string; count: number }[];
}

/** What importing the keys into a language would write. */
export function planImport(
  keys: ParsedKey[],
  lang: string,
  existing: readonly Translation[],
  replace: boolean,
  newId: () => string,
): ImportPlan {
  const current = new Map(
    existing.filter((t) => t.lang === lang).map((t) => [`${t.namespace}\u0000${t.key}`, t]),
  );
  // A key twice in the files: the last one wins, as JSON.parse does.
  const unique = new Map(keys.map((k) => [`${k.namespace}\u0000${k.key}`, k]));
  const plan: ImportPlan = {
    operations: [],
    added: 0,
    changed: 0,
    same: 0,
    kept: 0,
    namespaces: [],
  };
  const byNamespace = new Map<string, number>();
  for (const [id, k] of unique) {
    byNamespace.set(k.namespace, (byNamespace.get(k.namespace) ?? 0) + 1);
    const record = current.get(id);
    if (!record) {
      plan.added++;
      plan.operations.push({
        action: "create",
        model: "Translation",
        id: newId(),
        data: { lang, namespace: k.namespace, key: k.key, value: k.value },
      });
    } else if (record.value === k.value) {
      plan.same++;
    } else if (!replace && record.value.trim()) {
      plan.kept++;
    } else {
      plan.changed++;
      plan.operations.push({
        action: "update",
        model: "Translation",
        id: record.id,
        data: { value: k.value, confidence: null, translated_by: "human" },
      });
    }
  }
  plan.namespaces = [...byNamespace]
    .map(([namespace, count]) => ({ namespace, count }))
    .sort((a, b) => a.namespace.localeCompare(b.namespace));
  return plan;
}

/**
 * A language's texts as one flat JSON object, keys sorted. With
 * `namespace`, just that namespace's keys; without it, every key carries
 * its namespace (`billing:invoice.due`), so importing it puts each back.
 */
export function exportFile(
  translations: readonly Translation[],
  lang: string,
  namespace?: string,
): string {
  const rows = translations
    .filter((t) => t.lang === lang && t.value !== "" && (!namespace || t.namespace === namespace))
    .map((t) => [namespace ? t.key : `${t.namespace}:${t.key}`, t.value] as const)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return `${JSON.stringify(Object.fromEntries(rows), null, 2)}\n`;
}

/** Who wrote each text and how sure a model was, as `translations.meta.json` has it. */
export function exportMeta(translations: readonly Translation[]): string {
  const meta: Record<string, Record<string, Record<string, unknown>>> = Object.create(null);
  const sorted = [...translations].sort(
    (a, b) =>
      a.lang.localeCompare(b.lang) ||
      a.namespace.localeCompare(b.namespace) ||
      (a.key < b.key ? -1 : a.key > b.key ? 1 : 0),
  );
  for (const t of sorted) {
    if (!t.value) continue;
    const language = meta[t.lang] ?? Object.create(null);
    meta[t.lang] = language;
    const keys = language[t.namespace] ?? Object.create(null);
    language[t.namespace] = keys;
    keys[t.key] = {
      confidence: t.confidence,
      translated_by: t.translated_by,
      ...(t.context ? { context: t.context } : {}),
      timestamp: t.updated_at,
    };
  }
  return `${JSON.stringify(meta, null, 2)}\n`;
}
