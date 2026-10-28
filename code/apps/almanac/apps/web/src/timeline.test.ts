import { expect, test } from "bun:test";
import type { Point } from "@almanac/types";
import { inOrder, when } from "./timeline";

test("a year, a span and a day", () => {
  expect(when("1959")).toEqual({ key: 19590000, year: "1959", day: null, span: false });
  expect(when("1987–1993")).toMatchObject({ key: 19870000, year: "1987–1993", span: true });
  expect(when("1987 bis etwa 1993")).toMatchObject({ key: 19870000, span: true });
  expect(when("11. September 2001")).toEqual({
    key: 20010911,
    year: "2001",
    day: "11. September",
    span: false,
  });
  expect(when("September 11, 2001")).toMatchObject({ key: 20010911, day: "September 11" });
  expect(when("11.09.2001")).toMatchObject({ key: 20010911, year: "2001", day: "11.09." });
  expect(when("Mai 1945")).toMatchObject({ key: 19450500, year: "1945", day: "Mai" });
});

test("before the year 1, the older comes first", () => {
  expect(when("500 v. Chr.")).toMatchObject({ key: -5000000, year: "500 v. Chr." });
  expect(when("15. März 44 v. Chr.")).toMatchObject({
    key: -440000 + 315,
    year: "44 v. Chr.",
    day: "15. März",
  });
  expect(when("753 BC").key).toBeLessThan(when("44 BC").key ?? 0);
});

test("decades and centuries", () => {
  expect(when("1960er").key).toBe(19600000);
  expect(when("um 1900").key).toBe(19000000);
  expect(when("19. Jahrhundert").key).toBe(18000000);
  expect(when("18th century").key).toBe(17000000);
});

test("events go oldest first, those with no year last", () => {
  const point = (happened: string) => ({ id: happened, happened }) as Point;
  const sorted = inOrder(["2005", "irgendwann", "1967", "1987–1993", "500 v. Chr."].map(point));
  expect(sorted.map(([p]) => p.happened)).toEqual([
    "500 v. Chr.",
    "1967",
    "1987–1993",
    "2005",
    "irgendwann",
  ]);
});
