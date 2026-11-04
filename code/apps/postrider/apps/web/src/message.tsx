import {
  ago,
  apiUrl,
  authHeaders,
  credentialsFor,
  type Place,
  shortcutFree,
  UNKNOWN_PROBLEM,
} from "@code-villain/web";
import type { Attachment, MailSummary, Message, MoveMail } from "@postrider/types";
import { useNavigate, useParams, useSearch } from "@tanstack/react-router";
import {
  Archive,
  ArrowLeft,
  Flag,
  Forward,
  FolderInput,
  Mail,
  MailOpen,
  Paperclip,
  Reply,
  ReplyAll,
  Trash2,
} from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, messageOf } from "./api";
import { AnswerDialog, type Answering, bytes, DraftDialog } from "./compose";
import { markSeen, useListedMails } from "./folders";
import {
  anchorOf,
  type From,
  hasMail,
  initial,
  isYours,
  mailKey,
  openedFrom,
  ownAddresses,
  rememberOpened,
  type Row,
  rowsOf,
  unreadRow,
  useRowsShown,
} from "./inbox";
import { PageBody } from "./layout";
import { hue, useAccounts, useFolders, useInbox } from "./mail";
import { flaggedRow, flagToggle, readToggle, useMarkMail } from "./marks";
import { conversationGoing, MoveDialog, openList, useMoveMail } from "./move";
import { pageName } from "./newsletters";
import { isDesktop, openInBrowser, saveAndOpen } from "./platform";
import { useDoings, type Wanted, want } from "./sender";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/**
 * `/$workspace/mail/$account/$uid`: a mail, opened beside the list it was
 * opened from: the Inbox, a folder (`?folder=`) or a search (`?q=`). From
 * the Inbox it opens its whole conversation, oldest first. Esc closes it;
 * J and K, or ↓ and ↑, open the next and the previous one in the list,
 * while Space and the wheel scroll the mail; E archives it, ⌫ deletes it
 * and V moves it to a folder, the conversation's Inbox mail all at once,
 * and it closes on the list, at the next; R answers it, a conversation's
 * last mail but yours. ⇧U marks it unread or read, S flags it or not, a
 * conversation as its row in the Inbox. A newsletter, opened from its sender's list or not,
 * leads with Unsubscribe (U) and Keep in Inbox (I), done in its sender's
 * view (`sender.tsx`). Postrider opens no mail you didn't: opening one
 * marks it read.
 */
export function MailPage() {
  const { account, uid } = useParams({ strict: false }) as { account: string; uid: string };
  const from = useSearch({ strict: false }) as From;
  const { workspace } = useWorkspace();
  const { inbox } = useInbox();
  const moveMail = useMoveMail();
  const markMail = useMarkMail();
  const accounts = useAccounts();
  const navigate = useNavigate();
  const back = useBack(from);
  const key = mailKey({ folder: from.folder, account_id: account, uid: Number(uid) });
  const listed = from.folder !== undefined || from.q !== undefined || from.flagged === true;
  const listedMails = useListedMails(from);
  const own = useMemo(() => ownAddresses(accounts), [accounts]);
  const folder = from.folder;
  const conversation = listed ? undefined : inbox?.conversations.find((c) => hasMail(c, key));
  // The Inbox steps by conversation, as its rows do; the other lists by
  // mail; both through what the list's filter shows.
  const rows = useMemo(
    () => (listed ? rowsOf(listedMails) : rowsOf(undefined, inbox?.conversations ?? [])),
    [listed, listedMails, inbox],
  );
  const mails = useRowsShown(from, rows, key).map((row) => row.mail);
  const at = conversation ? anchorOf(conversation) : undefined;
  const index = mails.findIndex((mail) => mailKey(mail) === (at ? mailKey(at) : key));
  const [answering, setAnswering] = useState<Answering>();
  const [moving, setMoving] = useState(false);
  const letters: Letter[] = conversation
    ? conversation.mails.map((mail) => ({
        key: mailKey(mail),
        account: mail.account_id,
        uid: mail.uid,
        folder: mail.folder ?? undefined,
        mail,
      }))
    : [
        {
          key,
          account,
          uid: Number(uid),
          folder: from.folder,
          mail: (listed ? listedMails : (inbox?.mails ?? [])).find((m) => mailKey(m) === key),
        },
      ];
  const thread = letters[0]?.key ?? key;
  // What its server sent of each, kept while it shows.
  const [messages, setMessages] = useState<Record<string, Message>>({});
  const shown = useRef(new Set<string>());
  shown.current = new Set(letters.map((letter) => letter.key));
  const onRead = useCallback(
    (letter: string, message: Message) =>
      setMessages((was) => ({
        ...Object.fromEntries(Object.entries(was).filter(([k]) => shown.current.has(k))),
        [letter]: message,
      })),
    [],
  );
  // Its line in the Inbox shows at once; the rest comes from its server.
  const summary: MailSummary | undefined =
    messages[key] ?? letters.find((letter) => letter.key === key)?.mail;
  // A draft opens to write on, over the page; it isn't answered.
  const drafting = (summary?.folder ?? folder)?.toLowerCase() === "drafts";
  // A conversation's answer goes to the last who wrote in it but you.
  const answered = drafting
    ? undefined
    : conversation
      ? (conversation.mails.findLast((mail) => !isYours(mail, own)) ?? conversation.mails.at(-1))
      : summary;
  const answeredKey = answered && (conversation ? mailKey(answered) : key);
  const unfolded = useUnfolded(thread, letters, [key, answeredKey]);
  // Read or not, flagged or not, as its row in the list says now: a
  // conversation as its Inbox mail is.
  const live = letters.find((letter) => letter.key === key)?.mail ?? summary;
  const row: Row | undefined =
    conversation && at ? { mail: at, conversation } : live && { mail: live };
  const unread = row !== undefined && unreadRow(row);
  const flagged = row !== undefined && flaggedRow(row);

  useEffect(() => rememberOpened(key), [key]);

  const open = (mail: MailSummary | undefined) => {
    if (!mail) return;
    void navigate({
      to: "/$workspace/mail/$account/$uid",
      params: { workspace: workspace.url_key, account: mail.account_id, uid: String(mail.uid) },
      search: openedFrom(mail, from),
    });
  };

  // Mail in Archive stays there, and mail in Trash is deleted only by its server.
  const kept = summary?.folder?.toLowerCase();
  const archivable = summary !== undefined && kept !== "archive";
  const deletable = summary !== undefined && kept !== "trash";
  // A newsletter's sender, as its own view knows it.
  const newsletter = kept === "newsletters" ? (from.sender ?? summary?.from_address) : undefined;
  const doing = useDoings().get(newsletter?.toLowerCase() ?? "");
  const askable = newsletter !== undefined && !doing?.busy && !doing?.done && !doing?.wanted;

  /** Unsubscribes from its sender or keeps it in the Inbox, in the sender's view. */
  const ask = (wanted: Wanted) => {
    if (!newsletter || !askable) return;
    want(newsletter, wanted);
    void navigate({
      to: "/$workspace/newsletters/$sender",
      params: { workspace: workspace.url_key, sender: newsletter },
    });
  };

  /**
   * Moves it to its account's Archive or Trash, or to a folder picked or
   * made, and closes it on the list, at the next mail, unopened. A
   * conversation's Inbox mail goes all together; what you sent of it stays
   * in Sent. A server that refuses says so in the frame's status, and the
   * mail it kept comes back to the Inbox.
   */
  const fileAway = (to: "archive" | "trash" | MoveMail) => {
    if (!summary) return;
    if ((to === "archive" && !archivable) || (to === "trash" && !deletable)) return;
    moveMail(conversationGoing(conversation) ?? [{ mail: summary, folder }], to, from);
    const next = mails[index + 1] ?? mails[index - 1];
    if (index >= 0 && next) rememberOpened(mailKey(next));
    back.go();
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!shortcutFree(event)) return;
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      if (key === "Escape") {
        event.preventDefault();
        back.go();
      } else if (key === "j" || key === "k" || key === "ArrowDown" || key === "ArrowUp") {
        event.preventDefault();
        // Lists are newest first: J goes down, to older mail.
        if (index >= 0) open(mails[index + (key === "j" || key === "ArrowDown" ? 1 : -1)]);
      } else if (key === "e") {
        event.preventDefault();
        fileAway("archive");
      } else if (key === "Backspace" || key === "Delete") {
        event.preventDefault();
        fileAway("trash");
      } else if (key === "v" && summary) {
        event.preventDefault();
        setMoving(true);
      } else if (key === "r" && answered) {
        event.preventDefault();
        setAnswering("reply");
      } else if (key === "a" && answered && !event.defaultPrevented) {
        // G then A goes to Archive: the shortcuts took that A already.
        event.preventDefault();
        setAnswering("all");
      } else if (key === "u" && event.shiftKey && row) {
        event.preventDefault();
        markMail(readToggle(row));
      } else if (key === "s" && !event.shiftKey && !event.defaultPrevented && row) {
        // G then S goes to Sent: the shortcuts took that S already.
        event.preventDefault();
        markMail(flagToggle(row));
      } else if (
        (key === "u" || key === "i") &&
        !event.shiftKey &&
        newsletter &&
        !event.defaultPrevented
      ) {
        // G then I goes to the Inbox: the shortcuts took that I already.
        event.preventDefault();
        ask(key === "u" ? "unsubscribe" : "keep");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  // F forwards the open mail. It goes first, so the list beside it, whose
  // F picks what it shows, leaves it.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "f" || event.shiftKey || !answered) return;
      if (!shortcutFree(event)) return;
      event.preventDefault();
      setAnswering("forward");
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [answered]);

  const subject = summary?.subject || "(no subject)";
  return (
    <>
      <header
        className="app-topbar flex min-h-12 shrink-0 items-center gap-1 px-4"
        data-tauri-drag-region="deep"
      >
        {/* Only a narrow window, showing the mail in its list's place, needs a way back. */}
        <a
          href={back.href}
          onClick={(event) => {
            event.preventDefault();
            back.go();
          }}
          className="reader-back ui-button -ml-2 gap-1.5 px-2 text-xs text-muted"
          title={`Back to ${back.name} (Esc)`}
        >
          <ArrowLeft size={14} strokeWidth={1.75} aria-hidden="true" className="text-faint" />
          {back.name}
        </a>
        <div className="-mr-2 ml-auto flex items-center gap-1">
          {newsletter && (
            <>
              <Button
                disabled={!askable}
                title="Unsubscribe (U)"
                onClick={() => ask("unsubscribe")}
              >
                Unsubscribe
              </Button>
              <Button
                variant="secondary"
                className="mr-1"
                disabled={!askable}
                title="Keep in Inbox (I)"
                onClick={() => ask("keep")}
              >
                Keep in Inbox
              </Button>
            </>
          )}
          {answered && (
            <>
              <ReaderButton icon={Reply} label="Reply (R)" onClick={() => setAnswering("reply")} />
              <ReaderButton
                icon={ReplyAll}
                label="Reply all (A)"
                onClick={() => setAnswering("all")}
              />
              <ReaderButton
                icon={Forward}
                label="Forward (F)"
                onClick={() => setAnswering("forward")}
              />
            </>
          )}
          {row && !drafting && (
            <>
              <ReaderButton
                icon={unread ? MailOpen : Mail}
                label={unread ? "Mark as read (⇧U)" : "Mark as unread (⇧U)"}
                onClick={() => markMail(readToggle(row))}
              />
              <ReaderButton
                icon={Flag}
                label={flagged ? "Unflag (S)" : "Flag (S)"}
                on={flagged}
                onClick={() => markMail(flagToggle(row))}
              />
            </>
          )}
          {archivable && (
            <ReaderButton icon={Archive} label="Archive (E)" onClick={() => fileAway("archive")} />
          )}
          {summary && (
            <ReaderButton icon={FolderInput} label="Move to… (V)" onClick={() => setMoving(true)} />
          )}
          {deletable && (
            <ReaderButton icon={Trash2} label="Delete (⌫)" onClick={() => fileAway("trash")} />
          )}
        </div>
      </header>
      <PageBody>
        <article className="letter" aria-label={subject}>
          {summary && <h2 className="text-title font-semibold">{subject}</h2>}
          {letters.map((letter) => (
            <Said
              key={letter.key}
              letter={letter}
              summary={messages[letter.key] ?? letter.mail}
              message={messages[letter.key]}
              folded={letters.length > 1 && !unfolded.has(letter.key)}
              onToggle={letters.length > 1 ? () => unfolded.flip(letter.key) : undefined}
              onRead={onRead}
              list={listed ? from : undefined}
            />
          ))}
        </article>
      </PageBody>
      {answered && answeredKey && (
        <AnswerDialog
          key={answeredKey}
          how={answering}
          onClose={() => setAnswering(undefined)}
          summary={messages[answeredKey] ?? answered}
          message={messages[answeredKey]}
        />
      )}
      {drafting && (
        <DraftDialog key={key} account={account} uid={Number(uid)} onClose={() => back.go()} />
      )}
      <MoveDialog
        open={moving}
        onOpenChange={setMoving}
        from={summary?.folder ?? "inbox"}
        onMove={(to) => fileAway({ to })}
      />
    </>
  );
}

/** A mail on the page: where it is, and its line in its list once that's known. */
type Letter = {
  key: string;
  account: string;
  uid: number;
  folder: string | undefined;
  mail: MailSummary | undefined;
};

/**
 * Which of a conversation's mail shows whole: its last, what was unread
 * when it opened, and the `kept` (the one opened, the one R answers);
 * the rest is a line each, and a click on one turns it.
 */
function useUnfolded(
  thread: string,
  letters: Letter[],
  kept: (string | undefined)[],
): { has: (key: string) => boolean; flip: (key: string) => void } {
  const unread = () =>
    new Set(letters.filter((l) => l.mail?.folder === null && !l.mail.seen).map((l) => l.key));
  const [state, setState] = useState(() => ({
    thread,
    unread: unread(),
    flipped: new Set<string>(),
  }));
  // Another conversation starts as it opens; read now, its mail stays open.
  if (state.thread !== thread) setState({ thread, unread: unread(), flipped: new Set() });
  const last = letters.at(-1)?.key;
  return {
    has: (key) =>
      (key === last || kept.includes(key) || state.unread.has(key)) !== state.flipped.has(key),
    flip: (key) =>
      setState((was) => {
        const flipped = new Set(was.flipped);
        if (!flipped.delete(key)) flipped.add(key);
        return { ...was, flipped };
      }),
  };
}

/**
 * A mail of those open: who wrote it, to whom and when, then its files and
 * its words, read from its server once it shows whole, which marks it read.
 * Folded, a conversation's mail is one line: who and how it starts.
 */
function Said({
  letter,
  summary,
  message,
  folded,
  onToggle,
  onRead,
  list,
}: {
  letter: Letter;
  summary: MailSummary | undefined;
  message: Message | undefined;
  folded: boolean;
  onToggle: (() => void) | undefined;
  onRead: (key: string, message: Message) => void;
  /** The list it was opened from, but the Inbox. */
  list: From | undefined;
}) {
  const { workspace, place } = useWorkspace();
  const { refresh } = useFolders();
  const accounts = useAccounts();
  const [error, setError] = useState<string>();
  const { key, account, uid, folder } = letter;
  // Read now: its folder's unread count goes down once it's open.
  const unread = useRef(false);
  unread.current = summary?.seen === false;
  const from = useRef(list);
  from.current = list;
  const wanted = !folded && message === undefined;

  useEffect(() => {
    if (!wanted) return;
    const stop = new AbortController();
    const inFolder = folder === undefined ? "" : `?${new URLSearchParams({ folder })}`;
    setError(undefined);
    api<Message>(
      "GET",
      `/api/w/${workspace.id}/mails/${account}/${uid}${inFolder}`,
      undefined,
      place,
      stop.signal,
    ).then(
      (answer) => {
        onRead(key, answer);
        if (from.current) markSeen(from.current, key);
        if (folder === undefined) return;
        if (unread.current) refresh();
      },
      (problem: unknown) => {
        if (!stop.signal.aborted) setError(messageOf(problem));
      },
    );
    return () => stop.abort();
  }, [wanted, workspace.id, place, account, uid, key, folder, onRead, refresh]);

  const sender = accounts.find((a) => a.id === account);
  if (!summary) return error ? <ErrorMessage>{error}</ErrorMessage> : null;
  const when = (
    <time
      className="ml-auto shrink-0 text-xs text-muted"
      dateTime={summary.date}
      title={new Date(summary.date).toLocaleString()}
    >
      {ago(summary.date)}
    </time>
  );
  if (folded) {
    return (
      <button type="button" className="folded" style={hue(sender?.hue ?? 1)} onClick={onToggle}>
        <span className="av sm" aria-hidden="true">
          {initial(summary.from_name)}
        </span>
        <b className="name font-semibold">{summary.from_name}</b>
        <span className="min-w-0 truncate text-faint">{summary.snippet}</span>
        {when}
      </button>
    );
  }
  const head = (
    <>
      <span className="av" aria-hidden="true">
        {initial(summary.from_name)}
      </span>
      <span className="min-w-0">
        <span className="block truncate">
          <b className="font-semibold">{summary.from_name}</b>
          {summary.from_address !== summary.from_name && (
            <span className="text-muted"> {summary.from_address}</span>
          )}
        </span>
        {message && message.to.length > 0 && (
          <span className="block truncate text-xs text-muted">to {message.to.join(", ")}</span>
        )}
      </span>
      {when}
    </>
  );
  return (
    <section className="said" aria-label={`From ${summary.from_name}`}>
      {onToggle ? (
        <button
          type="button"
          className="from"
          style={hue(sender?.hue ?? 1)}
          onClick={onToggle}
          title="Fold"
        >
          {head}
        </button>
      ) : (
        <div className="from" style={hue(sender?.hue ?? 1)}>
          {head}
        </div>
      )}
      {error && <ErrorMessage>{error}</ErrorMessage>}
      {message && message.attachments.length > 0 && (
        <Attachments
          files={message.attachments}
          path={`/api/w/${workspace.id}/mails/${account}/${uid}/files`}
          folder={folder}
          place={place}
        />
      )}
      {message &&
        (message.html?.trim() ? (
          <MailHtml html={message.html} instead={<MailText text={message.text} />} />
        ) : (
          <MailText text={message.text} />
        ))}
    </section>
  );
}

/** Where Esc and the back button go: the list the mail was opened from. */
function useBack(from: From): { name: string; href: string; go: () => void } {
  const { workspace } = useWorkspace();
  const { folders } = useFolders();
  const navigate = useNavigate();
  const listed = useListedMails(from);
  const go = () => openList(navigate, workspace.url_key, from);
  if (from.flagged) return { name: "Flagged", href: `/${workspace.url_key}/flagged`, go };
  if (from.q !== undefined) {
    const search = { q: from.q };
    return {
      name: "Search",
      href: `/${workspace.url_key}/search?${new URLSearchParams(search)}`,
      go,
    };
  }
  if (from.sender !== undefined) {
    const sender = from.sender;
    return {
      name: pageName(workspace.id, sender) ?? (listed[0]?.from_name || sender),
      href: `/${workspace.url_key}/newsletters/${encodeURIComponent(sender)}`,
      go,
    };
  }
  const folder = from.folder;
  if (folder !== undefined) {
    const shown = folders.find((f) => f.key.toLowerCase() === folder.toLowerCase());
    return {
      name: shown?.name ?? folder,
      href: `/${workspace.url_key}/folders/${encodeURIComponent(folder)}`,
      go,
    };
  }
  return { name: "Inbox", href: `/${workspace.url_key}`, go };
}

/** One of the open mail's tools, in its top bar. */
function ReaderButton({
  icon: Icon,
  label,
  on,
  onClick,
}: {
  icon: typeof Reply;
  label: string;
  /** A mark the mail has, as a flag: filled, in its colour. */
  on?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`ui-button ui-icon-button ${on ? "text-warn" : ""}`}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      <Icon size={16} strokeWidth={1.75} fill={on ? "currentColor" : "none"} aria-hidden="true" />
    </button>
  );
}

/**
 * The files that came with a mail, by name and size. One clicked opens: in
 * the desktop app it's saved in Downloads and opened there; in a browser,
 * downloaded. One that can't be read says why under them.
 */
function Attachments({
  files,
  path,
  folder,
  place,
}: {
  files: Attachment[];
  path: string;
  folder: string | undefined;
  place: Place;
}) {
  const [error, setError] = useState<string>();
  // The server knows a file by its place among them.
  const numbered = useMemo(() => files.map((file, at) => ({ ...file, at })), [files]);
  const open = (index: number, name: string) => {
    setError(undefined);
    const inFolder = folder === undefined ? "" : `?${new URLSearchParams({ folder })}`;
    openFile(apiUrl(`${path}/${index}${inFolder}`, place), place, name).catch((problem: unknown) =>
      setError(messageOf(problem)),
    );
  };
  return (
    <div className="grid gap-2">
      <ul className="files" aria-label="Attachments">
        {numbered.map((file) => (
          <li key={file.at}>
            <button
              type="button"
              className="min-w-0"
              title={`Open ${file.name}`}
              onClick={() => open(file.at, file.name)}
            >
              <Paperclip size={14} strokeWidth={1.75} aria-hidden="true" className="text-muted" />
              <span className="truncate">{file.name}</span>
              <span className="text-faint tabular-nums">{bytes(file.size)}</span>
            </button>
          </li>
        ))}
      </ul>
      {error && <ErrorMessage>{error}</ErrorMessage>}
    </div>
  );
}

/** Reads a mail's file from the server and opens it, or downloads it. */
async function openFile(url: string, place: Place, name: string) {
  const response = await fetch(url, {
    credentials: credentialsFor(place),
    headers: authHeaders(place),
  });
  if (!response.ok) {
    const said = (await response.json().catch(() => null)) as { message?: string } | null;
    throw new Error(said?.message ?? UNKNOWN_PROBLEM);
  }
  const content = new Uint8Array(await response.arrayBuffer());
  if (isDesktop) {
    await saveAndOpen(name, content);
    return;
  }
  const link = document.createElement("a");
  link.href = URL.createObjectURL(
    new Blob([content], { type: response.headers.get("content-type") ?? "" }),
  );
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(link.href), 60_000);
}

const URLS = /\bhttps?:\/\/[^\s<>"]*[^\s<>".,;:!?)\]'}*]/g;

/** A mail's text with its addresses as links. */
function linked(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  let at = 0;
  for (const match of text.matchAll(URLS)) {
    parts.push(text.slice(at, match.index));
    parts.push(
      <a key={match.index} href={match[0]} target="_blank" rel="noreferrer" className="ui-link">
        {match[0]}
      </a>,
    );
    at = match.index + match[0].length;
  }
  parts.push(text.slice(at));
  return parts;
}

/** A mail's text, or that it has none to show. */
function MailText({ text }: { text: string }) {
  if (!text.trim()) return <p className="letter-none">Postrider can't show this mail's text.</p>;
  return <div className="letter-text">{linked(text)}</div>;
}

/**
 * A mail's HTML, on paper of its own: in a frame of no origin, where none of
 * the mail's scripts, forms or pop-ups run, that loads pictures and styles
 * but sends no referrer. One script of Postrider's runs there and tells the
 * page its height, its links and its keys: the page can't listen in a
 * frame of no origin, and WebKit runs no listener in a frame without
 * scripts. So it grows to its content and the page scrolls as one, its
 * links open in the browser, and Esc, J, K and the arrows work while it
 * has focus: the arrows move to the next mail there too, not down the page.
 * A frame 0 tall, or one that says no height in time (`SILENT_AFTER`), is
 * still shown, and scrolls, until it says more. One that says it shows
 * nothing, no text and no picture, gives way to `instead`.
 */
function MailHtml({ html, instead }: { html: string; instead: ReactNode }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState<number>();
  const [silent, setSilent] = useState(false);
  const [emptyOne, setEmptyOne] = useState<string>();
  const { srcDoc, token } = useMemo(() => paper(html), [html]);

  useEffect(() => {
    setSilent(false);
    const wait = setTimeout(() => setSilent(true), SILENT_AFTER);
    return () => clearTimeout(wait);
  }, [srcDoc]);

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow) return;
      const said = fromFrame(event.data, token);
      if (!said) return;
      if (typeof said.height === "number") {
        setHeight(said.height);
      } else if (said.empty === true) {
        setEmptyOne(frame.current.srcdoc);
      } else if (typeof said.link === "string") {
        // Only a whole address: the frame's relative ones would be the app's.
        const link = said.link.trim();
        if (!/^(https?|mailto):/i.test(link)) return;
        if (isDesktop) void openInBrowser(link);
        else window.open(link, "_blank", "noopener,noreferrer");
      } else if (typeof said.key === "string") {
        document.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: said.key,
            metaKey: said.metaKey === true,
            ctrlKey: said.ctrlKey === true,
            altKey: said.altKey === true,
            shiftKey: said.shiftKey === true,
            bubbles: true,
            cancelable: true,
          }),
        );
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [token]);

  if (emptyOne === srcDoc) return instead;
  return (
    <div className="paper">
      <iframe
        ref={frame}
        title="The mail"
        sandbox="allow-scripts"
        srcDoc={srcDoc}
        style={{ height: height || (height === 0 || silent ? "70vh" : 0) }}
      />
    </div>
  );
}

/** How long a mail's frame has to say its height, in milliseconds. */
const SILENT_AFTER = 2000;

/** What the frame's script tells the page. */
type FrameMessage = {
  token?: unknown;
  height?: unknown;
  empty?: unknown;
  link?: unknown;
  key?: unknown;
  metaKey?: unknown;
  ctrlKey?: unknown;
  altKey?: unknown;
  shiftKey?: unknown;
};

/**
 * What the frame says, if Postrider's script said it: a page the frame was
 * sent to, which could post a key that deletes the mail or a link to open,
 * doesn't know the render's token.
 */
export function fromFrame(data: unknown, token: string): FrameMessage | undefined {
  if (typeof data !== "object" || data === null) return undefined;
  const said = data as FrameMessage;
  return said.token === token ? said : undefined;
}

/**
 * The rules that keep the paper light and the mail as tall as it is, after
 * the mail's own: at the end of the frame's document, and again once it's
 * read, since a comment or title the mail leaves open swallows the first.
 */
const LAST_RULES = `html:root { color-scheme: only light !important; }
html:root, html:root > body { height: auto !important; min-height: 0 !important; max-height: none !important; }`;

/**
 * The frame's script: its height, its links and its keys, to the page. A
 * mail that makes its html or body as tall as the frame in their own
 * `style`, `!important`, beats any rule, and a frame that grows to its
 * content is then 0 tall: once it's read, that height goes. One as tall
 * as the frame and more (`100vh`) would grow with it for ever: when it
 * grew just as much as the frame did, the frame stays. A mail that keeps
 * itself in a box as tall as the frame, which scrolls or clips, is as tall
 * as what that box holds, if the box grows with the frame: one that didn't
 * is the mail's own size, and its height stays. Nothing is said before the
 * mail is read, so a 0 is the mail's. Loaded with no text and no picture
 * to show, it says it's empty. Everything it says carries the render's
 * token, which comes on its own tag: it takes itself out of the document
 * before the mail is read, so the mail's styles can't read it.
 *
 * The text is the same for every mail. A frame written from `srcDoc` has
 * the window's Content-Security-Policy as well as its own, and the window's
 * (`tauri.conf.json`) can allow an inline script only by its hash, which
 * is `REPORTER_HASH`.
 */
export const REPORTER = `
const token = document.currentScript.dataset.token;
document.currentScript.remove();
const say = (message) => parent.postMessage({ ...message, token }, "*");
const boxes = new Map();
const reach = (height) => {
  let most = height;
  if (!document.body || innerHeight === 0) return most;
  for (const box of [document.body, ...document.body.getElementsByTagName("*")]) {
    const seen = boxes.get(box);
    if (seen === false) continue;
    if (seen === undefined) {
      if (box.scrollHeight <= box.clientHeight + 1 || getComputedStyle(box).overflowY === "visible") continue;
      const height = box.getBoundingClientRect().height;
      if (height < innerHeight - 1) continue;
      boxes.set(box, { height, frame: innerHeight });
    } else if (seen !== true && seen.frame !== innerHeight) {
      const follows = Math.abs(box.getBoundingClientRect().height - seen.height - (innerHeight - seen.frame)) <= 1;
      boxes.set(box, follows);
      if (!follows) continue;
    }
    const more = box.scrollHeight - box.clientHeight;
    const loose = /^(absolute|fixed)$/.test(getComputedStyle(box).position);
    most = Math.max(most, loose ? Math.ceil(box.getBoundingClientRect().bottom + scrollY) + more : height + more);
  }
  return most;
};
let was;
const tall = () => {
  if (document.readyState === "loading") return;
  const now = { height: reach(Math.ceil(document.documentElement.getBoundingClientRect().height)), frame: innerHeight };
  if (was && now.height === was.height && now.frame === was.frame) return;
  const grew = was ? now.frame - was.frame : 0;
  const following = grew !== 0 && now.height - was.height === grew && now.height > now.frame;
  was = now;
  if (!following) say({ height: now.height });
};
new ResizeObserver(tall).observe(document.documentElement);
addEventListener("DOMContentLoaded", () => {
  for (const pinned of [document.documentElement, document.body]) {
    for (const rule of ["height", "min-height", "max-height"]) pinned.style.removeProperty(rule);
  }
  const last = document.createElement("style");
  last.textContent = ${JSON.stringify(LAST_RULES)};
  document.body.append(last);
  tall();
});
addEventListener("resize", tall);
const seen = { visibilityProperty: true, opacityProperty: true, checkVisibilityCSS: true, checkOpacity: true };
const shows = () => {
  if (typeof document.body.checkVisibility !== "function") return true;
  const texts = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let text = texts.nextNode(); text; text = texts.nextNode()) {
    if (text.data.trim() && text.parentElement?.checkVisibility(seen)) return true;
  }
  return [...document.body.querySelectorAll("img, svg, video, canvas, picture, object, embed, input, button, [background], [style*='url(']")].some((picture) => {
    const { width, height } = picture.getBoundingClientRect();
    return width > 2 && height > 2 && picture.checkVisibility(seen);
  });
};
addEventListener("load", () => {
  tall();
  if (!shows()) say({ empty: true });
});
document.addEventListener("click", (event) => {
  const link = event.target instanceof Element && event.target.closest("a[*|href], area[href]");
  if (!link) return;
  event.preventDefault();
  say({ link: link.getAttribute("href") ?? link.getAttributeNS("http://www.w3.org/1999/xlink", "href") });
});
document.addEventListener("keydown", (event) => {
  const target = event.target;
  if (target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))) return;
  const plain = !(event.metaKey || event.ctrlKey || event.altKey);
  if (plain && (event.key === "ArrowDown" || event.key === "ArrowUp")) event.preventDefault();
  say({ key: event.key, metaKey: event.metaKey, ctrlKey: event.ctrlKey, altKey: event.altKey, shiftKey: event.shiftKey });
});`;

/** `REPORTER`'s sha256, the one script either policy lets the frame run. */
export const REPORTER_HASH = "sha256-O+uR5pDQJzYbLosPZckeQ10SR4FkJhyZUjHB27wzXcc=";

/**
 * The frame's document: the mail's HTML under a policy that runs only
 * Postrider's script (by its hash, which no other script has) and lets
 * nothing post back, on white paper whatever the app's theme, as senders design for.
 * A mail that also offers a dark look (`color-scheme: light dark`) would
 * turn the paper dark at night and keep its own dark text: black on black.
 * So the paper stays light by a rule after the mail's, and the frame tells
 * the mail it's day (`.paper iframe` in the styles), so its dark rules sleep.
 * Newsletters often make html and body 100% tall, `!important`: in a frame
 * that grows to its content that is no height at all, so a rule after the
 * mail's, and surer than theirs, keeps them as tall as what they hold.
 * `token` is what the frame's script signs what it says with.
 */
export function paper(html: string): { srcDoc: string; token: string } {
  const token = crypto.randomUUID();
  const body = withoutRefresh(html);
  const srcDoc = `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src '${REPORTER_HASH}'; img-src https: http: data:; style-src 'unsafe-inline' https:; font-src https: data:">
<meta name="referrer" content="no-referrer">
<meta name="color-scheme" content="light">
<base target="_blank">
<script data-token="${token}">${REPORTER}</script>
<style>
html { background: Canvas; color: CanvasText; height: auto !important; min-height: 0 !important; }
body { margin: 0; padding: 20px 24px; font: 14px/1.5 -apple-system, system-ui, sans-serif; overflow-wrap: break-word; height: auto !important; min-height: 0 !important; }
img { max-width: 100%; height: auto; }
</style></head><body>${body}
<style>${LAST_RULES}</style></body></html>`;
  return { srcDoc, token };
}

/**
 * The mail without a `<meta http-equiv="refresh">`, which would send the
 * frame to a page of the sender's. It's found as the browser reads the
 * mail, entities and quoted `>` and all. A `noscript` goes too: the frame
 * runs a script, so it shows none, but DOMParser reads as if scripts were
 * off, and could find only text in one where the frame finds a meta. A
 * mail with neither is left as it came.
 */
function withoutRefresh(html: string): string {
  const mail = new DOMParser().parseFromString(html, "text/html");
  const refreshes = [...mail.querySelectorAll("meta[http-equiv]")].filter(
    (meta) => meta.getAttribute("http-equiv")?.trim().toLowerCase() === "refresh",
  );
  const unseen = [...mail.querySelectorAll("noscript")];
  if (refreshes.length === 0 && unseen.length === 0) return html;
  for (const element of [...refreshes, ...unseen]) element.remove();
  return mail.documentElement.outerHTML;
}
