import { describe, expect, test } from "bun:test";
import type { Reminder } from "@almanac/types";
import { canDo } from "./reminders";

const reminder = (due_on: string, every_days: number | null, status = "on"): Reminder => ({
  id: "r1",
  workspace_id: "w",
  topic_id: "t",
  theme: null,
  title: "Lisa kämmen",
  due_on,
  at: null,
  every_days,
  notify: true,
  status,
  message_id: null,
  replaces: [],
  last_done_on: null,
  notified_for: null,
  point_id: null,
  created_at: "",
  updated_at: "",
});

describe("canDo", () => {
  const sunday = "2026-11-01";

  test("one late since Thursday is done today, or any day since", () => {
    const daily = reminder("2026-10-29", 1);
    for (const day of ["2026-10-29", "2026-10-30", "2026-10-31", sunday]) {
      expect(canDo(daily, day, sunday)).toBe(true);
    }
  });

  test("the times after today come round again", () => {
    expect(canDo(reminder("2026-10-29", 1), "2026-11-02", sunday)).toBe(false);
    expect(canDo(reminder(sunday, 1), "2026-11-02", sunday)).toBe(false);
  });

  test("the next time it's due is done ahead too", () => {
    expect(canDo(reminder("2026-11-04", 7), "2026-11-04", sunday)).toBe(true);
  });
});
