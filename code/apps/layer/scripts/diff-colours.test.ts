import { describe, expect, test } from "bun:test";
import { hunkSides, parseDiff, wordMarks } from "../src/lib/diff";

const patch = `diff --git a/c.ts b/c.ts
--- a/c.ts
+++ b/c.ts
@@ -1,4 +1,5 @@
 import { luminance } from "./color";
+/** WCAG contrast, 1 to 21. */
 export function contrast(a: string, b: string): number {
-  const [low, high] = [luminance(a), luminance(b)].sort();
+  const [low, high] = [luminance(a), luminance(b)].sort((x, y) => x - y);
   return (high + 0.05) / (low + 0.05);
@@ -9,2 +10,2 @@
-export const FLOOR = 4.5;
+export const FLOOR = 4.6;
 export const CEILING = 21;
`;
const lines = parseDiff(patch);
const at = (text: string) => lines.findIndex((l) => l.text === text);
const marked = (i: number) => {
  const m = wordMarks(lines).get(i);
  return m && lines[i].text.slice(1).slice(m[0], m[1]);
};

describe("a hunk coloured as code", () => {
  test("reads the old side as context and removed lines, the new as context and added", () => {
    const [first, second] = hunkSides(lines);
    const text = (ids: number[]) => ids.map((i) => lines[i].text.slice(1));
    expect(text(first.old)).toEqual([
      'import { luminance } from "./color";',
      "export function contrast(a: string, b: string): number {",
      "  const [low, high] = [luminance(a), luminance(b)].sort();",
      "  return (high + 0.05) / (low + 0.05);",
    ]);
    expect(text(first.new)).toContain("/** WCAG contrast, 1 to 21. */");
    expect(text(first.new)).not.toContain("  const [low, high] = [luminance(a), luminance(b)].sort();");
    expect(text(second.old)).toEqual(["export const FLOOR = 4.5;", "export const CEILING = 21;"]);
  });

  test("leaves the file's header out of every side", () => {
    const all = hunkSides(lines).flatMap((s) => [...s.old, ...s.new]);
    expect(all.some((i) => lines[i].kind === "meta")).toBe(false);
  });
});

describe("the changed words of a replaced line", () => {
  test("are what the pair does not share at the start and the end", () => {
    expect(marked(at("+  const [low, high] = [luminance(a), luminance(b)].sort((x, y) => x - y);"))).toBe("(x, y) => x - y");
    expect(marked(at("-  const [low, high] = [luminance(a), luminance(b)].sort();"))).toBeUndefined();
  });

  test("grow to the edge of a word cut in half", () => {
    const pair = parseDiff("@@ -1 +1 @@\n-let attempt = 1;\n+let attempts = 1;\n");
    const marks = wordMarks(pair);
    const word = (i: number) => {
      const m = marks.get(i);
      return m && pair[i].text.slice(1).slice(m[0], m[1]);
    };
    expect(word(1)).toBe("attempt");
    expect(word(2)).toBe("attempts");
  });

  test("are left off a line rewritten throughout, and off an added line with no removed partner", () => {
    const rewritten = parseDiff("@@ -1 +1 @@\n-const a = 1;\n+return fetch(url);\n");
    expect(wordMarks(rewritten).size).toBe(0);
    expect(wordMarks(lines).has(at("+/** WCAG contrast, 1 to 21. */"))).toBe(false);
  });
});
