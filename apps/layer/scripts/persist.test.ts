import { describe, expect, test } from "bun:test";
import { forgetAllBut } from "../src/lib/persist";

function storage(keys: string[]) {
  const held = [...keys];
  return {
    held,
    get length() { return held.length; },
    key: (i: number) => held[i] ?? null,
    removeItem: (k: string) => { const i = held.indexOf(k); if (i >= 0) held.splice(i, 1); },
  };
}

describe("the layout remembered for a task", () => {
  test("goes with the task", () => {
    const s = storage([
      "villain.activePane.t-gone", "villain.browserOpen.t-gone",
      "villain.activePane.t-kept", "villain.browserOpen.t-kept",
    ]);
    forgetAllBut(["activePane", "browserOpen"], new Set(["t-kept"]), s);
    expect(s.held).toEqual(["villain.activePane.t-kept", "villain.browserOpen.t-kept"]);
  });

  test("leaves every other preference alone", () => {
    const keys = ["villain.view", "villain.selectedTask", "villain.other.t-gone", "someone-else.activePane.t-gone"];
    const s = storage(keys);
    forgetAllBut(["activePane", "browserOpen"], new Set(), s);
    expect(s.held).toEqual(keys);
  });

  test("forgets every gone task's keys, however many sit side by side", () => {
    const s = storage(["villain.activePane.a", "villain.activePane.b", "villain.activePane.c"]);
    forgetAllBut(["activePane"], new Set(["c"]), s);
    expect(s.held).toEqual(["villain.activePane.c"]);
  });
});
