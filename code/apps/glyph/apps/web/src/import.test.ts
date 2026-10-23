import { describe, expect, test } from "bun:test";
import type { Translation } from "@glyph/types";
import { exportFile, exportMeta, parseFile, planImport, splitKey } from "./import";

const t = (lang: string, namespace: string, key: string, value: string): Translation => ({
  id: `${lang}/${namespace}/${key}`,
  workspace_id: "w",
  lang,
  namespace,
  key,
  value,
  confidence: null,
  translated_by: "human",
  context: "",
  created_at: "2026-10-20T00:00:00Z",
  updated_at: "2026-10-20T00:00:00Z",
});

let n = 0;
const newId = () => `new-${++n}`;

describe("splitKey", () => {
  test("a locize prefix names the namespace", () => {
    expect(splitKey("billing:invoice.due", "translation")).toEqual({
      namespace: "billing",
      key: "invoice.due",
    });
    expect(splitKey("url:help:en", "translation")).toEqual({ namespace: "url", key: "help:en" });
  });

  test("keys that only look like one stay whole", () => {
    for (const raw of ["https://example.com", "no prefix", ":lead", "billing:", "a b:c"]) {
      expect(splitKey(raw, "common")).toEqual({ namespace: "common", key: raw });
    }
  });
});

describe("parseFile", () => {
  test("flat and nested files give the same keys", () => {
    const flat = parseFile('{"login.title":"Sign in","common:ok":"OK"}', "auth");
    const nested = parseFile('{"login":{"title":"Sign in"},"common:ok":"OK"}', "auth");
    expect(flat).toEqual([
      { namespace: "auth", key: "login.title", value: "Sign in" },
      { namespace: "common", key: "ok", value: "OK" },
    ]);
    expect(nested).toEqual(flat);
  });

  test("a value keeps its quotes and colons", () => {
    expect(parseFile('{"hint":"Say \\"hi\\": HH:mm"}', "t")[0]?.value).toBe('Say "hi": HH:mm');
  });

  test("files that aren't keys and texts are refused", () => {
    expect(() => parseFile("nope", "t")).toThrow("isn't a JSON file");
    expect(() => parseFile("[1]", "t")).toThrow("one object");
    expect(() => parseFile('{"a":[1]}', "t")).toThrow("“a” isn't text");
  });
});

describe("planImport", () => {
  const existing = [
    t("sv", "common", "ok", "OK"),
    t("sv", "common", "no", "Nej"),
    t("en", "common", "ok", "OK"),
  ];
  const keys = [
    { namespace: "common", key: "ok", value: "OK" },
    { namespace: "common", key: "no", value: "Nä" },
    { namespace: "auth", key: "in", value: "Logga in" },
  ];

  test("adds new keys and replaces changed ones", () => {
    const plan = planImport(keys, "sv", existing, true, newId);
    expect([plan.added, plan.changed, plan.same, plan.kept]).toEqual([1, 1, 1, 0]);
    expect(plan.operations).toHaveLength(2);
    expect(plan.namespaces).toEqual([
      { namespace: "auth", count: 1 },
      { namespace: "common", count: 2 },
    ]);
  });

  test("keeps texts that are there when asked", () => {
    const plan = planImport(keys, "sv", existing, false, newId);
    expect([plan.added, plan.changed, plan.kept]).toEqual([1, 0, 1]);
    expect(plan.operations.every((op) => op.action === "create")).toBe(true);
  });
});

describe("export", () => {
  const rows = [t("sv", "common", "b", "B"), t("sv", "auth", "a", "A"), t("sv", "common", "e", "")];

  test("one namespace gives bare keys; every namespace gives prefixed ones", () => {
    expect(JSON.parse(exportFile(rows, "sv", "common"))).toEqual({ b: "B" });
    expect(Object.keys(JSON.parse(exportFile(rows, "sv")))).toEqual(["auth:a", "common:b"]);
  });

  test("an export imports back to the same keys", () => {
    const back = parseFile(exportFile(rows, "sv"), "translation");
    expect(planImport(back, "sv", rows, true, newId).same).toBe(2);
  });

  test("the meta file nests language, namespace and key", () => {
    const meta = JSON.parse(exportMeta(rows));
    expect(meta.sv.common.b.translated_by).toBe("human");
    expect(meta.sv.common.e).toBeUndefined();
  });
});

test("metadata preserves special namespaces and keys without changing prototypes", () => {
  const before = Object.getOwnPropertyNames(Object.prototype);
  const rows = [
    t("sv", "__proto__", "vaultProbe", "Hej"),
    t("sv", "constructor", "__proto__", "Ja"),
    t("sv", "common", "__proto__", "Nej"),
  ];
  const meta = JSON.parse(exportMeta(rows));
  expect(
    Object.getOwnPropertyDescriptor(meta.sv, "__proto__")?.value.vaultProbe.translated_by,
  ).toBe("human");
  expect(
    Object.getOwnPropertyDescriptor(meta.sv.constructor, "__proto__")?.value.confidence,
  ).toBeNull();
  expect(Object.hasOwn(meta.sv.common, "__proto__")).toBe(true);
  expect(Object.getOwnPropertyNames(Object.prototype)).toEqual(before);
});
