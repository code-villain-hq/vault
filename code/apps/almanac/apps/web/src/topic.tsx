// A topic's page: what it sticks to, what it kept by theme (a card each,
// or a document's sections) or, for a routine, by what each point is, the
// questions it waits on, its week, and the talk beside it. The workspace's own topic, Almanac, has the index of the
// others for its page.

import type { ClaudeKeyInfo, Point, Question, Reminder, Topic } from "@almanac/types";
import type { SyncClient } from "@code-villain/sync-client";
import { type Confirmation, ConfirmDialog, MoreMenu, RowAction, useApi } from "@code-villain/web";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Circle,
  CircleCheck,
  MessagesSquare,
  X,
} from "lucide-react";
import {
  type ReactNode,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { type Asking, useTopicTalks } from "./asking";
import {
  addDays,
  ago,
  dayKey,
  dueWords,
  fromKey,
  rhythmWords,
  timeWords,
  today,
  weekdayWords,
  weekOf,
  weekWords,
} from "./format";
import { topicIcon } from "./icons";
import { isOwn, PageHeader, useTopics, useUnread } from "./layout";
import type { Picked } from "./photos";
import {
  AddPhoto,
  DropHint,
  PickedPhotos,
  type Picking,
  PickingProblem,
  useDrop,
  usePicking,
} from "./picking";
import { inline } from "./prose";
import { NotifyBell, occurrences, ReminderMenu } from "./reminders";
import type { Schema } from "./sync";
import { TalkPanel, useTalks } from "./talk";
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
  const talks = useTalks(id);
  const { asking, ask, open: kept, setOpen } = useTopicTalks(id);

  const open = useCallback(
    (talk: string | null | undefined) => {
      const shown = talk === undefined ? undefined : (talk ?? "new");
      setOpen(shown);
      void navigate({ to: ".", search: shown === undefined ? {} : { talk: shown }, replace: true });
    },
    [navigate, setOpen],
  );
  // A link that names the talk is where the topic now is.
  useEffect(() => {
    if (search.talk !== undefined) setOpen(search.talk);
  }, [search.talk, setOpen]);

  if (!topic) {
    if (!ready) return null;
    return (
      <>
        <PageHeader title="Topic" />
        <Empty title="This topic is gone">It was deleted, or it's in another workspace.</Empty>
      </>
    );
  }
  // A link without one (the sidebar's) opens the talk the topic was left on,
  // while it's there.
  const left =
    kept === "new" || talks.some((t) => t.id === kept) || (kept && asking?.talkId === kept)
      ? kept
      : undefined;
  const shown = search.talk ?? left;
  const talkId = shown && shown !== "new" ? shown : null;
  return (
    <TopicView
      // A new topic starts with its own state: what it asked, what it saw.
      key={topic.id}
      topic={topic}
      panel={shown !== undefined}
      talkId={talkId}
      draft={search.ask ?? ""}
      asking={asking}
      onAsk={(text, talk, photos) => {
        if (talk === null) open(null);
        void ask(text, talk, photos);
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
  asking: Asking | null;
  onAsk: (text: string, talkId: string | null, photos?: Picked[]) => void;
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
  // Without the talk, a photo dropped on the page goes in the bar.
  const bar = useRef<HTMLInputElement>(null);
  const picking = usePicking(() => bar.current?.focus());
  const drop = useDrop(picking);

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
              // The workspace's own topic stays; you can rename it.
              ...(isOwn(topic)
                ? []
                : [
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
                  ]),
            ]}
          />
        )}
      </PageHeader>
      <div className="flex min-h-0 flex-1">
        <div className="relative flex min-w-0 flex-1 flex-col" {...(panel ? {} : drop.handlers)}>
          <div className="page-content min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto flex max-w-4xl flex-col gap-8 px-6 py-6">
              {isOwn(topic) ? (
                <OwnPageBody topic={topic} onAsk={(text) => onAsk(text, null)} />
              ) : (
                <TopicPageBody topic={topic} onAsk={(text) => onAsk(text, null)} />
              )}
            </div>
          </div>
          {!panel && (
            <AskBar
              topic={topic}
              field={bar}
              picking={picking}
              onAsk={(text, photos) => onAsk(text, null, photos)}
            />
          )}
          {!panel && drop.dropping && <DropHint className="bg-bg/80" />}
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
  if (topic.shape === "routine" && !empty) {
    return <RoutinePage topic={topic} {...mine} onAsk={onAsk} />;
  }

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

/**
 * Without a Claude key nothing answers: say so where it's noticed. On this
 * Mac Claude Code answers, and a talk says when it can't; asking here would
 * start it on every page.
 */
function NoKey() {
  const { place } = useWorkspace();
  return place.kind === "mac" ? null : <NoKeyOnServer />;
}

function NoKeyOnServer() {
  const { workspace } = useWorkspace();
  const { data } = useApi<ClaudeKeyInfo>(`/api/w/${workspace.id}/claude`);
  if (data?.via !== "key" || data.set || data.from_settings) return null;
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

/**
 * The workspace's own topic: every other topic by group, as in the sidebar,
 * with what it sticks to and what's new. Almanac keeps it; nobody writes it.
 */
function OwnPageBody({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  const { workspace } = useWorkspace();
  const topics = useTopics();
  const Icon = topicIcon(topic.icon);
  const groups = useMemo(() => {
    const byName = new Map<string, Topic[]>();
    for (const other of topics.filter((t) => !isOwn(t))) {
      const list = byName.get(other.group_name) ?? [];
      list.push(other);
      byName.set(other.group_name, list);
    }
    return [...byName.entries()].sort(([a], [b]) => (a && b ? a.localeCompare(b) : a ? 1 : -1));
  }, [topics]);

  return (
    <>
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <Icon size={18} strokeWidth={1.75} aria-hidden="true" className="text-muted" />
          <p className="text-title font-semibold">{topic.name}</p>
        </div>
        <p className="text-muted">
          Knows what's in {workspace.name}, and starts a topic for whatever you want to keep.
        </p>
      </div>
      <NoKey />
      {groups.length === 0 ? (
        <div className="flex flex-col gap-6">
          <div className="flex flex-col gap-1">
            <p className="font-medium">No topics in {workspace.name} yet.</p>
            <p className="text-muted">
              Tell {topic.name} what you want to keep track of. It starts a topic for each, and
              lists them here.
            </p>
          </div>
          <Starters topic={topic} onAsk={onAsk} />
        </div>
      ) : (
        <div className="flex flex-col gap-0.5">
          {groups.map(([group, list]) => (
            <section key={group || "-"} aria-label={group || "Topics"} className="flex flex-col">
              <h2 className="px-2.5 pt-3 pb-0.5 text-xs text-faint first:pt-0">
                {group || "Topics"}
              </h2>
              {list.map((other) => (
                <IndexRow key={other.id} topic={other} />
              ))}
            </section>
          ))}
        </div>
      )}
    </>
  );
}

/** One topic in the index: its name, what's new, how much it keeps, what it sticks to. */
function IndexRow({ topic }: { topic: Topic }) {
  const { workspace } = useWorkspace();
  const unread = useUnread();
  const points = useRecords("Point");
  const talks = useRecords("Talk");
  const Icon = topicIcon(topic.icon);
  const kept = points.filter((p) => p.topic_id === topic.id).length;
  const last = talks
    .filter((t) => t.topic_id === topic.id)
    .reduce<string | null>((at, t) => (at && at > t.last_at ? at : t.last_at), null);
  const detail = [
    topic.shape === "document"
      ? "a document"
      : kept === 0
        ? "nothing kept yet"
        : `${kept} ${kept === 1 ? "point" : "points"}`,
    last ? ago(last) : null,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <Link
      to="/$workspace/topics/$topic"
      params={{ workspace: workspace.url_key, topic: topic.id }}
      className="ui-row flex items-start gap-2.5 rounded-md px-2.5 py-2 hover:bg-hover"
    >
      <Icon
        size={15}
        strokeWidth={1.75}
        aria-hidden="true"
        className="mt-0.5 shrink-0 text-muted"
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-baseline gap-2">
          <span className="truncate font-semibold">{topic.name}</span>
          {unread.has(topic.id) && (
            <span
              role="img"
              aria-label="New points"
              className="size-1.5 shrink-0 self-center rounded-full bg-accent"
            />
          )}
          <span className="ml-auto shrink-0 text-xs text-faint">{detail}</span>
        </div>
        {topic.sticks_to && <p className="text-muted">{topic.sticks_to}</p>}
      </div>
    </Link>
  );
}

function EmptyPage({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <p className="font-medium">Nothing kept yet.</p>
        <p className="text-muted">
          What you ask {topic.name} is kept here by theme, with where it comes from. Ask below, or
          tell it what you already know.
        </p>
      </div>
      <Starters topic={topic} onAsk={onAsk} />
    </div>
  );
}

/** The topic's first questions, to ask with a click. */
function Starters({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  if (topic.starters.length === 0) return null;
  return (
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
  );
}

/** This week of the topic's reminders, a day each. */
function WeekStrip({ topic, reminders }: { topic: Topic; reminders: Reminder[] }) {
  const now = today();
  const [days, setDays] = useState(() => weekOf(now));
  const first = days[0] ?? now;
  return (
    <section className="flex flex-col gap-2" aria-label={`${topic.name} this week`}>
      <div className="flex items-center gap-2">
        <h2 className="text-xs font-semibold text-muted">{weekWords(days)}</h2>
        <div className="flex items-center">
          <button
            type="button"
            onClick={() => setDays(weekOf(addDays(first, -7)))}
            aria-label="The week before"
            title="The week before"
            className="ui-button ui-icon-button"
          >
            <ChevronLeft size={15} strokeWidth={1.75} aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={() => setDays(weekOf(addDays(first, 7)))}
            aria-label="The week after"
            title="The week after"
            className="ui-button ui-icon-button"
          >
            <ChevronRight size={15} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </div>
        {!days.includes(now) && (
          <button type="button" onClick={() => setDays(weekOf(now))} className="ui-button">
            This week
          </button>
        )}
      </div>
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
          {theme.points.length > 0 && <Points points={theme.points} passages />}
          {theme.questions.map((q) => (
            <Ask key={q.id} question={q} />
          ))}
        </section>
      ))}
    </article>
  );
}

/**
 * A theme's points: on a card, short notes with their source after them; in
 * a document, passages to read, with their source and their talk under them.
 */
function Points({ points, passages = false }: { points: Point[]; passages?: boolean }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const talks = useRecords("Talk");
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  return (
    <ul className={passages ? "flex flex-col gap-4" : "flex flex-col gap-1.5"}>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
      {points.map((point) => {
        const source =
          point.source &&
          (point.source_url ? (
            <a href={point.source_url} target="_blank" rel="noreferrer" className="ui-link">
              {point.source}
            </a>
          ) : (
            point.source
          ));
        const talk = passages ? talks.find((t) => t.id === point.talk_id) : undefined;
        const dot = passages ? "mt-2.5" : "mt-2";
        return (
          <li key={point.id} className="group flex items-start gap-2">
            {point.seen ? (
              <span aria-hidden="true" className={`${dot} size-1.5 shrink-0`} />
            ) : (
              <span
                role="img"
                aria-label="New"
                className={`${dot} size-1.5 shrink-0 rounded-full bg-accent`}
              />
            )}
            {passages ? (
              <div className="min-w-0 flex-1">
                <p className="max-w-measure text-body">{inline(point.text)}</p>
                {(source || talk) && (
                  <p className="mt-1 flex flex-wrap items-center gap-x-1.5 text-xs text-faint">
                    {source}
                    {source && talk && <span aria-hidden="true">·</span>}
                    {talk && (
                      <Link
                        to="."
                        search={{ talk: talk.id }}
                        replace
                        className="ui-link inline-flex items-center gap-1"
                      >
                        <MessagesSquare size={13} strokeWidth={1.75} aria-hidden="true" />
                        {talk.title || "The talk"}
                      </Link>
                    )}
                  </p>
                )}
              </div>
            ) : (
              <p className="min-w-0 flex-1">
                {inline(point.text)}
                {source && <span className="text-xs text-faint"> {source}</span>}
              </p>
            )}
            {!guest && (
              <RowAction
                label="Remove this point…"
                onClick={() =>
                  setConfirmation({
                    title: "Remove this point?",
                    body: "It leaves the page for everyone in the workspace. This can't be undone.",
                    action: "Remove point",
                    onConfirm: () => sync.delete("Point", point.id),
                  })
                }
                className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              >
                <X size={13} strokeWidth={1.75} aria-hidden="true" />
              </RowAction>
            )}
          </li>
        );
      })}
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

/** The bar under the page: a question, or photos, start a talk. */
function AskBar({
  topic,
  field,
  picking,
  onAsk,
}: {
  topic: Topic;
  field: RefObject<HTMLInputElement | null>;
  picking: Picking;
  onAsk: (text: string, photos: Picked[]) => void;
}) {
  const [text, setText] = useState("");
  // "/" puts you in the bar, as in a search field.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input,textarea,[contenteditable],[role=dialog],[role=menu]")) return;
      event.preventDefault();
      field.current?.focus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [field]);
  return (
    <form
      className="mx-auto flex w-full max-w-4xl flex-col gap-2 px-6 pb-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (!text.trim() && picking.photos.length === 0) return;
        onAsk(text, picking.photos);
        setText("");
        picking.clear();
      }}
    >
      <PickingProblem picking={picking} />
      <div className="ui-field flex flex-col rounded-lg">
        <PickedPhotos picking={picking} />
        <div className="flex items-center pr-1">
          <input
            ref={field}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onPaste={picking.onPaste}
            aria-label={`Ask ${topic.name}`}
            placeholder={
              isOwn(topic)
                ? `Tell ${topic.name} what you want to keep track of`
                : `Ask ${topic.name}, or tell it something`
            }
            className="h-10 min-w-0 flex-1 bg-transparent px-4 outline-none placeholder:text-faint"
          />
          <AddPhoto picking={picking} />
        </div>
      </div>
    </form>
  );
}

/**
 * A routine's page, by what each point is: the day's steps, its week, what's
 * true of you, your shelf and the advice. Notes not sorted yet stay on cards
 * under them, with a way to have them sorted.
 */
function RoutinePage({
  topic,
  points,
  questions,
  reminders,
  onAsk,
}: {
  topic: Topic;
  points: Point[];
  questions: Question[];
  reminders: Reminder[];
  onAsk: (text: string) => void;
}) {
  const of = (kind: string) => points.filter((p) => p.kind === kind);
  const shelf = byTheme({ points: of("thing"), questions: [], reminders: [] });
  const things = new Set(shelf.map((t) => t.name));
  const notes = byTheme({
    points: of("note"),
    questions: questions.filter((q) => q.theme && !things.has(q.theme)),
    reminders: [],
  });
  const routines = routineDays(of("step"), reminders);
  const loose = questions.filter((q) => !q.theme);
  return (
    <>
      <div className="flex flex-col gap-1">
        <p className="text-title font-semibold">{topic.name}</p>
        {topic.sticks_to && <p className="text-muted">{topic.sticks_to}</p>}
      </div>
      <NoKey />
      <Steps topic={topic} routines={routines} />
      {reminders.length > 0 && <WeekStrip topic={topic} reminders={reminders} />}
      {loose.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Waiting on you">
          {loose.map((q) => (
            <Ask key={q.id} question={q} />
          ))}
        </section>
      )}
      <AboutYou points={of("about")} />
      {shelf.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Your shelf">
          <h2 className="text-xs text-faint">Your shelf</h2>
          <div className="@container">
            <div className="grid gap-2.5 @lg:grid-cols-2 @3xl:grid-cols-3">
              {shelf.map((thing) => (
                <Thing
                  key={thing.name}
                  thing={thing}
                  days={thingDays(thing.name, routines)}
                  questions={questions.filter((q) => q.theme === thing.name)}
                />
              ))}
            </div>
          </div>
        </section>
      )}
      <Advice points={of("tip")} />
      {notes.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Not sorted yet">
          <div className="flex items-center gap-2">
            <h2 className="text-xs text-faint">Not sorted yet</h2>
            <button
              type="button"
              className="ui-button ml-auto"
              onClick={() => onAsk("Sort the notes that aren't sorted yet into the routine.")}
            >
              Sort them
            </button>
          </div>
          <Cards themes={notes} />
        </section>
      )}
    </>
  );
}

/** A routine day: its name, its steps in order, and the reminders it comes round with. */
interface RoutineDay {
  name: string;
  steps: Point[];
  reminders: Reminder[];
}

/**
 * The routines the steps make, each with its reminders: the ones called by
 * its name, or starting with it ("Elvital wash + conditioner").
 */
function routineDays(steps: Point[], reminders: Reminder[]): RoutineDay[] {
  return byTheme({ points: steps, questions: [], reminders: [] }).map(({ name, points }) => {
    const exact = reminders.filter((r) => r.title === name);
    const called = name.toLowerCase();
    return {
      name,
      steps: points,
      reminders: exact.length
        ? exact
        : reminders.filter((r) => r.title.toLowerCase().startsWith(called)),
    };
  });
}

/**
 * The routine due today, or the next one this week, with the day it's on;
 * without reminders to go by, the first routine, for today.
 */
function nextRoutine(
  routines: RoutineDay[],
): { day: string; dated: boolean; routine: RoutineDay } | null {
  const now = today();
  for (let i = 0; i < 7; i++) {
    const day = addDays(now, i);
    const routine = routines.find((r) => r.reminders.some((m) => occurrences(m, day, day).length));
    if (routine) return { day, dated: true, routine };
  }
  const first = routines[0];
  return first ? { day: now, dated: false, routine: first } : null;
}

/** "Elvital shampoo: on the scalp" as what and how. */
function whatAndHow(text: string): [string, string | null] {
  const at = text.indexOf(": ");
  if (at <= 0 || at > 60) return [text, null];
  const how = text.slice(at + 2);
  return [text.slice(0, at), how.charAt(0).toUpperCase() + how.slice(1)];
}

/** The day's steps, to tick off as you go. Ticks stay on this device, for that day. */
function Steps({ topic, routines }: { topic: Topic; routines: RoutineDay[] }) {
  const { sync } = useWorkspace();
  const next = nextRoutine(routines);
  const key = next && `almanac.ticked.${topic.id}.${next.day}`;
  const [ticked, setTicked] = useState<string[]>([]);
  useEffect(() => {
    if (!key) return;
    try {
      setTicked(JSON.parse(localStorage.getItem(key) ?? "[]"));
    } catch {
      setTicked([]);
    }
  }, [key]);
  if (!next) return null;
  const { day, dated, routine } = next;
  const reminder = routine.reminders.find((r) => occurrences(r, day, day).length);
  const tick = (id: string) => {
    const now = ticked.includes(id) ? ticked.filter((t) => t !== id) : [...ticked, id];
    setTicked(now);
    try {
      if (key) localStorage.setItem(key, JSON.stringify(now));
    } catch {}
  };
  const when = dueWords(day).replace(/^./, (c) => c.toUpperCase());
  const heading = dated ? `${when}, ${routine.name}` : routine.name;
  return (
    <section className="flex flex-col gap-2" aria-label={heading}>
      <div className="flex items-center gap-2">
        <h2 className="font-semibold">{heading}</h2>
        {reminder && <ReminderMenu sync={sync} reminder={reminder} />}
      </div>
      <div className="@container">
        <ol className="grid gap-2.5 @lg:grid-cols-2 @3xl:grid-cols-3">
          {routine.steps.map((step, n) => {
            const [what, how] = whatAndHow(step.text);
            const done = ticked.includes(step.id);
            return (
              <li key={step.id}>
                <button
                  type="button"
                  aria-pressed={done}
                  aria-label={`${whatAndHow(step.text)[0]}: ${done ? "done" : "to do"}`}
                  onClick={() => tick(step.id)}
                  className="flex w-full items-start gap-3 rounded-lg bg-surface px-4 py-3.5 text-left hover:bg-hover"
                >
                  <span
                    aria-hidden="true"
                    className={`text-title leading-none font-bold ${done ? "text-faint" : "text-muted"}`}
                  >
                    {n + 1}
                  </span>
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className={`font-semibold ${done ? "text-faint line-through" : ""}`}>
                      {what}
                    </span>
                    {how && <span className="text-xs text-muted">{how}</span>}
                  </span>
                  {done ? (
                    <CircleCheck
                      size={18}
                      strokeWidth={1.75}
                      aria-hidden="true"
                      className="shrink-0 text-accent"
                    />
                  ) : (
                    <Circle
                      size={18}
                      strokeWidth={1.75}
                      aria-hidden="true"
                      className="shrink-0 text-faint"
                    />
                  )}
                </button>
              </li>
            );
          })}
        </ol>
      </div>
    </section>
  );
}

/** What's true of you, as chips: "Low porosity, by the float test". */
function AboutYou({ points }: { points: Point[] }) {
  if (points.length === 0) return null;
  return (
    <section className="flex flex-col gap-2" aria-label="You">
      <h2 className="text-xs text-faint">You</h2>
      <ul className="flex flex-wrap gap-1.5">
        {points.map((point) => {
          const at = point.text.indexOf(", ");
          return (
            <li
              key={point.id}
              title={point.source ?? undefined}
              className="ui-chip flex items-center gap-1.5 px-2.5 py-1"
            >
              {!point.seen && (
                <span role="img" aria-label="New" className="size-1.5 rounded-full bg-accent" />
              )}
              {at > 0 ? (
                <span>
                  <span className="font-semibold text-fg">{point.text.slice(0, at)}</span>
                  {point.text.slice(at)}
                </span>
              ) : (
                <span className="text-fg">{point.text}</span>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** The weekdays a thing is used on: those of the routines whose steps name it. */
function thingDays(name: string, routines: RoutineDay[]): string[] {
  // A step names the thing by its first word: "Elvital shampoo" for Elvital Hyaluron Plump.
  const word = name.split(/\s+/)[0]?.toLowerCase() ?? "";
  if (word.length < 3) return [];
  const days = weekOf(today());
  return days.filter((day) =>
    routines.some(
      (r) =>
        r.steps.some((s) => whatAndHow(s.text)[0].toLowerCase().includes(word)) &&
        r.reminders.some((m) => occurrences(m, day, day).length),
    ),
  );
}

/** "Wed, Fri and Sun". */
function daysWords(days: string[]): string {
  const names = days.map((d) => fromKey(d).toLocaleDateString("en", { weekday: "short" }));
  return names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : names.join("");
}

/** A thing on your shelf: what it is, when and how you use it, and its other notes. */
function Thing({
  thing,
  days,
  questions,
}: {
  thing: Theme;
  days: string[];
  questions: Question[];
}) {
  const [open, setOpen] = useState(false);
  const [what, how, ...rest] = thing.points;
  const fresh = thing.points.some((p) => !p.seen);
  return (
    <article className="flex flex-col gap-1.5 rounded-lg bg-surface px-4 py-3.5">
      {what && <p className="text-xs text-faint">{what.text}</p>}
      <h3 className="flex items-center gap-2 font-semibold">
        {thing.name}
        {fresh && <span role="img" aria-label="New" className="size-1.5 rounded-full bg-accent" />}
      </h3>
      {days.length > 0 && (
        <p className="flex items-center gap-1.5 text-xs text-muted">
          <CalendarDays size={12} strokeWidth={1.75} aria-hidden="true" className="text-faint" />
          {daysWords(days)}
        </p>
      )}
      {how && <p className="text-xs">{inline(how.text)}</p>}
      {questions.map((q) => (
        <Ask key={q.id} question={q} />
      ))}
      {rest.length > 0 && (
        <div className="mt-1 flex flex-col gap-2">
          <button
            type="button"
            className="ui-disclosure self-start"
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            <ChevronRight size={13} strokeWidth={1.75} aria-hidden="true" />
            {rest.length} {rest.length === 1 ? "note" : "notes"}
          </button>
          {open && <Points points={rest} />}
        </div>
      )}
    </article>
  );
}

/** Advice worth following, the newest three, with where it comes from. */
function Advice({ points }: { points: Point[] }) {
  const [all, setAll] = useState(false);
  if (points.length === 0) return null;
  const shown = all ? points : points.slice(-3);
  return (
    <section className="flex flex-col gap-1" aria-label="Advice">
      <h2 className="text-xs text-faint">Advice</h2>
      <ul className="flex flex-col">
        {shown.map((point) => (
          <li key={point.id} className="group flex items-baseline gap-3 rounded-md px-2.5 py-1.5">
            {!point.seen && (
              <span
                role="img"
                aria-label="New"
                className="size-1.5 shrink-0 self-center rounded-full bg-accent"
              />
            )}
            <p className="min-w-0 flex-1">{inline(point.text)}</p>
            {point.source && (
              <span className="shrink-0 text-xs text-faint">
                {point.source_url ? (
                  <a href={point.source_url} target="_blank" rel="noreferrer" className="ui-link">
                    {point.source}
                  </a>
                ) : (
                  point.source
                )}
              </span>
            )}
            <RemovePoint point={point} />
          </li>
        ))}
      </ul>
      {points.length > 3 && (
        <button
          type="button"
          className="ui-disclosure self-start px-2.5"
          aria-expanded={all}
          onClick={() => setAll(!all)}
        >
          <ChevronRight size={13} strokeWidth={1.75} aria-hidden="true" />
          {all ? "The newest three" : `${points.length - 3} more`}
        </button>
      )}
    </section>
  );
}

/** Takes a point off the page, once you say so. */
function RemovePoint({ point }: { point: Point }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  if (guest) return null;
  return (
    <>
      <RowAction
        label="Remove this point…"
        onClick={() =>
          setConfirmation({
            title: "Remove this point?",
            body: "It leaves the page for everyone in the workspace. This can't be undone.",
            action: "Remove point",
            onConfirm: () => sync.delete("Point", point.id),
          })
        }
        className="self-center opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
      >
        <X size={13} strokeWidth={1.75} aria-hidden="true" />
      </RowAction>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}
