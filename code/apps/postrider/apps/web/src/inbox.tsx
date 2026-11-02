import { shortcutFree } from "@code-villain/web";
import type { AccountStatus, Day, MailAccount, MailSummary } from "@postrider/types";
import { Link, useNavigate } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { FIRST_HOUR, HOURS, hoursIntoTrack, inLanes, nextUp } from "./day";
import { SearchField } from "./folders";
import { PageBody, PageHeader } from "./layout";
import { clockTime, hue, mailTime, useAccounts, useInbox } from "./mail";
import { buttonClass, Empty } from "./ui";
import { useRecords, useSyncStatus, useWorkspace } from "./workspace";

/** `/$workspace`: today's events, then one Inbox for every account. */
export function InboxPage() {
  const { inbox } = useInbox();
  const accounts = useAccounts();
  const { ready } = useSyncStatus();
  const now = useNow();
  const byId = useMemo(() => new Map(accounts.map((a) => [a.id, a])), [accounts]);

  if (!ready) return null;
  if (!accounts.length) return <NoAccounts />;
  return (
    <>
      <PageHeader title="Inbox">
        <SearchField />
      </PageHeader>
      <PageBody>
        <Today now={now} />
        <AccountProblems accounts={accounts} statuses={inbox?.accounts ?? []} />
        {inbox &&
          (inbox.mails.length ? (
            <MailList mails={inbox.mails} byId={byId} now={now} />
          ) : (
            <Empty title="Nothing here." />
          ))}
      </PageBody>
    </>
  );
}

/** A mail's key in its list, and in the page that opens it: UIDs count per folder. */
export function mailKey(mail: { folder?: string | null; account_id: string; uid: number }): string {
  return `${mail.folder?.toLowerCase() ?? ""}:${mail.account_id}:${mail.uid}`;
}

/** Which list a mail was opened from: the Inbox, a folder, or a search. */
export type From = { folder?: string; q?: string };

/** The mail page's search for a mail of the list `from`. */
export function openedFrom(mail: MailSummary, from: From): From {
  if (from.q !== undefined) return { folder: mail.folder ?? undefined, q: from.q };
  return from.folder ? { folder: from.folder } : {};
}

/** The mail opened last, so the Inbox comes back to it. */
let lastOpened: string | undefined;

export function rememberOpened(key: string) {
  lastOpened = key;
}

/**
 * A list of mail, the Inbox's or another's: J and K (and the arrows) move
 * through it, Enter or O opens the one they're on.
 */
export function MailList({
  mails,
  byId,
  now,
  from = {},
}: {
  mails: MailSummary[];
  byId: Map<string, MailAccount>;
  now: Date;
  from?: From;
}) {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const list = useRef<HTMLOListElement>(null);
  // By key, so new mail at the top doesn't move it to another.
  const [focusedKey, setFocusedKey] = useState(lastOpened);
  const focused = Math.max(
    0,
    mails.findIndex((mail) => mailKey(mail) === focusedKey),
  );

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!shortcutFree(event)) return;
      // Letters whatever the case (Caps Lock, Shift).
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      const step =
        key === "j" || key === "ArrowDown" ? 1 : key === "k" || key === "ArrowUp" ? -1 : 0;
      if (step) {
        event.preventDefault();
        const next = mails[Math.min(Math.max(focused + step, 0), mails.length - 1)];
        if (next) setFocusedKey(mailKey(next));
      } else if ((key === "Enter" && event.target === document.body) || key === "o") {
        const mail = mails[focused];
        if (!mail) return;
        event.preventDefault();
        rememberOpened(mailKey(mail));
        void navigate({
          to: "/$workspace/mail/$account/$uid",
          params: { workspace: workspace.url_key, account: mail.account_id, uid: String(mail.uid) },
          search: openedFrom(mail, from),
        });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  useEffect(() => {
    list.current?.querySelector(`[data-index="${focused}"]`)?.scrollIntoView({ block: "nearest" });
  }, [focused]);

  return (
    <ol ref={list} className="mails" aria-label="Mail">
      {mails.map((mail, index) => (
        <MailRow
          key={mailKey(mail)}
          mail={mail}
          index={index}
          focused={index === focused}
          account={byId.get(mail.account_id)}
          now={now}
          from={from}
        />
      ))}
    </ol>
  );
}

/** The clock, a minute at a time: for the line for now, and "in 20 min". */
export function useNow(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

function MailRow({
  mail,
  index,
  focused,
  account,
  now,
  from,
}: {
  mail: MailSummary;
  index: number;
  focused: boolean;
  account: MailAccount | undefined;
  now: Date;
  from: From;
}) {
  const { workspace } = useWorkspace();
  const unread = !mail.seen;
  // Sent mail names who it went to.
  const who = mail.to_name ?? mail.from_name;
  return (
    <li>
      <Link
        to="/$workspace/mail/$account/$uid"
        params={{ workspace: workspace.url_key, account: mail.account_id, uid: String(mail.uid) }}
        search={openedFrom(mail, from)}
        onClick={() => rememberOpened(mailKey(mail))}
        className={`mail ${unread ? "unread" : ""} ${focused ? "focused" : ""}`}
        style={hue(account?.hue ?? 1)}
        title={account ? `${mail.from_address}, to ${account.name}` : mail.from_address}
        data-index={index}
      >
        {unread ? <span className="ui-live-dot" role="img" aria-label="Unread" /> : <span />}
        <span className="av" aria-hidden="true">
          {initial(who)}
        </span>
        <span className="who">{mail.to_name === null ? who : `To ${who}`}</span>
        <span className="line">
          <b>{mail.subject || "(no subject)"}</b>
          {mail.snippet && ` — ${mail.snippet}`}
        </span>
        <time className="when" dateTime={mail.date} title={new Date(mail.date).toLocaleString()}>
          {mailTime(mail.date, now)}
        </time>
      </Link>
    </li>
  );
}

/** A sender's first letter, for their avatar. */
export function initial(name: string): string {
  return name.trim().charAt(0).toUpperCase();
}

/** An account Postrider can't read right now says so, above the mail. */
export function AccountProblems({
  accounts,
  statuses,
}: {
  accounts: MailAccount[];
  statuses: AccountStatus[];
}) {
  const { workspace } = useWorkspace();
  const problems = accounts.flatMap((account) => {
    const status = statuses.find((s) => s.account_id === account.id);
    return status && (status.state === "offline" || status.state === "refused")
      ? [{ account, status }]
      : [];
  });
  if (!problems.length) return null;
  return (
    <ul className="problems">
      {problems.map(({ account, status }) => (
        <li key={account.id} className={status.state === "refused" ? "text-danger-ink" : ""}>
          <span className="av sm" style={hue(account.hue)} aria-hidden="true">
            {account.name.charAt(0).toUpperCase()}
          </span>
          {status.state === "refused" ? (
            <span>
              {account.name}: {status.message ?? "its server refused the password."}{" "}
              <Link
                to="/$workspace/settings/accounts"
                params={{ workspace: workspace.url_key }}
                className="ui-link"
              >
                Reconnect
              </Link>
            </span>
          ) : (
            <span>
              Can't reach {account.name}. {status.message ?? "Its server didn't answer."} Postrider
              tries again.
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}

/** An event is drawn at least this wide, in pixels, so its name fits. */
const NARROWEST = 80;
/** A row of events: one 32px event and the gap under it. */
const LANE = 36;

const dayName = new Intl.DateTimeFormat("en", { weekday: "long", month: "short", day: "numeric" });

/** Today, from the calendar server: its events on one track, with a line for now. */
function Today({ now }: { now: Date }) {
  const { workspace } = useWorkspace();
  const calendars = useRecords("Calendar");
  const day = useDay(now);
  const events = day?.events ?? [];
  const timed = events.filter((e) => !e.all_day);
  const track = useRef<HTMLDivElement>(null);
  const width = useWidth(track);
  // How many hours the narrowest event takes on a track this wide.
  const placed = inLanes(timed, (NARROWEST / (width || 896)) * HOURS);
  const lanes = Math.max(1, ...placed.map((p) => p.lane + 1));
  const nowAt = hoursIntoTrack(now);

  let note: ReactNode = null;
  if (!calendars.length) {
    note = (
      <Link
        to="/$workspace/settings/accounts"
        params={{ workspace: workspace.url_key }}
        className="ui-link"
      >
        Add a calendar
      </Link>
    );
  } else if (day?.error) {
    note = day.error;
  } else if (day) {
    note = nextUp(events, now);
  }

  return (
    <section className="today" aria-label="Today">
      <div className="dayhead">
        <b>{dayName.format(now)}</b>
        {events
          .filter((e) => e.all_day)
          .map((e) => (
            <span key={`${e.calendar}:${e.title}`} className="allday" style={hue(e.hue)}>
              {e.title}
            </span>
          ))}
        {note && <span>{note}</span>}
      </div>
      <div ref={track} className="track" style={{ height: lanes * LANE + 22 }}>
        {placed.map(({ event, lane, from, to }) => (
          <div
            key={`${event.calendar}:${event.start}:${event.title}`}
            className="ev"
            style={{
              ...hue(event.hue),
              left: percent(from),
              width: percent(to - from),
              top: lane * LANE,
            }}
            title={`${event.title}, ${clockTime(new Date(event.start))}–${clockTime(new Date(event.end))}`}
          >
            <b>{event.title}</b>
            <span>{clockTime(new Date(event.start))}</span>
          </div>
        ))}
        {nowAt >= 0 && nowAt <= HOURS && (
          <div className="now" style={{ left: percent(nowAt) }} aria-hidden="true" />
        )}
        {Array.from({ length: HOURS / 2 }, (_, i) => FIRST_HOUR + i * 2).map((hour) => (
          <span key={hour} className="hour" style={{ left: percent(hour - FIRST_HOUR) }}>
            {hour}
          </span>
        ))}
      </div>
    </section>
  );
}

/** Today's events, asked again every minute (the clock) and when the day turns. */
function useDay(now: Date): Day | undefined {
  const { workspace, place } = useWorkspace();
  const calendars = useRecords("Calendar");
  const [day, setDay] = useState<{ key: string; day: Day }>();
  const from = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const to = new Date(from);
  to.setDate(to.getDate() + 1);
  const query = new URLSearchParams({ from: from.toISOString(), to: to.toISOString() }).toString();
  // A new day, or another calendar server, starts empty.
  const key = `${query}:${calendars.map((c) => c.id).join()}`;
  // A calendar wearing another colour is asked for again, in place.
  const changed = calendars.map((c) => c.updated_at).join();
  const minute = now.getTime();

  useEffect(() => {
    const stop = new AbortController();
    api<Day>("GET", `/api/w/${workspace.id}/day?${query}`, undefined, place, stop.signal).then(
      (answer) => setDay({ key, day: answer }),
      // Offline, the day stays as it was; the next minute asks again.
      () => undefined,
    );
    return () => stop.abort();
  }, [workspace.id, place, query, key, changed, minute]);

  return day?.key === key ? day.day : undefined;
}

function percent(hours: number): string {
  return `${(hours / HOURS) * 100}%`;
}

/** An element's width, as it changes with the window. */
function useWidth(element: { current: HTMLElement | null }): number {
  const [width, setWidth] = useState(0);
  useEffect(() => {
    if (!element.current) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry?.contentRect.width ?? 0));
    observer.observe(element.current);
    return () => observer.disconnect();
  }, [element]);
  return width;
}

/** A workspace that reads no mail yet: the one thing to do is add an account. */
function NoAccounts() {
  const { workspace } = useWorkspace();
  return (
    <>
      <PageHeader title="Inbox" />
      <PageBody>
        <Empty title="No accounts yet.">
          <p className="mb-4">
            Postrider reads your mail over IMAP. Add an account, and its Inbox shows here.
          </p>
          <Link
            to="/$workspace/settings/accounts"
            params={{ workspace: workspace.url_key }}
            className={buttonClass("primary")}
          >
            Add an account
          </Link>
        </Empty>
      </PageBody>
    </>
  );
}
