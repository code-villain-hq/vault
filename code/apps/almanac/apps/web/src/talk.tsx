// A talk in the panel beside a topic's page. Asking posts the question
// and reads the answer as it's written (server-sent events); the talk,
// the answer and what the page kept from it then arrive through sync. In
// the workspace's own topic, an answer may propose a topic to start, or
// say what it changed in one, with Undo.

import type { Message, Proposal, Reminder, StartTopicRequest, Talk, Topic } from "@almanac/types";
import { MoreMenu, messageOf, overlayOpen, UNKNOWN_PROBLEM } from "@code-villain/web";
import { Link, useNavigate } from "@tanstack/react-router";
import { Plus, X } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { ago, dueWords, rhythmWords, timeWords } from "./format";
import { topicIcon } from "./icons";
import { isOwn, useNewTopic, useTopics } from "./layout";
import { apiUrl, authHeaders, credentialsFor } from "./server";
import { TopicForm } from "./topic-dialog";
import { Button, ErrorMessage } from "./ui";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";

/** An answer on its way, until the talk's own records have it. */
export interface Asking {
  talkId: string | null;
  question: string;
  /** How many questions the talk had before this one. */
  asked: number;
  text: string;
  answerId: string | null;
  problem: string | null;
}

/** Asks in a topic, and what's on its way. */
export function useAsk(topicId: string, onTalk: (talkId: string) => void) {
  const { workspace, place, sync } = useWorkspace();
  const [asking, setAsking] = useState<Asking | null>(null);
  const latest = useRef(onTalk);
  latest.current = onTalk;

  const ask = useCallback(
    async (question: string, talkId: string | null) => {
      const text = question.trim();
      if (!text) return;
      const asked = talkId
        ? sync.query("Message").filter((m) => m.talk_id === talkId && m.role === "you").length
        : 0;
      const update = (change: Partial<Asking>) =>
        setAsking((a) => (a && a.question === text ? { ...a, ...change } : a));
      setAsking({ talkId, question: text, asked, text: "", answerId: null, problem: null });
      const path = `/api/w/${workspace.id}/topics/${topicId}/ask`;
      let response: Response;
      try {
        response = await fetch(apiUrl(path, place), {
          method: "POST",
          credentials: credentialsFor(place),
          headers: { ...authHeaders(place), "content-type": "application/json" },
          body: JSON.stringify(talkId ? { talk_id: talkId, text } : { text }),
        });
      } catch {
        update({ problem: "Can't reach Almanac. Check your connection, then ask again." });
        return;
      }
      if (!response.ok || !response.body) {
        const error = (await response.json().catch(() => null)) as { message?: string } | null;
        update({ problem: messageOf(new Error(error?.message ?? UNKNOWN_PROBLEM)) });
        return;
      }
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = "";
      let said = "";
      for (;;) {
        const { value, done } = await reader.read().catch(() => ({ value: "", done: true }));
        if (done) break;
        buffer += value;
        let end = buffer.indexOf("\n\n");
        while (end >= 0) {
          const block = buffer.slice(0, end);
          buffer = buffer.slice(end + 2);
          end = buffer.indexOf("\n\n");
          const event = /^event: (.*)$/m.exec(block)?.[1];
          const data = block
            .split("\n")
            .filter((line) => line.startsWith("data: "))
            .map((line) => line.slice(6))
            .join("\n");
          if (!event || !data) continue;
          const body = JSON.parse(data) as { talk_id?: string; text?: string; message_id?: string };
          if (event === "talk" && body.talk_id) {
            update({ talkId: body.talk_id });
            latest.current(body.talk_id);
          } else if (event === "text") {
            said += body.text ?? "";
            update({ text: said });
          } else if (event === "done") {
            update({ answerId: body.message_id ?? null });
          } else if (event === "problem") {
            update({ problem: body.text ?? UNKNOWN_PROBLEM });
          }
        }
      }
    },
    [workspace.id, place, sync, topicId],
  );

  return { asking, ask };
}

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
  onAsk: (text: string, talkId: string | null) => void;
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
  const end = useRef<HTMLDivElement>(null);

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
    if (!text.trim()) return;
    onAsk(text, talkId);
    setText("");
  };

  return (
    <aside
      aria-label="Talk"
      className="flex w-[min(26rem,45%)] shrink-0 flex-col border-l border-line bg-panel"
    >
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
                    label: "Delete this talk",
                    danger: true,
                    onSelect: () => {
                      sync.delete("Talk", talk.id);
                      onOpen(null);
                    },
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
            />
          ))}
          {showQuestion && <Question text={mine.question} />}
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
        <textarea
          // biome-ignore lint/a11y/noAutofocus: the panel opens to ask
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
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
          className="ui-field resize-none rounded-md px-3 py-2"
        />
      </form>
    </aside>
  );
}

/** The question an answer answered: what was asked just before it. */
function before(messages: Message[], index: number): string {
  const asked = messages[index - 1];
  return asked?.role === "you" ? asked.text : "";
}

function Question({ text }: { text: string }) {
  return <p className="ml-8 self-end whitespace-pre-wrap rounded-lg bg-chip px-3 py-2">{text}</p>;
}

/** One thing said in a talk, and for an answer, what came of it. */
function Said({
  message,
  topic,
  question,
  reminders,
  toStart,
}: {
  message: Message;
  topic: Topic;
  question: string;
  reminders: Reminder[];
  /** The proposal ⌘↩ starts. */
  toStart: boolean;
}) {
  if (message.role === "you") return <Question text={message.text} />;
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
    const what =
      started.shape === "document"
        ? ", a document"
        : kept
          ? `, with ${kept} ${kept === 1 ? "point" : "points"}`
          : "";
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
    proposal.shape === "document"
      ? "A document"
      : proposal.reminders.length > 0
        ? "Cards, with a week"
        : "Cards";
  return (
    <div className="flex flex-col gap-2.5 rounded-lg bg-surface px-4 py-3.5">
      <div className="flex items-center gap-2">
        <Icon size={15} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />
        <p className="min-w-0 truncate font-semibold">{proposal.name}</p>
        <p className="ml-auto shrink-0 text-xs text-faint">
          {[proposal.group_name, page].filter(Boolean).join(" · ")}
        </p>
      </div>
      {proposal.sticks_to && <p className="text-muted">{proposal.sticks_to}</p>}
      {proposal.points.length + proposal.reminders.length > 0 && (
        <ul className="flex list-disc flex-col gap-0.5 pl-4 text-xs text-muted">
          {proposal.points.map((point) => (
            <li key={`${point.theme}${point.text}`}>
              {point.theme}: {point.text}
            </li>
          ))}
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
          fields={{ ...proposal, title: proposal.title ?? "" }}
          onSave={(fields) => start({ ...fields, title: fields.title || undefined })}
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
        <p key={`${change.topic_id}${change.said}`}>
          <span className="font-medium">{change.name}</span>
          <span className="text-muted"> · {change.said}</span>
        </p>
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

/** A reminder an answer offered: on with a yes, gone with a no. */
function Offer({ reminder }: { reminder: Reminder }) {
  const { sync } = useWorkspace();
  const time = timeWords(reminder.at);
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
      </div>
      <div className="flex gap-2">
        <Button onClick={() => sync.update("Reminder", reminder.id, { status: "on" })}>
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

/**
 * An answer as written: paragraphs and short lists, with **bold** kept.
 * Answers are told to stay short and plain, so this is all they use.
 */
export function Prose({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length) {
      blocks.push(<p key={blocks.length}>{inline(paragraph.join(" "))}</p>);
      paragraph = [];
    }
    if (list) {
      const items = list.items.map((item, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: an answer's lines don't move
        <li key={i}>{inline(item)}</li>
      ));
      blocks.push(
        list.ordered ? (
          <ol key={blocks.length} className="list-decimal pl-5">
            {items}
          </ol>
        ) : (
          <ul key={blocks.length} className="list-disc pl-5">
            {items}
          </ul>
        ),
      );
      list = null;
    }
  };
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    const bullet = /^[-*•]\s+(.*)$/.exec(line);
    const numbered = /^\d+[.)]\s+(.*)$/.exec(line);
    const item = bullet ?? numbered;
    if (!line) {
      flush();
    } else if (item) {
      if (paragraph.length || (list && list.ordered !== !!numbered)) flush();
      list ??= { ordered: !!numbered, items: [] };
      list.items.push(item[1] ?? "");
    } else {
      if (list) flush();
      paragraph.push(line.replace(/^#+\s*/, ""));
    }
  }
  flush();
  return <div className="flex flex-col gap-2">{blocks}</div>;
}

function inline(text: string): ReactNode {
  // Bold parts sit at the odd places the split leaves them.
  return text.split(/\*\*([^*]+)\*\*/).map((part, i) =>
    i % 2 === 1 ? (
      <strong key={`b${part}`} className="font-semibold">
        {part}
      </strong>
    ) : (
      part
    ),
  );
}
