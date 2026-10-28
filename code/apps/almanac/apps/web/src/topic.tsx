// A topic's page: what it sticks to, what it kept by theme (a card each,
// or a document's sections), the questions it waits on, its week, and the
// talk beside it.

import type { ClaudeKeyInfo, Point, Question, Reminder, Topic } from "@almanac/types";
import type { SyncClient } from "@code-villain/sync-client";
import { type Confirmation, ConfirmDialog, MoreMenu, RowAction, useApi } from "@code-villain/web";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { MessagesSquare, X } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  dayKey,
  dueWords,
  rhythmWords,
  timeWords,
  today,
  weekdayWords,
  weekOf,
  weekWords,
} from "./format";
import { PageHeader } from "./layout";
import { NotifyBell, occurrences, ReminderMenu } from "./reminders";
import type { Schema } from "./sync";
import { TalkPanel, useAsk, useTalks } from "./talk";
import { TopicDialog } from "./topic-dialog";
import { Button, Empty } from "./ui";
import { useIsGuest, useRecords, useSyncStatus, useWorkspace } from "./workspace";

/** `?talk=` opens the talk panel (`new` for a new talk); `?ask=` starts it with a question. */
export interface TopicSearch {
  talk?: string;
  ask?: string;
}

/** What a theme holds: its points, the questions about it, its reminders. */
interface Theme {
  name: string;
  points: Point[];
  questions: Question[];
  reminders: Reminder[];
}

export function TopicPage() {
  const { topic: id } = useParams({ strict: false }) as { topic: string };
  const search = useSearch({ strict: false }) as TopicSearch;
  const navigate = useNavigate();
  const { ready } = useSyncStatus();
  const topics = useRecords("Topic");
  const topic = topics.find((t) => t.id === id);

  const open = useCallback(
    (talk: string | null | undefined) =>
      void navigate({
        to: ".",
        search: talk === undefined ? {} : { talk: talk ?? "new" },
        replace: true,
      }),
    [navigate],
  );
  const { asking, ask } = useAsk(id, (talk) => open(talk));

  if (!topic) {
    if (!ready) return null;
    return (
      <>
        <PageHeader title="Topic" />
        <Empty title="This topic is gone">It was deleted, or it's in another workspace.</Empty>
      </>
    );
  }
  const talkId = search.talk && search.talk !== "new" ? search.talk : null;
  return (
    <TopicView
      // A new topic starts with its own state: what it asked, what it saw.
      key={topic.id}
      topic={topic}
      panel={search.talk !== undefined}
      talkId={talkId}
      draft={search.ask ?? ""}
      asking={asking}
      onAsk={(text, talk) => {
        if (talk === null) open(null);
        void ask(text, talk);
      }}
      onOpen={open}
    />
  );
}

function TopicView({
  topic,
  panel,
  talkId,
  draft,
  asking,
  onAsk,
  onOpen,
}: {
  topic: Topic;
  panel: boolean;
  talkId: string | null;
  draft: string;
  asking: ReturnType<typeof useAsk>["asking"];
  onAsk: (text: string, talkId: string | null) => void;
  onOpen: (talkId: string | null | undefined) => void;
}) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const talks = useTalks(topic.id);
  const [editing, setEditing] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const navigate = useNavigate();
  const { workspace } = useWorkspace();
  useSeenOnLeave(sync, topic.id);

  return (
    <>
      <PageHeader title={topic.name}>
        <button
          type="button"
          onClick={() => (panel ? onOpen(undefined) : onOpen(talks[0]?.id ?? null))}
          aria-pressed={panel}
          aria-label="Talks"
          title={panel ? "Close the talk (esc)" : "Talks"}
          className="ui-button ui-icon-button aria-pressed:bg-selected"
        >
          <MessagesSquare size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
        {!guest && (
          <MoreMenu
            label="More for this topic"
            items={[
              { label: "Edit topic…", onSelect: () => setEditing(true) },
              {
                label: "Delete topic…",
                danger: true,
                onSelect: () =>
                  setConfirmation({
                    title: `Delete ${topic.name}?`,
                    body: "Its page, its talks and its reminders go with it. This can't be undone.",
                    action: "Delete topic",
                    onConfirm: () => {
                      sync.delete("Topic", topic.id);
                      void navigate({
                        to: "/$workspace",
                        params: { workspace: workspace.url_key },
                      });
                    },
                  }),
              },
            ]}
          />
        )}
      </PageHeader>
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="page-content min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto flex max-w-4xl flex-col gap-8 px-6 py-6">
              <TopicPageBody topic={topic} onAsk={(text) => onAsk(text, null)} />
            </div>
          </div>
          {!panel && <AskBar topic={topic} onAsk={(text) => onAsk(text, null)} />}
        </div>
        {panel && (
          <TalkPanel
            key={talkId ?? "new"}
            topic={topic}
            talkId={talkId}
            asking={asking}
            draft={draft}
            onAsk={onAsk}
            onOpen={onOpen}
            onClose={() => onOpen(undefined)}
          />
        )}
      </div>
      {editing && <TopicDialog topic={topic} onClose={() => setEditing(false)} />}
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** New points carry a dot while you're on the page, and lose it once you leave. */
function useSeenOnLeave(sync: SyncClient<Schema>, topicId: string) {
  useEffect(
    () => () => {
      const unread = sync.query("Point").filter((p) => p.topic_id === topicId && !p.seen);
      if (unread.length === 0) return;
      void sync.transact(
        unread.map((p) => ({
          action: "update" as const,
          model: "Point" as const,
          id: p.id,
          data: { seen: true },
        })),
      );
    },
    [sync, topicId],
  );
}

function TopicPageBody({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  const points = useRecords("Point");
  const questions = useRecords("Question");
  const reminders = useRecords("Reminder");
  const mine = useMemo(() => {
    const ofTopic = <T extends { topic_id: string }>(list: T[]) =>
      list.filter((r) => r.topic_id === topic.id);
    return {
      points: ofTopic(points).sort((a, b) => a.position - b.position),
      questions: ofTopic(questions)
        .filter((q) => q.status === "open")
        .sort((a, b) => a.created_at.localeCompare(b.created_at)),
      reminders: ofTopic(reminders)
        .filter((r) => r.status === "on")
        .sort((a, b) => a.due_on.localeCompare(b.due_on)),
    };
  }, [points, questions, reminders, topic.id]);
  const themes = useMemo(() => byTheme(mine), [mine]);
  const loose = mine.questions.filter((q) => !q.theme);
  const empty = themes.length === 0 && loose.length === 0;

  return (
    <>
      <div className="flex flex-col gap-1">
        <p className="text-title font-semibold">{topic.name}</p>
        {topic.sticks_to && <p className="text-muted">{topic.sticks_to}</p>}
      </div>
      <NoKey />
      {mine.reminders.length > 0 && <WeekStrip topic={topic} reminders={mine.reminders} />}
      {loose.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Waiting on you">
          {loose.map((q) => (
            <Ask key={q.id} question={q} />
          ))}
        </section>
      )}
      {empty ? (
        <EmptyPage topic={topic} onAsk={onAsk} />
      ) : topic.shape === "document" ? (
        <Document topic={topic} themes={themes} />
      ) : (
        <Cards themes={themes} />
      )}
    </>
  );
}

/** The themes in page order: the first point's place, then questions and reminders. */
function byTheme({
  points,
  questions,
  reminders,
}: {
  points: Point[];
  questions: Question[];
  reminders: Reminder[];
}): Theme[] {
  const themes: Theme[] = [];
  const theme = (name: string) => {
    let found = themes.find((t) => t.name === name);
    if (!found) {
      found = { name, points: [], questions: [], reminders: [] };
      themes.push(found);
    }
    return found;
  };
  for (const point of points) theme(point.theme).points.push(point);
  // A question waits where it's about, even before that has a point.
  for (const question of questions) {
    if (question.theme) theme(question.theme).questions.push(question);
  }
  for (const reminder of reminders) {
    if (reminder.theme) theme(reminder.theme).reminders.push(reminder);
  }
  return themes;
}

/** Without a Claude key nothing answers: say so where it's noticed. */
function NoKey() {
  const { workspace } = useWorkspace();
  const { data } = useApi<ClaudeKeyInfo>(`/api/w/${workspace.id}/claude`);
  if (!data || data.set || data.from_settings) return null;
  return (
    <p className="ui-tint-info rounded-md px-3 py-2">
      Almanac answers with Claude, and has no key yet.{" "}
      <Link
        to="/$workspace/settings/claude"
        params={{ workspace: workspace.url_key }}
        className="ui-link"
      >
        Add one in Settings → Claude
      </Link>
      .
    </p>
  );
}

function EmptyPage({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <p className="font-medium">Nothing kept yet.</p>
        <p className="text-muted">
          What you ask {topic.name} is kept here by theme, as short points with where they come
          from. Ask below, or tell it what you already know.
        </p>
      </div>
      {topic.starters.length > 0 && (
        <section className="flex flex-col gap-2">
          <h2 className="text-xs font-semibold text-muted">To start</h2>
          <ul className="flex flex-col gap-1">
            {topic.starters.map((starter) => (
              <li key={starter}>
                <button
                  type="button"
                  onClick={() => onAsk(starter)}
                  className="ui-row w-full rounded-md px-3 py-2 text-left hover:bg-hover"
                >
                  {starter}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/** This week of the topic's reminders, a day each. */
function WeekStrip({ topic, reminders }: { topic: Topic; reminders: Reminder[] }) {
  const days = weekOf(today());
  const now = today();
  return (
    <section className="flex flex-col gap-2" aria-label={`${topic.name} this week`}>
      <h2 className="text-xs font-semibold text-muted">{weekWords(days)}</h2>
      <div className="grid grid-cols-7 gap-1">
        {days.map((day) => {
          const due = reminders.filter((r) => occurrences(r, day, day).length > 0);
          return (
            <div
              key={day}
              aria-current={day === now ? "date" : undefined}
              className="flex min-h-16 min-w-0 flex-col gap-1 rounded-md bg-surface p-2 aria-[current=date]:bg-selected"
            >
              <span className="text-xs text-muted">{weekdayWords(day)}</span>
              {due.map((reminder) => (
                <span
                  key={reminder.id}
                  className="flex min-w-0 items-center gap-1 text-xs"
                  title={`${reminder.title}, ${rhythmWords(reminder.every_days)}`}
                >
                  <span className="truncate">{reminder.title}</span>
                  <NotifyBell reminder={reminder} />
                </span>
              ))}
              {due.length === 0 && <span className="sr-only">Nothing due</span>}
            </div>
          );
        })}
      </div>
    </section>
  );
}

/** A card per theme: its points, its reminders, what it waits on. */
function Cards({ themes }: { themes: Theme[] }) {
  return (
    // Two columns only when the page has room, talk open or not.
    <div className="@container">
      <div className="grid gap-3 @xl:grid-cols-2">
        {themes.map((theme) => (
          <article key={theme.name} className="flex flex-col gap-3 rounded-lg bg-surface p-4">
            <h2 className="font-semibold">{theme.name}</h2>
            {theme.reminders.length > 0 && <Dates reminders={theme.reminders} />}
            {theme.points.length > 0 && <Points points={theme.points} />}
            {theme.questions.map((q) => (
              <Ask key={q.id} question={q} />
            ))}
            <KeptFrom points={theme.points} />
          </article>
        ))}
      </div>
    </div>
  );
}

/** A document that grows: a section per theme. */
function Document({ topic, themes }: { topic: Topic; themes: Theme[] }) {
  return (
    <article className="flex flex-col gap-6">
      {topic.title && topic.title !== topic.name && (
        <h2 className="text-title font-semibold">{topic.title}</h2>
      )}
      {themes.map((theme) => (
        <section key={theme.name} className="flex flex-col gap-2">
          <h3 className="font-semibold">{theme.name}</h3>
          {theme.reminders.length > 0 && <Dates reminders={theme.reminders} />}
          {theme.points.length > 0 && <Points points={theme.points} />}
          {theme.questions.map((q) => (
            <Ask key={q.id} question={q} />
          ))}
        </section>
      ))}
    </article>
  );
}

function Points({ points }: { points: Point[] }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  return (
    <ul className="flex flex-col gap-1.5">
      {points.map((point) => (
        <li key={point.id} className="group flex items-start gap-2">
          {point.seen ? (
            <span aria-hidden="true" className="mt-2 size-1.5 shrink-0" />
          ) : (
            <span
              role="img"
              aria-label="New"
              className="mt-2 size-1.5 shrink-0 rounded-full bg-accent"
            />
          )}
          <p className="min-w-0 flex-1">
            {point.text}
            {point.source && (
              <span className="text-xs text-faint">
                {" "}
                {point.source_url ? (
                  <a href={point.source_url} target="_blank" rel="noreferrer" className="ui-link">
                    {point.source}
                  </a>
                ) : (
                  point.source
                )}
              </span>
            )}
          </p>
          {!guest && (
            <RowAction
              label="Remove this point"
              onClick={() => sync.delete("Point", point.id)}
              className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
            >
              <X size={13} strokeWidth={1.75} aria-hidden="true" />
            </RowAction>
          )}
        </li>
      ))}
    </ul>
  );
}

/** A theme's reminders: what, how often, when next. */
function Dates({ reminders }: { reminders: Reminder[] }) {
  const { sync } = useWorkspace();
  return (
    <ul className="flex flex-col gap-1">
      {reminders.map((reminder) => {
        const time = timeWords(reminder.at);
        return (
          <li key={reminder.id} className="flex items-center gap-2 text-xs text-muted">
            <NotifyBell reminder={reminder} />
            <span className="min-w-0 flex-1 truncate">
              {reminder.title}, {rhythmWords(reminder.every_days)}: next {dueWords(reminder.due_on)}
              {time ? ` at ${time}` : ""}
            </span>
            <ReminderMenu sync={sync} reminder={reminder} />
          </li>
        );
      })}
    </ul>
  );
}

/** A question Almanac waits on, where it's about. */
function Ask({ question }: { question: Question }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const add = () => {
    if (question.point) {
      sync.create("Point", {
        topic_id: question.topic_id,
        theme: question.theme ?? "Notes",
        text: question.point,
        source: question.source ?? "You said",
        talk_id: question.talk_id ?? undefined,
        seen: true,
      });
    }
    sync.update("Question", question.id, { status: question.point ? "added" : "answered" });
  };
  return (
    <div className="ui-tint-info flex flex-col gap-2 rounded-md px-3 py-2">
      <p>{question.text}</p>
      {!guest && (
        <div className="flex flex-wrap gap-2">
          <Button onClick={add}>{question.point ? "Add it" : "Yes"}</Button>
          <Button
            variant="secondary"
            onClick={() => sync.update("Question", question.id, { status: "left_out" })}
          >
            {question.point ? "Leave it out" : "No"}
          </Button>
        </div>
      )}
    </div>
  );
}

/** "Kept from 2 talks, last today." */
function KeptFrom({ points }: { points: Point[] }) {
  if (points.length === 0) return null;
  const talks = new Set(points.map((p) => p.talk_id).filter(Boolean)).size;
  const last = points.reduce((a, b) => (a.created_at > b.created_at ? a : b));
  const when = dueWords(dayKey(new Date(last.created_at)));
  const words: ReactNode = talks
    ? `Kept from ${talks} ${talks === 1 ? "talk" : "talks"}, last ${when}`
    : `Last kept ${when}`;
  return <p className="mt-auto text-xs text-faint">{words}</p>;
}

/** The bar under the page: a question starts a talk. */
function AskBar({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  const [text, setText] = useState("");
  const input = useRef<HTMLInputElement>(null);
  // "/" puts you in the bar, as in a search field.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input,textarea,[contenteditable],[role=dialog],[role=menu]")) return;
      event.preventDefault();
      input.current?.focus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  return (
    <form
      className="mx-auto w-full max-w-4xl px-6 pb-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (!text.trim()) return;
        onAsk(text);
        setText("");
      }}
    >
      <input
        ref={input}
        value={text}
        onChange={(e) => setText(e.target.value)}
        aria-label={`Ask ${topic.name}`}
        placeholder={`Ask ${topic.name}, or tell it something`}
        className="ui-field h-10 w-full rounded-lg px-4"
      />
    </form>
  );
}
