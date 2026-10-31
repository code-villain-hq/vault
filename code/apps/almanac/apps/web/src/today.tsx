// Today on the workspace's own page: a ring of how much of today is done,
// and what's due today or still to do from before, from every topic, as
// chips to tick, done for today. What's ticked today stays, faint.
import type { Reminder, Topic } from "@almanac/types";
import { Link } from "@tanstack/react-router";
import { useMemo } from "react";
import { today, weekdayWords } from "./format";
import { topicIcon } from "./icons";
import { doneFor, isDue, markDone, markNotDone } from "./reminders";
import { useRecords, useWorkspace } from "./workspace";

export function Today({ topics }: { topics: Topic[] }) {
  const { workspace, sync } = useWorkspace();
  const all = useRecords("Reminder");
  const now = today();
  const byId = useMemo(() => new Map(topics.map((t) => [t.id, t])), [topics]);
  const due = useMemo(
    () =>
      all
        .filter((r) => isDue(r) && byId.has(r.topic_id))
        .sort((a, b) => a.due_on.localeCompare(b.due_on) || (a.at ?? "").localeCompare(b.at ?? "")),
    [all, byId],
  );
  const ticked = useMemo(
    () => all.filter((r) => doneFor(r) === now && byId.has(r.topic_id)),
    [all, byId, now],
  );
  const count = due.length + ticked.length;
  if (count === 0) return null;

  const chip = (reminder: Reminder, done: boolean) => {
    const topic = byId.get(reminder.topic_id);
    const Icon = topicIcon(topic?.icon ?? "");
    return (
      <li key={reminder.id} className="min-w-0">
        <label
          title={topic ? `${reminder.title} · ${topic.name}` : reminder.title}
          className={`flex max-w-full cursor-pointer items-center gap-1.5 rounded-full py-1 pr-3 pl-2 hover:bg-hover ${done ? "bg-transparent text-faint" : "bg-chip"}`}
        >
          <input
            type="checkbox"
            checked={done}
            onChange={() => (done ? markNotDone(sync, reminder) : markDone(sync, reminder, now))}
            aria-label={`${reminder.title}: done`}
            className="size-3.5 shrink-0 accent-accent"
          />
          <Icon size={13} strokeWidth={1.75} aria-hidden="true" className="shrink-0 text-muted" />
          <span className={`truncate ${done ? "line-through" : ""}`}>{reminder.title}</span>
        </label>
      </li>
    );
  };

  return (
    <section aria-label="Today" className="flex flex-col gap-3 rounded-lg bg-surface p-4">
      <div className="flex items-center gap-3">
        <Ring done={ticked.length} of={count} />
        <div className="flex min-w-0 flex-col">
          <h2 className="font-semibold">Today</h2>
          <p className="text-xs text-muted">{weekdayWords(now)}</p>
        </div>
        <Link
          to="/$workspace/calendar"
          params={{ workspace: workspace.url_key }}
          className="ml-auto text-muted text-xs hover:text-fg"
        >
          Calendar
        </Link>
      </div>
      <ul className="flex flex-wrap gap-1.5">
        {due.map((r) => chip(r, false))}
        {ticked.map((r) => chip(r, true))}
      </ul>
    </section>
  );
}

/** How much of today is done, as a ring with the count in it. */
function Ring({ done, of }: { done: number; of: number }) {
  const r = 18;
  const around = 2 * Math.PI * r;
  return (
    <span className="relative grid size-11 shrink-0 place-items-center">
      <svg viewBox="0 0 44 44" className="-rotate-90 absolute inset-0" aria-hidden="true">
        <circle cx="22" cy="22" r={r} fill="none" strokeWidth="4" className="stroke-chip" />
        <circle
          cx="22"
          cy="22"
          r={r}
          fill="none"
          strokeWidth="4"
          strokeLinecap="round"
          strokeDasharray={`${(done / of) * around} ${around}`}
          className={done === of ? "stroke-ok" : "stroke-accent"}
        />
      </svg>
      <span
        className="font-semibold text-xs tabular-nums"
        role="img"
        aria-label={`${done} of ${of} done`}
      >
        {done}/{of}
      </span>
    </span>
  );
}
