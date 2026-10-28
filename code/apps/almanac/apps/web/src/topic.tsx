// A topic's page: what it sticks to, then its blocks in order, each laying
// out what it keeps one way (the day's steps, the week, chips, a shelf,
// cards, text, a list or a timeline), the questions it waits on, and the
// talk beside it. A section opens on a page of its own, in the page's
// place. The workspace's own topic, Almanac, has the index of the others
// for its page.

import type { Block, ClaudeKeyInfo, Point, Question, Reminder, Topic } from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import type { SyncClient } from "@code-villain/sync-client";
import {
  type Confirmation,
  ConfirmDialog,
  CreateDialog,
  MoreMenu,
  overlayOpen,
  RowAction,
  useApi,
} from "@code-villain/web";
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
  useLayoutEffect,
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
import { about, inline } from "./prose";
import { NotifyBell, occurrences, ReminderMenu } from "./reminders";
import type { Schema } from "./sync";
import { TalkPanel, useTalks } from "./talk";
import { inOrder } from "./timeline";
import { TopicDialog } from "./topic-dialog";
import { Button, Empty } from "./ui";
import { useIsGuest, useRecords, useSyncStatus, useWorkspace } from "./workspace";

/**
 * `?talk=` opens the talk panel (`new` for a new talk); `?ask=` starts it
 * with a question; `?section=` opens a section, by its theme, in the page's
 * place.
 */
export interface TopicSearch {
  talk?: string;
  ask?: string;
  section?: string;
}

/** The search that opens `section` (none: the page), keeping the open talk. */
function inSection(section: string | undefined) {
  return (prev: TopicSearch): TopicSearch => ({
    ...(prev.talk ? { talk: prev.talk } : {}),
    ...(section ? { section } : {}),
  });
}

/** The search that opens a talk beside what's open. */
function withTalk(talk: string) {
  return (prev: TopicSearch): TopicSearch => ({
    ...(prev.section ? { section: prev.section } : {}),
    talk,
  });
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
          shown === undefined ? inSection(prev.section)({}) : withTalk(shown)(prev),
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
  // The page keeps where it was scrolled to while a section is open.
  const scroller = useRef<HTMLDivElement>(null);
  const scrolled = useRef(new Map<string, number>());
  useLayoutEffect(() => {
    if (scroller.current) scroller.current.scrollTop = scrolled.current.get(section ?? "") ?? 0;
  }, [section]);
  // Esc goes back from a section to the page, once the talk is closed.
  useEffect(() => {
    if (!section || panel) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || overlayOpen()) return;
      event.preventDefault();
      void navigate({ to: ".", search: inSection(undefined) });
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [section, panel, navigate]);

  return (
    <>
      <PageHeader
        title={section ?? topic.name}
        parent={
          section && (
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
          <div
            ref={scroller}
            onScroll={(e) => scrolled.current.set(section ?? "", e.currentTarget.scrollTop)}
            className="page-content min-h-0 flex-1 overflow-y-auto"
          >
            <div className="mx-auto flex max-w-4xl flex-col gap-8 px-6 py-6">
              {isOwn(topic) ? (
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

function TopicPageBody({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  const page = usePage(topic);
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
      <p className="text-title font-semibold">{topic.name}</p>
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
  const blocks = page.blocks.filter((b) => points.some((p) => p.block_id === b.id));
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
              <Timeline points={held} sections={false} />
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

/** A section's name, which opens it on a page of its own. */
function SectionName({ name }: { name: string }) {
  return (
    <Link
      to="."
      search={inSection(name)}
      className="group/name inline-flex items-center gap-1 hover:text-fg"
    >
      {name}
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
  timeline: "What happened when, oldest first, a line each, with the section it belongs to.",
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
          <Timeline points={points} />
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
      className="group/block flex flex-col gap-2"
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

/** ⋯ beside a block's title: move it, rename it, or remove it and what it keeps. */
function BlockMenu({ block, blocks }: { block: Block; blocks: Block[] }) {
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
    <span className="ml-auto opacity-0 group-hover/block:opacity-100 focus-within:opacity-100 has-[[data-popup-open]]:opacity-100">
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

/** A block's title is the topic's own: "You" in one, "Terms" in another. */
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

/** A card per theme: its first points, its reminders, what it waits on. */
function Cards({ block, themes }: { block: Block; themes: Theme[] }) {
  return (
    // Two columns only when the page has room, talk open or not.
    <div className="@container">
      <div className="grid gap-3 @xl:grid-cols-2">
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
      <div className="@container">
        <ol className="grid gap-2.5 @lg:grid-cols-2 @3xl:grid-cols-3">
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
      </div>
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
    <div className="@container">
      <div className="grid gap-2.5 @lg:grid-cols-2 @3xl:grid-cols-3">
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
    <div className="flex flex-col">
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
 * What happened when, oldest first, a line each: the year, a mark on the
 * rail (longer for a span), the event, and the section it belongs to,
 * which opens it. In a section, its own events, without the section.
 */
function Timeline({ points, sections = true }: { points: Point[]; sections?: boolean }) {
  // Where the page is narrow (a phone, or the talk open beside it), the
  // section goes under its event instead of in a column of its own.
  const section = (point: Point, where: string) =>
    sections &&
    point.theme && (
      <Link
        to="."
        search={inSection(point.theme)}
        className={`text-xs text-faint hover:text-muted ${where}`}
      >
        {point.theme}
      </Link>
    );
  return (
    <div className="@container">
      <ol
        className={`grid max-w-[86ch] grid-cols-[minmax(3.5rem,max-content)_0.875rem_1fr] gap-x-3 ${
          sections
            ? "@lg:grid-cols-[minmax(5.5rem,max-content)_0.875rem_1fr_auto]"
            : "@lg:grid-cols-[minmax(5.5rem,max-content)_0.875rem_1fr]"
        }`}
      >
        {inOrder(points).map(([point, at]) => (
          <li key={point.id} className="group col-span-full grid grid-cols-subgrid">
            <span className="py-[5px] text-right whitespace-nowrap text-muted tabular-nums">
              {at.year}
            </span>
            <span
              aria-hidden="true"
              className={`relative before:absolute before:inset-y-0 before:left-1.5 before:w-[1.5px] before:bg-line group-first:before:top-3.5 group-last:before:bottom-auto group-last:before:h-3.5 group-only:before:hidden after:absolute after:left-[3px] after:w-2 after:rounded-full ${
                at.span ? "after:top-[9px] after:h-3.5" : "after:top-3 after:h-2"
              } ${point.seen ? "after:bg-muted" : "after:bg-accent"}`}
            />
            <div className="flex min-w-0 items-start gap-2 py-[5px]">
              <div className="min-w-0 flex-1">
                <p>
                  {!point.seen && <span className="sr-only">New: </span>}
                  {at.day && <span className="text-muted">{at.day}: </span>}
                  {inline(point.text)}
                </p>
                {section(point, "block @lg:hidden")}
              </div>
              <RemovePoint point={point} />
            </div>
            {sections && (
              <span className="hidden max-w-40 truncate py-1.5 @lg:block">
                {section(point, "")}
              </span>
            )}
          </li>
        ))}
      </ol>
    </div>
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
