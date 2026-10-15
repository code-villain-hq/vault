import { describe, expect, test } from "bun:test";
import { mergeEntries } from "../src/lib/acpEntries";
import type { AcpEntry } from "../src/lib/types";

const say = (index: number, text: string, rev = index): AcpEntry => ({ index, rev, kind: "agent", text });
const view = (entries: AcpEntry[], base = 0, reset = false) => ({ entries, base, reset });

describe("a conversation's entries as they change (ACP-7)", () => {
  test("a token streamed into the last reply replaces only that entry", () => {
    const held = [say(0, "hi"), say(1, "Look")];
    const next = mergeEntries(held, view([say(1, "Looking", 5)]));
    expect(next.map((e) => e.index)).toEqual([0, 1]);
    expect(next[0]).toBe(held[0]);
    expect(next[1]).toEqual(say(1, "Looking", 5));
    expect(held[1]).toEqual(say(1, "Look"));
  });

  test("new entries go after, in order, whatever order they came in", () => {
    const next = mergeEntries([say(0, "a")], view([say(2, "c"), say(1, "b")]));
    expect(next.map((e) => e.index)).toEqual([0, 1, 2]);
  });

  test("an entry changed in the middle stays where it was", () => {
    const held = [say(0, "a"), say(1, "b"), say(2, "c")];
    expect(mergeEntries(held, view([say(1, "B", 9)])).map((e) => e.index)).toEqual([0, 1, 2]);
  });

  test("entries below the base are dropped, held or new", () => {
    const held = [say(0, "a"), say(1, "b"), say(2, "c")];
    expect(mergeEntries(held, view([say(1, "B", 9), say(3, "d")], 2)).map((e) => e.index)).toEqual([2, 3]);
  });

  test("a reset replaces everything held", () => {
    const held = [say(0, "a"), say(1, "b")];
    expect(mergeEntries(held, view([say(5, "x")], 5, true)).map((e) => e.index)).toEqual([5]);
  });

  test("nothing changed is the list already held", () => {
    const held = [say(0, "a")];
    expect(mergeEntries(held, view([]))).toBe(held);
  });
});
