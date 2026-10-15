import { describe, expect, test } from "bun:test";
import { keep, only, reuse } from "../src/lib/same";

const byId = (t: { id: string }) => t.id;

describe("a refresh that found nothing new", () => {
  test("hands back the list already held, not an equal copy", () => {
    const held = [{ id: "a", n: 1 }, { id: "b", n: 2 }];
    expect(reuse(held, structuredClone(held), byId)).toBe(held);
  });

  test("keeps a value already held when the new one says the same", () => {
    const held = { mine: [{ n: 1 }], team: null };
    expect(keep(held, structuredClone(held))).toBe(held);
    const moved = { mine: [{ n: 2 }], team: null };
    expect(keep(held, moved)).toBe(moved);
  });
});

describe("a refresh that changed one item", () => {
  test("is a new list that keeps every other item as it was", () => {
    const held = [{ id: "a", n: 1 }, { id: "b", n: 2 }];
    const next = reuse(held, [{ id: "a", n: 1 }, { id: "b", n: 3 }], byId);
    expect(next).not.toBe(held);
    expect(next[0]).toBe(held[0]);
    expect(next[1]).toEqual({ id: "b", n: 3 });
  });

  test("notices an item added, removed or moved", () => {
    const held = [{ id: "a" }, { id: "b" }];
    expect(reuse(held, [{ id: "a" }], byId)).toEqual([{ id: "a" }]);
    expect(reuse(held, [{ id: "a" }, { id: "b" }, { id: "c" }], byId)).toHaveLength(3);
    const swapped = reuse(held, [{ id: "b" }, { id: "a" }], byId);
    expect(swapped).not.toBe(held);
    expect(swapped[0]).toBe(held[1]);
  });

  test("judges sameness by what a comparison is told to look at", () => {
    const held = [{ id: "a", seen: 1, at: 5 }];
    const next = reuse(held, [{ id: "a", seen: 2, at: 5 }], byId, (x, y) => x.at === y.at);
    expect(next).toBe(held);
  });
});

describe("per-task state when the task list changes", () => {
  test("drops what belonged to a deleted task", () => {
    expect(only({ a: 1, b: 2 }, new Set(["a"]))).toEqual({ a: 1 });
  });

  test("is the same object when every task is still there", () => {
    const held = { a: 1, b: 2 };
    expect(only(held, new Set(["a", "b", "c"]))).toBe(held);
  });
});
