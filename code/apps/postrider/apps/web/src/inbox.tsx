import { shortcutFree } from "@code-villain/web";
import type { AccountStatus, Conversation, Day, MailAccount, MailSummary } from "@postrider/types";
import { Link, useNavigate } from "@tanstack/react-router";
import { Flag } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { FIRST_HOUR, HOURS, hoursIntoTrack, inLanes, nextUp } from "./day";
import { carry, putDown } from "./drag";
import { SearchField } from "./folders";
import { PageBody, PageHeader } from "./layout";
import { clockTime, hue, mailTime, useAccounts, useInbox } from "./mail";
import { flaggedRow, flagToggle, readToggle, useMarkMail } from "./marks";
import { conversationGoing, openList } from "./move";
import { listOf, NoneShown, pick, ShownControl, useShown } from "./shown";
import { buttonClass, Empty } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/**
 * The Inbox, in the list column: today's events, then one Inbox for every
 * account, a row for each conversation.
 */
export function InboxList({ open }: { open: string | undefined }) {
  const { inbox } = useInbox();
  const accounts = useAccounts();
  const now = useNow();
  const byId = useMemo(() => new Map(accounts.map((a) => [a.id, a])), [accounts]);
  return (
    <>
      <PageHeader title="Inbox">
        <ShownControl list="inbox" />
        <SearchField />
      </PageHeader>
      <PageBody>
        <Today now={now} />
        <AccountProblems accounts={accounts} statuses={inbox?.accounts ?? []} />
        {inbox &&
          (inbox.conversations.length ? (
            <MailList conversations={inbox.conversations} byId={byId} now={now} open={open} />
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

/**
 * The mail a conversation's row opens: its newest in the Inbox. Every
 * conversation has one; what you sent and archived of it comes along.
 */
export function anchorOf(conversation: Conversation): MailSummary | undefined {
  return conversation.mails.findLast((mail) => mail.folder === null);
}

/** Whether a conversation has the mail `key`. */
export function hasMail(conversation: Conversation, key: string | undefined): boolean {
  return conversation.mails.some((mail) => mailKey(mail) === key);
}

/** Your accounts' addresses, in lower case, to tell your mail by. */
export function ownAddresses(accounts: Iterable<MailAccount>): Set<string> {
  return new Set([...accounts].map((account) => account.address.toLowerCase()));
}

/** Whether you wrote it: it's in Sent, or it's from one of your accounts. */
export function isYours(mail: MailSummary, own: Set<string>): boolean {
  return mail.folder === "sent" || own.has(mail.from_address.toLowerCase());
}

/**
 * Which list a mail was opened from: the Inbox, a folder, a search, a
 * sender's mail or Flagged; and the folder it's in, but the Inbox.
 */
export type From = { folder?: string; q?: string; sender?: string; flagged?: boolean };

/** The mail page's search for a mail of the list `from`. */
export function openedFrom(mail: MailSummary, from: From): From {
  if (from.flagged) return { folder: mail.folder ?? undefined, flagged: true };
  if (from.q !== undefined) return { folder: mail.folder ?? undefined, q: from.q };
  if (from.sender !== undefined) return { folder: from.folder, sender: from.sender };
  return from.folder ? { folder: from.folder } : {};
}

/** The mail opened last, so the Inbox comes back to it. */
let lastOpened: string | undefined;

export function rememberOpened(key: string) {
  lastOpened = key;
}

/** A row of a list: a mail, or in the Inbox a conversation, opened by its newest Inbox mail. */
export type Row = { mail: MailSummary; conversation?: Conversation };

/** A list's rows: the Inbox's `conversations`, or the other lists' `mails`. */
export function rowsOf(mails?: MailSummary[], conversations?: Conversation[]): Row[] {
  return (
    conversations?.flatMap((conversation) => {
      const mail = anchorOf(conversation);
      return mail ? [{ mail, conversation }] : [];
    }) ??
    mails?.map((mail) => ({ mail })) ??
    []
  );
}

/** Whether a row is unread: a conversation is while any of its Inbox mail is. */
export function unreadRow({ mail, conversation }: Row): boolean {
  return (conversation?.mails ?? [mail]).some((m) => m.folder === mail.folder && !m.seen);
}

/** Whether a row is the mail `key`, or a conversation that has it. */
function holds(row: Row, key: string | undefined): boolean {
  return row.conversation ? hasMail(row.conversation, key) : mailKey(row.mail) === key;
}

/**
 * The rows of the list `from` its filter shows (`shown.tsx`), and always
 * the one with the mail `open`.
 */
export function useRowsShown(from: From, rows: Row[], open: string | undefined): Row[] {
  const list = listOf(from);
  const to = useShown(list);
  return pick(
    list,
    to,
    rows,
    (row) => mailKey(row.mail),
    unreadRow,
    (row) => holds(row, open),
  );
}

/**
 * A list of mail, the Inbox's or another's: J and K (and the arrows) move
 * through it, Enter or O opens the one they're on. With a mail open beside
 * it (`open`, its key), the mail takes the keys and the list follows it.
 * One that leaves the list, filed away, leaves them on the mail that takes
 * its place. ⇧U marks the one they're on read or unread, S flags it or
 * not (`marks.tsx`). The Inbox lists `conversations`, the others `mails`, as
 * its filter shows them: all, unread or read. A row dragged onto a folder
 * in the sidebar moves there (`drag.tsx`); the open one closes, as E does.
 */
export function MailList({
  mails,
  conversations,
  byId,
  now,
  from = {},
  open,
}: {
  mails?: MailSummary[];
  conversations?: Conversation[];
  byId: Map<string, MailAccount>;
  now: Date;
  from?: From;
  open?: string;
}) {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const list = useRef<HTMLOListElement>(null);
  const listed = useMemo(() => rowsOf(mails, conversations), [mails, conversations]);
  const rows = useRowsShown(from, listed, open);
  const own = useMemo(() => ownAddresses(byId.values()), [byId]);
  const markMail = useMarkMail();
  // By key, so new mail at the top doesn't move it to another; where it
  // was, for when it leaves.
  const [focus, setFocus] = useState({ key: open ?? lastOpened, at: 0 });
  if (open !== undefined && open !== focus.key) setFocus({ key: open, at: focus.at });
  const found = rows.findIndex((row) => holds(row, focus.key));
  const focused = found >= 0 ? found : Math.min(focus.at, Math.max(rows.length - 1, 0));
  const taking = found < 0 && open === undefined ? rows[focused] : undefined;
  if (taking) setFocus({ key: mailKey(taking.mail), at: focused });
  else if (found >= 0 && found !== focus.at) setFocus({ key: focus.key, at: found });
  const setFocusedKey = (key: string) => setFocus({ key, at: focused });
  // The open mail, moved away, closes on the list, at the next.
  const closeAt = (index: number) => () => {
    const next = rows[index + 1] ?? rows[index - 1];
    if (next) rememberOpened(mailKey(next.mail));
    openList(navigate, workspace.url_key, from);
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (open !== undefined || !shortcutFree(event)) return;
      // Letters whatever the case (Caps Lock, Shift).
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      const step =
        key === "j" || key === "ArrowDown" ? 1 : key === "k" || key === "ArrowUp" ? -1 : 0;
      if (step) {
        event.preventDefault();
        const next = rows[Math.min(Math.max(focused + step, 0), rows.length - 1)];
        if (next) setFocusedKey(mailKey(next.mail));
      } else if (key === "u" && event.shiftKey && rows[focused]) {
        event.preventDefault();
        markMail(readToggle(rows[focused]));
      } else if (key === "s" && !event.shiftKey && !event.defaultPrevented && rows[focused]) {
        // G then S goes to Sent: the shortcuts took that S already.
        event.preventDefault();
        markMail(flagToggle(rows[focused]));
      } else if ((key === "Enter" && event.target === document.body) || key === "o") {
        const mail = rows[focused]?.mail;
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

  if (rows.length === 0 && listed.length > 0) return <NoneShown list={listOf(from)} />;
  return (
    <ol ref={list} className="mails" aria-label="Mail">
      {rows.map((row, index) => {
        const selected = open !== undefined && holds(row, open);
        return (
          <MailRow
            key={mailKey(row.mail)}
            mail={row.mail}
            conversation={row.conversation}
            own={own}
            index={index}
            focused={index === focused}
            selected={selected}
            account={byId.get(row.mail.account_id)}
            now={now}
            from={from}
            close={selected ? closeAt(index) : undefined}
          />
        );
      })}
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

/**
 * A mail's lines in a list: who and when, its subject, then how it starts.
 * A conversation's names who wrote in it but you, counts its mail, and
 * starts as its last mail does; it's unread while any of it in the Inbox is.
 */
function MailRow({
  mail,
  conversation,
  own,
  index,
  focused,
  selected,
  account,
  now,
  from,
  close,
}: {
  mail: MailSummary;
  conversation: Conversation | undefined;
  own: Set<string>;
  index: number;
  focused: boolean;
  selected: boolean;
  account: MailAccount | undefined;
  now: Date;
  from: From;
  /** Closes it, open beside the list, once it's moved away. */
  close?: () => void;
}) {
  const { workspace } = useWorkspace();
  const [dragged, setDragged] = useState(false);
  const all = conversation?.mails ?? [mail];
  const last = all.at(-1) ?? mail;
  const unread = unreadRow({ mail, conversation });
  const flagged = flaggedRow({ mail, conversation });
  const others = [...new Set(all.filter((m) => !isYours(m, own)).map((m) => m.from_name))];
  // Sent mail names who it went to.
  const who = conversation ? others.join(", ") || mail.from_name : (mail.to_name ?? mail.from_name);
  const snippet = conversation && isYours(last, own) ? `You: ${last.snippet}` : last.snippet;
  return (
    <li>
      <Link
        to="/$workspace/mail/$account/$uid"
        params={{ workspace: workspace.url_key, account: mail.account_id, uid: String(mail.uid) }}
        search={openedFrom(mail, from)}
        onClick={() => rememberOpened(mailKey(mail))}
        onDragStart={(event) => {
          const going = conversationGoing(conversation) ?? [
            { mail, folder: mail.folder ?? from.folder },
          ];
          carry(event, { going, from, close }, mail.subject || "(no subject)");
          setDragged(true);
        }}
        onDragEnd={() => {
          putDown();
          setDragged(false);
        }}
        className={`mail ${unread ? "unread" : ""} ${flagged ? "flagged" : ""} ${focused ? "focused" : ""} ${selected ? "selected" : ""} ${dragged ? "opacity-50" : ""}`}
        aria-current={selected ? "true" : undefined}
        style={hue(account?.hue ?? 1)}
        title={account ? `${mail.from_address}, to ${account.name}` : mail.from_address}
        data-index={index}
      >
        {unread ? <span className="ui-live-dot" role="img" aria-label="Unread" /> : <span />}
        <span className="av" aria-hidden="true">
          {initial(who)}
        </span>
        <span className="who">{mail.to_name === null ? who : `To ${who}`}</span>
        {all.length > 1 && (
          <span className="count" aria-label={`${all.length} mails`}>
            {all.length}
          </span>
        )}
        {last.unsent ? (
          // In the outbox: it goes once its server can be reached.
          <span className="when" title="Goes once its server can be reached">
            Not sent yet
          </span>
        ) : (
          <time className="when" dateTime={last.date} title={new Date(last.date).toLocaleString()}>
            {mailTime(last.date, now)}
          </time>
        )}
        <span className="subject">{mail.subject || "(no subject)"}</span>
        {flagged && (
          <Flag
            className="flag"
            size={13}
            strokeWidth={1.75}
            fill="currentColor"
            role="img"
            aria-label="Flagged"
          />
        )}
        {last.snippet && <span className="snippet">{snippet}</span>}
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
              {status.waiting > 0 &&
                ` ${status.waiting} ${status.waiting === 1 ? "change waits" : "changes wait"} for it.`}
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
    // Unread now: the events shown are the last ones it could read.
    note = day.read_at ? (
      <span title={day.error}>Offline · as of {clockTime(new Date(day.read_at))}</span>
    ) : (
      day.error
    );
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
  const { synced } = useInbox();
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
  }, [workspace.id, place, query, key, changed, minute, synced]);

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
export function NoAccounts() {
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
