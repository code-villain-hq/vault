import { describe, expect, test } from "bun:test";
import { limitsTitle, limitWindows, timeLeft, windowView } from "../src/lib/limits";
import type { PlanLimits } from "../src/lib/types-limits";

const MIN = 60_000;
const now = Date.UTC(2026, 10, 11, 12, 0);

describe("a plan's limits in the corner (§27)", () => {
  test("the countdown says the two largest units", () => {
    expect(timeLeft(now + 30_000, now)).toBe("<1m");
    expect(timeLeft(now + 42 * MIN, now)).toBe("42m");
    expect(timeLeft(now + 133 * MIN, now)).toBe("2h 13m");
    expect(timeLeft(now + 120 * MIN, now)).toBe("2h");
    expect(timeLeft(now + (3 * 24 * 60 + 4 * 60 + 59) * MIN, now)).toBe("3d 4h");
  });

  test("a window warns from 80% and is danger from 95%", () => {
    const at = (used: number) =>
      windowView("5h", { used_percent: used, resets_at: now + MIN }, now)!.tone;
    expect(at(79.4)).toBe("ok");
    expect(at(80)).toBe("warn");
    expect(at(95)).toBe("danger");
    expect(windowView("5h", { used_percent: 81.6, resets_at: now + 133 * MIN }, now)).toEqual({
      label: "5h",
      used: "82%",
      left: "2h 13m",
      tone: "warn",
    });
  });

  test("a window says when it resets only once it needs a look", () => {
    // At the sidebar's usual width, a countdown on every window was cut short.
    const left = (used: number) =>
      windowView("5h", { used_percent: used, resets_at: now + 133 * MIN }, now)!.left;
    expect(left(42)).toBeNull();
    expect(left(80)).toBe("2h 13m");
    expect(left(97)).toBe("2h 13m");
  });

  test("a window past its reset reads reset, not the old number (LIMIT-5)", () => {
    expect(windowView("wk", { used_percent: 97, resets_at: now - MIN }, now)).toEqual({
      label: "wk",
      used: "reset",
      left: null,
      tone: "muted",
    });
    expect(windowView("wk", { used_percent: 10, resets_at: null }, now)?.left).toBeNull();
    expect(windowView("wk", null, now)).toBeNull();
  });

  test("a provider shows the windows it said, five hours first", () => {
    const openai: PlanLimits = {
      provider: "openai",
      session: null,
      weekly: { used_percent: 0, resets_at: now + 7 * 24 * 60 * MIN },
      read_at: now - 3 * MIN,
    };
    expect(limitWindows(openai, now).map((w) => `${w.label} ${w.used} ${w.left}`)).toEqual([
      "wk 0% null",
    ]);
    const title = limitsTitle(openai, now);
    expect(title).toContain("OpenAI plan, read 3 minutes ago");
    expect(title).toContain("Week: 0% used, resets");
    expect(title).not.toContain("5-hour");
    expect(title).toContain("ask OpenAI again");
  });
});
