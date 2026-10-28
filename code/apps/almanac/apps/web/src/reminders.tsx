// What every page does with a reminder: when it comes round within some
// days, marking it done, and the small menu that changes it.

import type { Reminder } from "@almanac/types";
import type { SyncClient } from "@code-villain/sync-client";
import { type Confirmation, ConfirmDialog, MoreMenu } from "@code-villain/web";
import { Bell } from "lucide-react";
import { useState } from "react";
import { addDays, daysBetween, today } from "./format";
import type { Schema } from "./sync";

/** The days from `from` to `to` (both kept) a reminder that's on comes due. */
export function occurrences(reminder: Reminder, from: string, to: string): string[] {
  if (reminder.status !== "on") return [];
  const days: string[] = [];
  let day = reminder.due_on;
  const every = reminder.every_days ?? 0;
  // A date already past stays on its day: it's still to do.
  if (every > 0 && daysBetween(day, from) > 0) {
    day = addDays(day, Math.ceil(daysBetween(day, from) / every) * every);
  }
  while (daysBetween(day, to) >= 0) {
    if (daysBetween(from, day) >= 0) days.push(day);
    if (every <= 0) break;
    day = addDays(day, every);
  }
  return days;
}

/** Whether a reminder that's on is due by today and not done. */
export function isDue(reminder: Reminder): boolean {
  return reminder.status === "on" && daysBetween(reminder.due_on, today()) >= 0;
}

/**
 * Done for the day it was due, which keeps it there, ticked: one that
 * repeats comes back its rhythm after that day, or after today when it was
 * late; one that doesn't is finished.
 */
export function markDone(sync: SyncClient<Schema>, reminder: Reminder, day = reminder.due_on) {
  if (reminder.every_days && reminder.every_days > 0) {
    const from = daysBetween(today(), day) > 0 ? day : today();
    sync.update("Reminder", reminder.id, {
      due_on: addDays(from, reminder.every_days),
      last_done_on: day,
    });
  } else {
    sync.update("Reminder", reminder.id, { status: "done", last_done_on: day });
  }
}

/**
 * The day a reminder is ticked for, if it is: the day it was last done for,
 * while it's done or comes due after it. Unticked, it's due on that day again.
 */
export function doneFor(reminder: Reminder): string | undefined {
  const day = reminder.last_done_on;
  if (!day) return undefined;
  if (reminder.status === "done") return day;
  return reminder.status === "on" && daysBetween(day, reminder.due_on) > 0 ? day : undefined;
}

/** Not done after all: it's due again on the day it was done for. */
export function markNotDone(sync: SyncClient<Schema>, reminder: Reminder) {
  const day = doneFor(reminder);
  if (!day) return;
  sync.update("Reminder", reminder.id, { status: "on", due_on: day });
}

/** The bell on a reminder that reaches the Mac. */
export function NotifyBell({ reminder }: { reminder: Reminder }) {
  if (!reminder.notify) return null;
  return (
    <Bell
      size={12}
      strokeWidth={1.75}
      role="img"
      aria-label="Notifies the Mac"
      className="shrink-0 text-faint"
    />
  );
}

/** Done, the bell, and removing it. */
export function ReminderMenu({ sync, reminder }: { sync: SyncClient<Schema>; reminder: Reminder }) {
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  return (
    <>
      <MoreMenu
        label={`More for ${reminder.title}`}
        items={[
          { label: "Done", onSelect: () => markDone(sync, reminder) },
          {
            label: reminder.notify ? "Don't notify the Mac" : "Notify the Mac",
            onSelect: () => sync.update("Reminder", reminder.id, { notify: !reminder.notify }),
          },
          {
            label: "Remove reminder…",
            danger: true,
            onSelect: () =>
              setConfirmation({
                title: `Remove "${reminder.title}"?`,
                body: "It leaves Calendar for everyone in the workspace, and the Mac isn't told again. This can't be undone.",
                action: "Remove reminder",
                onConfirm: () => sync.delete("Reminder", reminder.id),
              }),
          },
        ]}
      />
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}
