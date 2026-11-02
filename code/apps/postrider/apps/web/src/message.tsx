import { ago, shortcutFree } from "@code-villain/web";
import type { Attachment, MailSummary, Message } from "@postrider/types";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { ArrowLeft, ChevronDown, ChevronUp, type LucideIcon, Paperclip } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { api, messageOf } from "./api";
import { listedMails, markSeen } from "./folders";
import { type From, initial, mailKey, openedFrom, rememberOpened } from "./inbox";
import { PageBody, PageHeader } from "./layout";
import { hue, useAccounts, useFolders, useInbox } from "./mail";
import { isDesktop, openInBrowser } from "./platform";
import { ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/**
 * `/$workspace/mail/$account/$uid`: a mail, opened in the place of the list
 * it was opened from: the Inbox, a folder (`?folder=`) or a search (`?q=`).
 * Esc goes back to it; J and K open the next and the previous one in it.
 */
export function MailPage() {
  const { account, uid } = useParams({ strict: false }) as { account: string; uid: string };
  const from = useSearch({ strict: false }) as From;
  const { workspace, place } = useWorkspace();
  const { inbox } = useInbox();
  const accounts = useAccounts();
  const navigate = useNavigate();
  const back = useBack(from);
  const key = mailKey({ folder: from.folder, account_id: account, uid: Number(uid) });
  const listed = from.folder !== undefined || from.q !== undefined;
  const mails = listed ? listedMails(from) : (inbox?.mails ?? []);
  const index = mails.findIndex((mail) => mailKey(mail) === key);
  const [opened, setOpened] = useState<{ key: string; message?: Message; error?: string }>();
  const message = opened?.key === key ? opened.message : undefined;
  const error = opened?.key === key ? opened.error : undefined;
  // Its line in the Inbox shows at once; the rest comes from its server.
  const summary: MailSummary | undefined = message ?? mails[index];

  const folder = from.folder;
  const q = from.q;
  useEffect(() => {
    rememberOpened(key);
    const stop = new AbortController();
    const inFolder = folder === undefined ? "" : `?${new URLSearchParams({ folder })}`;
    api<Message>(
      "GET",
      `/api/w/${workspace.id}/mails/${account}/${uid}${inFolder}`,
      undefined,
      place,
      stop.signal,
    ).then(
      (answer) => {
        setOpened({ key, message: answer });
        if (folder !== undefined) markSeen({ folder, q }, key);
      },
      (problem: unknown) => {
        if (!stop.signal.aborted) setOpened({ key, error: messageOf(problem) });
      },
    );
    return () => stop.abort();
  }, [workspace.id, place, account, uid, key, folder, q]);

  useEffect(() => {
    const open = (mail: MailSummary | undefined) => {
      if (!mail) return;
      void navigate({
        to: "/$workspace/mail/$account/$uid",
        params: { workspace: workspace.url_key, account: mail.account_id, uid: String(mail.uid) },
        search: openedFrom(mail, from),
      });
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (!shortcutFree(event)) return;
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      if (key === "Escape") {
        event.preventDefault();
        back.go();
      } else if (key === "j" || key === "k") {
        event.preventDefault();
        // Lists are newest first: J goes down, to older mail.
        if (index >= 0) open(mails[index + (key === "j" ? 1 : -1)]);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  const subject = summary?.subject || "(no subject)";
  const sender = accounts.find((a) => a.id === summary?.account_id);
  return (
    <>
      <PageHeader
        title={summary ? subject : "Mail"}
        back={
          <a
            href={back.href}
            onClick={(event) => {
              event.preventDefault();
              back.go();
            }}
            className="ui-button -ml-2 gap-1.5 px-2 text-xs text-muted"
            title={`Back to ${back.name} (Esc)`}
          >
            <ArrowLeft size={14} strokeWidth={1.75} aria-hidden="true" className="text-faint" />
            {back.name}
          </a>
        }
      >
        {index >= 0 && (
          <>
            <span className="shrink-0 text-xs whitespace-nowrap text-muted tabular-nums">
              {index + 1} of {mails.length}
            </span>
            <div className="-mr-2 flex">
              <MailStep
                mail={mails[index - 1]}
                from={from}
                icon={ChevronUp}
                label="Previous mail (K)"
              />
              <MailStep
                mail={mails[index + 1]}
                from={from}
                icon={ChevronDown}
                label="Next mail (J)"
              />
            </div>
          </>
        )}
      </PageHeader>
      <PageBody>
        <article className="letter" aria-label={subject}>
          {summary && (
            <header className="letter-head">
              <p className="text-title font-semibold">{subject}</p>
              <div className="from" style={hue(sender?.hue ?? 1)}>
                <span className="av" aria-hidden="true">
                  {initial(summary.from_name)}
                </span>
                <div className="min-w-0">
                  <p className="truncate">
                    <b className="font-semibold">{summary.from_name}</b>
                    {summary.from_address !== summary.from_name && (
                      <span className="text-muted"> {summary.from_address}</span>
                    )}
                  </p>
                  {message && message.to.length > 0 && (
                    <p className="truncate text-xs text-muted">to {message.to.join(", ")}</p>
                  )}
                </div>
                <time
                  className="ml-auto shrink-0 text-xs text-muted"
                  dateTime={summary.date}
                  title={new Date(summary.date).toLocaleString()}
                >
                  {ago(summary.date)}
                </time>
              </div>
            </header>
          )}
          {error && <ErrorMessage>{error}</ErrorMessage>}
          {message && message.attachments.length > 0 && <Attachments files={message.attachments} />}
          {message &&
            (message.html ? (
              <MailHtml html={message.html} />
            ) : (
              <div className="letter-text">{linked(message.text)}</div>
            ))}
        </article>
      </PageBody>
    </>
  );
}

/** Where Esc and the back button go: the list the mail was opened from. */
function useBack(from: From): { name: string; href: string; go: () => void } {
  const { workspace } = useWorkspace();
  const { folders } = useFolders();
  const navigate = useNavigate();
  const params = { workspace: workspace.url_key };
  if (from.q !== undefined) {
    const search = { q: from.q };
    return {
      name: "Search",
      href: `/${workspace.url_key}/search?${new URLSearchParams(search)}`,
      go: () => void navigate({ to: "/$workspace/search", params, search }),
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

/** An arrow to the mail above or below this one in its list, if there is one. */
function MailStep({
  mail,
  from,
  icon: Icon,
  label,
}: {
  mail: MailSummary | undefined;
  from: From;
  icon: LucideIcon;
  label: string;
}) {
  const { workspace } = useWorkspace();
  const arrow = <Icon size={16} strokeWidth={1.75} aria-hidden="true" />;
  if (!mail) {
    return (
      <button type="button" className="ui-button ui-icon-button" aria-label={label} disabled>
        {arrow}
      </button>
    );
  }
  return (
    <Link
      to="/$workspace/mail/$account/$uid"
      params={{ workspace: workspace.url_key, account: mail.account_id, uid: String(mail.uid) }}
      search={openedFrom(mail, from)}
      className="ui-button ui-icon-button"
      aria-label={label}
      title={label}
    >
      {arrow}
    </Link>
  );
}

/** The files that came with a mail, by name and size. */
function Attachments({ files }: { files: Attachment[] }) {
  return (
    <ul className="files" aria-label="Attachments">
      {files.map((file) => (
        <li key={`${file.name}:${file.size}`}>
          <Paperclip size={14} strokeWidth={1.75} aria-hidden="true" className="text-muted" />
          <span className="truncate">{file.name}</span>
          <span className="text-faint tabular-nums">{bytes(file.size)}</span>
        </li>
      ))}
    </ul>
  );
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

/**
 * A mail's HTML, on paper of its own: in a frame of no origin, where none of
 * the mail's scripts, forms or pop-ups run, that loads pictures and styles
 * but sends no referrer. One script of Postrider's runs there and tells the
 * page its height, its links and its keys: the page can't listen in a
 * frame of no origin, and WebKit runs no listener in a frame without
 * scripts. So it grows to its content and the page scrolls as one, its
 * links open in the browser, and Esc, J and K work while it has focus.
 */
function MailHtml({ html }: { html: string }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(0);
  const srcDoc = useMemo(() => paper(html), [html]);

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow) return;
      const said = event.data as FrameMessage;
      if (typeof said.height === "number") {
        setHeight(said.height);
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

  return (
    <div className="paper">
      <iframe
        ref={frame}
        title="The mail"
        sandbox="allow-scripts"
        srcDoc={srcDoc}
        style={{ height }}
      />
    </div>
  );
}

/** What the frame's script tells the page. */
type FrameMessage = {
  height?: unknown;
  link?: unknown;
  key?: unknown;
  metaKey?: unknown;
  ctrlKey?: unknown;
  altKey?: unknown;
  shiftKey?: unknown;
};

/** The frame's script: its height, its links and its keys, to the page. */
const REPORTER = `
const say = (message) => parent.postMessage(message, "*");
new ResizeObserver(() =>
  say({ height: Math.ceil(document.documentElement.getBoundingClientRect().height) }),
).observe(document.documentElement);
document.addEventListener("click", (event) => {
  const link = event.target instanceof Element && event.target.closest("a[href]");
  if (!link) return;
  event.preventDefault();
  say({ link: link.getAttribute("href") });
});
document.addEventListener("keydown", (event) =>
  say({ key: event.key, metaKey: event.metaKey, ctrlKey: event.ctrlKey, altKey: event.altKey, shiftKey: event.shiftKey }),
);`;

/**
 * The frame's document: the mail's HTML under a policy that runs only
 * Postrider's script (by a nonce the mail can't know) and lets nothing post
 * back, on white paper whatever the app's theme, as senders design for.
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
</style></head><body>${body}</body></html>`;
}
