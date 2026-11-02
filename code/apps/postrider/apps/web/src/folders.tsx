import { shortcutFree } from "@code-villain/web";
import type { Folder, FolderLink, Listing, RenameFolder } from "@postrider/types";
import { useNavigate } from "@tanstack/react-router";
import { Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { api, messageOf } from "./api";
import { AccountProblems, type From, MailList, mailKey, useNow } from "./inbox";
import { PageBody, PageHeader } from "./layout";
import { useAccounts, useFolders, useInbox } from "./mail";
import { Empty, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/**
 * The lists a folder or a search showed last, by `listKey`: shown at once
 * when opened again, while the servers are asked, and stepped through
 * from a mail opened from them. The list beside an open mail follows it.
 */
const listings = new Map<string, Listing>();
const watchers = new Set<() => void>();
let version = 0;

function keep(key: string, listing: Listing) {
  listings.set(key, listing);
  version += 1;
  for (const watcher of watchers) watcher();
}

function watch(watcher: () => void) {
  watchers.add(watcher);
  return () => void watchers.delete(watcher);
}

function listKey(from: From): string {
  return from.q === undefined ? `folder:${from.folder?.toLowerCase()}` : `search:${from.q}`;
}

/** The mails of the list a mail was opened from, as it shows them. */
export function useListedMails(from: From) {
  useSyncExternalStore(watch, () => version);
  return listings.get(listKey(from))?.mails ?? [];
}

/** A mail of a folder's or a search's, now read: so it shows read in its list. */
export function markSeen(from: From, key: string) {
  const listing = listings.get(listKey(from));
  if (!listing) return;
  keep(listKey(from), {
    ...listing,
    mails: listing.mails.map((mail) => (mailKey(mail) === key ? { ...mail, seen: true } : mail)),
  });
}

/** A mail that left the list it was opened from, as when it's archived. */
export function forgetListed(from: From, key: string) {
  const listing = listings.get(listKey(from));
  if (!listing) return;
  keep(listKey(from), {
    ...listing,
    mails: listing.mails.filter((mail) => mailKey(mail) !== key),
  });
}

/** A folder's or a search's mail, from every account's server. */
function useListing(from: From, path: string | undefined, again = 0) {
  const { workspace, place } = useWorkspace();
  const { refresh } = useFolders();
  const { synced } = useInbox();
  const key = listKey(from);
  const [failed, setFailed] = useState<{ key: string; error: string }>();
  useSyncExternalStore(watch, () => version);

  useEffect(() => {
    if (!path) return;
    const stop = new AbortController();
    api<Listing>("GET", `/api/w/${workspace.id}${path}`, undefined, place, stop.signal).then(
      (listing) => {
        setFailed(undefined);
        keep(key, listing);
        // What's unread may have changed since the sidebar asked.
        refresh();
      },
      (problem: unknown) => {
        if (!stop.signal.aborted) setFailed({ key, error: messageOf(problem) });
      },
    );
    return () => stop.abort();
  }, [workspace.id, place, path, key, refresh, synced, again]);

  const error = failed?.key === key ? failed.error : undefined;
  const listing = listings.get(key);
  return {
    listing,
    error,
    // Nothing to show yet: the servers are being asked.
    waiting: !listing && !error,
  };
}

/** A folder's newest mail, every account's, in the list column. */
export function FolderList({ folder, open }: { folder: string; open: string | undefined }) {
  const { folders, minutes } = useFolders();
  const shown = folders.find((f) => f.key.toLowerCase() === folder.toLowerCase());
  const name = shown?.name ?? folder;
  const from = useMemo(() => ({ folder }), [folder]);
  // Read again with the sidebar's counts, for mail filed or read elsewhere.
  const { listing, error, waiting } = useListing(
    from,
    `/folders/${encodeURIComponent(folder)}`,
    minutes,
  );
  return (
    <>
      <PageHeader
        title={name}
        crumb={shown?.role === null ? <FolderName folder={shown} /> : undefined}
      >
        <SearchField />
      </PageHeader>
      <PageBody>
        <Mails from={from} open={open} listing={listing} error={error} waiting={waiting} />
      </PageBody>
    </>
  );
}

/**
 * A folder's name in the top bar, which renames it when clicked: in every
 * account that has it, with its mail and the rules that sort into it, as
 * the chat does. Enter or leaving the field renames it, Esc doesn't. A
 * folder inside another keeps its place: only its own name changes.
 */
function FolderName({ folder }: { folder: Folder }) {
  const { workspace, place, refusals } = useWorkspace();
  const { moved } = useFolders();
  const navigate = useNavigate();
  const parts = folder.name.split(" / ");
  const own = parts.pop() ?? folder.name;
  const parent = parts.length ? `${parts.join(" / ")} / ` : "";
  const [text, setText] = useState<string>();
  const [renaming, setRenaming] = useState(false);
  // Esc lets go of the field, and leaving it then renames nothing.
  const dropped = useRef(false);

  const rename = () => {
    const name = text?.trim();
    if (renaming || dropped.current) return;
    if (!name || name === own) {
      setText(undefined);
      return;
    }
    setRenaming(true);
    const renamed: RenameFolder = { name };
    api<FolderLink>(
      "POST",
      `/api/w/${workspace.id}/folders/${encodeURIComponent(folder.key)}/rename`,
      renamed,
      place,
    ).then(
      (to) => {
        moved();
        setText(undefined);
        setRenaming(false);
        void navigate({
          to: "/$workspace/folders/$folder",
          params: { workspace: workspace.url_key, folder: to.key },
          replace: true,
        });
      },
      (problem: unknown) => {
        setRenaming(false);
        refusals.dispatchEvent(new CustomEvent("refused", { detail: messageOf(problem) }));
      },
    );
  };

  if (text === undefined) {
    return (
      <button
        type="button"
        className="crumb-name"
        aria-current="page"
        title="Rename the folder"
        onClick={() => {
          dropped.current = false;
          setText(own);
        }}
      >
        {folder.name}
      </button>
    );
  }
  return (
    <span className="flex min-w-0 items-center">
      {parent && <span className="shrink-0 font-medium text-fg">{parent}</span>}
      <input
        autoFocus
        value={text}
        readOnly={renaming}
        onChange={(event) => setText(event.target.value)}
        onFocus={(event) => event.target.select()}
        onBlur={rename}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            rename();
          } else if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            if (renaming) return;
            dropped.current = true;
            setText(undefined);
          }
        }}
        aria-label="The folder's name"
        spellCheck={false}
        // As wide as the name, a letter to spare.
        style={{ width: `calc(${Math.max(text.length, 4) + 1}ch + 14px)` }}
        className={`crumb-name ui-field ${renaming ? "text-muted" : ""}`}
      />
    </span>
  );
}

/**
 * The mail with every word of `q` in it, in every folder, in the list
 * column. Esc with no mail open goes back to the Inbox.
 */
export function SearchList({ q, open }: { q: string; open: string | undefined }) {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const from = useMemo(() => ({ q }), [q]);
  const { listing, error, waiting } = useListing(
    from,
    q.trim() ? `/search?${new URLSearchParams({ q })}` : undefined,
  );

  const reading = open !== undefined;
  useEffect(() => {
    if (reading) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !shortcutFree(event)) return;
      event.preventDefault();
      void navigate({ to: "/$workspace", params: { workspace: workspace.url_key } });
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [reading, navigate, workspace.url_key]);

  return (
    <>
      <PageHeader title={q.trim() ? `“${q.trim()}”` : "Search"}>
        <SearchField query={q} />
      </PageHeader>
      <PageBody>
        {q.trim() ? (
          <Mails
            from={from}
            open={open}
            listing={listing}
            error={error}
            waiting={waiting}
            none="Nothing found."
          />
        ) : (
          <Empty title="Search your mail.">
            <p>Every word you type is looked for in the Inbox, Sent, Archive and your folders.</p>
          </Empty>
        )}
      </PageBody>
    </>
  );
}

/** A list from the servers: why an account couldn't be read, then its mail. */
function Mails({
  from,
  open,
  listing,
  error,
  waiting,
  none = "Nothing here.",
}: {
  from: From;
  open: string | undefined;
  listing: Listing | undefined;
  error: string | undefined;
  waiting: boolean;
  none?: string;
}) {
  const accounts = useAccounts();
  const now = useNow();
  const byId = useMemo(() => new Map(accounts.map((a) => [a.id, a])), [accounts]);
  if (error) {
    return (
      <div className="px-6 py-4">
        <ErrorMessage>{error}</ErrorMessage>
      </div>
    );
  }
  if (waiting) return <p className="px-6 py-4 text-xs text-muted">Asking the servers…</p>;
  if (!listing) return null;
  return (
    <>
      <AccountProblems accounts={accounts} statuses={listing.problems} />
      {listing.mails.length ? (
        <MailList mails={listing.mails} byId={byId} now={now} from={from} open={open} />
      ) : (
        <Empty title={none} />
      )}
    </>
  );
}

/**
 * "Search mail", in the top bar: / goes to it, Enter searches, Esc leaves
 * it. On the search page it holds what was searched for.
 */
export function SearchField({ query = "" }: { query?: string }) {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const field = useRef<HTMLInputElement>(null);
  const [text, setText] = useState(query);
  useEffect(() => setText(query), [query]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "/" || !shortcutFree(event)) return;
      event.preventDefault();
      field.current?.focus();
      field.current?.select();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <form
      role="search"
      className="search ui-field"
      onSubmit={(event) => {
        event.preventDefault();
        if (!text.trim()) return;
        field.current?.blur();
        void navigate({
          to: "/$workspace/search",
          params: { workspace: workspace.url_key },
          search: { q: text.trim() },
        });
      }}
    >
      <Search size={14} strokeWidth={1.75} aria-hidden="true" />
      <input
        ref={field}
        type="search"
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Escape") return;
          event.preventDefault();
          event.stopPropagation();
          setText(query);
          field.current?.blur();
        }}
        placeholder="Search mail"
        aria-label="Search mail"
        spellCheck={false}
      />
      {!text && (
        <kbd className="ui-kbd" aria-hidden="true">
          /
        </kbd>
      )}
    </form>
  );
}
