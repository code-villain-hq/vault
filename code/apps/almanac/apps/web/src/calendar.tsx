// Calendar: what's due in the workspace this week, from every topic, and
// what's still to do from before it. What's done stays on its day, ticked.
// A day under the other, at the reading measure on any screen, a card a
// reminder, and a bar of how much of it is done. A bell marks the
// reminders that notify the Mac.
import type { Reminder, Topic } from "@almanac/types";
import { Link } from "@tanstack/react-router";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useMemo, useState } from "react";
import {
  addDays,
  dayOfMonth,
  daysBetween,
  dueWords,
  rhythmWords,
  timeWords,
  weekdayShort,
  weekdayWords,
  weekOf,
  weekWords,
} from "./format";
import { topicIcon } from "./icons";
import { PageBody, PageHeader, useTopics } from "./layout";
import {
  canDo,
  doneFor,
  markDone,
  markNotDone,
  NotifyBell,
  occurrences,
  ReminderMenu,
  useToday,
} from "./reminders";
import { Empty, Section } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

export function CalendarPage() {
  const { workspace } = useWorkspace();
  const all = useRecords("Reminder");
  const topics = useTopics();
  const now = useToday();
  const [week, setWeek] = useState(() => weekOf(now));
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
        <div className="flex max-w-measure flex-col gap-8">
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
                  <ul className="flex flex-col gap-1.5">
                    {overdue.map((r) => (
                      <Row
                        key={r.id}
                        reminder={r}
                        topic={byId.get(r.topic_id)}
                        day={r.due_on}
                        late
                      />
                    ))}
                  </ul>
                </Section>
              )}
              <div className="flex flex-col gap-6">
                {week.map((day) => {
                  const due = reminders.filter((r) => occurrences(r, day, day).length > 0);
                  const ticked = done.filter((r) => doneFor(r) === day);
                  const all = due.length + ticked.length;
                  return (
                    <section
                      key={day}
                      aria-label={weekdayWords(day)}
                      aria-current={day === now ? "date" : undefined}
                      className="flex min-w-0 flex-col gap-2"
                    >
                      <h2 className="flex items-end gap-2 border-line border-b pb-2">
                        <span
                          className={`font-semibold text-2xl tabular-nums leading-none ${day === now ? "text-accent" : all ? "" : "text-faint"}`}
                        >
                          {dayOfMonth(day)}
                        </span>
                        <span
                          className={`text-xs ${day === now ? "font-semibold text-fg" : "text-muted"}`}
                        >
                          {day === now ? "Today" : weekdayShort(day)}
                        </span>
                        {all > 0 && (
                          <span
                            role="img"
                            aria-label={`${ticked.length} of ${all} done`}
                            className="mb-1 ml-auto h-1.5 w-10 overflow-hidden rounded-full bg-chip"
                          >
                            <span
                              className={`block h-full rounded-full ${ticked.length === all ? "bg-ok" : "bg-info"}`}
                              style={{ width: `${(ticked.length / all) * 100}%` }}
                            />
                          </span>
                        )}
                      </h2>
                      {all > 0 && (
                        <ul className="flex flex-col gap-1.5">
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

/** A reminder on its day, a card: done, what, which topic, when, the bell. */
function Row({
  reminder,
  topic,
  day,
  done = false,
  late = false,
}: {
  reminder: Reminder;
  topic: Topic | undefined;
  day: string;
  /** Done for this day: ticked, and unticking puts it back. */
  done?: boolean;
  /** From a week before, so it says its day. */
  late?: boolean;
}) {
  const { workspace, sync } = useWorkspace();
  const Icon = topicIcon(topic?.icon ?? "");
  const time = timeWords(reminder.at);
  const next = done || canDo(reminder, day);
  const when = late ? dueWords(day) : time;
  return (
    <li className="group/card relative flex min-w-0 flex-col gap-1.5 rounded-md bg-surface px-2.5 py-2">
      <span className="flex items-start gap-2">
        <input
          type="checkbox"
          checked={done}
          disabled={!next}
          onChange={() => (done ? markNotDone(sync, reminder) : markDone(sync, reminder, day))}
          aria-label={`${reminder.title}: done`}
          title={
            done ? "Not done" : next ? "Done" : `Comes round ${rhythmWords(reminder.every_days)}`
          }
          className="mt-0.5 size-4 shrink-0 accent-accent"
        />
        <span
          className={`line-clamp-3 min-w-0 flex-1 break-words leading-snug ${done ? "text-faint line-through" : ""}`}
        >
          {reminder.title}
        </span>
      </span>
      <span className="flex min-w-0 items-center gap-1.5 text-xs text-muted">
        {topic && (
          <Link
            to="/$workspace/topics/$topic"
            params={{ workspace: workspace.url_key, topic: topic.id }}
            title={topic.name}
            className="flex min-w-0 items-center gap-1.5 hover:text-fg"
          >
            <Icon size={13} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
            <span className="truncate">{topic.name}</span>
          </Link>
        )}
        {when && <span className="ml-auto shrink-0 tabular-nums">{when}</span>}
        <span className={`flex shrink-0 ${when ? "" : "ml-auto"}`}>
          <NotifyBell reminder={reminder} />
        </span>
      </span>
      {/* Over the card's corner on hover, so the card keeps its words. */}
      <span className="absolute top-1 right-1 rounded-md bg-surface opacity-0 focus-within:opacity-100 group-hover/card:opacity-100 has-[[aria-expanded=true]]:opacity-100">
        <ReminderMenu sync={sync} reminder={reminder} day={day} />
      </span>
    </li>
  );
}
