// Reminders that ring on this Mac from a server's workspace. This Mac's own
// Almanac rings its own (`reminders.rs`, `this_mac.rs`); a server can't
// reach the Mac, so the desktop app keeps every server workspace's copy in
// sync and rings from it, offline too. Each Mac remembers what it rang, so
// the server knows nothing of it and every Mac you use rings once.

import type { Reminder } from "@almanac/types";
import { useEffect } from "react";
import { holdCopy } from "./copies";
import { fromKey, today } from "./format";
import { isDesktop } from "./platform";
import type { Place } from "./server";
import { useSession } from "./session";

/**
 * What this Mac remembers of a workspace's reminders: the day it began
 * ringing them, and the due date it rang each for.
 */
export type Rung = { since: string; told: Record<string, string> };

/** When a reminder comes due: at its time, or at nine when it has none. */
export function dueAt(reminder: Reminder): Date {
  const [hours = 9, minutes = 0] = (reminder.at ?? "09:00").split(":").map(Number);
  const at = fromKey(reminder.due_on);
  at.setHours(hours, minutes, 0, 0);
  return at;
}

/**
 * The reminders to ring now: on, notify, come due, and not yet rung for
 * their date, here or by the Mac that told it. Those due before this Mac
 * began ringing stay quiet, so the first look isn't every late one at once.
 */
export function toRing(reminders: Reminder[], rung: Rung, now: Date): Reminder[] {
  return reminders.filter(
    (r) =>
      r.status === "on" &&
      r.notify &&
      r.due_on >= rung.since &&
      dueAt(r) <= now &&
      r.notified_for !== r.due_on &&
      rung.told[r.id] !== r.due_on,
  );
}

/** What to remember once `rang` rang: theirs, and the rest still at the date they rang for. */
export function rememberRung(reminders: Reminder[], rung: Rung, rang: Reminder[]): Rung {
  const told: Record<string, string> = {};
  for (const r of reminders) if (rung.told[r.id] === r.due_on) told[r.id] = r.due_on;
  for (const r of rang) told[r.id] = r.due_on;
  return { since: rung.since, told };
}

const rungKey = (workspaceId: string) => `almanac:rung:${workspaceId}`;

function readRung(workspaceId: string): Rung {
  try {
    const saved = JSON.parse(localStorage.getItem(rungKey(workspaceId)) ?? "null") as Rung | null;
    if (saved) return saved;
  } catch {}
  return { since: today(), told: {} };
}

function writeRung(workspaceId: string, rung: Rung) {
  try {
    localStorage.setItem(rungKey(workspaceId), JSON.stringify(rung));
  } catch {}
}

async function ring(topic: string, title: string) {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("ring", { topic, title }).catch((error: unknown) =>
    console.warn("couldn't show a reminder", error),
  );
}

/** Holds the workspace's copy and rings from it every 30 seconds, as this Mac's Almanac does. */
function ringFrom(place: Place, userId: string, workspaceId: string): () => void {
  const { sync, release } = holdCopy(place, userId, workspaceId);
  const look = () => {
    if (!sync.status().ready) return;
    const reminders = sync.query("Reminder");
    const rung = readRung(workspaceId);
    const rang = toRing(reminders, rung, new Date());
    for (const r of rang) void ring(sync.get("Topic", r.topic_id)?.name ?? "Almanac", r.title);
    writeRung(workspaceId, rememberRung(reminders, rung, rang));
  };
  // The first look once the copy has loaded, then every 30 seconds.
  let ready = false;
  const first = () => {
    if (ready || !sync.status().ready) return;
    ready = true;
    look();
  };
  const stopWatching = sync.subscribe(first);
  first();
  const timer = setInterval(look, 30_000);
  return () => {
    clearInterval(timer);
    stopWatching();
    release();
  };
}

function RingFor({
  place,
  userId,
  workspaceId,
}: {
  place: Place;
  userId: string;
  workspaceId: string;
}) {
  useEffect(() => ringFrom(place, userId, workspaceId), [place, userId, workspaceId]);
  return null;
}

/** In the desktop app, rings the reminders of every server workspace you're signed in to. */
export function Ringing() {
  const session = useSession();
  if (!isDesktop || session.status !== "signed-in") return null;
  return session.places
    .filter(({ place }) => place.kind !== "mac")
    .flatMap(({ place, me }) =>
      me.workspaces.map((w) => (
        <RingFor key={`${place.id} ${w.id}`} place={place} userId={me.user.id} workspaceId={w.id} />
      )),
    );
}
