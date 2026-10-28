import { expect, test } from "bun:test";
import type { Point } from "@almanac/types";
import { bySection, inOrder, when, years } from "./timeline";

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

test("the years an event covers", () => {
  expect(years("1959")).toEqual([1959, 1959]);
  expect(years("1987–1993")).toEqual([1987, 1993]);
  expect(years("1987 bis etwa 1993")).toEqual([1987, 1993]);
  expect(years("1945–46")).toEqual([1945, 1946]);
  expect(years("13. Mai 1948 – 1949")).toEqual([1948, 1949]);
  expect(years("11. September 2001")).toEqual([2001, 2001]);
  expect(years("500–400 v. Chr.")).toEqual([-500, -400]);
  expect(years("15. März 44 v. Chr.")).toEqual([-44, -44]);
  expect(years("irgendwann")).toBeNull();
});

test("the topic page's timeline is a line per section, oldest first", () => {
  const point = (theme: string, happened: string) =>
    ({ id: `${theme} ${happened}`, theme, happened }) as Point;
  const lines = bySection([
    point("Intifada", "2000–2005"),
    point("Kuba", "1959"),
    point("Intifada", "1967"),
    point("", "1945"),
    point("Kuba", "1898"),
    point("Intifada", "1987–1993"),
    point("Rom", "15. März 44 v. Chr."),
    point("Rom", "753 v. Chr."),
    point("9/11", "11. September 2001"),
  ]);
  expect(lines.map((l) => [l.section, l.year, l.span, l.points.length])).toEqual([
    ["Rom", "753–44 v. Chr.", true, 2],
    ["Kuba", "1898–1959", true, 2],
    [null, "1945", false, 1],
    ["Intifada", "1967–2005", true, 3],
    ["9/11", "2001", false, 1],
  ]);
  expect(lines[3]?.points.map((p) => p.happened)).toEqual(["1967", "1987–1993", "2000–2005"]);
});
