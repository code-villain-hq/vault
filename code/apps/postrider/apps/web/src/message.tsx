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
import { Archive, ArrowLeft, FolderInput, Paperclip, Reply, Trash2 } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, messageOf } from "./api";
import { ReplyDialog } from "./compose";
import { forgetListed, markSeen, useListedMails } from "./folders";
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
} from "./inbox";
import { PageBody } from "./layout";
import { hue, useAccounts, useFolders, useInbox } from "./mail";
import { MoveDialog } from "./move";
import { isDesktop, openInBrowser, saveAndOpen } from "./platform";
import { ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/**
 * `/$workspace/mail/$account/$uid`: a mail, opened beside the list it was
 * opened from: the Inbox, a folder (`?folder=`) or a search (`?q=`). From
 * the Inbox it opens its whole conversation, oldest first. Esc closes it;
 * J and K, or ↓ and ↑, open the next and the previous one in the list,
 * while Space and the wheel scroll the mail; E archives it, ⌫ deletes it
 * and V moves it to a folder, the conversation's Inbox mail all at once,
 * and the next opens; R answers it, a conversation's last mail but yours.
 */
export function MailPage() {
  const { account, uid } = useParams({ strict: false }) as { account: string; uid: string };
  const from = useSearch({ strict: false }) as From;
  const { workspace, place, refusals } = useWorkspace();
  const { inbox, leave, stay } = useInbox();
  const { refresh } = useFolders();
  const accounts = useAccounts();
  const navigate = useNavigate();
  const back = useBack(from);
  const key = mailKey({ folder: from.folder, account_id: account, uid: Number(uid) });
  const listed = from.folder !== undefined || from.q !== undefined;
  const listedMails = useListedMails(from);
  const own = useMemo(() => ownAddresses(accounts), [accounts]);
  const folder = from.folder;
  const conversation = listed ? undefined : inbox?.conversations.find((c) => hasMail(c, key));
  // The Inbox steps by conversation, as its rows do; the other lists by mail.
  const mails = listed
    ? listedMails
    : (inbox?.conversations ?? []).flatMap((c) => anchorOf(c) ?? []);
  const at = conversation ? anchorOf(conversation) : undefined;
  const index = mails.findIndex((mail) => mailKey(mail) === (at ? mailKey(at) : key));
  const [replying, setReplying] = useState(false);
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
  // A conversation's answer goes to the last who wrote in it but you.
  const answered = conversation
    ? (conversation.mails.findLast((mail) => !isYours(mail, own)) ?? conversation.mails.at(-1))
    : summary;
  const answeredKey = answered && (conversation ? mailKey(answered) : key);
  const unfolded = useUnfolded(thread, letters, [key, answeredKey]);

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

  /**
   * Moves it to its account's Archive or Trash, or to a folder picked or
   * made, and opens the next mail at once, or closes it after the last. A
   * conversation's Inbox mail goes all together; what you sent of it stays
   * in Sent. A server that refuses says so in the frame's status, and the
   * mail it kept comes back to the Inbox.
   */
  const fileAway = (to: "archive" | "trash" | MoveMail) => {
    if (!summary) return;
    if ((to === "archive" && !archivable) || (to === "trash" && !deletable)) return;
    const going = conversation
      ? conversation.mails
          .filter((mail) => mail.folder === null)
          .map((mail) => ({ mail, folder: undefined }))
      : [{ mail: summary, folder }];
    const picked = typeof to === "object";
    void Promise.allSettled(
      going.map(({ mail, folder }) => {
        const fromInbox = (mail.folder ?? "inbox").toLowerCase() === "inbox";
        if (fromInbox) leave(mail.account_id, mail.uid);
        if (listed) forgetListed(from, key);
        const query = folder === undefined ? "" : `?${new URLSearchParams({ folder })}`;
        return api(
          "POST",
          `/api/w/${workspace.id}/mails/${mail.account_id}/${mail.uid}/${picked ? "move" : to}${query}`,
          picked ? to : undefined,
          place,
        ).catch((problem: unknown) => {
          if (fromInbox) stay(mail.account_id, mail.uid);
          throw problem;
        });
      }),
    ).then((done) => {
      refresh();
      const refused = done.find((one) => one.status === "rejected");
      if (refused) {
        refusals.dispatchEvent(new CustomEvent("refused", { detail: messageOf(refused.reason) }));
      }
    });
    const next = mails[index + 1] ?? mails[index - 1];
    if (index >= 0 && next) open(next);
    else back.go();
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
        setReplying(true);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

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
          {answered && (
            <ReaderButton icon={Reply} label="Reply (R)" onClick={() => setReplying(true)} />
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
        <ReplyDialog
          key={answeredKey}
          open={replying}
          onClose={() => setReplying(false)}
          summary={messages[answeredKey] ?? answered}
          message={messages[answeredKey]}
        />
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
        if (folder === undefined) return;
        if (from.current) markSeen(from.current, key);
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
  const params = { workspace: workspace.url_key };
  if (from.q !== undefined) {
    const search = { q: from.q };
    return {
      name: "Search",
      href: `/${workspace.url_key}/search?${new URLSearchParams(search)}`,
      go: () => void navigate({ to: "/$workspace/search", params, search }),
    };
  }
  if (from.sender !== undefined) {
    const sender = from.sender;
    return {
      name: listed[0]?.from_name || sender,
      href: `/${workspace.url_key}/newsletters/${encodeURIComponent(sender)}`,
      go: () =>
        void navigate({ to: "/$workspace/newsletters/$sender", params: { ...params, sender } }),
    };
  }
  const folder = from.folder;
  if (folder !== undefined) {
    const shown = folders.find((f) => f.key.toLowerCase() === folder.toLowerCase());
    return {
      name: shown?.name ?? folder,
      href: `/${workspace.url_key}/folders/${encodeURIComponent(folder)}`,
      go: () => void navigate({ to: "/$workspace/folders/$folder", params: { ...params, folder } }),
    };
  }
  return {
    name: "Inbox",
    href: `/${workspace.url_key}`,
    go: () => void navigate({ to: "/$workspace", params }),
  };
}

/** One of the open mail's tools, in its top bar. */
function ReaderButton({
  icon: Icon,
  label,
  onClick,
}: {
  icon: typeof Reply;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="ui-button ui-icon-button"
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
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

/** "820 bytes", "14 KB", "2.3 MB". */
function bytes(size: number): string {
  if (size < 1000) return `${size} bytes`;
  if (size < 1_000_000) return `${Math.round(size / 1000)} KB`;
  return `${(size / 1_000_000).toFixed(1)} MB`;
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
  const srcDoc = useMemo(() => paper(html), [html]);

  useEffect(() => {
    setSilent(false);
    const wait = setTimeout(() => setSilent(true), SILENT_AFTER);
    return () => clearTimeout(wait);
  }, [srcDoc]);

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow) return;
      const said = event.data as FrameMessage;
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
  }, []);

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
 * to show, it says it's empty.
 */
const REPORTER = `
const say = (message) => parent.postMessage(message, "*");
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
  const link = event.target instanceof Element && event.target.closest("a[href]");
  if (!link) return;
  event.preventDefault();
  say({ link: link.getAttribute("href") });
});
document.addEventListener("keydown", (event) => {
  const target = event.target;
  if (target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))) return;
  const plain = !(event.metaKey || event.ctrlKey || event.altKey);
  if (plain && (event.key === "ArrowDown" || event.key === "ArrowUp")) event.preventDefault();
  say({ key: event.key, metaKey: event.metaKey, ctrlKey: event.ctrlKey, altKey: event.altKey, shiftKey: event.shiftKey });
});`;

/**
 * The frame's document: the mail's HTML under a policy that runs only
 * Postrider's script (by a nonce the mail can't know) and lets nothing post
 * back, on white paper whatever the app's theme, as senders design for.
 * A mail that also offers a dark look (`color-scheme: light dark`) would
 * turn the paper dark at night and keep its own dark text: black on black.
 * So the paper stays light by a rule after the mail's, and the frame tells
 * the mail it's day (`.paper iframe` in the styles), so its dark rules sleep.
 * Newsletters often make html and body 100% tall, `!important`: in a frame
 * that grows to its content that is no height at all, so a rule after the
 * mail's, and surer than theirs, keeps them as tall as what they hold.
 */
function paper(html: string): string {
  const nonce = crypto.randomUUID();
  const body = html.replace(/<meta[^>]+http-equiv\s*=\s*["']?refresh[^>]*>/gi, "");
  return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-${nonce}'; img-src https: http: data:; style-src 'unsafe-inline' https:; font-src https: data:">
<meta name="referrer" content="no-referrer">
<meta name="color-scheme" content="light">
<base target="_blank">
<script nonce="${nonce}">${REPORTER}</script>
<style>
html { background: Canvas; color: CanvasText; height: auto !important; min-height: 0 !important; }
body { margin: 0; padding: 20px 24px; font: 14px/1.5 -apple-system, system-ui, sans-serif; overflow-wrap: break-word; height: auto !important; min-height: 0 !important; }
img { max-width: 100%; height: auto; }
</style></head><body>${body}
<style>${LAST_RULES}</style></body></html>`;
}
