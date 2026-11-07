import { useCallback, useMemo, useState } from "react";
import { read, write } from "./persist";

/**
 * Pinned chats (CHAT-6), kept by room folder (CHAT-1): a chat's pane gets a
 * fresh id at every launch and restart, its room keeps the one it was made
 * with.
 */
const KEY = "pinnedChats";

function stored(): string[] {
  const v = read<unknown>(KEY, []);
  return Array.isArray(v) ? v.filter((r): r is string => typeof r === "string") : [];
}

/** Splits `items` into the pinned ones, in the order pinned, and the rest, in their own. */
export function pinnedFirst<T extends { cwd: string | null }>(
  items: readonly T[],
  pins: readonly string[],
): { pinned: T[]; rest: T[] } {
  const at = (x: T) => (x.cwd === null ? -1 : pins.indexOf(x.cwd));
  const pinned = items.filter((x) => at(x) >= 0).sort((a, b) => at(a) - at(b));
  const rest = items.filter((x) => at(x) < 0);
  return { pinned, rest };
}

/** Pins a room it lacks, at the end; unpins one it has. */
export function toggled(pins: readonly string[], room: string): string[] {
  return pins.includes(room) ? pins.filter((r) => r !== room) : [...pins, room];
}

export interface Pins {
  rooms: readonly string[];
  has: (x: { cwd: string | null }) => boolean;
  toggle: (room: string) => void;
  /** For a chat closed or forgotten: its room is not opened again. */
  drop: (room: string | null) => void;
}

export function usePins(): Pins {
  const [rooms, setRooms] = useState<string[]>(stored);
  const change = useCallback((next: (was: string[]) => string[]) => {
    setRooms((was) => {
      const now = next(was);
      if (now !== was) write(KEY, now);
      return now;
    });
  }, []);
  return useMemo(
    () => ({
      rooms,
      has: (x) => x.cwd !== null && rooms.includes(x.cwd),
      toggle: (room) => change((was) => toggled(was, room)),
      drop: (room) =>
        change((was) =>
          room !== null && was.includes(room) ? was.filter((r) => r !== room) : was,
        ),
    }),
    [rooms, change],
  );
}
