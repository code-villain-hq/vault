import type { Event } from "@postrider/types";
import { clockTime } from "./mail";

/** Today's track runs from 8 to 22. */
export const FIRST_HOUR = 8;
export const LAST_HOUR = 22;
export const HOURS = LAST_HOUR - FIRST_HOUR;

export function hoursIntoTrack(when: Date): number {
  return when.getHours() + when.getMinutes() / 60 - FIRST_HOUR;
}

export type Placed = { event: Event; lane: number; from: number; to: number };

/**
 * Where each event goes on the track, in hours from its start: at least
 * `shortest` long, inside the track, and in a lane below any it would
 * overlap.
 */
export function inLanes(events: Event[], shortest: number): Placed[] {
  const ends: number[] = [];
  return [...events]
    .sort((a, b) => Date.parse(a.start) - Date.parse(b.start))
    .flatMap((event) => {
      const start = hoursIntoTrack(new Date(event.start));
      const end = hoursIntoTrack(new Date(event.end));
      if (end <= 0 || start >= HOURS) return [];
      const to = Math.min(HOURS, Math.max(end, Math.max(start, 0) + shortest));
      const from = Math.max(0, Math.min(start, to - shortest));
      let lane = ends.findIndex((free) => free <= from);
      if (lane === -1) lane = ends.length;
      ends[lane] = to;
      return [{ event, lane, from, to }];
    });
}

/** "Zahnarzt in 1 h 20 min", "Zahnarzt until 11:00", or that the day is clear. */
export function nextUp(events: Event[], now: Date): string {
  const timed = events.filter((e) => !e.all_day);
  if (!timed.length) return "Nothing on today.";
  const current = timed.find((e) => new Date(e.start) <= now && now < new Date(e.end));
  if (current) return `${current.title} until ${clockTime(new Date(current.end))}`;
  const next = timed
    .filter((e) => new Date(e.start) > now)
    .sort((a, b) => a.start.localeCompare(b.start))[0];
  if (!next) return "Nothing else today.";
  const minutes = Math.ceil((new Date(next.start).getTime() - now.getTime()) / 60_000);
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  const wait = hours ? (rest ? `${hours} h ${rest} min` : `${hours} h`) : `${rest} min`;
  return `${next.title} in ${wait}`;
}
