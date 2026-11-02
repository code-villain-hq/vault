// Writing mail, in a dialog over the page: R replies to the mail that's
// open, C writes a new one. The account it goes from sits beside the
// buttons; More professional writes it again with Claude Code on this Mac
// (`rewrite.rs`), in your language and with every fact kept, and Back to
// yours (or ⌘Z) puts your words back.

import { Dialog } from "@base-ui/react/dialog";
import { CreateDialog, shortcutFree } from "@code-villain/web";
import type {
  MailRef,
  MailSummary,
  Message,
  NewMail,
  Reply,
  Rewrite,
  Rewritten,
} from "@postrider/types";
import { ChevronDown, Sparkles, Undo2 } from "lucide-react";
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
import { initial, mailKey } from "./inbox";
import { hue, useAccounts, useInbox } from "./mail";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/**
 * What was written and not sent, by the mail it answers, or "new": Esc
 * keeps it until the page reloads. `yours` is your text while Postrider's
 * stands in its place; `wroteTo` is the account the person a new mail goes
 * to wrote to.
 */
type Draft = {
  text?: string;
  from?: string;
  to?: string;
  subject?: string;
  yours?: string;
  wroteTo?: string;
};
const drafts = new Map<string, Draft>();

function keep(key: string, next: Draft) {
  const draft = { ...drafts.get(key), ...next };
  if (draft.text?.trim() || draft.to?.trim() || draft.subject?.trim()) drafts.set(key, draft);
  else drafts.delete(key);
}

/** A new mail's draft. */
const NEW = "new";

/** The account a new mail went from last, which the next one goes from too. */
let lastFrom: string | undefined;

/**
 * R: a reply to a mail. It goes to whom the mail asks replies to go, from
 * the account it came to unless you pick another; ⌘Enter sends it, and Esc
 * closes it with what you wrote kept.
 */
export function ReplyDialog({
  open,
  onClose,
  summary,
  message,
}: {
  open: boolean;
  onClose: () => void;
  summary: MailSummary;
  message: Message | undefined;
}) {
  return (
    <CreateDialog open={open} onClose={onClose} width="40rem">
      <ReplyForm summary={summary} message={message} onSent={onClose} />
    </CreateDialog>
  );
}

function ReplyForm({
  summary,
  message,
  onSent,
}: {
  summary: MailSummary;
  message: Message | undefined;
  onSent: () => void;
}) {
  const { workspace, place } = useWorkspace();
  const accounts = useAccounts();
  const mine = accounts.find((a) => a.id === summary.account_id);
  const to = message?.reply_to ?? summary.from_name;
  const subject = replySubject(summary.subject);
  const replying: MailRef = {
    account_id: summary.account_id,
    uid: summary.uid,
    folder: summary.folder ?? undefined,
  };

  const send = (text: string, from: string) => {
    const reply: Reply = {
      text,
      from_account: from === summary.account_id ? undefined : from,
      folder: summary.folder ?? undefined,
    };
    return api(
      "POST",
      `/api/w/${workspace.id}/mails/${summary.account_id}/${summary.uid}/reply`,
      reply,
      place,
    );
  };

  return (
    <Writer
      draft={mailKey(summary)}
      defaultFrom={summary.account_id}
      label="Your reply"
      about={{ to, subject, replying }}
      send={send}
      onSent={onSent}
    >
      <Dialog.Title className="sr-only">Reply to {to}</Dialog.Title>
      <div className="compose-line">
        <span className="compose-label">To</span>
        <span className="ui-chip compose-to" style={hue(mine?.hue ?? 1)}>
          <span className="av sm" aria-hidden="true">
            {initial(to)}
          </span>
          {to}
        </span>
      </div>
      <div className="compose-line">
        <span className="compose-label">Subject</span>
        <span className="truncate">{subject}</span>
      </div>
    </Writer>
  );
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
  const [to, setTo] = useState(kept?.to ?? "");
  const [subject, setSubject] = useState(kept?.subject ?? "");
  const [wroteTo, setWroteTo] = useState(kept?.wroteTo);
  const known = (id: string | undefined) => accounts.find((a) => a.id === id)?.id;
  const defaultFrom = known(lastFrom) ?? accounts[0]?.id ?? "";

  const send = (text: string, from: string) => {
    const mail: NewMail = { from_account: from, to, subject, text };
    return api("POST", `/api/w/${workspace.id}/mails`, mail, place).then(() => {
      lastFrom = from;
    });
  };

  return (
    <Writer
      draft={NEW}
      defaultFrom={defaultFrom}
      suggestedFrom={wroteTo}
      label="Your mail"
      about={{ to: to.trim() || undefined, subject: subject.trim() || undefined }}
      ready={to.trim() !== ""}
      send={send}
      onSent={onSent}
    >
      <Dialog.Title className="sr-only">New mail</Dialog.Title>
      <div className="compose-line">
        <label htmlFor="new-mail-to" className="compose-label">
          To
        </label>
        <ToField
          value={to}
          onChange={(next) => {
            setTo(next);
            keep(NEW, { to: next });
          }}
          onPick={(person) => {
            setWroteTo(person.account_id);
            keep(NEW, { wroteTo: person.account_id });
          }}
        />
      </div>
      <div className="compose-line">
        <label htmlFor="new-mail-subject" className="compose-label">
          Subject
        </label>
        <input
          id="new-mail-subject"
          value={subject}
          onChange={(event) => {
            setSubject(event.target.value);
            keep(NEW, { subject: event.target.value });
          }}
          className="compose-input"
        />
      </div>
    </Writer>
  );
}

/**
 * The text, the account it goes from, More professional and Send, under
 * the lines `children` give: whom it goes to and what about.
 */
function Writer({
  draft,
  defaultFrom,
  suggestedFrom,
  label,
  about,
  ready = true,
  send,
  onSent,
  children,
}: {
  draft: string;
  defaultFrom: string;
  /** The account the person it goes to wrote to: it goes from there, unless you pick one. */
  suggestedFrom?: string;
  label: string;
  /** What More professional is told besides your words. */
  about: Omit<Rewrite, "text" | "from_account">;
  /** Whether it has what it needs besides the text. */
  ready?: boolean;
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
    keep(draft, { text: next, yours: undefined });
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
        keep(draft, { text: rewritten.text, yours: asked.text });
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
    if (busy || !ready || !text.trim()) return;
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
        autoFocus={draft !== NEW}
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
                keep(draft, { from: event.target.value });
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
          disabled={busy !== undefined || !ready || !text.trim()}
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
 * To: addresses with commas between. What you type after the last one
 * finds the people who wrote to you, by name or address.
 */
function ToField({
  value,
  onChange,
  onPick,
}: {
  value: string;
  onChange: (to: string) => void;
  onPick: (person: Person) => void;
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
    onPick(person);
    setActive(0);
  };

  return (
    <div className="relative min-w-0 flex-1">
      <input
        id="new-mail-to"
        autoFocus
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
        aria-controls="new-mail-people"
        aria-activedescendant={shown ? `new-mail-person-${active}` : undefined}
        autoComplete="off"
        spellCheck={false}
        placeholder="Name or address"
        className="compose-input"
      />
      {shown && (
        <div
          id="new-mail-people"
          role="listbox"
          aria-label="People"
          className="ui-menu absolute top-full left-0 z-10 mt-1 w-full p-1.5"
        >
          {matches.map((person, index) => (
            <div
              key={person.address}
              id={`new-mail-person-${index}`}
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
function written({ name, address }: Person): string {
  if (!name || name.toLowerCase() === address.toLowerCase()) return address;
  const safe = /[",;<>@]/.test(name) ? `"${name.replaceAll('"', "")}"` : name;
  return `${safe} <${address}>`;
}

/** "Re: " the subject, unless it's a reply already. */
function replySubject(subject: string): string {
  return /^(re|aw):/i.test(subject.trim()) ? subject.trim() : `Re: ${subject.trim()}`;
}
