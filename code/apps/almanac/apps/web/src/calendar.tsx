// Calendar: what's due in the workspace this week, from every topic, and
// what's still to do from before it. What's done stays on its day, ticked.
// A bell marks the reminders that notify the Mac.
import type { Reminder, Topic } from "@almanac/types";
import { Link } from "@tanstack/react-router";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import {
  addDays,
  daysBetween,
  dueWords,
  rhythmWords,
  timeWords,
  today,
  weekdayWords,
  weekOf,
  weekWords,
} from "./format";
import { topicIcon } from "./icons";
import { PageBody, PageHeader, useTopics } from "./layout";
import {
  doneFor,
  isDue,
  markDone,
  markNotDone,
  NotifyBell,
  occurrences,
  ReminderMenu,
} from "./reminders";
import { Empty, Section } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

export function CalendarPage() {
  const { workspace } = useWorkspace();
  const all = useRecords("Reminder");
  const topics = useTopics();
  const [week, setWeek] = useState(() => weekOf(today()));
  const now = today();
  const reminders = useMemo(
    () =>
      all
        .filter((r) => r.status === "on")
        .sort((a, b) => a.due_on.localeCompare(b.due_on) || (a.at ?? "").localeCompare(b.at ?? "")),
    [all],
  );
  // Done for a day, which keeps them on it.
  const done = useMemo(() => all.filter((r) => doneFor(r)), [all]);
  const byId = useMemo(() => new Map(topics.map((t) => [t.id, t])), [topics]);
  const first = week[0] ?? now;
  const overdue = reminders.filter(
    (r) => daysBetween(r.due_on, now) > 0 && daysBetween(r.due_on, first) > 0,
  );

  return (
    <>
      <PageHeader title="Calendar">
        <button
          type="button"
          onClick={() => setWeek(weekOf(addDays(first, -7)))}
          aria-label="The week before"
          title="The week before"
          className="ui-button ui-icon-button"
        >
          <ChevronLeft size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
        <button
          type="button"
          onClick={() => setWeek(weekOf(now))}
          disabled={week.includes(now)}
          className="ui-button"
        >
          This week
        </button>
        <button
          type="button"
          onClick={() => setWeek(weekOf(addDays(first, 7)))}
          aria-label="The week after"
          title="The week after"
          className="ui-button ui-icon-button"
        >
          <ChevronRight size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
      </PageHeader>
      <PageBody>
        {/* A week of lines: short enough to read across. */}
        <div className="flex max-w-[86ch] flex-col gap-8">
          <div className="flex flex-col gap-1">
            <p className="text-title font-semibold">{weekWords(week)}</p>
            <p className="text-muted">What's due in {workspace.name}, from every topic.</p>
          </div>
          {reminders.length === 0 && done.length === 0 ? (
            <Empty title="Nothing due yet">
              When a talk settles something that comes round, like watering every week, Almanac
              offers a reminder. Those you take show here.
            </Empty>
          ) : (
            <>
              {overdue.length > 0 && (
                <Section title="Still to do">
                  <ul className="flex flex-col gap-1">
                    {overdue.map((r) => (
                      <Row key={r.id} reminder={r} topic={byId.get(r.topic_id)} day={r.due_on} />
                    ))}
                  </ul>
                </Section>
              )}
              <div className="flex flex-col gap-4">
                {week.map((day) => {
                  const due = reminders.filter((r) => occurrences(r, day, day).length > 0);
                  const ticked = done.filter((r) => doneFor(r) === day);
                  return (
                    <section
                      key={day}
                      aria-label={weekdayWords(day)}
                      aria-current={day === now ? "date" : undefined}
                      className="flex gap-4"
                    >
                      <h2
                        className={`w-16 shrink-0 pt-1.5 text-xs ${day === now ? "font-semibold text-fg" : "text-muted"}`}
                      >
                        {weekdayWords(day)}
                        {day === now && <span className="block font-normal text-faint">Today</span>}
                      </h2>
                      {due.length + ticked.length > 0 ? (
                        <ul className="flex min-w-0 flex-1 flex-col gap-1">
                          {ticked.map((r) => (
                            <Row
                              key={`${r.id}-done`}
                              reminder={r}
                              topic={byId.get(r.topic_id)}
                              day={day}
                              done
                            />
                          ))}
                          {due.map((r) => (
                            <Row key={r.id} reminder={r} topic={byId.get(r.topic_id)} day={day} />
                          ))}
                        </ul>
                      ) : (
                        <p className="flex-1 pt-1.5 text-xs text-faint">Nothing due</p>
                      )}
                    </section>
                  );
                })}
              </div>
            </>
          )}
        </div>
      </PageBody>
    </>
  );
}

/** A reminder on its day: done, what, which topic, when, the bell. */
function Row({
  reminder,
  topic,
  day,
  done = false,
}: {
  reminder: Reminder;
  topic: Topic | undefined;
  day: string;
  /** Done for this day: ticked, and unticking puts it back. */
  done?: boolean;
}) {
  const { workspace, sync } = useWorkspace();
  const Icon = topicIcon(topic?.icon ?? "");
  const time = timeWords(reminder.at);
  // Only the next time it's due can be done; the ones after it come round again.
  const next =
    done || day === reminder.due_on || (isDue(reminder) && daysBetween(reminder.due_on, day) <= 0);
  return (
    <li className="ui-row flex min-h-9 items-center gap-3 rounded-md px-2">
      <input
        type="checkbox"
        checked={done}
        disabled={!next}
        onChange={() => (done ? markNotDone(sync, reminder) : markDone(sync, reminder, day))}
        aria-label={`${reminder.title}: done`}
        title={
          done ? "Not done" : next ? "Done" : `Comes round ${rhythmWords(reminder.every_days)}`
        }
        className="size-4 shrink-0 accent-accent"
      />
      <span className={`min-w-0 flex-1 truncate ${done ? "text-faint line-through" : ""}`}>
        {reminder.title}
      </span>
      {topic && (
        <Link
          to="/$workspace/topics/$topic"
          params={{ workspace: workspace.url_key, topic: topic.id }}
          className="flex min-w-0 items-center gap-1.5 text-xs text-muted hover:text-fg"
        >
          <Icon size={13} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
          <span className="truncate">{topic.name}</span>
        </Link>
      )}
      <span className="w-24 shrink-0 text-right text-xs text-muted tabular-nums">
        {time ?? dueWords(day)}
      </span>
      <span className="flex w-4 justify-center">
        <NotifyBell reminder={reminder} />
      </span>
      <ReminderMenu sync={sync} reminder={reminder} />
    </li>
  );
}
