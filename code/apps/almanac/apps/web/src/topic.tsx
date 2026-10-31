// A topic's page: its blocks in order, on a grid of twelve columns, each one
// of the set (a heading, a paragraph, a callout or a collection), the
// questions it waits on, and the talk beside it. A section, and a
// collection's row, opens on a page of its own, in the page's place, and so
// does the topic's
// About: its name, what it sticks to, what it knows about you and its
// blocks. The workspace's own topic, Almanac, has the index of the others
// for its page.

import type { Block, ClaudeKeyInfo, Field, Point, Question, Reminder, Topic } from "@almanac/types";
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
import { ArrowLeft, ChevronDown, ChevronRight, MessagesSquare, X } from "lucide-react";
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
import { cellOf, cellWords, checkOf, lanesOf, numberOf } from "./collection";
import { ago, dateWords, dayKey, dueWords, rhythmWords, timeWords } from "./format";
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
import { NotifyBell, ReminderMenu } from "./reminders";
import { samples } from "./samples";
import {
  Callout,
  ChecklistCount,
  Collection,
  choiceDot,
  itemDone,
  LINES,
  named,
  Opens,
  type PageTo,
  Paragraphs,
  useItemReminders,
} from "./set";
import type { Schema } from "./sync";
import { TalkPanel, useTalks } from "./talk";
import { Button, Empty } from "./ui";
import { ownWidth, shownAs, VIEW_NAMES, viewOf, viewsFor, widthOf } from "./views";
import { LookOnly, useIsGuest, useRecords, useSyncStatus, useWorkspace } from "./workspace";

/**
 * `?talk=` opens the talk panel (`new` for a new talk); `?ask=` starts it
 * with a question; `?section=` opens a section, by its theme, in the page's
 * place, with `&row=` a collection's row, by its id, and `?about` the
 * topic's About.
 */
export interface TopicSearch {
  talk?: string;
  ask?: string;
  section?: string;
  row?: string;
  about?: true;
}

/** What's open in the page's place: a section or a row, About, or the page. */
function placeOf(search: TopicSearch): TopicSearch {
  return search.section
    ? { section: search.section, ...(search.row ? { row: search.row } : {}) }
    : search.about
      ? { about: true }
      : {};
}

/** The search that opens `section` (none: the page), keeping the open talk. */
function inSection(section: string | undefined, row?: string) {
  return (prev: TopicSearch): TopicSearch => ({
    ...(prev.talk ? { talk: prev.talk } : {}),
    ...(section ? { section } : {}),
    ...(section && row ? { row } : {}),
  });
}

/** Opens a section's or a row's own page, for the blocks of a topic's page. */
function OpenPage({
  section,
  row,
  className,
  label,
  children,
}: PageTo & { className?: string; label?: string; children: ReactNode }) {
  return (
    <Link
      to="."
      search={inSection(section, row)}
      aria-label={label}
      title={label}
      className={className}
      onClick={(e) => e.stopPropagation()}
    >
      {children}
    </Link>
  );
}

/** The search that opens the topic's About, keeping the open talk. */
function inAbout(prev: TopicSearch): TopicSearch {
  return { ...(prev.talk ? { talk: prev.talk } : {}), about: true };
}

/** The search that opens a talk beside what's open. */
function withTalk(talk: string) {
  return (prev: TopicSearch): TopicSearch => ({ ...placeOf(prev), talk });
}

export function TopicPage() {
  const { topic: id } = useParams({ strict: false }) as { topic: string };
  const search = useSearch({ strict: false }) as TopicSearch;
  const navigate = useNavigate();
  const { ready } = useSyncStatus();
  const topics = useRecords("Topic");
  const topic = topics.find((t) => t.id === id);
  const points = useRecords("Point");
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
  // Asked with a section or a row open, a question is about it, in the talk
  // too. A row's page is its name's section, under what it is now called.
  const row = isOwn(topic) ? undefined : points.find((p) => p.id === search.row);
  const section = isOwn(topic) ? undefined : (row?.text ?? search.section);
  return (
    <TopicView
      // A new topic starts with its own state: what it asked, what it saw.
      key={topic.id}
      topic={topic}
      section={section}
      row={row}
      about={!section && search.about === true}
      panel={shown !== undefined}
      talkId={talkId}
      draft={search.ask ?? ""}
      asking={asking}
      onAsk={(text, talk, photos) => {
        if (talk === null) open(null);
        void ask(text, talk, photos, section ? { section, row: row?.id } : undefined);
      }}
      onOpen={open}
    />
  );
}

function TopicView({
  topic,
  section,
  row,
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
  /** The collection's row whose page that is. */
  row: Point | undefined;
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
            className="page-content relative min-h-0 flex-1 overflow-y-auto"
          >
            <div className="flex flex-col gap-8 px-6 py-6 sm:px-10">
              {about ? (
                <AboutPageBody topic={topic} />
              ) : isOwn(topic) ? (
                <OwnPageBody topic={topic} onAsk={(text) => onAsk(text, null)} />
              ) : section ? (
                <Opens.Provider value={OpenPage}>
                  <SectionPageBody topic={topic} section={section} row={row} />
                </Opens.Provider>
              ) : (
                <Opens.Provider value={OpenPage}>
                  <TopicPageBody topic={topic} onAsk={(text) => onAsk(text, null)} />
                </Opens.Provider>
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

/**
 * The topic's blocks in page order, each with the view and width the app
 * draws it at (`shownAs`), its points, its open questions and its reminders
 * that are on.
 */
function usePage(topic: Topic): PageParts {
  const blocks = useRecords("Block");
  const points = useRecords("Point");
  const questions = useRecords("Question");
  const reminders = useRecords("Reminder");
  return useMemo<PageParts>(() => {
    const ofTopic = <T extends { topic_id: string }>(list: T[]) =>
      list.filter((r) => r.topic_id === topic.id);
    const kept = ofTopic(points).sort((a, b) => a.position - b.position);
    return {
      blocks: ofTopic(blocks)
        .sort((a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at))
        .map((block) => drawn(block, kept)),
      points: kept,
      questions: ofTopic(questions)
        .filter((q) => q.status === "open")
        .sort((a, b) => a.created_at.localeCompare(b.created_at)),
      reminders: ofTopic(reminders)
        .filter((r) => r.status === "on")
        .sort((a, b) => a.due_on.localeCompare(b.due_on)),
    };
  }, [blocks, points, questions, reminders, topic.id]);
}

/**
 * A block as the page draws it: a collection with its picture as its view,
 * and each at the width its view needs. One with nothing to show has no
 * kind, and stays off the page.
 */
function drawn(block: Block, points: Point[]): Block {
  const held = points.filter((p) => p.block_id === block.id);
  const view = shownAs(block, held);
  return { ...block, kind: view && block.kind, view, width: widthOf(view, block, held) };
}

/**
 * What a page shows: the blocks that keep something, but not what it knows
 * about you, which About shows. The topic's page has the blocks that sit on
 * no row; a row's own (`row`), those that name it.
 */
function onThePage(page: PageParts, row: string | null = null): PageParts {
  const shown = page.blocks.filter((b) => b.kind && b.kind !== "you" && (b.row_id ?? null) === row);
  const ids = new Set(shown.map((b) => b.id));
  return { ...page, blocks: shown, points: page.points.filter((p) => ids.has(p.block_id)) };
}

/** Blocks on the page's grid, each at its width. */
function Blocks({ page }: { page: PageParts }) {
  return (
    <div className="page-grid">
      {page.blocks.map((block) => (
        <div
          key={block.id}
          data-width={block.width}
          data-narrow={NARROW.has(block.width) || undefined}
        >
          <BlockView block={block} page={page} />
        </div>
      ))}
    </div>
  );
}

function TopicPageBody({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  const all = usePage(topic);
  const page = useMemo(() => onThePage(all), [all]);
  // An answer may go in a block the page leaves out, which keeps nothing yet.
  const home = keeping(page.blocks);

  return (
    <>
      <TitleToAbout topic={topic} />
      <NoKey />
      {page.questions.length > 0 && (
        <section className="flex flex-col gap-2" aria-label="Waiting on you">
          {page.questions.map((q) => (
            <Ask key={q.id} question={q} block={home} />
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
      {page.blocks.length > 0 && <Blocks page={page} />}
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
 * A collection's row at the top of its page: which collection it's in, and
 * its fields, a tile each, the empty ones too, so a talk can fill them. A
 * number has a bar against the collection's largest, a status its steps.
 */
function RowFields({ row }: { row: Point }) {
  const blocks = useRecords("Block");
  const points = useRecords("Point");
  const block = blocks.find((b) => b.id === row.block_id);
  if (!block) return null;
  const rows = points.filter((p) => p.block_id === block.id);
  return (
    <section className="-mt-6 flex flex-col gap-3" aria-label={`In ${block.title}`}>
      <p className="text-muted">in {block.title || "a collection"}</p>
      {block.fields.length > 0 && (
        <dl className="grid grid-cols-[repeat(auto-fill,minmax(min(10rem,100%),1fr))] gap-2.5">
          {block.fields.map((field) => (
            <div
              key={field.name}
              className="flex min-w-0 flex-col-reverse justify-end gap-1 rounded-lg bg-surface px-4 py-3"
            >
              <dt className="text-xs text-muted">{field.name}</dt>
              <dd className="flex min-w-0 flex-col gap-2">
                <FieldValue field={field} row={row} rows={rows} />
              </dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}

/** One field of a row, large: a number with its bar, a status with its steps, a choice its pill. */
function FieldValue({ field, row, rows }: { field: Field; row: Point; rows: Point[] }) {
  const cell = cellOf(row, field);
  if (cell === undefined) return <span className="text-faint">—</span>;
  if (field.kind === "check") return <span className="text-2xl">{cell === true ? "✓" : "–"}</span>;
  if (typeof cell === "string" && (field.kind === "status" || field.kind === "choice")) {
    const lanes = lanesOf(field, rows);
    const at = lanes.indexOf(cell);
    return (
      <>
        <span className="flex items-baseline gap-2">
          {field.kind === "choice" && (
            <span
              aria-hidden="true"
              className={`size-2.5 shrink-0 rounded-full ${choiceDot(field, cell)}`}
            />
          )}
          <span
            className={`break-words font-semibold leading-tight ${[...cell].length > 12 ? "text-lg" : "text-2xl"}`}
          >
            {cell}
          </span>
        </span>
        {field.kind === "status" && lanes.length > 1 && (
          <span
            role="img"
            aria-label={`Step ${at + 1} of ${lanes.length}`}
            className="flex h-1.5 gap-0.5"
          >
            {lanes.map((lane, i) => (
              <span
                key={lane}
                className={`flex-1 rounded-full ${i > at ? "bg-chip" : at === lanes.length - 1 ? "bg-ok" : "bg-info"}`}
              />
            ))}
          </span>
        )}
      </>
    );
  }
  const words = cellWords(field, cell);
  const most = Math.max(...rows.map((r) => Math.abs(numberOf(r, field) ?? 0)));
  return (
    <>
      <span
        className={`font-semibold tabular-nums leading-tight ${[...words].length > 12 ? "text-lg" : "text-2xl"} break-words`}
      >
        {words || "—"}
      </span>
      {typeof cell === "number" && rows.length > 1 && most > 0 && (
        <span aria-hidden="true" className="h-1.5 overflow-hidden rounded-full bg-chip">
          <span
            className="block h-full rounded-full bg-accent"
            style={{ width: `${(Math.abs(cell) / most) * 100}%` }}
          />
        </span>
      )}
    </>
  );
}

/**
 * A section on a page of its own: everything kept under its name, from every
 * block, as its block lays it out (the events, then the passages), with its
 * reminders, the questions about it, and the talks it came from. A row's
 * page has its fields first, then the blocks that sit on it.
 */
function SectionPageBody({
  topic,
  section,
  row,
}: {
  topic: Topic;
  section: string;
  row: Point | undefined;
}) {
  const page = usePage(topic);
  const talks = useRecords("Talk");
  const own = useMemo(() => onThePage(page, row?.id ?? null), [page, row?.id]);
  const points = page.points.filter((p) => p.theme === section);
  const questions = page.questions.filter((q) => q.theme === section);
  const reminders = page.reminders.filter((r) => r.theme === section);
  const blocks = page.blocks.filter(
    (b) => !own.blocks.includes(b) && points.some((p) => p.block_id === b.id),
  );
  // A question's point goes on the row's page, where its sentences are.
  const home = keeping(own.blocks) ?? keeping(page.blocks.filter((b) => !b.row_id));
  // Passages name their talk; the talks of the rest are listed under them.
  const passages = new Set(blocks.filter((b) => b.kind === "paragraph").map((b) => b.id));
  const named = new Set(points.filter((p) => passages.has(p.block_id)).map((p) => p.talk_id));
  const from = talks
    .filter((t) => !named.has(t.id) && [...points, ...own.points].some((p) => p.talk_id === t.id))
    .sort((a, b) => b.last_at.localeCompare(a.last_at));

  if (!row && points.length + questions.length + reminders.length === 0) {
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
      {row && <RowFields row={row} />}
      {own.blocks.length > 0 && <Blocks page={own} />}
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
            {block.kind === "collection" ? (
              <Collection block={block} rows={held} />
            ) : (
              <Points points={held} passages={passages.has(block.id)} />
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
  const blocks = page.blocks.filter((b) => b.kind !== "you" && !b.row_id);
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
  heading: ["Heading", "", ""],
  paragraph: ["Paragraph", "point", "points"],
  callout: ["Callout", "point", "points"],
  collection: ["Collection", "row", "rows"],
};

/** What a block holds, for its line on About: "Collection · 3 rows", "Heading". */
function holds(block: Block, page: PageParts): string {
  const points = page.points.filter((p) => p.block_id === block.id);
  const [kind, one, many] = HOLDS[block.kind] ?? ["", "point", "points"];
  if (block.kind === "heading") return kind;
  const count = points.length;
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

/** The block a question's point goes in: the first paragraph of `blocks`. */
function keeping(blocks: Block[]): Block | undefined {
  return blocks.find((b) => b.kind === "paragraph");
}

/** The widths that read at a glance, and so hold out narrower. */
const NARROW = new Set(["1/4", "1/3"]);

/** A block's name, for its menu and its dialogs. */
function blockName(block: Block): string {
  return block.title || "This block";
}

/**
 * Settings → Blocks: every kind of block a page can have, with sample
 * points, drawn as a page draws it. Only to look at: it changes nothing.
 */
export function BlockGallery() {
  const all = useMemo(() => samples(), []);
  return (
    <LookOnly>
      {all.map((sample) => {
        const page: PageParts = {
          blocks: [sample.block],
          points: sample.points,
          questions: [],
          reminders: [],
        };
        const block = drawn(sample.block, sample.points);
        const width = block.width;
        return (
          <section key={sample.block.id} aria-label={sample.name} className="flex flex-col gap-3">
            <div className="flex flex-col gap-0.5">
              <h2 className="font-semibold">{sample.name}</h2>
              <p className="max-w-measure text-muted">{sample.says}</p>
            </div>
            {sample.block.kind === "you" ? (
              <Facts topic={sample.topic} facts={sample.points} />
            ) : (
              // At its view's width, as a page lays it out.
              <div className="page-grid" data-alone>
                <div data-width={width} data-narrow={NARROW.has(width) || undefined}>
                  <BlockView block={block} page={{ ...page, blocks: [block] }} />
                </div>
              </div>
            )}
          </section>
        );
      })}
    </LookOnly>
  );
}

/** One block, laid out by its kind. */
function BlockView({ block, page }: { block: Block; page: PageParts }) {
  const points = page.points.filter((p) => p.block_id === block.id);
  const frame = { block, blocks: page.blocks };
  switch (block.kind) {
    case "heading":
      return <BlockFrame {...frame} lead />;
    case "paragraph":
      return (
        <BlockFrame {...frame}>
          <Paragraphs points={points} />
        </BlockFrame>
      );
    case "callout":
      return (
        <BlockFrame {...frame}>
          <Callout points={points} />
        </BlockFrame>
      );
    case "collection":
      return (
        <BlockFrame
          {...frame}
          aside={
            block.view === "checklist" || block.view === "steps" ? (
              <ChecklistCount block={block} rows={points} />
            ) : undefined
          }
        >
          <Collection block={block} rows={points} />
        </BlockFrame>
      );
    default:
      return null;
  }
}

/**
 * A block: its title, with ⋯ beside it on hover, then what it holds, or a
 * line on what will be there while it's empty.
 */
function BlockFrame({
  block,
  blocks,
  lead = false,
  aside,
  children,
}: {
  block: Block;
  blocks: Block[];
  /** A heading's title: as loud as a section's. */
  lead?: boolean;
  /** At the end of the title's line, such as what's done of a checklist. */
  aside?: ReactNode;
  children?: ReactNode;
}) {
  const title = block.title;
  return (
    <section
      id={`block-${block.id}`}
      className="group/block flex scroll-mt-6 flex-col gap-2"
      aria-label={title || blockName(block)}
    >
      <div className="relative flex min-h-6 items-center gap-2">
        {title && (
          <h2 className={`min-w-0 truncate ${lead ? "font-semibold" : "text-xs text-faint"}`}>
            {title}
          </h2>
        )}
        {aside && <span className="ml-auto shrink-0">{aside}</span>}
        {/* Over the line on hover, so a half block's title keeps its room. */}
        <span className="absolute inset-y-0 right-0 flex items-center gap-3 bg-bg pl-3 opacity-0 group-hover/block:opacity-100 focus-within:opacity-100 has-[[aria-expanded=true]]:opacity-100">
          <ViewAs block={block} />
          <BlockMenu block={block} blocks={blocks} className="group-hover/block:opacity-100" />
        </span>
      </div>
      {children}
    </section>
  );
}

/**
 * View as, beside a block's title on hover: the views that can draw what
 * it keeps, the one it's drawn with pressed. A person's pick stays until
 * they pick another, for everyone in the workspace, and a talk never
 * changes it; the app's own pick leaves the view to the app again.
 */
function ViewAs({ block }: { block: Block }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const blocks = useRecords("Block");
  const points = useRecords("Point");
  // The block as kept, not as drawn: its kind is the talk's.
  const kept = blocks.find((b) => b.id === block.id);
  if (guest || !kept) return null;
  const held = points.filter((p) => p.block_id === block.id);
  const views = viewsFor(kept, held);
  if (views.length === 0) return null;
  const pick = viewOf(kept, held);
  return (
    <span className="flex items-center gap-2 text-xs text-faint opacity-0 group-hover/block:opacity-100 focus-within:opacity-100">
      <span aria-hidden="true">View as</span>
      <fieldset aria-label={`View ${blockName(block)} as`} className="ui-segmented -my-0.5">
        {views.map((view) => (
          <button
            key={view}
            type="button"
            className="ui-segment"
            aria-pressed={view === block.kind}
            onClick={() => sync.update("Block", block.id, { view: view === pick ? "" : view })}
          >
            {VIEW_NAMES[view]}
          </button>
        ))}
      </fieldset>
    </span>
  );
}

/**
 * ⋯ beside a block's title, or its line on About: move it, make it half
 * the page or the whole, rename it, or remove it and what it keeps. It
 * shows while `className`'s hover holds.
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
  const kept = useRecords("Block").find((b) => b.id === block.id);
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
  const held = points.filter((p) => p.block_id === block.id);
  const name = blockName(block);
  // The other width; the app's own leaves the width to the app again.
  const drawn = kept && widthOf(shownAs(kept, held), kept, held);
  const other = drawn === "whole" ? "1/2" : "whole";
  const resize =
    kept && kept.kind !== "heading"
      ? [
          {
            label: other === "whole" ? "Full width" : "Half width",
            onSelect: () =>
              sync.update("Block", block.id, {
                width: other === ownWidth(shownAs(kept, held), kept, held) ? "" : other,
              }),
          },
        ]
      : [];
  return (
    <span
      className={`opacity-0 focus-within:opacity-100 has-[[data-popup-open]]:opacity-100 ${className}`}
    >
      <MoreMenu
        label={`More for ${name}`}
        items={[
          ...(at > 0 ? [{ label: "Move up", onSelect: () => move(-1) }] : []),
          ...(at < blocks.length - 1 ? [{ label: "Move down", onSelect: () => move(1) }] : []),
          ...resize,
          { label: "Rename…", onSelect: () => setRenaming(true) },
          {
            label: "Remove…",
            danger: true,
            onSelect: () =>
              setConfirmation({
                title: `Remove ${name}?`,
                body: held.length
                  ? `Its ${held.length} ${held.length === 1 ? "point goes" : "points go"} with it, for everyone in the workspace. This can't be undone.`
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
 * The workspace's own topic: every other topic by group, a tile each, with
 * how much it keeps, how far its checklists are and what's new. Almanac
 * keeps it; nobody writes it.
 */
function OwnPageBody({ topic, onAsk }: { topic: Topic; onAsk: (text: string) => void }) {
  const { workspace } = useWorkspace();
  const topics = useTopics();
  const blocks = useRecords("Block");
  const points = useRecords("Point");
  const talks = useRecords("Talk");
  const rings = useItemReminders();
  const Icon = topicIcon(topic.icon);
  const others = useMemo(() => topics.filter((t) => !isOwn(t)), [topics]);
  const groups = useMemo(() => {
    const byName = new Map<string, Topic[]>();
    for (const other of others) {
      const list = byName.get(other.group_name) ?? [];
      list.push(other);
      byName.set(other.group_name, list);
    }
    return [...byName.entries()].sort(([a], [b]) => (a && b ? a.localeCompare(b) : a ? 1 : -1));
  }, [others]);
  const tiles = useMemo(() => {
    const ids = new Set(others.map((t) => t.id));
    const byTopic = new Map<string, TileFacts>();
    for (const id of ids) byTopic.set(id, { kept: 0, done: 0, todo: 0, last: null });
    for (const point of points) {
      const facts = byTopic.get(point.topic_id);
      if (facts) facts.kept += 1;
    }
    for (const block of blocks) {
      const facts = byTopic.get(block.topic_id);
      const check = facts && block.kind === "collection" ? checkOf(block) : undefined;
      if (!facts || !check) continue;
      for (const row of points) {
        if (row.block_id !== block.id) continue;
        facts.todo += 1;
        if (itemDone(row, check, rings.get(row.id))) facts.done += 1;
      }
    }
    for (const talk of talks) {
      const facts = byTopic.get(talk.topic_id);
      if (facts && (!facts.last || talk.last_at > facts.last)) facts.last = talk.last_at;
    }
    return byTopic;
  }, [others, blocks, points, talks, rings]);
  const all = [...tiles.values()];
  const kept = all.reduce((n, t) => n + t.kept, 0);
  const done = all.reduce((n, t) => n + t.done, 0);
  const todo = all.reduce((n, t) => n + t.todo, 0);

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
        <div className="flex flex-col gap-6">
          <dl className="grid grid-cols-[repeat(auto-fit,minmax(min(9rem,100%),1fr))] gap-2.5">
            <Overall label="Topics" value={String(others.length)} />
            <Overall label="Points" value={String(kept)} />
            <Overall
              label="Talks"
              value={String(talks.filter((t) => tiles.has(t.topic_id)).length)}
            />
            {todo > 0 && <Overall label="Ticked off" value={`${done}/${todo}`} />}
          </dl>
          <ul
            aria-label="Topics"
            className="grid grid-cols-[repeat(auto-fill,minmax(min(14rem,100%),1fr))] gap-2.5"
          >
            {groups.flatMap(([, list]) =>
              list.map((other) => (
                <li key={other.id} className="flex">
                  <TopicTile
                    topic={other}
                    facts={tiles.get(other.id) ?? { kept: 0, done: 0, todo: 0, last: null }}
                  />
                </li>
              )),
            )}
          </ul>
        </div>
      )}
    </>
  );
}

/** What a topic's tile shows: its points, its checklists' rows ticked and to tick, its last talk. */
type TileFacts = { kept: number; done: number; todo: number; last: string | null };

/** One figure over every topic, large. */
function Overall({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col-reverse gap-0.5 rounded-lg bg-surface px-4 py-3">
      <dt className="text-xs text-muted">{label}</dt>
      <dd className="font-semibold text-2xl tabular-nums">{value}</dd>
    </div>
  );
}

/** One topic on the index: its points large, its checklists as a bar, what's new. */
function TopicTile({ topic, facts }: { topic: Topic; facts: TileFacts }) {
  const { workspace } = useWorkspace();
  const unread = useUnread();
  const Icon = topicIcon(topic.icon);
  return (
    <Link
      to="/$workspace/topics/$topic"
      params={{ workspace: workspace.url_key, topic: topic.id }}
      title={about(topic.sticks_to) || undefined}
      className="flex min-w-0 flex-1 flex-col gap-3 rounded-lg bg-surface px-4 py-3 hover:bg-hover"
    >
      <span className="flex items-center gap-2">
        <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0 text-muted" />
        <span className="min-w-0 truncate font-semibold">{topic.name}</span>
        {unread.has(topic.id) && (
          <span
            role="img"
            aria-label="New points"
            className="size-1.5 shrink-0 rounded-full bg-accent"
          />
        )}
        {topic.group_name && (
          <span className="ml-auto shrink-0 text-xs text-faint">{topic.group_name}</span>
        )}
      </span>
      <span className="flex items-baseline gap-1.5">
        <span className="font-semibold text-2xl tabular-nums">{facts.kept}</span>
        <span className="text-xs text-muted">{facts.kept === 1 ? "point" : "points"}</span>
        {facts.last && <span className="ml-auto text-xs text-faint">{ago(facts.last)}</span>}
      </span>
      {facts.todo > 0 && (
        <span className="flex items-center gap-2">
          <span aria-hidden="true" className="h-1.5 flex-1 overflow-hidden rounded-full bg-chip">
            <span
              className={`block h-full rounded-full ${facts.done === facts.todo ? "bg-ok" : "bg-info"}`}
              style={{ width: `${(facts.done / facts.todo) * 100}%` }}
            />
          </span>
          <span className="text-xs text-muted tabular-nums">
            {facts.done}/{facts.todo}
          </span>
        </span>
      )}
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

/**
 * A theme's points: on a card, short notes with their source after them; in
 * a document, passages to read, with their source and their talk under them.
 * On the topic's page a note shows only a named source.
 */
function Points({
  points,
  passages = false,
  glance = false,
}: {
  points: Point[];
  passages?: boolean;
  /** On the topic's page, not a section's own. */
  glance?: boolean;
}) {
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
          (!glance || named(point)) &&
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
          <li
            key={point.id}
            title={source ? undefined : point.source || undefined}
            className="group flex items-start gap-2"
          >
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
  const guest = useIsGuest();
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
            {!guest && <ReminderMenu sync={sync} reminder={reminder} />}
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
