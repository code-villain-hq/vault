import type { AccountStatus, Day, MailAccount, MailSummary } from "@postrider/types";
import { Link } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { FIRST_HOUR, HOURS, hoursIntoTrack, inLanes, nextUp } from "./day";
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
      <PageHeader title="Inbox" />
      <PageBody>
        <Today now={now} />
        <AccountProblems accounts={accounts} statuses={inbox?.accounts ?? []} />
        {inbox &&
          (inbox.mails.length ? (
            <ol className="mails" aria-label="Mail">
              {inbox.mails.map((mail) => (
                <MailRow
                  key={`${mail.account_id}:${mail.uid}`}
                  mail={mail}
                  account={byId.get(mail.account_id)}
                  now={now}
                />
              ))}
            </ol>
          ) : (
            <Empty title="Nothing here." />
          ))}
      </PageBody>
    </>
  );
}

/** The clock, a minute at a time: for the line for now, and "in 20 min". */
function useNow(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

function MailRow({
  mail,
  account,
  now,
}: {
  mail: MailSummary;
  account: MailAccount | undefined;
  now: Date;
}) {
  const unread = !mail.seen;
  return (
    <li
      className={`mail ${unread ? "unread" : ""}`}
      style={hue(account?.hue ?? 1)}
      title={account ? `${mail.from_address}, to ${account.name}` : mail.from_address}
    >
      {unread ? <span className="ui-live-dot" role="img" aria-label="Unread" /> : <span />}
      <span className="av" aria-hidden="true">
        {mail.from_name.trim().charAt(0).toUpperCase()}
      </span>
      <span className="who">{mail.from_name}</span>
      <span className="line">
        <b>{mail.subject || "(no subject)"}</b>
        {mail.snippet && ` — ${mail.snippet}`}
      </span>
      <time className="when" dateTime={mail.date} title={new Date(mail.date).toLocaleString()}>
        {mailTime(mail.date, now)}
      </time>
    </li>
  );
}

/** An account Postrider can't read right now says so, above the mail. */
function AccountProblems({
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
const LANE = 40;

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
      <div ref={track} className="track" style={{ height: lanes * LANE + 18 }}>
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
  const minute = now.getTime();

  useEffect(() => {
    const stop = new AbortController();
    api<Day>("GET", `/api/w/${workspace.id}/day?${query}`, undefined, place, stop.signal).then(
      (answer) => setDay({ key, day: answer }),
      // Offline, the day stays as it was; the next minute asks again.
      () => undefined,
    );
    return () => stop.abort();
  }, [workspace.id, place, query, key, minute]);

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
