import { expect, test } from "bun:test";
import type { Event } from "@postrider/types";
import { inLanes, nextUp } from "./day";
import { clockTime } from "./mail";

/** Today at `at` on this machine's clock, as the server sends it. */
function at(time: string): Date {
  const [hours, minutes] = time.split(":").map(Number);
  const date = new Date();
  date.setHours(hours ?? 0, minutes ?? 0, 0, 0);
  return date;
}

function event(title: string, from: string, to: string): Event {
  return {
    title,
    start: at(from).toISOString(),
    end: at(to).toISOString(),
    all_day: false,
    calendar: "lazarski",
    hue: 2,
  } as Event;
}

test("events that overlap go in lanes below each other", () => {
  const placed = inLanes(
    [
      event("Bouldern", "18:30", "20:00"),
      event("Zahnarzt", "10:00", "11:00"),
      event("Call", "19:00", "19:30"),
    ],
    0.25,
  );
  expect(placed.map((p) => [p.event.title, p.lane])).toEqual([
    ["Zahnarzt", 0],
    ["Bouldern", 0],
    ["Call", 1],
  ]);
});

test("a short event is drawn wide enough to read, and still pushes the next one down", () => {
  const [first, second] = inLanes(
    [event("Stand-up", "10:00", "10:05"), event("Coffee", "10:30", "11:00")],
    1,
  );
  expect(first?.to).toBe(3);
  expect(second?.lane).toBe(1);
});

test("an event late in the day ends at the track's end, and one outside it isn't drawn", () => {
  const placed = inLanes([event("Call", "21:45", "22:00"), event("Night", "06:00", "07:00")], 1);
  expect(placed).toHaveLength(1);
  expect(placed[0]).toMatchObject({ from: 13, to: 14 });
});

test("next up says what's on now, what's next, or that the day is clear", () => {
  const day = [event("Zahnarzt", "10:00", "11:00"), event("Mittag", "12:30", "13:30")];
  expect(nextUp(day, at("08:40"))).toBe("Zahnarzt in 1 h 20 min");
  expect(nextUp(day, at("10:15"))).toBe(`Zahnarzt until ${clockTime(at("11:00"))}`);
  expect(nextUp(day, at("12:00"))).toBe("Mittag in 30 min");
  expect(nextUp(day, at("14:00"))).toBe("Nothing else today.");
  expect(nextUp([], at("14:00"))).toBe("Nothing on today.");
});
