import { describe, expect, test } from "bun:test";
import { age, ago } from "../src/lib/time";

const now = Date.parse("2026-09-24T12:00:00Z");
const before = (minutes: number) => now - minutes * 60_000;

describe("how old a ticket is", () => {
  test("says it in the largest whole unit, rounded down", () => {
    expect(age(before(0.5), now)).toBe("now");
    expect(age(before(59), now)).toBe("59m");
    expect(age(before(60 * 23), now)).toBe("23h");
    expect(age(before(60 * 24 * 6), now)).toBe("6d");
    expect(age(before(60 * 24 * 13), now)).toBe("1w");
    expect(age(before(60 * 24 * 45), now)).toBe("1mo");
    expect(age(before(60 * 24 * 800), now)).toBe("2y");
  });

  test("reads the dates the backend sends", () => {
    expect(age("2026-09-17T12:00:00+00:00", now)).toBe("1w");
  });

  test("says nothing about a date it cannot read, or one from the future", () => {
    expect(age("soon", now)).toBe("");
    expect(age(now + 60_000, now)).toBe("now");
  });
});

describe("how long ago something happened", () => {
  test("reads in words, as every Code Villain app says it", () => {
    expect(ago(before(0.5), now)).toBe("just now");
    expect(ago(before(1), now)).toBe("1 minute ago");
    expect(ago(before(5), now)).toBe("5 minutes ago");
    expect(ago(before(60 * 3), now)).toBe("3 hours ago");
    expect(ago(before(60 * 24), now)).toBe("yesterday");
    expect(ago(before(60 * 24 * 3), now)).toBe("3 days ago");
  });

  test("gives a date once it is a week or more ago", () => {
    expect(ago("2026-09-03T12:00:00Z", now)).toBe("Sep 3");
  });

  test("says nothing about a date it cannot read, and just now for one ahead of the clock", () => {
    expect(ago("soon", now)).toBe("");
    expect(ago(now + 120_000, now)).toBe("just now");
  });
});
