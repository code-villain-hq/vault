import type { AccountStatus, Inbox, MailAccount } from "@postrider/types";
import {
  createContext,
  type CSSProperties,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { api } from "./api";
import { useRecords, useWorkspace } from "./workspace";

/** Waits between tries while the server can't be reached, in seconds. */
const RETRY = [2, 5, 15, 30];

type InboxState = {
  /** Undefined until the server first answers. */
  inbox?: Inbox;
  /** The server stopped answering; the Inbox shown is the last it sent. */
  offline: boolean;
};

const Context = createContext<InboxState>({ offline: false });

/**
 * Keeps the workspace's Inbox up to date while mounted: it asks once, then
 * waits at `?after=` for the next change. The server follows each account
 * over IMAP, so new mail shows within a second or so.
 */
export function InboxProvider({ children }: { children: ReactNode }) {
  const { workspace, place } = useWorkspace();
  const [state, setState] = useState<InboxState>({ offline: false });

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

  return <Context.Provider value={state}>{children}</Context.Provider>;
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
