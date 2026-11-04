// Writing mail, in a dialog over the page: C writes a new one, and on an
// open mail R replies, A replies to all and F forwards it. To, Cc and Bcc
// take addresses with commas between and suggest the people who wrote to
// you. The account it goes from sits beside the buttons; More professional
// writes it again with Claude Code on this Mac (`rewrite.rs`), in your
// language and with every fact kept, and Back to yours (or ⌘Z) puts your
// words back. It all goes to `POST /mails` (`send.rs`).

import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog, shortcutFree } from "@code-villain/web";
import type {
  Attachment,
  MailRef,
  MailSummary,
  Message,
  Outgoing,
  Party,
  Rewrite,
  Rewritten,
} from "@postrider/types";
import { ChevronDown, Paperclip, Sparkles, Undo2 } from "lucide-react";
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
import { hue, useAccounts, useInbox } from "./mail";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** How an open mail is answered: R, A or F. */
export type Answering = "reply" | "all" | "forward";

/** Whom it goes to and what about, as the lines above the text hold them. */
type Lines = { to: string; cc: string; bcc: string; subject: string };

const NO_LINES: Lines = { to: "", cc: "", bcc: "", subject: "" };

/**
 * What was written and not sent, by how it answers which mail, or "new":
 * Esc keeps it until the page reloads. `yours` is your text while
 * Postrider's stands in its place; `wroteTo` is the account the person a
 * new mail goes to wrote to.
 */
type Draft = Partial<Lines> & {
  text?: string;
  from?: string;
  yours?: string;
  wroteTo?: string;
};
const drafts = new Map<string, Draft>();

/** Keeps what changed, unless nothing is written and the lines are as they began. */
function keep(key: string, next: Draft, began: Lines = NO_LINES) {
  const draft = { ...drafts.get(key), ...next };
  const changed = (Object.keys(began) as (keyof Lines)[]).some(
    (line) => (draft[line] ?? began[line]).trim() !== began[line].trim(),
  );
  if (draft.text?.trim() || changed) drafts.set(key, draft);
  else drafts.delete(key);
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
      {how && (
        <AnswerForm key={how} how={how} summary={summary} message={message} onSent={onClose} />
      )}
    </CreateDialog>
  );
}

function AnswerForm({
  how,
  summary,
  message,
  onSent,
}: {
  how: Answering;
  summary: MailSummary;
  message: Message | undefined;
  onSent: () => void;
}) {
  const { workspace, place } = useWorkspace();
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
  const mail: MailRef = {
    account_id: summary.account_id,
    uid: summary.uid,
    folder: summary.folder ?? undefined,
  };

  const send = (text: string, from: string) => {
    const outgoing: Outgoing = {
      ...lines,
      from_account: from,
      text,
      answers: forwarding ? undefined : mail,
      forwards: forwarding ? mail : undefined,
    };
    return api("POST", `/api/w/${workspace.id}/mails`, outgoing, place);
  };

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
      send={send}
      onSent={onSent}
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
      {forwarding && message && message.attachments.length > 0 && (
        <FilesGoing files={message.attachments} />
      )}
    </Writer>
  );
}

/**
 * The lines an answer starts with. A reply goes to the mail's Reply-To,
 * else its sender; to all, also to whom else it went to and copied, but
 * you; both "Re: " its subject. A forward goes to no one yet, "Fwd: " it.
 */
function begin(
  how: Answering,
  summary: MailSummary,
  message: Message | undefined,
  own: Set<string>,
): Lines {
  if (how === "forward") return { ...NO_LINES, subject: prefixed("Fwd", summary.subject) };
  const answer: Party[] = message?.answer_to.length
    ? message.answer_to
    : [{ name: summary.from_name, address: summary.from_address }];
  const others = (people: Party[]) => people.filter((p) => !own.has(p.address.toLowerCase()));
  // To all: but you, unless it's you who asked for the answers.
  const to = how === "all" && others(answer).length > 0 ? others(answer) : answer;
  const seen = new Set(to.map((p) => p.address.toLowerCase()));
  const cc: Party[] = [];
  if (how === "all") {
    for (const person of others([...(message?.sent_to ?? []), ...(message?.copied ?? [])])) {
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
        <NewMailForm onSent={() => setOpen(false)} />
      </CreateDialog>
    </NewMailContext.Provider>
  );
}

function NewMailForm({ onSent }: { onSent: () => void }) {
  const { workspace, place } = useWorkspace();
  const accounts = useAccounts();
  const kept = drafts.get(NEW);
  const [lines, setLines] = useState<Lines>({ ...NO_LINES, ...kept });
  const [wroteTo, setWroteTo] = useState(kept?.wroteTo);
  const known = (id: string | undefined) => accounts.find((a) => a.id === id)?.id;
  const defaultFrom = known(lastFrom) ?? accounts[0]?.id ?? "";

  const send = (text: string, from: string) => {
    const mail: Outgoing = { ...lines, from_account: from, text };
    return api("POST", `/api/w/${workspace.id}/mails`, mail, place).then(() => {
      lastFrom = from;
    });
  };

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
      send={send}
      onSent={onSent}
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

/** The files a forward takes along, by name and size. */
function FilesGoing({ files }: { files: Attachment[] }) {
  const numbered = useMemo(() => files.map((file, at) => ({ ...file, at })), [files]);
  return (
    <ul className="files compose-files" aria-label="Files it takes along">
      {numbered.map((file) => (
        <li key={file.at}>
          <Paperclip size={14} strokeWidth={1.75} aria-hidden="true" className="text-muted" />
          <span className="truncate">{file.name}</span>
          <span className="text-faint tabular-nums">{bytes(file.size)}</span>
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
 * the lines `children` give: whom it goes to and what about.
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
  send,
  onSent,
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
  /** Sends `text` from the account `from`. */
  send: (text: string, from: string) => Promise<unknown>;
  onSent: () => void;
  children: ReactNode;
}) {
  const { workspace, place } = useWorkspace();
  const accounts = useAccounts();
  const kept = drafts.get(draft);
  const [text, setText] = useState(kept?.text ?? "");
  const [from, setFrom] = useState(kept?.from ?? suggestedFrom ?? defaultFrom);
  const [yours, setYours] = useState(kept?.yours);
  const [busy, setBusy] = useState<"sending" | "rewriting">();
  const [error, setError] = useState<string>();
  const sender = accounts.find((a) => a.id === from);
  const field = useRef<HTMLTextAreaElement>(null);
  const picked = useRef(kept?.from !== undefined);

  useEffect(() => {
    if (suggestedFrom && !picked.current) setFrom(suggestedFrom);
  }, [suggestedFrom]);

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

  const submit = () => {
    if (busy || !ready || (needsText && !text.trim())) return;
    setBusy("sending");
    setError(undefined);
    send(text, from).then(() => {
      drafts.delete(draft);
      onSent();
    }, done);
  };

  return (
    <form
      className="compose"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
      onKeyDown={(event) => {
        const command = event.metaKey || event.ctrlKey;
        if (event.key === "Enter" && command) {
          event.preventDefault();
          submit();
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
        readOnly={busy === "rewriting"}
        onChange={(event) => write(event.target.value)}
        className="compose-text"
      />
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
        {/* One button that turns into the other, so the focus stays on it. */}
        <Button
          variant="secondary"
          className="ml-auto"
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
          disabled={busy !== undefined || !ready || (needsText && !text.trim())}
          title="Send (⌘Enter)"
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
