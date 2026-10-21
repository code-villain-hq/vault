// The workspace's strings, arranged the ways the pages read them: by key
// across languages, by namespace, and as coverage per language.
import type { Language, LanguagePair, Translation } from "@glyph/types";
import { useMemo } from "react";
import { useRecords } from "./workspace";

/** The namespace a key is in when none is given, as in i18next. */
export const DEFAULT_NAMESPACE = "translation";

/** How sure a model must be before its translation needs no look, unless a pair says otherwise. */
export const DEFAULT_THRESHOLD = 0.7;

const names = new Intl.DisplayNames(["en"], { type: "language" });

/** "Swedish" for `sv`, "Brazilian Portuguese" for `pt-BR`; the code itself when unknown. */
export function languageName(code: string): string {
  try {
    return names.of(code) ?? code;
  } catch {
    return code;
  }
}

/** One key across languages. */
export interface Entry {
  namespace: string;
  key: string;
  /** Each language's record, by code. */
  values: Map<string, Translation>;
}

/** Every key, by namespace, each with its text in every language. */
export interface Catalog {
  namespaces: Map<string, Entry[]>;
  /** By `entryId(namespace, key)`. */
  entries: Map<string, Entry>;
}

export const entryId = (namespace: string, key: string) => `${namespace}\u0000${key}`;

/** Namespaces sorted with the default first; keys sorted within each. */
export function buildCatalog(translations: readonly Translation[]): Catalog {
  const entries = new Map<string, Entry>();
  for (const t of translations) {
    const id = entryId(t.namespace, t.key);
    let entry = entries.get(id);
    if (!entry) {
      entry = { namespace: t.namespace, key: t.key, values: new Map() };
      entries.set(id, entry);
    }
    entry.values.set(t.lang, t);
  }
  const grouped = new Map<string, Entry[]>();
  for (const entry of entries.values()) {
    const list = grouped.get(entry.namespace) ?? [];
    list.push(entry);
    grouped.set(entry.namespace, list);
  }
  const order = [...grouped.keys()].sort((a, b) =>
    a === DEFAULT_NAMESPACE ? -1 : b === DEFAULT_NAMESPACE ? 1 : a.localeCompare(b),
  );
  const namespaces = new Map<string, Entry[]>();
  for (const name of order) {
    namespaces.set(
      name,
      (grouped.get(name) ?? []).sort((a, b) => a.key.localeCompare(b.key)),
    );
  }
  return { namespaces, entries };
}

/** The text in a language, if there is any. */
export function textOf(entry: Entry, lang: string): string {
  return entry.values.get(lang)?.value ?? "";
}

/** A key needs a translation into `target` when another language has text for it and `target` has none. */
export function isMissing(entry: Entry, target: string, source: string): boolean {
  if (textOf(entry, target).trim()) return false;
  // Into the source, keys need text that some other language has (a back-fill).
  if (target === source) return [...entry.values.values()].some((t) => t.value.trim());
  return Boolean(textOf(entry, source).trim());
}

/** A model wrote it, and wasn't as sure as the pair asks. */
export function isUnsure(translation: Translation | undefined, threshold: number): boolean {
  return (
    translation !== undefined &&
    translation.confidence !== null &&
    translation.value.trim() !== "" &&
    translation.confidence < threshold
  );
}

export interface Coverage {
  lang: string;
  /** Keys with source text. */
  total: number;
  translated: number;
  missing: number;
  unsure: number;
}

/** For each target language, how much of the source it has. */
export function coverage(
  catalog: Catalog,
  languages: readonly Language[],
  source: string,
  threshold: (target: string) => number,
  namespace?: string,
): Coverage[] {
  const entries = namespace
    ? (catalog.namespaces.get(namespace) ?? [])
    : [...catalog.entries.values()];
  const withSource = entries.filter((e) => textOf(e, source).trim());
  return languages
    .filter((l) => l.code !== source)
    .map((l) => {
      let translated = 0;
      let unsure = 0;
      for (const entry of withSource) {
        if (!textOf(entry, l.code).trim()) continue;
        translated++;
        if (isUnsure(entry.values.get(l.code), threshold(l.code))) unsure++;
      }
      return {
        lang: l.code,
        total: withSource.length,
        translated,
        missing: withSource.length - translated,
        unsure,
      };
    });
}

/** The source language first, the others by name. */
export function sortLanguages(languages: readonly Language[]): Language[] {
  return [...languages].sort((a, b) =>
    a.is_source === b.is_source
      ? languageName(a.code).localeCompare(languageName(b.code))
      : a.is_source
        ? -1
        : 1,
  );
}

export function useLanguages(): Language[] {
  const languages = useRecords("Language");
  return useMemo(() => sortLanguages(languages), [languages]);
}

/** The language keys are written in first. */
export function useSource(): string {
  const languages = useRecords("Language");
  return languages.find((l) => l.is_source)?.code ?? languages[0]?.code ?? "en";
}

export function useCatalog(): Catalog {
  const translations = useRecords("Translation");
  return useMemo(() => buildCatalog(translations), [translations]);
}

/** The pair for a direction, if one has been set up. */
export function usePair(source: string, target: string): LanguagePair | undefined {
  const pairs = useRecords("LanguagePair");
  return pairs.find((p) => p.source_lang === source && p.target_lang === target);
}

/** Each target's confidence threshold, from its pair with the source. */
export function useThresholds(source: string): (target: string) => number {
  const pairs = useRecords("LanguagePair");
  return useMemo(() => {
    const byTarget = new Map(
      pairs.filter((p) => p.source_lang === source).map((p) => [p.target_lang, p]),
    );
    return (target: string) => byTarget.get(target)?.confidence_threshold ?? DEFAULT_THRESHOLD;
  }, [pairs, source]);
}

/** "3 keys", "1 key". */
export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count.toLocaleString()} ${count === 1 ? one : many}`;
}
