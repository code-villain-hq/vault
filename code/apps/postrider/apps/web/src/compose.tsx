// Writing mail, in a dialog over the page: C writes a new one, and on an
// open mail R replies, A replies to all and F forwards it. To, Cc and Bcc
// take addresses with commas between and suggest the people who wrote to
// you. The account it goes from sits beside the buttons; More professional
// writes it again with Claude Code on this Mac (`rewrite.rs`), in your
// language and with every fact kept, and Back to yours (or ⌘Z) puts your
// words back. Files come with the paperclip (⇧⌘A), dropped on the mail or
// pasted into it; each goes ahead with `POST /uploads`, and a forward takes
// its mail's along until you leave one out. It all goes to `POST /mails`
// (`send.rs`). What you write is kept in its account's Drafts as you go,
// each time the writing pauses and as the dialog closes (`POST /drafts`,
// `drafts.rs`), so it's there after Postrider quits; a draft opened from
// Drafts opens here to write on, and ⇧⌘⌫ throws one away, to Trash.

import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog, shortcutFree } from "@code-villain/web";
import type {
  Attach,
  DraftRef,
  MailRef,
  MailSummary,
  Message,
  NewUpload,
  Draft as OpenedDraft,
  Outgoing,
  Party,
  Rewrite,
  Rewritten,
  Upload,
} from "@postrider/types";
import { ChevronDown, Paperclip, Sparkles, Trash2, Undo2, X } from "lucide-react";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { api, messageOf } from "./api";
import { initial, mailKey, ownAddresses } from "./inbox";
import { hue, useAccounts, useFolders, useInbox } from "./mail";
import type { Place } from "./server";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** How an open mail is answered: R, A or F. */
export type Answering = "reply" | "all" | "forward";

/** Whom it goes to and what about, as the lines above the text hold them. */
type Lines = { to: string; cc: string; bcc: string; subject: string };

const NO_LINES: Lines = { to: "", cc: "", bcc: "", subject: "" };

/**
 * What was written and not sent, by how it answers which mail, "new", or
 * the draft it was opened as: Esc keeps it here, as Drafts keeps it once
 * the page reloads. `yours` is your text while
 * Postrider's stands in its place; `wroteTo` is the account the person a
 * new mail goes to wrote to.
 */
type Draft = Partial<Lines> & {
  text?: string;
  from?: string;
  yours?: string;
  wroteTo?: string;
  /** The files, once you added one or left one out. */
  files?: Going[];
};
const drafts = new Map<string, Draft>();

/**
 * Keeps what changed, unless nothing is written, the lines are as they
 * began and the files weren't touched.
 */
function keep(key: string, next: Draft, began: Lines = NO_LINES) {
  const draft = { ...drafts.get(key), ...next };
  const changed = (Object.keys(began) as (keyof Lines)[]).some(
    (line) => (draft[line] ?? began[line]).trim() !== began[line].trim(),
  );
  if (draft.text?.trim() || changed || draft.files) drafts.set(key, draft);
  else drafts.delete(key);
  later(key);
}

/**
 * What each draft was kept as in Drafts last: keeping it again takes that
 * one's place, and sending it deletes it.
 */
const savedAs = new Map<string, DraftRef>();

/** Each draft's keeping in Drafts, one after the other: the last, to wait for. */
const keeping = new Map<string, Promise<void>>();

/** Keepings waiting for the writing to pause. */
const waiting = new Map<string, ReturnType<typeof setTimeout>>();

/** Drafts being sent or thrown away: they're kept no more. */
const leaving = new Set<string>();

/** How long the writing pauses before what's written is kept in Drafts. */
const PAUSE_MS = 3000;

type FilesChange = (files: Going[]) => Going[];

/** What keeping a draft needs besides what's written: from its dialog. */
type Setup = {
  workspace: string;
  place: Place;
  began: Lines;
  /** The account it goes from, unless you picked one. */
  from: string;
  answers?: MailRef;
  forwards?: MailRef;
  startFiles: Going[];
  moved: () => void;
  /** While its dialog is open: changes the files it shows, and says what failed. */
  open?: {
    files: (change: FilesChange, quietly: boolean) => void;
    failed: (problem: string) => void;
  };
};
const setups = new Map<string, Setup>();

/** What a draft opened from Drafts answers or forwards. */
const opened = new Map<string, { answers?: MailRef; forwards?: MailRef }>();

/** The mail as written: what changed in its draft, the rest as it began. */
function outgoingOf(draft: Draft, setup: Setup, files: Attach[]): Outgoing {
  return {
    to: draft.to ?? setup.began.to,
    cc: draft.cc ?? setup.began.cc,
    bcc: draft.bcc ?? setup.began.bcc,
    subject: draft.subject ?? setup.began.subject,
    from_account: draft.from ?? setup.from,
    text: draft.text ?? "",
    answers: setup.answers,
    forwards: setup.forwards,
    files,
  };
}

/** Keeps the draft in Drafts once the writing pauses. */
function later(key: string) {
  if (leaving.has(key)) return;
  clearTimeout(waiting.get(key));
  waiting.set(
    key,
    setTimeout(() => {
      waiting.delete(key);
      keepNow(key);
    }, PAUSE_MS),
  );
}

/** Keeps it now if it waits to be kept: as its dialog closes. */
function flush(key: string) {
  if (!waiting.has(key)) return;
  clearTimeout(waiting.get(key));
  waiting.delete(key);
  keepNow(key);
}

/** Waits no longer to keep it, and for a keeping under way. */
function settle(key: string): Promise<void> {
  clearTimeout(waiting.get(key));
  waiting.delete(key);
  return keeping.get(key) ?? Promise.resolve();
}

/**
 * Keeps the draft in its account's Drafts, after any keeping under way, in
 * place of what it was kept as last; one with nothing left in it is
 * deleted there. Its files are then named by their place in the new one,
 * which keeps them however long it takes.
 */
function keepNow(key: string) {
  const run = async () => {
    const setup = setups.get(key);
    if (!setup || leaving.has(key)) return;
    const draft = drafts.get(key);
    const was = savedAs.get(key);
    const path = `/api/w/${setup.workspace}/drafts`;
    if (!draft) {
      if (!was) return;
      await api("POST", `${path}/delete`, was, setup.place);
      savedAs.delete(key);
      setup.moved();
      return;
    }
    const files = draft.files ?? setup.startFiles;
    const attached = await Promise.all(
      files.map((file) =>
        file.attach
          ? Promise.resolve(file.attach)
          : (arriving.get(file.key)?.catch(() => undefined) ?? Promise.resolve(undefined)),
      ),
    );
    const going = files.flatMap((file, index) => {
      const attach = attached[index];
      return attach ? [{ key: file.key, attach }] : [];
    });
    const outgoing: Outgoing = {
      ...outgoingOf(
        draft,
        setup,
        going.map((file) => file.attach),
      ),
      draft: was,
    };
    const kept = await api<DraftRef>("POST", path, outgoing, setup.place);
    savedAs.set(key, kept);
    const at = new Map(going.map((file, index) => [file.key, index]));
    changeFiles(
      key,
      (now) =>
        now.map((file) => {
          const index = at.get(file.key);
          if (index === undefined || file.attach?.kind === "kept") return file;
          return { ...file, attach: { kind: "drafted", index } };
        }),
      true,
    );
    setup.moved();
  };
  const queued = (keeping.get(key) ?? Promise.resolve()).then(run).catch((problem: unknown) => {
    setups.get(key)?.open?.failed(`Couldn't keep it in Drafts: ${messageOf(problem)}`);
  });
  keeping.set(key, queued);
}

/** Changes a draft's files: in its dialog while it's open, else where it's kept. */
function changeFiles(key: string, change: FilesChange, quietly = false) {
  const setup = setups.get(key);
  if (setup?.open) return setup.open.files(change, quietly);
  const closed = drafts.get(key);
  if (!closed?.files) return;
  drafts.set(key, { ...closed, files: change(closed.files) });
  if (!quietly) later(key);
}

/** Forgets a draft sent or thrown away. */
function forget(key: string) {
  drafts.delete(key);
  savedAs.delete(key);
  setups.delete(key);
  opened.delete(key);
  leaving.delete(key);
}

/**
 * Takes a draft opened from Drafts as it's written. One still written on
 * here, under another key, goes on from there: what was written since it
 * was kept stays.
 */
function adopt(key: string, draft: OpenedDraft) {
  const files: Going[] = draft.files.map((file, index) => ({
    key: `drafted:${index}`,
    name: file.name,
    size: file.size,
    attach: { kind: "drafted", index },
  }));
  const seed: Draft = {
    to: draft.to,
    cc: draft.cc,
    bcc: draft.bcc,
    subject: draft.subject,
    text: draft.text,
    from: draft.draft.account_id,
    files,
  };
  opened.set(key, { answers: draft.answers, forwards: draft.forwards });
  const other = [...savedAs].find(
    ([was, ref]) => was !== key && ref.message_id === draft.draft.message_id,
  )?.[0];
  if (other === undefined) {
    drafts.set(key, seed);
    savedAs.set(key, draft.draft);
    return;
  }
  drafts.set(key, { ...seed, ...drafts.get(other) });
  savedAs.set(key, draft.draft);
  const queued = keeping.get(other);
  if (queued) keeping.set(key, queued);
  for (const map of [drafts, savedAs, setups, opened, keeping]) map.delete(other);
}

/**
 * A file the mail takes along: one of the mail it forwards, or one you
 * added. `attach` is missing while an added one is on its way.
 */
type Going = { key: string; name: string; size: number; attach?: Attach };

const NO_FILES: Going[] = [];

/** Added files on their way to the server, by key: what each becomes there. */
const arriving = new Map<string, Promise<Attach>>();

/** The most a file may hold, and a mail's files together, as `send.rs` takes. */
const MOST_BYTES = 25_000_000;

/** A file's bytes in base64, as `POST /uploads` takes them. */
function base64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = typeof reader.result === "string" ? reader.result : "";
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.onerror = () => reject(new Error(`${file.name} couldn't be read.`));
    reader.readAsDataURL(file);
  });
}

/** A new mail's draft. */
const NEW = "new";

/** The account a new mail went from last, which the next one goes from too. */
let lastFrom: string | undefined;

/**
 * R, A or F on an open mail: a reply to whom it asks replies to go (A: and
 * to everyone else it went to, but you), or a forward. It goes from the
 * account the mail came to unless you pick another; ⌘Enter sends it, and
 * Esc closes it with what you wrote kept.
 */
export function AnswerDialog({
  how,
  onClose,
  summary,
  message,
}: {
  how: Answering | undefined;
  onClose: () => void;
  summary: MailSummary;
  message: Message | undefined;
}) {
  return (
    <CreateDialog open={how !== undefined} onClose={onClose} width="40rem">
      {/* What it answers comes first: whom it went to, and its files. */}
      {how &&
        (message ? (
          <AnswerForm key={how} how={how} summary={summary} message={message} onDone={onClose} />
        ) : (
          <>
            <Dialog.Title className="sr-only">Reading the mail</Dialog.Title>
            <p className="p-5 text-faint">Reading the mail…</p>
          </>
        ))}
    </CreateDialog>
  );
}

function AnswerForm({
  how,
  summary,
  message,
  onDone,
}: {
  how: Answering;
  summary: MailSummary;
  message: Message;
  onDone: () => void;
}) {
  const accounts = useAccounts();
  const draft = `${how}:${mailKey(summary)}`;
  const [began] = useState(() => begin(how, summary, message, ownAddresses(accounts)));
  const [lines, setLines] = useState<Lines>(() => {
    const kept = drafts.get(draft);
    return {
      to: kept?.to ?? began.to,
      cc: kept?.cc ?? began.cc,
      bcc: kept?.bcc ?? began.bcc,
      subject: kept?.subject ?? began.subject,
    };
  });
  const forwarding = how === "forward";
  const mail = useMemo<MailRef>(
    () => ({
      account_id: summary.account_id,
      uid: summary.uid,
      folder: summary.folder ?? undefined,
    }),
    [summary.account_id, summary.uid, summary.folder],
  );

  // A forward takes its mail's files along.
  const [kept] = useState<Going[]>(() =>
    forwarding
      ? message.attachments.map((file, index) => ({
          key: `kept:${index}`,
          name: file.name,
          size: file.size,
          attach: { kind: "kept", index },
        }))
      : [],
  );

  const title = forwarding
    ? `Forward ${summary.subject || "(no subject)"}`
    : `Reply to ${summary.from_name || summary.from_address}`;
  return (
    <Writer
      draft={draft}
      began={began}
      defaultFrom={summary.account_id}
      label={forwarding ? "Your note" : "Your reply"}
      about={{
        to: lines.to.trim() || undefined,
        subject: lines.subject.trim() || undefined,
        replying: forwarding ? undefined : mail,
      }}
      ready={addressed(lines)}
      needsText={!forwarding}
      focusText={!forwarding}
      startFiles={kept}
      answers={forwarding ? undefined : mail}
      forwards={forwarding ? mail : undefined}
      onDone={onDone}
    >
      <Dialog.Title className="sr-only">{title}</Dialog.Title>
      <AddressLines
        lines={lines}
        onChange={(next) => {
          setLines(next);
          keep(draft, next, began);
        }}
        autoFocus={forwarding}
      />
    </Writer>
  );
}

/**
 * The lines an answer starts with. A reply goes to the mail's Reply-To,
 * else its sender; to all, also to whom else it went to and copied, but
 * you; both "Re: " its subject. A forward goes to no one yet, "Fwd: " it.
 */
function begin(how: Answering, summary: MailSummary, message: Message, own: Set<string>): Lines {
  if (how === "forward") return { ...NO_LINES, subject: prefixed("Fwd", summary.subject) };
  const answer: Party[] = message.answer_to.length
    ? message.answer_to
    : [{ name: summary.from_name, address: summary.from_address }];
  const others = (people: Party[]) => people.filter((p) => !own.has(p.address.toLowerCase()));
  // To all: but you, unless it's you who asked for the answers.
  const to = how === "all" && others(answer).length > 0 ? others(answer) : answer;
  const seen = new Set(to.map((p) => p.address.toLowerCase()));
  const cc: Party[] = [];
  if (how === "all") {
    for (const person of others([...message.sent_to, ...message.copied])) {
      const address = person.address.toLowerCase();
      if (seen.has(address)) continue;
      seen.add(address);
      cc.push(person);
    }
  }
  return {
    to: to.map(written).join(", "),
    cc: cc.map(written).join(", "),
    bcc: "",
    subject: prefixed("Re", summary.subject),
  };
}

/** Whether it says whom it goes to. */
function addressed(lines: Lines): boolean {
  return [lines.to, lines.cc, lines.bcc].some((line) => line.replaceAll(/[,;\s]/g, "") !== "");
}

const NewMailContext = createContext<() => void>(() => {});

/** Opens a new mail. */
export function useNewMail() {
  return useContext(NewMailContext);
}

/** The new mail's dialog, opened by C anywhere in the workspace. */
export function NewMailProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const accounts = useAccounts();
  const can = accounts.length > 0;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "c" || event.shiftKey || !can || !shortcutFree(event)) return;
      event.preventDefault();
      setOpen(true);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [can]);

  const openNew = useCallback(() => {
    if (can) setOpen(true);
  }, [can]);

  return (
    <NewMailContext.Provider value={openNew}>
      {children}
      <CreateDialog open={open} onClose={() => setOpen(false)} width="40rem">
        <NewMailForm onDone={() => setOpen(false)} />
      </CreateDialog>
    </NewMailContext.Provider>
  );
}

function NewMailForm({ onDone }: { onDone: () => void }) {
  const accounts = useAccounts();
  const kept = drafts.get(NEW);
  const [lines, setLines] = useState<Lines>(() => linesOf(kept));
  const [wroteTo, setWroteTo] = useState(kept?.wroteTo);
  const known = (id: string | undefined) => accounts.find((a) => a.id === id)?.id;
  const defaultFrom = known(lastFrom) ?? accounts[0]?.id ?? "";

  return (
    <Writer
      draft={NEW}
      began={NO_LINES}
      defaultFrom={defaultFrom}
      suggestedFrom={wroteTo}
      label="Your mail"
      about={{ to: lines.to.trim() || undefined, subject: lines.subject.trim() || undefined }}
      ready={addressed(lines)}
      focusText={false}
      onDone={(sentFrom) => {
        if (sentFrom) lastFrom = sentFrom;
        onDone();
      }}
    >
      <Dialog.Title className="sr-only">New mail</Dialog.Title>
      <AddressLines
        lines={lines}
        onChange={(next) => {
          setLines(next);
          keep(NEW, next);
        }}
        onPick={(person) => {
          setWroteTo(person.account_id);
          keep(NEW, { wroteTo: person.account_id });
        }}
        autoFocus
      />
    </Writer>
  );
}

/** The lines a draft holds. */
function linesOf(draft: Draft | undefined): Lines {
  return {
    to: draft?.to ?? "",
    cc: draft?.cc ?? "",
    bcc: draft?.bcc ?? "",
    subject: draft?.subject ?? "",
  };
}

/**
 * A draft opened from Drafts, to write on as it was left: kept again as you
 * go, sent, or thrown away. It goes from the account whose Drafts holds it
 * unless you pick another.
 */
export function DraftDialog({
  account,
  uid,
  onClose,
}: {
  account: string;
  uid: number;
  onClose: () => void;
}) {
  const { workspace, place } = useWorkspace();
  const key = `drafted:${account}:${uid}`;
  const [ready, setReady] = useState(() => drafts.has(key) && opened.has(key));
  const [problem, setProblem] = useState<string>();

  useEffect(() => {
    if (ready) return;
    let live = true;
    api<OpenedDraft>("GET", `/api/w/${workspace.id}/drafts/${account}/${uid}`, undefined, place)
      .then((draft) => {
        if (!live) return;
        if (!(drafts.has(key) && opened.has(key))) adopt(key, draft);
        setReady(true);
      })
      .catch((failed: unknown) => {
        if (live) setProblem(messageOf(failed));
      });
    return () => {
      live = false;
    };
  }, [ready, workspace.id, account, uid, place, key]);

  return (
    <CreateDialog open onClose={onClose} width="40rem">
      {ready ? (
        <DraftForm draft={key} onDone={onClose} />
      ) : (
        <>
          <Dialog.Title className="sr-only">Opening the draft</Dialog.Title>
          {problem ? (
            <div className="p-5">
              <ErrorMessage>{problem}</ErrorMessage>
            </div>
          ) : (
            <p className="p-5 text-faint">Opening the draft…</p>
          )}
        </>
      )}
    </CreateDialog>
  );
}

function DraftForm({ draft, onDone }: { draft: string; onDone: () => void }) {
  const kept = drafts.get(draft);
  const { answers, forwards } = opened.get(draft) ?? {};
  const [lines, setLines] = useState<Lines>(() => linesOf(kept));
  const [focusText] = useState(() => addressed(lines));
  return (
    <Writer
      draft={draft}
      began={NO_LINES}
      defaultFrom={kept?.from ?? ""}
      label={answers ? "Your reply" : forwards ? "Your note" : "Your mail"}
      about={{
        to: lines.to.trim() || undefined,
        subject: lines.subject.trim() || undefined,
        replying: answers,
      }}
      ready={addressed(lines)}
      needsText={!forwards}
      focusText={focusText}
      answers={answers}
      forwards={forwards}
      onDone={onDone}
    >
      <Dialog.Title className="sr-only">{`Draft: ${lines.subject.trim() || "(no subject)"}`}</Dialog.Title>
      <AddressLines
        lines={lines}
        onChange={(next) => {
          setLines(next);
          keep(draft, next);
        }}
        autoFocus={!focusText}
      />
    </Writer>
  );
}

/**
 * To, Cc and Bcc, then Subject. Cc and Bcc show once there's something in
 * them, or once asked for.
 */
function AddressLines({
  lines,
  onChange,
  onPick,
  autoFocus = false,
}: {
  lines: Lines;
  onChange: (lines: Lines) => void;
  onPick?: (person: Person) => void;
  autoFocus?: boolean;
}) {
  const [copies, setCopies] = useState<"shown" | "asked" | undefined>(
    lines.cc.trim() || lines.bcc.trim() ? "shown" : undefined,
  );
  const line = (name: keyof Lines) => (value: string) => onChange({ ...lines, [name]: value });
  return (
    <>
      <div className="compose-line">
        <label htmlFor="compose-to" className="compose-label">
          To
        </label>
        <AddressField
          id="compose-to"
          value={lines.to}
          onChange={line("to")}
          onPick={onPick}
          autoFocus={autoFocus}
        />
        {!copies && (
          <button
            type="button"
            className="compose-more"
            title="Copy someone in, seen or unseen"
            onClick={() => setCopies("asked")}
          >
            Cc Bcc
          </button>
        )}
      </div>
      {copies && (
        <>
          <div className="compose-line">
            <label htmlFor="compose-cc" className="compose-label">
              Cc
            </label>
            <AddressField
              id="compose-cc"
              value={lines.cc}
              onChange={line("cc")}
              onPick={onPick}
              autoFocus={copies === "asked"}
            />
          </div>
          <div className="compose-line">
            <label htmlFor="compose-bcc" className="compose-label">
              Bcc
            </label>
            <AddressField
              id="compose-bcc"
              value={lines.bcc}
              onChange={line("bcc")}
              onPick={onPick}
            />
          </div>
        </>
      )}
      <div className="compose-line">
        <label htmlFor="compose-subject" className="compose-label">
          Subject
        </label>
        <input
          id="compose-subject"
          value={lines.subject}
          onChange={(event) => line("subject")(event.target.value)}
          className="compose-input"
        />
      </div>
    </>
  );
}

/** The files a mail takes along, by name and size, each to leave out. */
function FilesGoing({ files, onRemove }: { files: Going[]; onRemove: (key: string) => void }) {
  return (
    <ul className="files compose-files" aria-label="Files it takes along">
      {files.map((file) => (
        <li key={file.key}>
          <Paperclip size={14} strokeWidth={1.75} aria-hidden="true" className="text-muted" />
          <span className="truncate">{file.name}</span>
          <span className="text-faint tabular-nums">
            {file.attach ? bytes(file.size) : "Adding…"}
          </span>
          <button
            type="button"
            className="compose-remove"
            aria-label={`Leave ${file.name} out`}
            title="Leave it out"
            onClick={() => onRemove(file.key)}
          >
            <X size={13} strokeWidth={2} aria-hidden="true" />
          </button>
        </li>
      ))}
    </ul>
  );
}

/** "820 bytes", "14 KB", "2.3 MB". */
export function bytes(size: number): string {
  if (size < 1000) return `${size} bytes`;
  if (size < 1_000_000) return `${Math.round(size / 1000)} KB`;
  return `${(size / 1_000_000).toFixed(1)} MB`;
}

/**
 * The text, the account it goes from, More professional and Send, under
 * the lines `children` give: whom it goes to and what about. What's
 * written is kept in Drafts as the writing pauses and as it closes.
 */
function Writer({
  draft,
  began,
  defaultFrom,
  suggestedFrom,
  label,
  about,
  ready = true,
  needsText = true,
  focusText = true,
  startFiles = NO_FILES,
  answers,
  forwards,
  onDone,
  children,
}: {
  draft: string;
  /** The lines as they began, so a draft that's only those isn't kept. */
  began: Lines;
  defaultFrom: string;
  /** The account the person it goes to wrote to: it goes from there, unless you pick one. */
  suggestedFrom?: string;
  label: string;
  /** What More professional is told besides your words. */
  about: Omit<Rewrite, "text" | "from_account">;
  /** Whether it has what it needs besides the text. */
  ready?: boolean;
  /** Whether it can't go without text: a forward can. */
  needsText?: boolean;
  /** Whether the text takes the focus first, not whom it goes to. */
  focusText?: boolean;
  /** The files it starts with: a forward's. */
  startFiles?: Going[];
  /** The mail it answers, or forwards. */
  answers?: MailRef;
  forwards?: MailRef;
  /** It was sent, from `sentFrom`, or thrown away. */
  onDone: (sentFrom?: string) => void;
  children: ReactNode;
}) {
  const { workspace, place } = useWorkspace();
  const { moved } = useFolders();
  const accounts = useAccounts();
  const kept = drafts.get(draft);
  const [text, setText] = useState(kept?.text ?? "");
  const [from, setFrom] = useState(kept?.from ?? suggestedFrom ?? defaultFrom);
  const [yours, setYours] = useState(kept?.yours);
  const [busy, setBusy] = useState<"sending" | "rewriting" | "throwing">();
  const [error, setError] = useState<string>();
  const sender = accounts.find((a) => a.id === from);
  const field = useRef<HTMLTextAreaElement>(null);
  const picked = useRef(kept?.from !== undefined);
  const [files, setFiles] = useState<Going[]>(() => kept?.files ?? startFiles);
  const filesNow = useRef(files);
  const picker = useRef<HTMLInputElement>(null);
  const [dropping, setDropping] = useState(false);
  const setup: Setup = useMemo(
    () => ({ workspace: workspace.id, place, began, from, answers, forwards, startFiles, moved }),
    [workspace.id, place, began, from, answers, forwards, startFiles, moved],
  );

  // What keeping it in Drafts needs, kept current; and while it's open,
  // its files change here, and what fails shows here.
  useEffect(() => {
    setups.set(draft, {
      ...setup,
      open: {
        files: (change, quietly) => {
          filesNow.current = change(filesNow.current);
          setFiles(filesNow.current);
          const now = drafts.get(draft);
          if (!quietly) keep(draft, { files: filesNow.current }, began);
          else if (now?.files) drafts.set(draft, { ...now, files: filesNow.current });
        },
        failed: setError,
      },
    });
    return () => {
      const closed = setups.get(draft);
      if (closed) setups.set(draft, { ...closed, open: undefined });
    };
  }, [draft, setup, began]);

  // Closed while it waits to be kept, it's kept at once.
  useEffect(() => () => flush(draft), [draft]);

  // Waits for an added file to arrive, then names it by what it became.
  const follow = useCallback(
    (key: string) => {
      arriving.get(key)?.then(
        (attach) => {
          arriving.delete(key);
          const arrived: FilesChange = (now) =>
            now.map((file) => (file.key === key ? { ...file, attach } : file));
          changeFiles(draft, arrived, true);
        },
        (problem: unknown) => {
          arriving.delete(key);
          changeFiles(draft, (now) => now.filter((file) => file.key !== key));
          setups.get(draft)?.open?.failed(messageOf(problem));
        },
      );
    },
    [draft],
  );

  useEffect(() => {
    for (const file of filesNow.current) if (!file.attach) follow(file.key);
  }, [follow]);

  const add = (chosen: Iterable<File>) => {
    let together = filesNow.current.reduce((sum, file) => sum + file.size, 0);
    for (const file of chosen) {
      if (together + file.size > MOST_BYTES) {
        setError(
          file.size > MOST_BYTES
            ? `${file.name} holds ${bytes(file.size)}; a mail takes files up to 25 MB.`
            : `With ${file.name}, its files would hold ${bytes(together + file.size)}; a mail takes up to 25 MB.`,
        );
        continue;
      }
      setError(undefined);
      together += file.size;
      const key = crypto.randomUUID();
      arriving.set(
        key,
        base64(file)
          .then((data) => {
            const upload: NewUpload = { name: file.name, content_type: file.type, data };
            return api<Upload>("POST", `/api/w/${workspace.id}/uploads`, upload, place);
          })
          .then((added): Attach => ({ kind: "added", upload: added.id })),
      );
      changeFiles(draft, (now) => [...now, { key, name: file.name, size: file.size }]);
      follow(key);
    }
  };
  const arrived = files.every((file) => file.attach);
  const empty = !text.trim() && files.length === 0;

  useEffect(() => {
    if (suggestedFrom && !picked.current) setFrom(suggestedFrom);
  }, [suggestedFrom]);

  // Text written before goes on where it ends.
  useEffect(() => {
    const at = field.current?.value.length ?? 0;
    if (focusText) field.current?.setSelectionRange(at, at);
  }, [focusText]);

  const write = (next: string) => {
    setText(next);
    setYours(undefined);
    keep(draft, { text: next, yours: undefined }, began);
  };

  // A button that's busy lets go of the focus: the text takes it back, so
  // ⌘Enter and ⌘Z go on working.
  const done = (problem?: unknown) => {
    setBusy(undefined);
    if (problem !== undefined) setError(messageOf(problem));
    field.current?.focus();
  };

  const rewrite = () => {
    if (busy || !text.trim()) return;
    const asked: Rewrite = { ...about, text, from_account: from };
    setBusy("rewriting");
    setError(undefined);
    api<Rewritten>("POST", `/api/w/${workspace.id}/rewrite`, asked, place).then((rewritten) => {
      // Closed meanwhile and written on since, it's theirs that stays.
      if ((drafts.get(draft)?.text ?? "") === asked.text) {
        setText(rewritten.text);
        setYours(asked.text);
        keep(draft, { text: rewritten.text, yours: asked.text }, began);
      }
      done();
    }, done);
  };

  const backToYours = () => {
    if (yours === undefined) return;
    write(yours);
    field.current?.focus();
  };

  // Sending it, or throwing it away, waits for its keeping under way: then
  // its server knows the draft it deletes, or moves to Trash.
  const leave = (how: "sending" | "throwing", then: () => Promise<unknown>, sent?: string) => {
    setBusy(how);
    setError(undefined);
    leaving.add(draft);
    settle(draft)
      .then(then)
      .then(
        () => {
          forget(draft);
          moved();
          onDone(sent);
        },
        (problem: unknown) => {
          leaving.delete(draft);
          done(problem);
        },
      );
  };

  const submit = () => {
    if (busy || !ready || !arrived || (needsText && empty)) return;
    leave(
      "sending",
      () => {
        const going = filesNow.current.flatMap((file) => (file.attach ? [file.attach] : []));
        const outgoing: Outgoing = {
          ...outgoingOf(drafts.get(draft) ?? {}, setup, going),
          draft: savedAs.get(draft),
        };
        return api("POST", `/api/w/${workspace.id}/mails`, outgoing, place);
      },
      from,
    );
  };

  const throwAway = () => {
    if (busy) return;
    leave("throwing", async () => {
      const was = savedAs.get(draft);
      if (was) await api("POST", `/api/w/${workspace.id}/drafts/trash`, was, place);
    });
  };
  const something = drafts.has(draft) || savedAs.has(draft);

  return (
    <form
      className="compose"
      data-dropping={dropping || undefined}
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
      // Files dropped on the mail go with it; the page keeps the rest out
      // (`keepFilesOut`).
      onDragOver={(event) => {
        if (!event.dataTransfer.types.includes("Files")) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
        setDropping(true);
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropping(false);
      }}
      onDrop={(event) => {
        if (!event.dataTransfer.types.includes("Files")) return;
        event.preventDefault();
        setDropping(false);
        add(event.dataTransfer.files);
      }}
      onKeyDown={(event) => {
        const command = event.metaKey || event.ctrlKey;
        if (event.key === "Enter" && command) {
          event.preventDefault();
          submit();
        } else if (event.key.toLowerCase() === "a" && command && event.shiftKey) {
          event.preventDefault();
          picker.current?.click();
        } else if (event.key === "Backspace" && command && event.shiftKey) {
          event.preventDefault();
          throwAway();
        } else if (
          event.key.toLowerCase() === "z" &&
          command &&
          !event.shiftKey &&
          yours !== undefined &&
          !(event.target instanceof HTMLInputElement)
        ) {
          // Postrider's words came all at once: ⌘Z takes them back at once.
          event.preventDefault();
          backToYours();
        }
      }}
    >
      {children}
      <textarea
        ref={field}
        autoFocus={focusText}
        aria-label={label}
        value={text}
        readOnly={busy !== undefined}
        onChange={(event) => write(event.target.value)}
        onPaste={(event) => {
          // A file pasted goes with it, as a screenshot would.
          if (event.clipboardData.files.length === 0) return;
          event.preventDefault();
          add(event.clipboardData.files);
        }}
        className="compose-text"
      />
      {sender?.signature.trim() && (
        // Signed as it's sent, by the account it goes from (send.rs):
        // shown here so you see it, not written into your words.
        <p className="compose-signature" title="Your signature, from Settings → Accounts">
          {`-- \n${sender.signature}`}
        </p>
      )}
      {files.length > 0 && (
        <FilesGoing
          files={files}
          onRemove={(key) => {
            changeFiles(draft, (now) => now.filter((file) => file.key !== key));
            field.current?.focus();
          }}
        />
      )}
      {error && <ErrorMessage>{error}</ErrorMessage>}
      <div className="compose-foot">
        {accounts.length > 1 ? (
          <label className="compose-from" style={hue(sender?.hue ?? 1)}>
            <span className="av sm" aria-hidden="true">
              {initial(sender?.name ?? "")}
            </span>
            <select
              aria-label="From"
              value={from}
              onChange={(event) => {
                picked.current = true;
                setFrom(event.target.value);
                keep(draft, { from: event.target.value }, began);
              }}
            >
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.address}
                </option>
              ))}
            </select>
            <ChevronDown size={14} strokeWidth={1.75} aria-hidden="true" />
          </label>
        ) : (
          <span className="compose-from" style={hue(sender?.hue ?? 1)}>
            <span className="av sm" aria-hidden="true">
              {initial(sender?.name ?? "")}
            </span>
            {sender?.address}
          </span>
        )}
        <input
          ref={picker}
          type="file"
          multiple
          hidden
          onChange={(event) => {
            add(event.target.files ?? []);
            event.target.value = "";
            field.current?.focus();
          }}
        />
        {something && (
          <button
            type="button"
            className="ui-button ui-icon-button ml-auto"
            aria-label="Throw it away"
            title="Throw it away (⇧⌘⌫)"
            disabled={busy !== undefined}
            onClick={throwAway}
          >
            <Trash2 size={15} strokeWidth={1.75} aria-hidden="true" />
          </button>
        )}
        <button
          type="button"
          className={`ui-button ui-icon-button${something ? "" : " ml-auto"}`}
          aria-label="Add files"
          title="Add files (⇧⌘A)"
          onClick={() => picker.current?.click()}
        >
          <Paperclip size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
        {/* One button that turns into the other, so the focus stays on it. */}
        <Button
          variant="secondary"
          disabled={busy !== undefined || !text.trim()}
          onClick={yours === undefined ? rewrite : backToYours}
          title={
            yours === undefined
              ? "Write it again, more professionally, in your language"
              : "Back to yours (⌘Z)"
          }
        >
          {yours === undefined ? (
            <Sparkles size={15} strokeWidth={1.75} aria-hidden="true" />
          ) : (
            <Undo2 size={15} strokeWidth={1.75} aria-hidden="true" />
          )}
          {busy === "rewriting"
            ? "Writing…"
            : yours === undefined
              ? "More professional"
              : "Back to yours"}
        </Button>
        <Button
          type="submit"
          disabled={busy !== undefined || !ready || !arrived || (needsText && empty)}
          title={arrived ? "Send (⌘Enter)" : "Its files are still on their way"}
        >
          {busy === "sending" ? "Sending…" : "Send"}
        </Button>
      </div>
    </form>
  );
}

/** Someone who wrote to you, and the account they wrote to last. */
type Person = { name: string; address: string; account_id: string };

/**
 * To, Cc or Bcc: addresses with commas between. What you type after the
 * last one finds the people who wrote to you, by name or address.
 */
function AddressField({
  id,
  value,
  onChange,
  onPick,
  autoFocus = false,
}: {
  id: string;
  value: string;
  onChange: (to: string) => void;
  onPick?: (person: Person) => void;
  autoFocus?: boolean;
}) {
  const { inbox } = useInbox();
  const [focused, setFocused] = useState(false);
  const [active, setActive] = useState(0);

  const people = useMemo(() => {
    const seen = new Map<string, Person>();
    for (const mail of inbox?.mails ?? []) {
      const address = mail.from_address.toLowerCase();
      if (address && !seen.has(address)) {
        seen.set(address, {
          name: mail.from_name,
          address: mail.from_address,
          account_id: mail.account_id,
        });
      }
    }
    return [...seen.values()];
  }, [inbox]);

  const last = value.split(/[,;]/).at(-1) ?? "";
  const typed = last.trim().toLowerCase();
  const matches = typed
    ? people
        .filter(
          (p) => p.name.toLowerCase().includes(typed) || p.address.toLowerCase().includes(typed),
        )
        .slice(0, 5)
    : [];
  const shown = focused && matches.length > 0;

  const pick = (person: Person) => {
    const before = value.slice(0, value.length - last.length);
    onChange(`${before}${before ? " " : ""}${written(person)}, `);
    onPick?.(person);
    setActive(0);
  };

  return (
    <div className="relative min-w-0 flex-1">
      <input
        id={id}
        autoFocus={autoFocus}
        value={value}
        onChange={(event) => {
          onChange(event.target.value);
          setActive(0);
        }}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(event) => {
          if (!shown) return;
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setActive((i) => Math.min(i + 1, matches.length - 1));
          } else if (event.key === "ArrowUp") {
            event.preventDefault();
            setActive((i) => Math.max(i - 1, 0));
          } else if (event.key === "Enter" || event.key === "Tab") {
            const person = matches[active];
            if (!person || event.metaKey || event.ctrlKey) return;
            event.preventDefault();
            pick(person);
          }
        }}
        role="combobox"
        aria-expanded={shown}
        aria-controls={`${id}-people`}
        aria-activedescendant={shown ? `${id}-person-${active}` : undefined}
        autoComplete="off"
        spellCheck={false}
        placeholder="Name or address"
        className="compose-input"
      />
      {shown && (
        <div
          id={`${id}-people`}
          role="listbox"
          aria-label="People"
          className="ui-menu absolute top-full left-0 z-10 mt-1 w-full p-1.5"
        >
          {matches.map((person, index) => (
            <div
              key={person.address}
              id={`${id}-person-${index}`}
              role="option"
              aria-selected={index === active}
              tabIndex={-1}
              onMouseMove={() => setActive(index)}
              // Before the field's blur, so the pick lands.
              onMouseDown={(event) => {
                event.preventDefault();
                pick(person);
              }}
              className="ui-option flex h-9 cursor-default items-center gap-2.5 px-2.5"
            >
              <span className="av sm" aria-hidden="true">
                {initial(person.name)}
              </span>
              <span className="truncate">{person.name}</span>
              {person.name !== person.address && (
                <span className="truncate text-muted">{person.address}</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** A person as To reads them: "Anna Weber <anna@…>", quoted where the name needs it. */
function written({ name, address }: Party): string {
  if (!name || name.toLowerCase() === address.toLowerCase()) return address;
  const safe = /[",;<>@]/.test(name) ? `"${name.replaceAll('"', "")}"` : name;
  return `${safe} <${address}>`;
}

/** "Re: " or "Fwd: " the subject, unless it says so already (or AW:, WG:, Fw:). */
function prefixed(prefix: "Re" | "Fwd", subject: string): string {
  const said = subject.trim();
  const already = prefix === "Re" ? /^(re|aw):/i : /^(fwd?|wg):/i;
  return already.test(said) ? said : `${prefix}: ${said}`;
}
