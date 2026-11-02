import { expect, test } from "bun:test";
import type { Newsletter } from "@postrider/types";
import { leastReadFirst, neverOpened, opened, rhythm } from "./rhythm";

function sender(name: string, sent: number, read: number, perWeek = 1): Newsletter {
  return {
    account_id: "a",
    name,
    address: `${name.toLowerCase()}@news.test`,
    sent,
    opened: read,
    recent: sent,
    per_week: perWeek,
    newest_uid: 1,
    newest: "2026-11-02T08:00:00Z",
  };
}

test("how often a sender writes", () => {
  expect(rhythm(sender("Z", 90, 0, 7.06))).toBe("Daily");
  expect(rhythm(sender("L", 45, 0, 3.4))).toBe("3 a week");
  expect(rhythm(sender("M", 13, 0, 1.05))).toBe("Weekly");
  expect(rhythm(sender("B", 7, 0, 0.55))).toBe("Every 2 weeks");
  expect(rhythm(sender("N", 3, 0, 0.23))).toBe("Monthly");
  expect(rhythm(sender("R", 2, 0, 0.1))).toBe("Now and then");
  expect(rhythm(sender("O", 1, 0, 1))).toBe("Once");
});

test("how much of it you open", () => {
  expect(opened(sender("Z", 90, 0))).toBe("Never opened");
  expect(opened(sender("H", 22, 3))).toBe("Opened 3 of 22");
  expect(neverOpened(sender("Z", 90, 0))).toBe(true);
  expect(neverOpened(sender("O", 1, 0))).toBe(false);
  expect(neverOpened(sender("H", 22, 3))).toBe(false);
});

test("the least read come first, then who sends most", () => {
  const sorted = [
    sender("Heise", 22, 22),
    sender("Babylon", 4, 2),
    sender("Spotify", 13, 0),
    sender("Zalando", 90, 0),
  ].sort(leastReadFirst);
  expect(sorted.map((n) => n.name)).toEqual(["Zalando", "Spotify", "Babylon", "Heise"]);
});
