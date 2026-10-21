import { expect, test } from "bun:test";
import { ago } from "./format";

const now = new Date(2026, 9, 21, 15, 0).getTime();
const before = (ms: number) => new Date(now - ms).toISOString();
const minute = 60_000;

test("a moment ago reads just now", () => {
  expect(ago(before(10_000), now)).toBe("just now");
});

test("minutes and hours are spelled out", () => {
  expect(ago(before(minute), now)).toBe("a minute ago");
  expect(ago(before(5 * minute), now)).toBe("5 minutes ago");
  expect(ago(before(60 * minute), now)).toBe("an hour ago");
  expect(ago(before(3 * 60 * minute), now)).toBe("3 hours ago");
});

test("the day before reads yesterday, then a date", () => {
  expect(ago(new Date(2026, 9, 20, 9, 0).toISOString(), now)).toBe("yesterday");
  expect(ago(new Date(2026, 9, 3, 9, 0).toISOString(), now)).toBe("Oct 3");
});

test("another year names the year", () => {
  expect(ago(new Date(2025, 11, 30).toISOString(), now)).toBe("Dec 30, 2025");
});

test("no time reads never", () => {
  expect(ago(null, now)).toBe("never");
});
