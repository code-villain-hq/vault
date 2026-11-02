import type { AccountStatus, Folder, Inbox, MailAccount } from "@postrider/types";
import {
  createContext,
  type CSSProperties,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { api } from "./api";
import { notify, setBadge } from "./platform";
import { useRecords, useWorkspace } from "./workspace";

/** Waits between tries while the server can't be reached, in seconds. */
const RETRY = [2, 5, 15, 30];

/** How often the folders' unread counts are asked again while the app shows. */
const FOLDERS_MS = 60_000;

type InboxState = {
  /** Undefined until the server first answers. */
  inbox?: Inbox;
  /** The server stopped answering; the Inbox shown is the last it sent. */
  offline: boolean;
  /** Takes a mail out of the Inbox at once, as it's archived. */
  leave: (accountId: string, uid: number) => void;
  /** Puts it back, when its server refused. */
  stay: (accountId: string, uid: number) => void;
};

const Context = createContext<InboxState>({
  offline: false,
  leave: () => undefined,
  stay: () => undefined,
});

/**
 * Keeps the workspace's Inbox up to date while mounted: it asks once, then
 * waits at `?after=` for the next change. The server follows each account
 * over IMAP, so new mail shows within a second or so.
 */
export function InboxProvider({ children }: { children: ReactNode }) {
  const { workspace, place } = useWorkspace();
  const [state, setState] = useState<{ inbox?: Inbox; offline: boolean }>({ offline: false });
  const [leaving, setLeaving] = useState<ReadonlySet<string>>(new Set());

  useEffect(() => {
    const stop = new AbortController();
    void (async () => {
      let version: number | undefined;
      let tries = 0;
      while (!stop.signal.aborted) {
        const after = version === undefined ? "" : `?after=${version}`;
        try {
          const inbox = await api<Inbox>(
            "GET",
            `/api/w/${workspace.id}/inbox${after}`,
            undefined,
            place,
            // The server answers within 25 seconds, with or without news.
            AbortSignal.any([stop.signal, AbortSignal.timeout(40_000)]),
          );
          tries = 0;
          if (inbox.version !== version) setState({ inbox, offline: false });
          version = inbox.version;
        } catch {
          if (stop.signal.aborted) return;
          setState((s) => (s.offline ? s : { ...s, offline: true }));
          version = undefined;
          await sleep(RETRY[Math.min(tries, RETRY.length - 1)] ?? 30, stop.signal);
          tries += 1;
        }
      }
    })();
    return () => stop.abort();
  }, [workspace.id, place]);

  const leave = useCallback(
    (accountId: string, uid: number) =>
      setLeaving((keys) => new Set(keys).add(`${accountId}:${uid}`)),
    [],
  );
  const stay = useCallback(
    (accountId: string, uid: number) =>
      setLeaving((keys) => {
        const kept = new Set(keys);
        kept.delete(`${accountId}:${uid}`);
        return kept;
      }),
    [],
  );
  const shown = useMemo<InboxState>(() => {
    const inbox = state.inbox && {
      ...state.inbox,
      mails: state.inbox.mails.filter((mail) => !leaving.has(`${mail.account_id}:${mail.uid}`)),
    };
    return { inbox, offline: state.offline, leave, stay };
  }, [state, leaving, leave, stay]);
  useNewMailNotices(state.inbox);

  return (
    <Context.Provider value={shown}>
      <FoldersProvider version={state.inbox?.version}>{children}</FoldersProvider>
    </Context.Provider>
  );
}

/** More new mail at once than this is one notification. */
const NOTICES = 3;

/**
 * In the desktop app, the dock shows how much in the Inbox is unread, and
 * unread mail that comes while the app runs is a notification. What an
 * account's server has when it comes online (the app opened, the account
 * was connected) is there already: it doesn't pop up.
 */
function useNewMailNotices(inbox: Inbox | undefined) {
  const known = useRef({ online: new Set<string>(), mails: new Set<string>() });

  useEffect(() => {
    if (!inbox) return;
    void setBadge(inbox.mails.filter((mail) => !mail.seen).length);
    const { online, mails } = known.current;
    const fresh = inbox.mails.filter((mail) => {
      const key = `${mail.account_id}:${mail.uid}`;
      if (mails.has(key)) return false;
      mails.add(key);
      return !mail.seen && online.has(mail.account_id);
    });
    for (const status of inbox.accounts) {
      if (status.state === "online") online.add(status.account_id);
    }
    if (fresh.length > NOTICES) {
      const senders = [...new Set(fresh.map((mail) => mail.from_name))];
      void notify(`${fresh.length} new mails`, senders.join(", "));
    } else {
      for (const mail of fresh) void notify(mail.from_name, mail.subject || "(no subject)");
    }
  }, [inbox]);

  // Gone (another workspace, or the app quits): no count left on the dock.
  useEffect(() => () => void setBadge(0), []);
}

type FoldersState = {
  folders: Folder[];
  /** Asks the servers again, as after a folder's mail was read. */
  refresh: () => void;
};

const FoldersContext = createContext<FoldersState>({ folders: [], refresh: () => undefined });

/**
 * The workspace's folders with what's unread in them, read live: asked
 * when the sidebar shows, when the Inbox changes (an account connected,
 * mail came), when a folder's list or its unread mail is opened, and
 * every minute while the app shows, for mail filed or read elsewhere.
 */
function FoldersProvider({ version, children }: { version?: number; children: ReactNode }) {
  const { workspace, place } = useWorkspace();
  const [folders, setFolders] = useState<Folder[]>([]);
  const [asked, setAsked] = useState(0);

  useEffect(() => {
    const stop = new AbortController();
    api<Folder[]>("GET", `/api/w/${workspace.id}/folders`, undefined, place, stop.signal).then(
      setFolders,
      // Offline, the folders stay as they were.
      () => undefined,
    );
    return () => stop.abort();
  }, [workspace.id, place, version, asked]);

  const refresh = useCallback(() => setAsked((n) => n + 1), []);
  useEffect(() => {
    const ask = () => {
      if (document.visibilityState === "visible") refresh();
    };
    const timer = setInterval(ask, FOLDERS_MS);
    document.addEventListener("visibilitychange", ask);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", ask);
    };
  }, [refresh]);

  const state = useMemo(() => ({ folders, refresh }), [folders, refresh]);
  return <FoldersContext.Provider value={state}>{children}</FoldersContext.Provider>;
}

export function useFolders(): FoldersState {
  return useContext(FoldersContext);
}

function sleep(seconds: number, signal: AbortSignal) {
  return new Promise<void>((resolve) => {
    const timer = setTimeout(resolve, seconds * 1000);
    signal.addEventListener("abort", () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

export function useInbox(): InboxState {
  return useContext(Context);
}

/** How many mails in the Inbox are unread. */
export function useUnread(): number {
  const { inbox } = useInbox();
  return useMemo(() => inbox?.mails.filter((m) => !m.seen).length ?? 0, [inbox]);
}

/** Where an account's server stands, as the server last said. */
export function useAccountStatus(id: string): AccountStatus | undefined {
  return useInbox().inbox?.accounts.find((a) => a.account_id === id);
}

/** The workspace's accounts, in their order, from its local copy. */
export function useAccounts(): MailAccount[] {
  const accounts = useRecords("MailAccount");
  return useMemo(() => [...accounts].sort((a, b) => a.position - b.position), [accounts]);
}

/** An account's data hue, for its senders, its events, and itself. */
export function hue(hue: number): CSSProperties {
  return { "--hue": `var(--data-${hue})` } as CSSProperties;
}

const clock = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit" });
const weekday = new Intl.DateTimeFormat("en", { weekday: "short" });
const date = new Intl.DateTimeFormat("en", { month: "short", day: "numeric" });
const dateAndYear = new Intl.DateTimeFormat("en", {
  month: "short",
  day: "numeric",
  year: "numeric",
});

/** "10:00", in this Mac's time zone. */
export function clockTime(when: Date): string {
  return clock.format(when);
}

/**
 * When a mail came, as the Inbox shows it: the time today, "Yesterday",
 * the weekday within the week, then the date.
 */
export function mailTime(iso: string, now: Date): string {
  const when = new Date(iso);
  const days = Math.round((startOfDay(now) - startOfDay(when)) / 86_400_000);
  if (days <= 0) return clock.format(when);
  if (days === 1) return "Yesterday";
  if (days < 7) return weekday.format(when);
  return when.getFullYear() === now.getFullYear() ? date.format(when) : dateAndYear.format(when);
}

function startOfDay(when: Date): number {
  return new Date(when.getFullYear(), when.getMonth(), when.getDate()).getTime();
}
