// A talk in the panel beside a topic's page. The answer shows as it's
// written (`asking.tsx` reads it); the talk, the answer and what the page
// kept from it then arrive through sync. In
// the workspace's own topic, an answer may propose a topic to start, or
// say what it changed in one, with Undo. A question may come with photos,
// pasted, dropped on the talk or picked.

import type {
  Message,
  PageLayout,
  Photo,
  Proposal,
  Reminder,
  StartTopicRequest,
  Talk,
  Topic,
} from "@almanac/types";
import { Dialog } from "@base-ui/react/dialog";
import {
  type Confirmation,
  ConfirmDialog,
  CreateDialog,
  MoreMenu,
  messageOf,
  overlayOpen,
} from "@code-villain/web";
import { Link, useNavigate } from "@tanstack/react-router";
import { ChevronRight, Plus, X } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import type { Asking } from "./asking";
import { cellWords } from "./collection";
import { ago, dueWords, rhythmWords, timeWords } from "./format";
import { topicIcon } from "./icons";
import { isOwn, useNewTopic, useTopics } from "./layout";
import { type Picked, usePhoto } from "./photos";
import { AddPhoto, DropHint, PickedPhotos, PickingProblem, useDrop, usePicking } from "./picking";
import { about, Prose } from "./prose";
import { TopicForm } from "./topic-dialog";
import { Button, ErrorMessage } from "./ui";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";

/** The talks of a topic, the latest first. */
export function useTalks(topicId: string): Talk[] {
  const talks = useRecords("Talk");
  return useMemo(
    () =>
      talks
        .filter((t) => t.topic_id === topicId)
        .sort((a, b) => b.last_at.localeCompare(a.last_at)),
    [talks, topicId],
  );
}

export function TalkPanel({
  topic,
  talkId,
  asking,
  draft,
  onAsk,
  onOpen,
  onClose,
}: {
  topic: Topic;
  /** The talk shown; none for a new one. */
  talkId: string | null;
  asking: Asking | null;
  /** A question to start with, such as one moved from another topic. */
  draft: string;
  onAsk: (text: string, talkId: string | null, photos?: Picked[]) => void;
  onOpen: (talkId: string | null) => void;
  onClose: () => void;
}) {
  const { sync } = useWorkspace();
  const talks = useTalks(topic.id);
  const talk = talks.find((t) => t.id === talkId);
  const all = useRecords("Message");
  const reminders = useRecords("Reminder");
  const messages = useMemo(
    () =>
      talkId
        ? all
            .filter((m) => m.talk_id === talkId)
            .sort((a, b) => a.created_at.localeCompare(b.created_at))
        : [],
    [all, talkId],
  );
  const [text, setText] = useState(draft);
  const [viewing, setViewing] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const end = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const picking = usePicking(() => field.current?.focus());
  const drop = useDrop(picking);

  // What's on its way shows until the talk's records have it.
  const mine = asking && (asking.talkId === talkId || (!talkId && !asking.talkId)) ? asking : null;
  const asked = messages.filter((m) => m.role === "you").length;
  const showQuestion = mine && asked <= mine.asked;
  const answered = mine?.answerId ? messages.some((m) => m.id === mine.answerId) : false;
  const lastProblem = messages.at(-1)?.problem;
  const showAnswer = mine && !answered && !(mine.problem && lastProblem);
  const own = isOwn(topic);
  // ⌘↩ starts the topic the talk last proposed, until it's started.
  const lastAnswer = messages.findLast((m) => m.role === "almanac");
  const toStart = lastAnswer?.proposal && !lastAnswer.proposal.topic_id ? lastAnswer.id : null;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !overlayOpen()) {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll as the talk grows
  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [messages.length, mine?.text, showQuestion]);

  const send = () => {
    if (!text.trim() && picking.photos.length === 0) return;
    onAsk(text, talkId, picking.photos);
    setText("");
    picking.clear();
  };

  return (
    <aside
      aria-label="Talk"
      className="relative flex w-[min(26rem,45%)] shrink-0 flex-col border-l border-line bg-panel"
      {...drop.handlers}
    >
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
      <PhotoView url={viewing} onClose={() => setViewing(null)} />
      <div className="flex min-h-12 items-center gap-2 px-4">
        <div className="min-w-0 flex-1">
          <h2 className="truncate font-medium">{talk?.title || "New talk"}</h2>
          {talk && <p className="text-xs text-faint">Talk {ago(talk.last_at)}</p>}
        </div>
        <button
          type="button"
          onClick={() => onOpen(null)}
          aria-label="New talk"
          title="New talk"
          className="ui-button ui-icon-button"
        >
          <Plus size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
        <MoreMenu
          label="Talks"
          items={[
            ...talks
              .filter((t) => t.id !== talkId)
              .slice(0, 12)
              .map((t) => ({
                label: `${t.title || "Talk"}, ${ago(t.last_at)}`,
                onSelect: () => onOpen(t.id),
              })),
            ...(talk
              ? [
                  {
                    label: "Delete this talk…",
                    danger: true,
                    onSelect: () =>
                      setConfirmation({
                        title: `Delete ${talk.title ? `"${talk.title}"` : "this talk"}?`,
                        body: "Its messages go for everyone in the workspace; what it kept stays on the page. This can't be undone.",
                        action: "Delete talk",
                        onConfirm: () => {
                          sync.delete("Talk", talk.id);
                          onOpen(null);
                        },
                      }),
                  },
                ]
              : []),
          ]}
        />
        <button
          type="button"
          onClick={onClose}
          aria-label="Close the talk"
          title="Close (esc)"
          className="ui-button ui-icon-button"
        >
          <X size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        <div className="flex flex-col gap-4">
          {messages.length === 0 && !mine && (
            <p className="text-muted">
              {own
                ? `Tell ${topic.name} what you want to keep track of. It proposes a topic for it, and you start it.`
                : `Ask ${topic.name} something, or tell it what you know. What's worth keeping goes on the page.`}
            </p>
          )}
          {messages.map((message, index) => (
            <Said
              key={message.id}
              message={message}
              topic={topic}
              question={before(messages, index)}
              reminders={reminders.filter((r) => r.message_id === message.id)}
              toStart={message.id === toStart}
              onView={setViewing}
            />
          ))}
          {showQuestion && (
            <Question
              text={mine.question}
              photos={mine.photos.map((p) => (
                <PhotoThumb key={p.url} url={p.url} photo={p} onView={setViewing} />
              ))}
            />
          )}
          {showAnswer && (
            <div className="flex flex-col gap-2">
              {mine.text ? (
                <Prose text={mine.text} />
              ) : (
                !mine.problem && <p className="text-faint">Almanac is thinking…</p>
              )}
              {mine.problem && <Problem text={mine.problem} />}
            </div>
          )}
          <div ref={end} />
        </div>
      </div>
      <form
        className="flex flex-col gap-2 p-3"
        onSubmit={(event) => {
          event.preventDefault();
          send();
        }}
      >
        <PickingProblem picking={picking} />
        <div className="ui-field flex flex-col rounded-md">
          <PickedPhotos picking={picking} />
          <textarea
            ref={field}
            // biome-ignore lint/a11y/noAutofocus: the panel opens to ask
            autoFocus
            value={text}
            onChange={(e) => setText(e.target.value)}
            onPaste={picking.onPaste}
            onKeyDown={(event) => {
              // ⌘↩ is for starting a proposed topic, not for sending.
              const plain = !event.shiftKey && !event.metaKey && !event.ctrlKey;
              if (event.key === "Enter" && plain && !event.nativeEvent.isComposing) {
                event.preventDefault();
                send();
              }
            }}
            rows={2}
            aria-label={talkId ? "Ask more" : `Ask ${topic.name}`}
            placeholder={
              talkId
                ? "Ask more"
                : own
                  ? `Tell ${topic.name} what you want to keep track of`
                  : `Ask ${topic.name}, or tell it something`
            }
            className="resize-none bg-transparent px-3 py-2 outline-none placeholder:text-faint"
          />
          <div className="flex px-1 pb-1">
            <AddPhoto picking={picking} />
          </div>
        </div>
      </form>
      {drop.dropping && <DropHint />}
    </aside>
  );
}

/** The question an answer answered: what was asked just before it. */
function before(messages: Message[], index: number): string {
  const asked = messages[index - 1];
  return asked?.role === "you" ? asked.text : "";
}

/** What you asked, with the photos it came with above it. */
function Question({ text, photos }: { text: string; photos?: ReactNode[] }) {
  return (
    <div className="ml-8 flex flex-col items-end gap-1.5 self-end">
      {photos && photos.length > 0 && (
        <div className="flex flex-wrap justify-end gap-1.5">{photos}</div>
      )}
      {text && <p className="whitespace-pre-wrap rounded-lg bg-chip px-3 py-2">{text}</p>}
    </div>
  );
}

/** A photo in the talk, its room kept until it arrives; a click shows it large. */
function PhotoThumb({
  url,
  photo,
  onView,
}: {
  url: string | null;
  photo: { width: number; height: number };
  onView: (url: string) => void;
}) {
  const room = { aspectRatio: `${photo.width} / ${photo.height}` };
  if (!url) return <div className="h-28 max-w-full rounded-md bg-chip" style={room} />;
  return (
    <button
      type="button"
      onClick={() => onView(url)}
      aria-label="Show the photo"
      className="block max-w-full rounded-md"
    >
      <img
        src={url}
        alt=""
        className="h-28 w-auto max-w-full rounded-md object-cover"
        style={room}
      />
    </button>
  );
}

function SentPhoto({ photo, onView }: { photo: Photo; onView: (url: string) => void }) {
  const { place, workspace } = useWorkspace();
  const url = usePhoto(place, workspace.id, photo.id);
  return <PhotoThumb url={url} photo={photo} onView={onView} />;
}

/** A photo large, over the page. */
function PhotoView({ url, onClose }: { url: string | null; onClose: () => void }) {
  return (
    <CreateDialog open={url !== null} onClose={onClose} width="56rem">
      <Dialog.Title className="sr-only">Photo</Dialog.Title>
      {url && <img src={url} alt="" className="max-h-[76vh] w-full rounded-lg object-contain" />}
    </CreateDialog>
  );
}

/** One thing said in a talk, and for an answer, what came of it. */
function Said({
  message,
  topic,
  question,
  reminders,
  toStart,
  onView,
}: {
  message: Message;
  topic: Topic;
  question: string;
  reminders: Reminder[];
  /** The proposal ⌘↩ starts. */
  toStart: boolean;
  onView: (url: string) => void;
}) {
  if (message.role === "you") {
    return (
      <Question
        text={message.text}
        photos={message.photos.map((photo) => (
          <SentPhoto key={photo.id} photo={photo} onView={onView} />
        ))}
      />
    );
  }
  return (
    <div className="flex flex-col gap-2">
      {message.text && <Prose text={message.text} />}
      {message.problem && <Problem text={message.problem} />}
      {message.kept && <p className="text-xs text-muted">{message.kept}</p>}
      {message.proposal && (
        <Proposed message={message} proposal={message.proposal} toStart={toStart} />
      )}
      {message.changes.length > 0 && <Changes message={message} />}
      {reminders
        .filter((r) => r.status === "offered")
        .map((reminder) => (
          <Offer key={reminder.id} reminder={reminder} />
        ))}
      {message.elsewhere && (
        <Elsewhere name={message.elsewhere} topic={topic} question={question} />
      )}
    </div>
  );
}

/**
 * A topic the workspace's own topic would start, as a card: Start it as it
 * is, or change it first. Once started, a line that opens it.
 */
function Proposed({
  message,
  proposal,
  toStart,
}: {
  message: Message;
  proposal: Proposal;
  toStart: boolean;
}) {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const topics = useTopics();
  const points = useRecords("Point");
  const [changing, setChanging] = useState(false);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState("");

  const start = useCallback(
    async (fields: Omit<StartTopicRequest, "from_message">) => {
      await api("POST", `/api/w/${workspace.id}/topics`, { ...fields, from_message: message.id });
    },
    [workspace.id, message.id],
  );
  const startAsIs = useCallback(async () => {
    if (starting) return;
    setStarting(true);
    setProblem("");
    try {
      await start({ name: proposal.name });
      // Starting stays on until the proposal's topic arrives through sync.
    } catch (e) {
      setProblem(messageOf(e));
      setStarting(false);
    }
  }, [start, starting, proposal.name]);

  useEffect(() => {
    if (!toStart || guest) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && !overlayOpen()) {
        event.preventDefault();
        void startAsIs();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [toStart, guest, startAsIs]);

  if (proposal.topic_id) {
    const started = topics.find((t) => t.id === proposal.topic_id);
    if (!started) return <p className="text-muted">Started {proposal.name}, deleted since.</p>;
    const Icon = topicIcon(started.icon);
    const kept = points.filter((p) => p.topic_id === started.id).length;
    const what = kept ? `, with ${kept} ${kept === 1 ? "point" : "points"}` : "";
    return (
      <div className="flex items-center gap-2 text-muted">
        <Icon size={14} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
        <p className="min-w-0 flex-1">
          Started {started.name}
          {what}.
        </p>
        <Link
          to="/$workspace/topics/$topic"
          params={{ workspace: workspace.url_key, topic: started.id }}
          className="ui-link"
        >
          Open
        </Link>
      </div>
    );
  }

  const Icon = topicIcon(proposal.icon);
  const page =
    proposal.template === "routine"
      ? "A routine"
      : proposal.template === "study"
        ? "A study"
        : "A collection";
  return (
    <div className="flex flex-col gap-2.5 rounded-lg bg-surface px-4 py-3.5">
      <div className="flex items-center gap-2">
        <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
        <p className="min-w-0 truncate font-semibold">{proposal.name}</p>
        <p className="ml-auto shrink-0 text-xs text-faint">
          {[proposal.group_name, page].filter(Boolean).join(" · ")}
        </p>
      </div>
      {about(proposal.sticks_to) && <p className="text-muted">{about(proposal.sticks_to)}</p>}
      {proposal.points.length + proposal.reminders.length > 0 && (
        <ul className="flex list-disc flex-col gap-0.5 pl-4 text-xs text-muted">
          {proposal.points.map((point) => {
            const under = point.theme || point.block;
            // A row's cells after its name, in its fields' order.
            const cells = proposal.fields
              .map((field) => {
                const name = field.name.toLowerCase();
                const at = Object.keys(point.cells).find((key) => key.toLowerCase() === name);
                return at ? cellWords(field, point.cells[at]) : "";
              })
              .filter(Boolean);
            const text = [point.text, ...cells].join(" · ");
            return <li key={`${under}${point.text}`}>{under ? `${under}: ${text}` : text}</li>;
          })}
          {proposal.reminders.map((reminder) => (
            <li key={reminder.title}>
              {reminder.title},{" "}
              {reminder.every_days ? rhythmWords(reminder.every_days) : dueWords(reminder.due_on)}
            </li>
          ))}
        </ul>
      )}
      {!guest && (
        <div className="flex flex-wrap items-center gap-2">
          <Button
            disabled={starting}
            onClick={() => void startAsIs()}
            title={toStart ? `Start ${proposal.name} (⌘↩)` : undefined}
          >
            Start {proposal.name}
          </Button>
          <Button variant="secondary" disabled={starting} onClick={() => setChanging(true)}>
            Change
          </Button>
        </div>
      )}
      <ErrorMessage>{problem}</ErrorMessage>
      {changing && (
        <TopicForm
          heading={`Change ${proposal.name}`}
          action="Start topic"
          fields={proposal}
          onSave={start}
          onClose={() => setChanging(false)}
        />
      )}
    </div>
  );
}

/** What an answer changed in topics you named, and Undo, which puts it back. */
function Changes({ message }: { message: Message }) {
  const { workspace } = useWorkspace();
  const guest = useIsGuest();
  const [undoing, setUndoing] = useState(false);
  const [problem, setProblem] = useState("");
  const undone = message.changes.every((c) => c.undone);
  const undo = async () => {
    setUndoing(true);
    setProblem("");
    try {
      await api("POST", `/api/w/${workspace.id}/messages/${message.id}/undo`, {});
    } catch (e) {
      setProblem(messageOf(e));
      setUndoing(false);
    }
  };
  return (
    <div className="flex flex-col gap-2 rounded-lg bg-surface px-3 py-2">
      {message.changes.map((change) => (
        <div key={`${change.topic_id}${change.said}`} className="flex flex-col gap-2">
          <p>
            <span className="font-medium">{change.name}</span>
            <span className="text-muted"> · {change.said}</span>
          </p>
          {change.layout && <WhatMoved layout={change.layout} />}
        </div>
      ))}
      {undone ? (
        <p className="text-xs text-muted">Undone.</p>
      ) : (
        !guest && (
          <div>
            <Button variant="secondary" disabled={undoing} onClick={() => void undo()}>
              Undo
            </Button>
          </div>
        )
      )}
      <ErrorMessage>{problem}</ErrorMessage>
    </div>
  );
}

/**
 * A page laid out anew, block by block: what each is now and what it holds.
 * One block's change is said already, so it shows with two or more.
 */
function WhatMoved({ layout }: { layout: PageLayout }) {
  const [open, setOpen] = useState(false);
  if (layout.moves.length < 2) return null;
  return (
    <>
      <button
        type="button"
        className="ui-disclosure self-start"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <ChevronRight size={13} strokeWidth={1.75} aria-hidden="true" />
        What moved
      </button>
      {open && (
        <ul className="flex flex-col gap-1.5 pb-1 pl-1.5 text-xs">
          {layout.moves.map((move) => (
            <li key={`${move.block}${move.change}`} className="flex flex-col">
              <p>
                <span className="font-medium">{move.block}</span>
                <span className="text-faint"> · {move.change}</span>
              </p>
              <span className="text-muted">{move.holds}</span>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/** A reminder an answer offered: on with a yes, gone with a no. A yes also
 * takes off the reminders it replaces. */
function Offer({ reminder }: { reminder: Reminder }) {
  const { sync } = useWorkspace();
  const time = timeWords(reminder.at);
  const replaces = sync.query("Reminder").filter((r) => reminder.replaces.includes(r.id));
  return (
    <div className="flex flex-col gap-2 rounded-lg bg-surface px-3 py-2">
      <div>
        <p className="font-medium">{reminder.title}</p>
        <p className="text-xs text-muted">
          {reminder.every_days
            ? `${upperFirst(rhythmWords(reminder.every_days))}, from ${dueWords(reminder.due_on)}`
            : upperFirst(dueWords(reminder.due_on))}
          {time ? ` at ${time}` : ""}. Remind you?
        </p>
        {replaces.map((old) => (
          <p key={old.id} className="text-xs text-muted">
            In place of {old.title}, {rhythmWords(old.every_days)} from {dueWords(old.due_on)}.
          </p>
        ))}
      </div>
      <div className="flex gap-2">
        <Button
          onClick={() => {
            sync.update("Reminder", reminder.id, { status: "on" });
            for (const old of replaces) sync.delete("Reminder", old.id);
          }}
        >
          Remind me
        </Button>
        <Button
          variant="secondary"
          onClick={() => sync.update("Reminder", reminder.id, { status: "declined" })}
        >
          No
        </Button>
      </div>
    </div>
  );
}

/** "every week" starts a sentence as "Every week". */
function upperFirst(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** An answer that belongs in another topic: take the question there. */
function Elsewhere({ name, topic, question }: { name: string; topic: Topic; question: string }) {
  const { workspace } = useWorkspace();
  const topics = useTopics();
  const navigate = useNavigate();
  const newTopic = useNewTopic();
  const there = topics.find(
    (t) => t.id !== topic.id && t.name.toLowerCase() === name.toLowerCase(),
  );
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-lg bg-surface px-3 py-2">
      <p className="min-w-0 flex-1">
        {there ? `This fits ${there.name} better.` : `This fits a topic of its own: ${name}.`}
      </p>
      {there ? (
        <Button
          variant="secondary"
          onClick={() =>
            void navigate({
              to: "/$workspace/topics/$topic",
              params: { workspace: workspace.url_key, topic: there.id },
              search: { talk: "new", ask: question || undefined },
            })
          }
        >
          Ask it in {there.name}
        </Button>
      ) : (
        <Button variant="secondary" onClick={() => newTopic(name)}>
          Start {name}
        </Button>
      )}
    </div>
  );
}

function Problem({ text }: { text: string }) {
  const { workspace } = useWorkspace();
  return (
    <div role="alert" className="ui-tint-danger flex flex-col gap-1 rounded-md px-3 py-2">
      <p>{text}</p>
      {text.includes("Settings → Claude") && (
        <Link
          to="/$workspace/settings/claude"
          params={{ workspace: workspace.url_key }}
          className="ui-link"
        >
          Open Settings → Claude
        </Link>
      )}
    </div>
  );
}
