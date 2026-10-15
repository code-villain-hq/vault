import { expect, test } from "bun:test";
import { foldDiff, lineDiff, wordPairs } from "./diff";

test("a line diff keeps what's common and shows the rest as removed, then added", () => {
  expect(lineDiff("one\ntwo\nthree", "one\n2\nthree\nfour")).toEqual([
    { kind: "same", text: "one" },
    { kind: "removed", text: "two" },
    { kind: "added", text: "2" },
    { kind: "same", text: "three" },
    { kind: "added", text: "four" },
  ]);
  expect(lineDiff("", "new")).toEqual([{ kind: "added", text: "new" }]);
  expect(lineDiff("old", "")).toEqual([{ kind: "removed", text: "old" }]);
});

test("long unchanged runs fold away, next to the changes they don't", () => {
  const before = ["a", "b", "c", "d", "e", "f"].join("\n");
  const after = ["a", "b", "c", "d", "e", "F"].join("\n");
  expect(foldDiff(lineDiff(before, after), 1)).toEqual([
    { skipped: 4 },
    { kind: "same", text: "e" },
    { kind: "removed", text: "f" },
    { kind: "added", text: "F" },
  ]);
});

test("a changed line paired with the one it replaced says which words changed", () => {
  const [old, now, other] = wordPairs(
    lineDiff("at least 3 characters\nkept", "at least 3 and at most 255 characters\nkept"),
  );
  expect(old?.words?.filter((w) => w.kind !== "same")).toEqual([]);
  expect(
    now?.words
      ?.filter((w) => w.kind === "added")
      .map((w) => w.text)
      .join(""),
  ).toBe("and at most 255 ");
  expect(now?.words?.map((w) => w.text).join("")).toBe("at least 3 and at most 255 characters");
  expect(other?.words).toBeUndefined();
  // Nothing in common: the lines stay whole.
  const [a, b] = wordPairs(lineDiff("one", "two"));
  expect(a?.words).toBeUndefined();
  expect(b?.words).toBeUndefined();
});
