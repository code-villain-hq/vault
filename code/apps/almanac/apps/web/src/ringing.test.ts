import { describe, expect, test } from "bun:test";
import type { Reminder } from "@almanac/types";
import { rememberRung, type Rung, toRing } from "./ringing";

const reminder = (id: string, due_on: string, at: string | null = null): Reminder => ({
  id,
  workspace_id: "w",
  topic_id: "t",
  theme: null,
  title: "Monstera gießen",
  due_on,
  at,
  every_days: null,
  notify: true,
  status: "on",
  message_id: null,
  replaces: [],
  last_done_on: null,
  notified_for: null,
  point_id: null,
  created_at: "",
  updated_at: "",
});

const fresh: Rung = { since: "2026-11-02", told: {} };
const at = (time: string) => new Date(`2026-11-02T${time}`);
const ids = (list: Reminder[]) => list.map((r) => r.id);

describe("toRing", () => {
  test("one comes due at its time, or at nine without one", () => {
    const list = [reminder("timed", "2026-11-02", "14:30:00"), reminder("allDay", "2026-11-02")];
    expect(ids(toRing(list, fresh, at("08:59")))).toEqual([]);
    expect(ids(toRing(list, fresh, at("09:00")))).toEqual(["allDay"]);
    expect(ids(toRing(list, fresh, at("14:30")))).toEqual(["timed", "allDay"]);
  });

  test("rings once for its date, and again once it moves on", () => {
    const daily = reminder("daily", "2026-11-02");
    const rung = rememberRung([daily], fresh, toRing([daily], fresh, at("10:00")));
    expect(toRing([daily], rung, at("11:00"))).toEqual([]);
    const next = { ...daily, due_on: "2026-11-03" };
    expect(ids(toRing([next], rung, new Date("2026-11-03T09:00")))).toEqual(["daily"]);
  });

  test("stays quiet for what came due before this Mac began ringing", () => {
    expect(toRing([reminder("late", "2026-10-30")], fresh, at("10:00"))).toEqual([]);
  });

  test("leaves what the Mac it was made on told, what's off, and what doesn't notify", () => {
    const list = [
      { ...reminder("told", "2026-11-02"), notified_for: "2026-11-02" },
      { ...reminder("done", "2026-11-02"), status: "done" },
      { ...reminder("quiet", "2026-11-02"), notify: false },
    ];
    expect(toRing(list, fresh, at("10:00"))).toEqual([]);
  });
});

describe("rememberRung", () => {
  test("forgets what moved on or went", () => {
    const rung: Rung = { since: "2026-11-01", told: { moved: "2026-11-01", gone: "2026-11-01" } };
    const moved = reminder("moved", "2026-11-04");
    const now = reminder("now", "2026-11-02");
    expect(rememberRung([moved, now], rung, [now])).toEqual({
      since: "2026-11-01",
      told: { now: "2026-11-02" },
    });
  });
});
