// A topic's page: its blocks in order, each laying out what it keeps one
// way (the day's steps, the week, chips, a shelf, cards, text, a list or a
// timeline), the questions it waits on, and the talk beside it. A section
// opens on a page of its own, in the page's place, and so does the topic's
// About: its name, what it sticks to, what it knows about you and its
// blocks. The workspace's own topic, Almanac, has the index of the others
// for its page.

import type { Block, ClaudeKeyInfo, Point, Question, Reminder, Topic } from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import { Menu } from "@base-ui/react/menu";
import { Popover } from "@base-ui/react/popover";
import type { SyncClient } from "@code-villain/sync-client";
import {
  type Confirmation,
  ConfirmDialog,
  CreateDialog,
  focusBackAfterKeys,
  MoreMenu,
  overlayOpen,
  RowAction,
  useApi,
} from "@code-villain/web";
import { Link, useLocation, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import {
  ArrowLeft,
  CalendarDays,
  ChevronDown,
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
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { type Asking, useTopicTalks } from "./asking";
import {
  addDays,
  ago,
  dateWords,
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
import { TOPIC_ICON_NAMES, topicIcon } from "./icons";
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
import { about, inline } from "./prose";
import { NotifyBell, occurrences, ReminderMenu } from "./reminders";
import type { Schema } from "./sync";
import { TalkPanel, useTalks } from "./talk";
import { bySection, inOrder, type When, when } from "./timeline";
import { Button, Empty } from "./ui";
import { useIsGuest, useRecords, useSyncStatus, useWorkspace } from "./workspace";

/**
 * `?talk=` opens the talk panel (`new` for a new talk); `?ask=` starts it
 * with a question; `?section=` opens a section, by its theme, in the page's
 * place, and `?about` the topic's About.
 */
export interface TopicSearch {
  talk?: string;
  ask?: string;
  section?: string;
  about?: true;
}

/** What's open in the page's place: a section, About, or the page. */
function placeOf(search: TopicSearch): TopicSearch {
  return search.section ? { section: search.section } : search.about ? { about: true } : {};
}

/** The search that opens `section` (none: the page), keeping the open talk. */
function inSection(section: string | undefined) {
  return (prev: TopicSearch): TopicSearch => ({
    ...(prev.talk ? { talk: prev.talk } : {}),
    ...(section ? { section } : {}),
  });
}

/** The search that opens the topic's About, keeping the open talk. */
function inAbout(prev: TopicSearch): TopicSearch {
  return { ...(prev.talk ? { talk: prev.talk } : {}), about: true };
}

/** The search that opens a talk beside what's open. */
function withTalk(talk: string) {
  return (prev: TopicSearch): TopicSearch => ({ ...placeOf(prev), talk });
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
      void navigate({
        to: ".",
        search: (prev: TopicSearch) =>
          shown === undefined ? placeOf(prev) : withTalk(shown)(prev),
        replace: true,
      });
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
  // Asked with a section open, a question is about it, in the talk too.
  const section = isOwn(topic) ? undefined : search.section;
  return (
    <TopicView
      // A new topic starts with its own state: what it asked, what it saw.
      key={topic.id}
      topic={topic}
      section={section}
      about={!section && search.about === true}
      panel={shown !== undefined}
      talkId={talkId}
      draft={search.ask ?? ""}
      asking={asking}
      onAsk={(text, talk, photos) => {
        if (talk === null) open(null);
        void ask(text, talk, photos, section);
      }}
      onOpen={open}
    />
  );
}

function TopicView({
  topic,
  section,
  about,
  panel,
  talkId,
  draft,
  asking,
  onAsk,
  onOpen,
}: {
  topic: Topic;
  /** The section open in the page's place. */
  section: string | undefined;
  /** The topic's About, in the page's place. */
  about: boolean;
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
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const navigate = useNavigate();
  const { workspace } = useWorkspace();
  useSeenOnLeave(sync, topic.id);
  // Without the talk, a photo dropped on the page goes in the bar.
  const bar = useRef<HTMLInputElement>(null);
  const picking = usePicking(() => bar.current?.focus());
  const drop = useDrop(picking);
  // The page keeps where it was scrolled to while a section or About is
  // open; About's blocks open it at one (`#block-…`).
  const place = section ? `section ${section}` : about ? "about" : "page";
  const { hash } = useLocation();
  const scroller = useRef<HTMLDivElement>(null);
  const scrolled = useRef(new Map<string, number>());
  useLayoutEffect(() => {
    if (scroller.current) scroller.current.scrollTop = scrolled.current.get(place) ?? 0;
    if (hash) document.getElementById(hash)?.scrollIntoView({ block: "start" });
  }, [place, hash]);
  // Esc goes back from a section or About to the page, once the talk is closed.
  useEffect(() => {
    if (place === "page" || panel) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || overlayOpen()) return;
      event.preventDefault();
      void navigate({ to: ".", search: inSection(undefined) });
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [place, panel, navigate]);

  return (
    <>
      <PageHeader
        title={section ?? (about ? "About" : topic.name)}
        parent={
          (section || about) && (
            <>
              {workspace.name}
              <span aria-hidden="true" className="text-faint">
                ›
              </span>
              <Link to="." search={inSection(undefined)} className="truncate hover:text-fg">
                {topic.name}
              </Link>
            </>
          )
        }
      >
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
              ...(about
                ? []
                : [
                    {
                      label: `About ${topic.name}`,
                      onSelect: () => void navigate({ to: ".", search: inAbout }),
                    },
                  ]),
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
          <div
            ref={scroller}
            onScroll={(e) => scrolled.current.set(place, e.currentTarget.scrollTop)}
            className="page-content min-h-0 flex-1 overflow-y-auto"
          >
            <div className="flex flex-col gap-8 px-6 py-6 sm:px-10">
              {about ? (
                <AboutPageBody topic={topic} />
              ) : isOwn(topic) ? (
                <OwnPageBody topic={topic} onAsk={(text) => onAsk(text, null)} />
              ) : section ? (
                <SectionPageBody topic={topic} section={section} />
              ) : (
                <TopicPageBody topic={topic} onAsk={(text) => onAsk(text, null)} />
              )}
            </div>
          </div>
          {!panel && (
            <AskBar
              topic={topic}
              about={section}
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

/** What a topic's page holds, for its blocks to lay out. */
interface PageParts {
  blocks: Block[];
  points: Point[];
  questions: Question[];
  reminders: Reminder[];
}

/** The topic's blocks and points in page order, its open questions and its reminders that are on. */
function usePage(topic: Topic): PageParts {
  const blocks = useRecords("Block");
  const points = useRecords("Point");
  const questions = useRecords("Question");
  const reminders = useRecords("Reminder");
  return useMemo<PageParts>(() => {
    const ofTopic = <T extends { topic_id: string }>(list: T[]) =>
      list.filter((r) => r.topic_id === topic.id);
    return {
      blocks: ofTopic(blocks).sort(
        (a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at),
      ),
      points: ofTopic(points).sort((a, b) => a.position - b.position),
      questions: ofTopic(questions)
        .filter((q) => q.status === "open")
        .sort((a, b) => a.created_at.localeCompare(b.created_at)),
      reminders: ofTopic(reminders)
        .filter((r) => r.status === "on")
        .sort((a, b) => a.due_on.localeCompare(b.due_on)),
    };
  }, [blocks, points, questions, reminders, topic.id]);
}

/** What the page shows: all but what it knows about you, which About shows. */
function onThePage(page: PageParts): PageParts {
  const shown = page.blocks.filter((b) => b.kind !== "you");
  const ids = new Set(shown.map((b) => b.id));
  return { ...page, blocks: shown, points: page.points.filter((p) => ids.has(p.block_id)) };
}

function TopicPageBody({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  const all = usePage(topic);
  const page = useMemo(() => onThePage(all), [all]);
  const homes = useMemo(() => questionHomes(page), [page]);
  // The shelf's tiles say which days each thing is used on.
  const routines = useMemo(() => {
    const steps = new Set(page.blocks.filter((b) => b.kind === "today").map((b) => b.id));
    return routineDays(
      page.points.filter((p) => steps.has(p.block_id)),
      page.reminders,
    );
  }, [page]);
  const loose = page.questions.filter((q) => !homes.has(q.id));

  return (
    <>
      <TitleToAbout topic={topic} />
      <NoKey />
      {loose.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Waiting on you">
          {loose.map((q) => (
            <Ask key={q.id} question={q} block={keeping(page.blocks)} />
          ))}
        </section>
      )}
      {page.blocks.length === 0 && (
        <div className="flex flex-col gap-1">
          <p className="font-medium">Nothing kept yet.</p>
          <p className="text-muted">
            What you ask {topic.name} is kept here, with where it comes from. Ask below, or tell it
            what you already know.
          </p>
        </div>
      )}
      {page.blocks.map((block) => (
        <BlockView
          key={block.id}
          topic={topic}
          block={block}
          page={page}
          questions={page.questions.filter((q) => homes.get(q.id) === block.id)}
          routines={routines}
        />
      ))}
      {page.points.length === 0 && <Starters topic={topic} onAsk={onAsk} />}
    </>
  );
}

/** The page's title, the topic's name, which opens its About. */
function TitleToAbout({ topic }: { topic: Topic }) {
  return (
    <Link
      to="."
      search={inAbout}
      title={`About ${topic.name}`}
      className="group/name inline-flex min-w-0 items-center gap-1 self-start text-title font-semibold"
    >
      {topic.name}
      <ChevronRight
        size={18}
        strokeWidth={1.75}
        aria-hidden="true"
        className="shrink-0 text-faint group-hover/name:text-muted"
      />
    </Link>
  );
}

/** Back to the topic's page, above the title of a section's page and of About. */
function BackTo({ topic }: { topic: Topic }) {
  return (
    <Link
      to="."
      search={inSection(undefined)}
      className="ui-button -mb-5 -ml-2 gap-1.5 self-start px-2 text-xs text-muted"
    >
      <ArrowLeft size={14} strokeWidth={1.75} aria-hidden="true" className="text-faint" />
      {topic.name}
    </Link>
  );
}

/**
 * A section on a page of its own: everything kept under its name, from every
 * block, as its block lays it out (the events, then the passages), with its
 * reminders, the questions about it, and the talks it came from.
 */
function SectionPageBody({ topic, section }: { topic: Topic; section: string }) {
  const page = usePage(topic);
  const talks = useRecords("Talk");
  const points = page.points.filter((p) => p.theme === section);
  const questions = page.questions.filter((q) => q.theme === section);
  const reminders = page.reminders.filter((r) => r.theme === section);
  // Its events come first, then the rest as the page has them.
  const blocks = page.blocks
    .filter((b) => points.some((p) => p.block_id === b.id))
    .sort((a, b) => Number(b.kind === "timeline") - Number(a.kind === "timeline"));
  // A question's point goes where the section's others are.
  const home = blocks.find((b) => ASKS_IN.includes(b.kind)) ?? keeping(page.blocks);
  // Passages name their talk; the talks of the rest are listed under them.
  const passages = new Set(blocks.filter((b) => b.kind === "text").map((b) => b.id));
  const named = new Set(points.filter((p) => passages.has(p.block_id)).map((p) => p.talk_id));
  const from = talks
    .filter((t) => !named.has(t.id) && points.some((p) => p.talk_id === t.id))
    .sort((a, b) => b.last_at.localeCompare(a.last_at));

  if (points.length + questions.length + reminders.length === 0) {
    return (
      <div className="flex flex-col gap-1">
        <p className="font-medium">Nothing is kept under {section}.</p>
        <p className="text-muted">
          A talk may have renamed it or moved what it kept.{" "}
          <Link to="." search={inSection(undefined)} className="ui-link">
            Back to {topic.name}
          </Link>
        </p>
      </div>
    );
  }
  return (
    <>
      <BackTo topic={topic} />
      <p className="text-title font-semibold">{section}</p>
      {reminders.length > 0 && <Dates reminders={reminders} />}
      {questions.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Waiting on you">
          {questions.map((q) => (
            <Ask key={q.id} question={q} block={home} />
          ))}
        </section>
      )}
      {blocks.map((block) => {
        const held = points.filter((p) => p.block_id === block.id);
        return (
          <section
            key={block.id}
            className="flex flex-col gap-2"
            aria-label={block.title || blockName(block)}
          >
            {block.title && <h2 className="text-xs text-faint">{block.title}</h2>}
            {block.kind === "timeline" ? (
              <Timeline points={held} />
            ) : (
              <Points points={held} passages={block.kind === "text"} />
            )}
          </section>
        );
      })}
      {from.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Talks">
          <h2 className="text-xs text-faint">Talks</h2>
          <ul className="flex flex-col gap-0.5">
            {from.map((talk) => (
              <li key={talk.id}>
                <Link
                  to="."
                  search={withTalk(talk.id)}
                  replace
                  className="ui-row flex items-center gap-2.5 rounded-md px-2.5 py-1.5 hover:bg-hover"
                >
                  <MessagesSquare
                    size={13}
                    strokeWidth={1.75}
                    aria-hidden="true"
                    className="shrink-0 text-faint"
                  />
                  <span className="min-w-0 flex-1 truncate">{talk.title || "The talk"}</span>
                  <span className="shrink-0 text-xs text-faint">{ago(talk.last_at)}</span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}

/**
 * A topic's About: what makes it answer as it does. Its name, icon and
 * group, each changed where it is; what it sticks to, whole; what it knows
 * about you, a fact a line with the talk it came from; and its page's
 * blocks. The workspace's own topic has only its name and what it sticks to.
 */
function AboutPageBody({ topic }: { topic: Topic }) {
  const own = isOwn(topic);
  const page = usePage(topic);
  const talks = useTalks(topic.id);
  const blocks = page.blocks.filter((b) => b.kind !== "you");
  const you = new Set(page.blocks.filter((b) => b.kind === "you").map((b) => b.id));
  const facts = page.points.filter((p) => you.has(p.block_id));
  const points = page.points.length;
  return (
    <>
      <BackTo topic={topic} />
      {own ? (
        <TopicName topic={topic} />
      ) : (
        <div className="flex items-center gap-3">
          <TopicIcon topic={topic} />
          <div className="flex min-w-0 flex-col gap-0.5">
            <TopicName topic={topic} />
            <div className="flex items-center gap-2.5">
              <TopicGroup topic={topic} />
              <span className="text-xs text-faint">
                Started {onDay(topic.created_at)} · {points} {points === 1 ? "point" : "points"} ·{" "}
                {talks.length} {talks.length === 1 ? "talk" : "talks"}
              </span>
            </div>
          </div>
        </div>
      )}
      <Brief topic={topic} />
      {!own && <Facts topic={topic} facts={facts} />}
      {!own && blocks.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="On the page">
          <h2 className="text-xs text-faint">On the page</h2>
          <ul className={`flex flex-col ${LINES}`}>
            {blocks.map((block) => (
              <li
                key={block.id}
                className="group/row flex min-h-8 items-center gap-4 rounded-md px-2.5 py-0.5 hover:bg-hover"
              >
                <Link
                  to="."
                  search={inSection(undefined)}
                  hash={`block-${block.id}`}
                  className="min-w-0 truncate"
                >
                  {blockName(block)}
                </Link>
                <span className="ml-auto shrink-0 text-xs text-faint">{holds(block, page)}</span>
                <BlockMenu
                  block={block}
                  blocks={blocks}
                  className="-mr-1.5 group-hover/row:opacity-100"
                />
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}

/** "Oct 3": the day of an ISO time, in the reader's time zone. */
function onDay(at: string): string {
  return dateWords(dayKey(new Date(at)));
}

/** How About names a block's kind, and what it counts: one, many. */
const HOLDS: Record<string, [kind: string, one: string, many: string]> = {
  today: ["Today", "step", "steps"],
  week: ["", "reminder", "reminders"],
  chips: ["Chips", "point", "points"],
  shelf: ["Shelf", "thing", "things"],
  cards: ["Cards", "card", "cards"],
  text: ["Text", "section", "sections"],
  list: ["List", "point", "points"],
  timeline: ["Timeline", "event", "events"],
};

/** What a block holds, for its line on About: "Shelf · 3 things", "3 reminders". */
function holds(block: Block, page: PageParts): string {
  const points = page.points.filter((p) => p.block_id === block.id);
  const [kind, one, many] = HOLDS[block.kind] ?? ["", "point", "points"];
  // The week counts its reminders; cards and text, their themes.
  const count =
    block.kind === "week"
      ? page.reminders.length
      : block.kind === "cards" || block.kind === "text"
        ? new Set(points.map((p) => p.theme)).size
        : points.length;
  const amount = count ? `${count} ${count === 1 ? one : many}` : "nothing yet";
  // A block titled by its kind says it once.
  return kind && kind !== blockName(block) ? `${kind} · ${amount}` : amount;
}

/** The topic's name, changed where it is: Enter or clicking away keeps it, Esc drops it. */
function TopicName({ topic }: { topic: Topic }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const [draft, setDraft] = useState<string | null>(null);
  const keep = () => {
    const name = draft?.trim();
    if (name && name !== topic.name) sync.update("Topic", topic.id, { name });
    setDraft(null);
  };
  const look = "-ml-1.5 rounded-md px-1.5 text-left text-title font-semibold";
  if (guest) return <p className="text-title font-semibold">{topic.name}</p>;
  if (draft === null) {
    return (
      <button
        type="button"
        title="Rename it"
        onClick={() => setDraft(topic.name)}
        className={`${look} self-start hover:bg-hover`}
      >
        {topic.name}
      </button>
    );
  }
  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: it opens to type the name
      autoFocus
      aria-label="Name"
      value={draft}
      maxLength={80}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={keep}
      onKeyDown={(event) => {
        if (event.key === "Enter") keep();
        if (event.key === "Escape") {
          event.preventDefault();
          setDraft(null);
        }
      }}
      className={`ui-field ${look} w-full max-w-measure`}
    />
  );
}

/** The topic's icon, which opens the others to pick from. */
function TopicIcon({ topic }: { topic: Topic }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const [open, setOpen] = useState(false);
  const Icon = topicIcon(topic.icon);
  const look = "size-10 rounded-lg px-0 text-muted";
  if (guest) {
    return (
      <span className={`ui-chip grid place-items-center ${look}`}>
        <Icon size={20} strokeWidth={1.75} aria-hidden="true" />
      </span>
    );
  }
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger
        aria-label="Icon"
        title="Pick another icon"
        className={`ui-button ${look} data-popup-open:bg-hover`}
      >
        <Icon size={20} strokeWidth={1.75} aria-hidden="true" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner className="z-50 outline-none" sideOffset={4} align="start">
          <Popover.Popup
            finalFocus={focusBackAfterKeys}
            className="ui-menu grid grid-cols-8 gap-1 p-1.5 outline-none"
          >
            {TOPIC_ICON_NAMES.map((name) => {
              const Pick = topicIcon(name);
              return (
                <button
                  key={name}
                  type="button"
                  aria-label={name}
                  title={name}
                  aria-pressed={topic.icon === name}
                  onClick={() => {
                    if (name !== topic.icon) sync.update("Topic", topic.id, { icon: name });
                    setOpen(false);
                  }}
                  className="ui-button ui-icon-button aria-pressed:bg-selected"
                >
                  <Pick size={15} strokeWidth={1.75} aria-hidden="true" />
                </button>
              );
            })}
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

/** The sidebar group the topic is in: another of the workspace's, a new one, or none. */
function TopicGroup({ topic }: { topic: Topic }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const topics = useTopics();
  const [naming, setNaming] = useState<string | null>(null);
  const others = [...new Set(topics.filter((t) => !isOwn(t)).map((t) => t.group_name))]
    .filter((g) => g && g !== topic.group_name)
    .sort((a, b) => a.localeCompare(b));
  const move = (group_name: string) => {
    if (group_name !== topic.group_name) sync.update("Topic", topic.id, { group_name });
  };
  const look = "-ml-1.5 h-6 gap-1 px-1.5 text-xs";
  if (guest) return <span className="text-xs text-faint">{topic.group_name}</span>;
  if (naming !== null) {
    const keep = () => {
      if (naming.trim()) move(naming.trim());
      setNaming(null);
    };
    return (
      <input
        // biome-ignore lint/a11y/noAutofocus: it opens to type the group
        autoFocus
        aria-label="New group"
        placeholder="New group"
        value={naming}
        maxLength={40}
        onChange={(e) => setNaming(e.target.value)}
        onBlur={keep}
        onKeyDown={(event) => {
          if (event.key === "Enter") keep();
          if (event.key === "Escape") {
            event.preventDefault();
            setNaming(null);
          }
        }}
        className={`ui-field ${look} w-40 rounded-md`}
      />
    );
  }
  const item = "ui-option flex cursor-default items-center px-2 py-1.5 outline-none select-none";
  return (
    <Menu.Root>
      <Menu.Trigger
        title="Move it to another group"
        className={`ui-button ${look} data-popup-open:bg-hover`}
      >
        {topic.group_name || "No group"}
        <ChevronDown size={12} strokeWidth={1.75} aria-hidden="true" className="text-faint" />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="start">
          <Menu.Popup finalFocus={focusBackAfterKeys} className="ui-menu min-w-44 p-1 outline-none">
            {others.map((group) => (
              <Menu.Item key={group} className={item} onClick={() => move(group)}>
                {group}
              </Menu.Item>
            ))}
            {others.length > 0 && <Menu.Separator className="my-1 h-px bg-line" />}
            <Menu.Item className={item} onClick={() => setNaming("")}>
              New group…
            </Menu.Item>
            {topic.group_name && (
              <Menu.Item className={item} onClick={() => move("")}>
                No group
              </Menu.Item>
            )}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

/**
 * What the topic sticks to, whole, changed where it is: ⌘↩ or clicking
 * away keeps it, Esc drops it. Under it, when it was written and when a
 * talk last changed it.
 */
function Brief({ topic }: { topic: Topic }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const messages = useRecords("Message");
  const [draft, setDraft] = useState<string | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  // The field grows with what it holds, as the words it took the place of did.
  useLayoutEffect(() => {
    const el = field.current;
    if (!el || draft === null) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [draft]);
  const changed = messages
    .filter((m) =>
      m.changes.some(
        (c) => c.topic_id === topic.id && !c.undone && c.after.sticks_to !== undefined,
      ),
    )
    .reduce<string | undefined>(
      (last, m) => (last && last > m.created_at ? last : m.created_at),
      undefined,
    );
  const keep = () => {
    if (draft !== null && draft.trim() !== topic.sticks_to) {
      sync.update("Topic", topic.id, { sticks_to: draft.trim() });
    }
    setDraft(null);
  };
  const look = "-mx-2.5 rounded-md px-2.5 py-2 text-body max-w-measure";
  const lines = topic.sticks_to.split(/\n+/).filter((line) => line.trim());
  return (
    <section className="flex flex-col gap-2" aria-label="What it sticks to">
      <h2 className="text-xs text-faint">What it sticks to</h2>
      {draft !== null ? (
        <textarea
          ref={field}
          // biome-ignore lint/a11y/noAutofocus: it opens to change the brief
          autoFocus
          aria-label="What it sticks to"
          value={draft}
          maxLength={4000}
          onChange={(e) => setDraft(e.target.value)}
          onFocus={(e) => e.currentTarget.setSelectionRange(draft.length, draft.length)}
          onBlur={keep}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              event.currentTarget.blur();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              setDraft(null);
            }
          }}
          className={`${look} resize-none bg-surface outline-none ring-1 ring-accent ring-inset`}
        />
      ) : guest ? (
        <div className={`${look} flex flex-col gap-2`}>
          {lines.map((line, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a brief's lines don't move
            <p key={i}>{line}</p>
          ))}
        </div>
      ) : (
        <button
          type="button"
          title="Change what it sticks to"
          onClick={() => setDraft(topic.sticks_to)}
          className={`${look} flex flex-col gap-2 text-left hover:bg-hover`}
        >
          {lines.length ? (
            lines.map((line, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: a brief's lines don't move
              <span key={i}>{line}</span>
            ))
          ) : (
            <span className="text-faint">
              Nothing yet. Say what it's about, and how it should answer.
            </span>
          )}
        </button>
      )}
      <div className="flex max-w-measure items-center gap-2 text-xs text-faint">
        <span>
          Written {onDay(topic.created_at)}
          {changed && ` · A talk changed it, ${onDay(changed)}`}
        </span>
        {draft !== null && (
          <span className="ml-auto flex gap-2.5">
            <span>
              <span className="ui-kbd mr-1">⌘↩</span>Keep
            </span>
            <span>
              <span className="ui-kbd mr-1">esc</span>Drop
            </span>
          </span>
        )}
      </div>
    </section>
  );
}

/**
 * What the topic knows about you: a fact a line, with the talk it came
 * from, which opens beside the page, and × to take one out.
 */
function Facts({ topic, facts }: { topic: Topic; facts: Point[] }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const talks = useRecords("Talk");
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  return (
    <section className="flex flex-col gap-2" aria-label="What it knows about you">
      <h2 className="text-xs text-faint">What it knows about you</h2>
      {facts.length === 0 ? (
        <p className="max-w-measure text-muted">
          Nothing yet. What you tell {topic.name} about yourself in a talk is kept here, and its
          answers go by it.
        </p>
      ) : (
        <ul className={`flex flex-col ${LINES}`}>
          {facts.map((fact) => {
            const talk = talks.find((t) => t.id === fact.talk_id);
            return (
              <li
                key={fact.id}
                className="group/row flex min-h-8 items-center gap-4 rounded-md px-2.5 py-0.5 hover:bg-hover"
              >
                {!fact.seen && (
                  <span
                    role="img"
                    aria-label="New"
                    className="-mr-2 size-1.5 shrink-0 rounded-full bg-accent"
                  />
                )}
                <p className="min-w-0 flex-1">{inline(fact.text)}</p>
                <span className="shrink-0 text-xs text-faint">
                  {talk ? (
                    <Link to="." search={withTalk(talk.id)} replace className="ui-link">
                      {talk.title || "The talk"}
                    </Link>
                  ) : (
                    fact.source
                  )}
                  {(talk || fact.source) && ", "}
                  {onDay(fact.created_at)}
                </span>
                {!guest && (
                  <RowAction
                    label="Take it out…"
                    onClick={() =>
                      setConfirmation({
                        title: "Take this out?",
                        body: `${topic.name} stops going by it, for everyone in the workspace. This can't be undone.`,
                        action: "Take it out",
                        onConfirm: () => sync.delete("Point", fact.id),
                      })
                    }
                    className="-mr-1.5 opacity-0 group-hover/row:opacity-100 focus-visible:opacity-100"
                  >
                    <X size={13} strokeWidth={1.75} aria-hidden="true" />
                  </RowAction>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </section>
  );
}

/** A section's name, which opens it on a page of its own; `children` go before the arrow. */
function SectionName({ name, children }: { name: string; children?: ReactNode }) {
  return (
    <Link
      to="."
      search={inSection(name)}
      className="group/name inline-flex min-w-0 items-center gap-1 hover:text-fg"
    >
      {name}
      {children}
      <ChevronRight
        size={14}
        strokeWidth={1.75}
        aria-hidden="true"
        className="shrink-0 text-faint group-hover/name:text-muted"
      />
    </Link>
  );
}

/** "3 more": what a section keeps past what the page shows, which opens it. */
function More({ section, more }: { section: string; more: number }) {
  if (more <= 0) return null;
  return (
    <Link to="." search={inSection(section)} className="ui-disclosure self-start">
      <ChevronRight size={13} strokeWidth={1.75} aria-hidden="true" />
      {more} more
    </Link>
  );
}

/** The kinds of block that show a question beside what it's about. */
const ASKS_IN = ["cards", "text", "shelf"];

/**
 * Where each question waits: in the block whose points share its theme, or
 * else on a card or section of its own in the first block of cards or text.
 * The rest wait at the top of the page.
 */
function questionHomes({ blocks, points, questions }: PageParts): Map<string, string> {
  const asks = new Set(blocks.filter((b) => ASKS_IN.includes(b.kind)).map((b) => b.id));
  const fallback = blocks.find((b) => b.kind === "cards" || b.kind === "text");
  const homes = new Map<string, string>();
  for (const question of questions) {
    if (!question.theme) continue;
    const home =
      points.find((p) => p.theme === question.theme && asks.has(p.block_id))?.block_id ??
      fallback?.id;
    if (home) homes.set(question.id, home);
  }
  return homes;
}

/** The block a question's point goes in when it's about nothing on the page. */
function keeping(blocks: Block[]): Block | undefined {
  return (
    blocks.find((b) => b.kind === "cards" || b.kind === "text") ??
    blocks.find((b) => b.kind !== "week" && b.kind !== "today")
  );
}

/** A block's name, for its menu and its dialogs. */
function blockName(block: Block): string {
  return block.title || (block.kind === "week" ? "The week" : "This block");
}

/** What an empty block will show, in a line. */
const EMPTY: Record<string, string> = {
  today: "What comes round on the day, step by step, to tick off as you go.",
  week: "The days things come round, once a talk settles a reminder.",
  chips: "Short facts, a few words each, as the talks bring them up.",
  shelf: "A tile for each thing you use, with when and how you use it.",
  cards: "A card for each thing the topic is about, with what the talks find.",
  text: "Sections that explain, written up as the talks go.",
  list: "Things to follow or read, a line each, with where they're from.",
  timeline: "When each section happened, oldest first, a line each, with its events on its page.",
  you: "What you tell it about yourself, a line each, which its answers go by.",
};

/** One block, laid out by its kind. */
function BlockView({
  topic,
  block,
  page,
  questions,
  routines,
}: {
  topic: Topic;
  block: Block;
  page: PageParts;
  /** The questions that wait in it. */
  questions: Question[];
  routines: RoutineDay[];
}) {
  const points = page.points.filter((p) => p.block_id === block.id);
  const frame = { block, blocks: page.blocks };
  switch (block.kind) {
    case "today":
      return <Today {...frame} topic={topic} points={points} reminders={page.reminders} />;
    case "week":
      return <Week {...frame} reminders={page.reminders} />;
    case "chips":
      return (
        <BlockFrame {...frame} empty={points.length === 0}>
          <Chips points={points} />
        </BlockFrame>
      );
    case "shelf":
      return (
        <BlockFrame {...frame} empty={points.length === 0}>
          <Shelf block={block} points={points} questions={questions} routines={routines} />
        </BlockFrame>
      );
    case "list":
      return (
        <BlockFrame {...frame} empty={points.length === 0}>
          <List points={points} />
        </BlockFrame>
      );
    case "timeline":
      return (
        <BlockFrame {...frame} empty={points.length === 0}>
          <TimelineBySection points={points} />
        </BlockFrame>
      );
    default: {
      const themes = themesOf(points, questions, page.reminders);
      return (
        <BlockFrame {...frame} empty={themes.length === 0}>
          {block.kind === "text" ? (
            <Text block={block} themes={themes} />
          ) : (
            <Cards block={block} themes={themes} />
          )}
        </BlockFrame>
      );
    }
  }
}

/**
 * A block's themes, each with its reminders: those about something the
 * block already shows.
 */
function themesOf(points: Point[], questions: Question[], reminders: Reminder[]): Theme[] {
  const names = new Set([...points.map((p) => p.theme), ...questions.map((q) => q.theme)]);
  return byTheme({
    points,
    questions,
    reminders: reminders.filter((r) => r.theme && names.has(r.theme)),
  });
}

/**
 * A block: its title, with ⋯ beside it on hover, then what it holds, or a
 * line on what will be there while it's empty.
 */
function BlockFrame({
  block,
  blocks,
  heading,
  lead = false,
  tools,
  empty = false,
  children,
}: {
  block: Block;
  blocks: Block[];
  /** In place of its title: "Today, Elvital wash". */
  heading?: ReactNode;
  /** The page's first thing to look at: as loud as a card's name. */
  lead?: boolean;
  /** Beside the title, such as the week's arrows. */
  tools?: ReactNode;
  empty?: boolean;
  children?: ReactNode;
}) {
  const title = heading ?? block.title;
  return (
    <section
      id={`block-${block.id}`}
      className="group/block flex scroll-mt-6 flex-col gap-2"
      aria-label={typeof title === "string" ? title : blockName(block)}
    >
      <div className="flex min-h-6 items-center gap-2">
        {title && <h2 className={lead ? "font-semibold" : "text-xs text-faint"}>{title}</h2>}
        {tools}
        <BlockMenu block={block} blocks={blocks} />
      </div>
      {empty ? (
        <p className="rounded-lg px-4 py-3.5 text-faint ring-1 ring-line ring-inset">
          {EMPTY[block.kind]}
        </p>
      ) : (
        children
      )}
    </section>
  );
}

/**
 * ⋯ beside a block's title, or its line on About: move it, rename it, or
 * remove it and what it keeps. It shows while `className`'s hover holds.
 */
function BlockMenu({
  block,
  blocks,
  className = "ml-auto group-hover/block:opacity-100",
}: {
  block: Block;
  /** The page's blocks, in order, which it moves among. */
  blocks: Block[];
  className?: string;
}) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const points = useRecords("Point");
  const [renaming, setRenaming] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  if (guest) return null;
  const at = blocks.findIndex((b) => b.id === block.id);
  const move = (by: number) => {
    const order = blocks.filter((b) => b.id !== block.id);
    order.splice(at + by, 0, block);
    const moved = order.flatMap((b, n) =>
      b.position === n
        ? []
        : [{ action: "update" as const, model: "Block" as const, id: b.id, data: { position: n } }],
    );
    if (moved.length) sync.transact(moved);
  };
  const kept = points.filter((p) => p.block_id === block.id).length;
  const name = blockName(block);
  return (
    <span
      className={`opacity-0 focus-within:opacity-100 has-[[data-popup-open]]:opacity-100 ${className}`}
    >
      <MoreMenu
        label={`More for ${name}`}
        items={[
          ...(at > 0 ? [{ label: "Move up", onSelect: () => move(-1) }] : []),
          ...(at < blocks.length - 1 ? [{ label: "Move down", onSelect: () => move(1) }] : []),
          // The week is headed by its days.
          ...(block.kind === "week"
            ? []
            : [{ label: "Rename…", onSelect: () => setRenaming(true) }]),
          {
            label: "Remove…",
            danger: true,
            onSelect: () =>
              setConfirmation({
                title: `Remove ${name}?`,
                body: kept
                  ? `Its ${kept} ${kept === 1 ? "point goes" : "points go"} with it, for everyone in the workspace. This can't be undone.`
                  : "It keeps nothing yet. This can't be undone.",
                action: "Remove block",
                onConfirm: () => sync.delete("Block", block.id),
              }),
          },
        ]}
      />
      {renaming && <RenameBlock block={block} onClose={() => setRenaming(false)} />}
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </span>
  );
}

/** A block's title is the topic's own: "Advice" in one, "Terms" in another. */
function RenameBlock({ block, onClose }: { block: Block; onClose: () => void }) {
  const { sync } = useWorkspace();
  const [title, setTitle] = useState(block.title);
  const save = () => {
    if (!title.trim()) return;
    sync.update("Block", block.id, { title: title.trim() });
    onClose();
  };
  return (
    <CreateDialog open onClose={onClose} width="24rem">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
      >
        <div className="px-4 pt-4 text-xs text-muted">
          <Dialog.Title className="font-normal">Rename {blockName(block)}</Dialog.Title>
        </div>
        <div className="px-4 py-3">
          <input
            // biome-ignore lint/a11y/noAutofocus: the dialog opens to type the title
            autoFocus
            aria-label="Title"
            value={title}
            maxLength={60}
            onChange={(e) => setTitle(e.target.value)}
            className="ui-field h-9 w-full rounded-md px-3"
          />
        </div>
        <div className="flex items-center justify-end gap-2 px-4 pb-4">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!title.trim()}>
            Rename
          </Button>
        </div>
      </form>
    </CreateDialog>
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
          <TitleToAbout topic={topic} />
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
        <div className={`flex flex-col gap-0.5 ${LINES}`}>
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
    kept === 0 ? "nothing kept yet" : `${kept} ${kept === 1 ? "point" : "points"}`,
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
        {about(topic.sticks_to) && <p className="text-muted">{about(topic.sticks_to)}</p>}
      </div>
    </Link>
  );
}

/** The topic's first questions, to ask with a click. */
function Starters({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  if (topic.starters.length === 0) return null;
  return (
    <section className="flex flex-col gap-2" aria-label="Start with">
      <h2 className="text-xs text-faint">Start with</h2>
      <ul className="flex flex-col gap-0.5">
        {topic.starters.map((starter) => (
          <li key={starter}>
            <button
              type="button"
              onClick={() => onAsk(starter)}
              className="ui-row flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left hover:bg-hover"
            >
              <ChevronRight
                size={13}
                strokeWidth={1.75}
                aria-hidden="true"
                className="shrink-0 text-faint"
              />
              {starter}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The week of the topic's reminders, a day each, headed by its days. */
function Week({
  block,
  blocks,
  reminders,
}: {
  block: Block;
  blocks: Block[];
  reminders: Reminder[];
}) {
  const now = today();
  const [days, setDays] = useState(() => weekOf(now));
  const first = days[0] ?? now;
  if (reminders.length === 0) {
    return <BlockFrame block={block} blocks={blocks} heading={blockName(block)} empty />;
  }
  const tools = (
    <>
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
    </>
  );
  return (
    <BlockFrame
      block={block}
      blocks={blocks}
      heading={block.title || weekWords(days)}
      tools={tools}
    >
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
    </BlockFrame>
  );
}

/** The points a card shows on the page; the rest are in its section. */
const CARD_SHOWS = 4;

/**
 * The page runs the panel's width. Cards, steps and tiles take as many
 * columns as fit, each at least 18rem; lines, a list's or a timeline's,
 * stay short enough to read across, as writing keeps its measure.
 */
const FILL = "grid-cols-[repeat(auto-fill,minmax(min(18rem,100%),1fr))]";
const LINES = "max-w-[86ch]";

/** A card per theme: its first points, its reminders, what it waits on. */
function Cards({ block, themes }: { block: Block; themes: Theme[] }) {
  return (
    <div className={`grid gap-3 ${FILL}`}>
      {themes.map((theme) => (
        <article key={theme.name} className="flex flex-col gap-3 rounded-lg bg-surface p-4">
          <h2 className="font-semibold">
            <SectionName name={theme.name} />
          </h2>
          {theme.reminders.length > 0 && <Dates reminders={theme.reminders} />}
          {theme.points.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <Points points={theme.points.slice(0, CARD_SHOWS)} />
              <More section={theme.name} more={theme.points.length - CARD_SHOWS} />
            </div>
          )}
          {theme.questions.map((q) => (
            <Ask key={q.id} question={q} block={block} />
          ))}
          <KeptFrom points={theme.points} />
        </article>
      ))}
    </div>
  );
}

/** Writing that grows: a section per theme, its first passage, and the rest in the section. */
function Text({ block, themes }: { block: Block; themes: Theme[] }) {
  return (
    <article className="flex flex-col gap-6">
      {themes.map((theme) => (
        <section key={theme.name} className="flex flex-col gap-2">
          <h3 className="font-semibold">
            <SectionName name={theme.name} />
          </h3>
          {theme.reminders.length > 0 && <Dates reminders={theme.reminders} />}
          {theme.points.length > 0 && <Points points={theme.points.slice(0, 1)} passages />}
          <More section={theme.name} more={theme.points.length - 1} />
          {theme.questions.map((q) => (
            <Ask key={q.id} question={q} block={block} />
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
                        search={withTalk(talk.id)}
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

/** A question Almanac waits on, where it's about; its point goes in `block`. */
function Ask({ question, block }: { question: Question; block?: Block }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const add = () => {
    if (question.point && block) {
      sync.create("Point", {
        topic_id: question.topic_id,
        block_id: block.id,
        theme: block.kind === "chips" || block.kind === "list" ? "" : (question.theme ?? "Notes"),
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
  about,
  field,
  picking,
  onAsk,
}: {
  topic: Topic;
  /** The section open in the page's place, which the question is about. */
  about?: string;
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
      className="flex w-full flex-col gap-2 px-6 pb-4 sm:px-10"
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
            aria-label={about ? `Ask about ${about}` : `Ask ${topic.name}`}
            placeholder={
              isOwn(topic)
                ? `Tell ${topic.name} what you want to keep track of`
                : about
                  ? `Ask about ${about}`
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

/**
 * The day's steps, to tick off as you go: the routine due today, or the
 * next one this week. Without steps, what comes round today. Ticks stay on
 * this device, for that day.
 */
function Today({
  topic,
  block,
  blocks,
  points,
  reminders,
}: {
  topic: Topic;
  block: Block;
  blocks: Block[];
  points: Point[];
  reminders: Reminder[];
}) {
  const { sync } = useWorkspace();
  const now = today();
  const next = nextRoutine(routineDays(points, reminders));
  const day = next?.day ?? now;
  const key = `almanac.ticked.${topic.id}.${day}`;
  const [ticked, setTicked] = useState<string[]>([]);
  useEffect(() => {
    try {
      setTicked(JSON.parse(localStorage.getItem(key) ?? "[]"));
    } catch {
      setTicked([]);
    }
  }, [key]);
  const tick = (id: string) => {
    const now = ticked.includes(id) ? ticked.filter((t) => t !== id) : [...ticked, id];
    setTicked(now);
    try {
      localStorage.setItem(key, JSON.stringify(now));
    } catch {}
  };
  const frame = { block, blocks, lead: true };
  let heading: string = block.title;
  let tools: ReactNode = null;
  let steps: { id: string; what: string; how: string | null }[];
  if (next) {
    const { dated, routine } = next;
    const when =
      dated && day !== now ? dueWords(day).replace(/^./, (c) => c.toUpperCase()) : block.title;
    heading = when ? `${when}, ${routine.name}` : routine.name;
    const reminder = routine.reminders.find((r) => occurrences(r, day, day).length);
    if (reminder) tools = <ReminderMenu sync={sync} reminder={reminder} />;
    steps = routine.steps.map((step) => {
      const [what, how] = whatAndHow(step.text);
      return { id: step.id, what, how };
    });
  } else {
    steps = reminders
      .filter((r) => occurrences(r, now, now).length)
      .map((r) => {
        const time = timeWords(r.at);
        return { id: r.id, what: r.title, how: time ? `At ${time}` : null };
      });
  }
  if (steps.length === 0) {
    return points.length + reminders.length === 0 ? (
      <BlockFrame {...frame} empty />
    ) : (
      <BlockFrame {...frame}>
        <p className="text-muted">Nothing comes round today.</p>
      </BlockFrame>
    );
  }
  return (
    <BlockFrame {...frame} heading={heading} tools={tools}>
      <ol className={`grid gap-2.5 ${FILL}`}>
        {steps.map(({ id, what, how }, n) => {
          const done = ticked.includes(id);
          return (
            <li key={id}>
              <button
                type="button"
                aria-pressed={done}
                aria-label={`${what}: ${done ? "done" : "to do"}`}
                onClick={() => tick(id)}
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
    </BlockFrame>
  );
}

/** Facts in a few words, as chips: "Low porosity, by the float test". */
function Chips({ points }: { points: Point[] }) {
  return (
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
  );
}

/** A tile for each thing on the shelf, with the days it's used on. */
function Shelf({
  block,
  points,
  questions,
  routines,
}: {
  block: Block;
  points: Point[];
  questions: Question[];
  routines: RoutineDay[];
}) {
  const things = byTheme({ points, questions: [], reminders: [] });
  return (
    <div className={`grid gap-2.5 ${FILL}`}>
      {things.map((thing) => (
        <Thing
          key={thing.name}
          block={block}
          thing={thing}
          days={thingDays(thing.name, routines)}
          questions={questions.filter((q) => q.theme === thing.name)}
        />
      ))}
    </div>
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
  block,
  thing,
  days,
  questions,
}: {
  block: Block;
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
        <SectionName name={thing.name} />
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
        <Ask key={q.id} question={q} block={block} />
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

/** Things to follow or read, a line each with where it comes from: the newest five. */
function List({ points }: { points: Point[] }) {
  const [all, setAll] = useState(false);
  const shown = all ? points : points.slice(-5);
  return (
    <div className={`flex flex-col ${LINES}`}>
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
      {points.length > 5 && (
        <button
          type="button"
          className="ui-disclosure self-start px-2.5"
          aria-expanded={all}
          onClick={() => setAll(!all)}
        >
          <ChevronRight size={13} strokeWidth={1.75} aria-hidden="true" />
          {all ? "The newest five" : `${points.length - 5} more`}
        </button>
      )}
    </div>
  );
}

/**
 * The topic page's timeline: a line for each section, oldest first, when
 * its events begin and end and how many there are. Its name opens it, where
 * its events are. An event kept under no section is a line of its own.
 */
function TimelineBySection({ points }: { points: Point[] }) {
  return (
    <TimelineRail>
      {bySection(points).map((line) => {
        if (!line.section) return line.points.map((p) => <TimelineEvent key={p.id} point={p} />);
        const seen = line.points.every((p) => p.seen);
        const count = line.points.length;
        return (
          <TimelineLine key={line.section} year={line.year} span={line.span} seen={seen}>
            <p className="flex min-w-0 py-[5px]">
              {!seen && <span className="sr-only">New: </span>}
              <SectionName name={line.section}>
                <span className="ml-1.5 shrink-0 text-xs text-faint">
                  {count} {count === 1 ? "event" : "events"}
                </span>
              </SectionName>
            </p>
          </TimelineLine>
        );
      })}
    </TimelineRail>
  );
}

/** What happened when, oldest first, a line each: a section's own events. */
function Timeline({ points }: { points: Point[] }) {
  return (
    <TimelineRail>
      {inOrder(points).map(([point, at]) => (
        <TimelineEvent key={point.id} point={point} at={at} />
      ))}
    </TimelineRail>
  );
}

/** A timeline's lines, with the years right-aligned in a column of their own. */
function TimelineRail({ children }: { children: ReactNode }) {
  return (
    <div className="@container">
      <ol
        className={`grid ${LINES} grid-cols-[minmax(3.5rem,max-content)_0.875rem_1fr] gap-x-3 @lg:grid-cols-[minmax(5.5rem,max-content)_0.875rem_1fr]`}
      >
        {children}
      </ol>
    </div>
  );
}

/** A line on a timeline: the year, a mark on the rail (longer for a span), then what happened. */
function TimelineLine({
  year,
  span,
  seen,
  children,
}: {
  year: string;
  span: boolean;
  seen: boolean;
  children: ReactNode;
}) {
  return (
    <li className="group col-span-full grid grid-cols-subgrid">
      <span className="py-[5px] text-right whitespace-nowrap text-muted tabular-nums">{year}</span>
      <span
        aria-hidden="true"
        className={`relative before:absolute before:inset-y-0 before:left-1.5 before:w-[1.5px] before:bg-line group-first:before:top-3.5 group-last:before:bottom-auto group-last:before:h-3.5 group-only:before:hidden after:absolute after:left-[3px] after:w-2 after:rounded-full ${
          span ? "after:top-[9px] after:h-3.5" : "after:top-3 after:h-2"
        } ${seen ? "after:bg-muted" : "after:bg-accent"}`}
      />
      {children}
    </li>
  );
}

/** One event: a day's date before it, and ✕ on hover to take it off. */
function TimelineEvent({ point, at = when(point.happened) }: { point: Point; at?: When }) {
  return (
    <TimelineLine year={at.year} span={at.span} seen={point.seen}>
      <div className="flex min-w-0 items-start gap-2 py-[5px]">
        <p className="min-w-0 flex-1">
          {!point.seen && <span className="sr-only">New: </span>}
          {at.day && <span className="text-muted">{at.day}: </span>}
          {inline(point.text)}
        </p>
        <RemovePoint point={point} />
      </div>
    </TimelineLine>
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
