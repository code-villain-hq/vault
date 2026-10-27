import { describe, expect, test } from "bun:test";
import { groupByEpic } from "../src/lib/derive";
import { sortTickets } from "../src/lib/ticket-sort";
import type { Ticket } from "../src/lib/types";

const t = (key: string, o: Partial<Ticket>) =>
  ({ key, epic_key: null, epic_summary: null, status_category: "new", priority: null, created: null, updated: null, ...o }) as Ticket;
const keys = (l: Ticket[]) => l.map((i) => i.key);
const day = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();

const PRIORITIES = ["Blocker", "Major", "Minor"];
const list = [
  t("A-1", { priority: "Minor", created: day(1), updated: day(9) }),
  t("A-2", { priority: "Blocker", created: day(5), updated: day(2) }),
  t("A-3", { priority: null, created: day(3), updated: day(3) }),
  t("A-4", { priority: "Mystery", created: day(4), updated: day(8) }),
  t("A-5", { priority: "Blocker", created: day(2), updated: day(7) }),
];

describe("sorting tickets (TKT-13)", () => {
  test("by the site's own priorities, the unranked last, ties by most recent work", () => {
    expect(keys(sortTickets(list, "priority", PRIORITIES))).toEqual(["A-5", "A-2", "A-1", "A-4", "A-3"]);
  });

  test("by when each was filed, or last changed", () => {
    expect(keys(sortTickets(list, "newest", PRIORITIES))).toEqual(["A-2", "A-4", "A-3", "A-5", "A-1"]);
    expect(keys(sortTickets(list, "oldest", PRIORITIES))).toEqual(["A-1", "A-5", "A-3", "A-4", "A-2"]);
    expect(keys(sortTickets(list, "updated", PRIORITIES))).toEqual(["A-1", "A-4", "A-5", "A-3", "A-2"]);
  });

  test("leaves the list it was given as it was", () => {
    sortTickets(list, "priority", PRIORITIES);
    expect(keys(list)).toEqual(["A-1", "A-2", "A-3", "A-4", "A-5"]);
  });

  test("puts epics in flight first, then in the order of their top ticket, with no epic last", () => {
    const sorted = sortTickets(
      [
        t("Z-1", { epic_key: "E-Z", priority: "Blocker" }),
        t("N-1", { priority: "Blocker" }),
        t("A-1", { epic_key: "E-A", priority: "Minor" }),
        t("B-1", { epic_key: "E-B", priority: "Major", status_category: "indeterminate" }),
      ],
      "priority",
      PRIORITIES,
    );
    expect(groupByEpic(sorted).map((e) => e.key)).toEqual(["E-B", "E-Z", "E-A", ""]);
  });
});
