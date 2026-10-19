import { describe, expect, test } from "bun:test";
import type { Language, Suggestion, Translation } from "@glyph/types";
import { acceptSuggestion, deleteKey, renameNamespace } from "./bulk";
import { buildCatalog, coverage, entryId, isMissing, isUnsure, sortLanguages } from "./data";
import { mergeVariants } from "./glossary";
import { concerns, orderSuggestions } from "./review";

const stamp = { workspace_id: "w", created_at: "", updated_at: "" };
const t = (
  lang: string,
  namespace: string,
  key: string,
  value: string,
  confidence: number | null = null,
): Translation => ({
  id: `${lang}/${namespace}/${key}`,
  lang,
  namespace,
  key,
  value,
  confidence,
  translated_by: confidence === null ? "human" : "viking",
  context: "",
  ...stamp,
});
const language = (code: string, is_source = false): Language => ({
  id: code,
  code,
  is_source,
  ...stamp,
});
const suggestion = (key: string, fields: Partial<Suggestion> = {}): Suggestion => ({
  id: `s-${key}`,
  run_id: null,
  lang: "sv",
  namespace: "common",
  key,
  value: `sv:${key}`,
  confidence: 0.9,
  model: "viking",
  source_text: key,
  glossary_violations: [],
  placeholders_broken: false,
  ...stamp,
  ...fields,
});

const rows = [
  t("en", "common", "ok", "OK"),
  t("en", "common", "cancel", "Cancel"),
  t("sv", "common", "ok", "OK", 0.4),
  t("en", "translation", "title", "Glyph"),
  t("sv", "auth", "only.sv", "Bara svenska"),
];

describe("the catalog", () => {
  const catalog = buildCatalog(rows);

  test("the default namespace comes first, keys sorted within each", () => {
    expect([...catalog.namespaces.keys()]).toEqual(["translation", "auth", "common"]);
    expect(catalog.namespaces.get("common")?.map((e) => e.key)).toEqual(["cancel", "ok"]);
  });

  test("a key is missing where another language has text and this one doesn't", () => {
    const cancel = catalog.entries.get(entryId("common", "cancel"));
    const onlySv = catalog.entries.get(entryId("auth", "only.sv"));
    if (!cancel || !onlySv) throw new Error("missing entries");
    expect(isMissing(cancel, "sv", "en")).toBe(true);
    expect(isMissing(cancel, "en", "en")).toBe(false);
    // Into the source, it's a back-fill.
    expect(isMissing(onlySv, "en", "en")).toBe(true);
    expect(isMissing(onlySv, "fi", "en")).toBe(false);
  });

  test("coverage counts translated, missing and unsure keys", () => {
    const languages = [language("en", true), language("sv")];
    expect(coverage(catalog, languages, "en", () => 0.7)).toEqual([
      { lang: "sv", total: 3, translated: 1, missing: 2, unsure: 1 },
    ]);
    expect(coverage(catalog, languages, "en", () => 0.7, "common")[0]?.total).toBe(2);
  });

  test("only a model's text can be unsure", () => {
    expect(isUnsure(t("sv", "c", "k", "x", 0.4), 0.7)).toBe(true);
    expect(isUnsure(t("sv", "c", "k", "x", 0.9), 0.7)).toBe(false);
    expect(isUnsure(t("sv", "c", "k", "x"), 0.7)).toBe(false);
    expect(isUnsure(t("sv", "c", "k", "", 0.1), 0.7)).toBe(false);
  });

  test("the source language comes first, the rest by name", () => {
    const sorted = sortLanguages([language("sv"), language("de"), language("en", true)]);
    expect(sorted.map((l) => l.code)).toEqual(["en", "de", "sv"]);
  });
});

describe("changes to many keys", () => {
  const waiting = [suggestion("ok"), suggestion("other", { namespace: "auth" })];

  test("renaming a namespace moves every language and drops its suggestions", () => {
    const ops = renameNamespace(rows, waiting, "common", "shared");
    expect(ops.filter((o) => o.action === "update")).toHaveLength(3);
    expect(ops.filter((o) => o.action === "delete").map((o) => o.id)).toEqual(["s-ok"]);
  });

  test("renaming into a namespace with keys is refused", () => {
    expect(() => renameNamespace(rows, waiting, "common", "auth")).toThrow("already has keys");
  });

  test("deleting a key deletes it everywhere", () => {
    expect(deleteKey(rows, waiting, "common", "ok").map((o) => o.id)).toEqual([
      "s-ok",
      "en/common/ok",
      "sv/common/ok",
    ]);
  });

  test("accepting writes the model's text and drops the suggestion", () => {
    const [write, drop] = acceptSuggestion(suggestion("cancel"), undefined, undefined, () => "n");
    expect(write).toMatchObject({
      action: "create",
      id: "n",
      data: { value: "sv:cancel", confidence: 0.9, translated_by: "viking" },
    });
    expect(drop).toEqual({ action: "delete", model: "Suggestion", id: "s-cancel" });
  });

  test("an edited proposal is a person's text", () => {
    const existing = rows[2];
    const [write] = acceptSuggestion(suggestion("ok"), existing, "Okej", () => "n");
    expect(write).toMatchObject({
      action: "update",
      id: "sv/common/ok",
      data: { value: "Okej", confidence: null, translated_by: "human" },
    });
  });
});

describe("review", () => {
  test("proposals that need a look come first", () => {
    const list = [
      suggestion("b"),
      suggestion("a"),
      suggestion("c", { confidence: 0.3 }),
      suggestion("d", {
        placeholders_broken: true,
        glossary_violations: [{ source_term: "email", target_term: "e-post" }],
      }),
    ];
    expect(orderSuggestions(list, 0.7).map((s) => s.key)).toEqual(["d", "c", "a", "b"]);
    expect(concerns(list[3] as Suggestion, 0.7)).toHaveLength(2);
  });

  test("variants merge without repeats, ignoring case", () => {
    expect(mergeVariants(["e-posten"], ["E-posten", " e-postens ", ""])).toEqual([
      "e-posten",
      "e-postens",
    ]);
  });
});
