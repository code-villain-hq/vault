import { messageOf } from "@code-villain/web";
import type {
  AccountState,
  Calendar,
  CalendarConnect,
  MailAccount,
  MailAccountConnect,
  ServerCalendar,
} from "@postrider/types";
import { type FormEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { api } from "./api";
import { PageBody, PageHeader } from "./layout";
import { clockTime, hue, useAccounts, useAccountStatus } from "./mail";
import { PictureSendersList } from "./pictures";
import { Button, ErrorMessage, Section, TextField } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/**
 * Settings → Accounts: the mail accounts Postrider reads and the calendar
 * server. Each is signed in to before it's kept, so these forms work as a
 * whole and keep one button, "Connect". Passwords go to the keychain.
 */
export function AccountsPage() {
  const accounts = useAccounts();
  const [calendar] = useRecords("Calendar");
  return (
    <>
      <PageHeader title="Accounts" />
      <PageBody>
        <div className="mx-auto flex max-w-2xl flex-col gap-10 px-6 py-8">
          <Section title="Mail">
            {accounts.length > 0 && (
              <ul className="flex flex-col gap-1">
                {accounts.map((account) => (
                  <AccountRow key={account.id} account={account} />
                ))}
              </ul>
            )}
            <AddAccount first={!accounts.length} />
          </Section>
          <Section title="Calendar">
            {calendar ? <CalendarRow calendar={calendar} /> : <AddCalendar />}
          </Section>
          <Section title="Pictures always load from">
            <PictureSendersList />
          </Section>
        </div>
      </PageBody>
    </>
  );
}

const STATE_WORDS: Record<AccountState, string> = {
  connecting: "Connecting…",
  online: "Reading its Inbox.",
  offline: "Can't reach it.",
  refused: "Its server refused the password.",
};

function AccountRow({ account }: { account: MailAccount }) {
  const { workspace, sync } = useWorkspace();
  const [name, setName] = useState(account.name);
  const cancelled = useRef(false);
  const status = useAccountStatus(account.id);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const state = status?.state ?? "connecting";

  const remove = async () => {
    setRemoving(true);
    setError(null);
    try {
      await api("DELETE", `/api/w/${workspace.id}/accounts/${account.id}`);
    } catch (problem) {
      setError(messageOf(problem));
      setRemoving(false);
    }
  };

  // Renamed where it's written, as the kit's names are: kept when you
  // leave the field or press Enter.
  const rename = () => {
    const trimmed = name.trim();
    if (cancelled.current || !trimmed || trimmed === account.name) {
      cancelled.current = false;
      setName(account.name);
      return;
    }
    setError(null);
    try {
      sync.update("MailAccount", account.id, { name: trimmed });
    } catch (problem) {
      setError(messageOf(problem));
      setName(account.name);
    }
  };

  return (
    <li className="flex flex-col gap-3 rounded-md bg-surface px-4 py-3">
      <div className="flex items-center gap-3">
        <span className="av" style={hue(account.hue)} aria-hidden="true">
          {account.name.charAt(0).toUpperCase()}
        </span>
        <div className="flex min-w-0 flex-1 flex-col">
          <input
            aria-label="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onBlur={rename}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              if (e.key === "Escape") {
                cancelled.current = true;
                e.currentTarget.blur();
              }
            }}
            className="-ml-2 h-7 w-full rounded-md px-2 font-medium ui-field"
          />
          <span className="truncate text-xs text-muted">
            {account.address} · {account.host}
          </span>
        </div>
        <Button variant="secondary" className="ml-auto" disabled={removing} onClick={remove}>
          {removing ? "Removing…" : "Remove"}
        </Button>
      </div>
      <p className={`text-xs ${state === "refused" ? "text-danger-ink" : "text-muted"}`}>
        {state === "online" || state === "connecting"
          ? STATE_WORDS[state]
          : (status?.message ?? STATE_WORDS[state])}
        {state === "offline" && " Postrider tries again."}
      </p>
      {state === "refused" && <Reconnect account={account} />}
      <Signature account={account} />
      <ErrorMessage>{error}</ErrorMessage>
    </li>
  );
}

/**
 * What mail sent from the account is signed with. Kept as the name is,
 * when you leave the field; Esc puts back what it was.
 */
function Signature({ account }: { account: MailAccount }) {
  const { sync } = useWorkspace();
  const [text, setText] = useState(account.signature);
  const [error, setError] = useState<string | null>(null);
  const cancelled = useRef(false);
  const id = useId();

  // Changed elsewhere while not being written here, it shows that.
  const [was, setWas] = useState(account.signature);
  if (was !== account.signature) {
    setWas(account.signature);
    setText(account.signature);
  }

  const save = () => {
    const kept = text.replace(/\s+$/, "");
    if (cancelled.current || kept === account.signature) {
      cancelled.current = false;
      setText(account.signature);
      return;
    }
    setError(null);
    try {
      sync.update("MailAccount", account.id, { signature: kept });
      setText(kept);
    } catch (problem) {
      setError(messageOf(problem));
      setText(account.signature);
    }
  };

  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-xs font-medium text-muted">
        Signature
      </label>
      <textarea
        id={id}
        value={text}
        rows={Math.min(Math.max(text.split("\n").length, 2), 8)}
        placeholder={account.name}
        onChange={(e) => setText(e.target.value)}
        onBlur={save}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            cancelled.current = true;
            e.currentTarget.blur();
          }
        }}
        className="ui-field resize-none rounded-md px-3 py-2 leading-relaxed"
      />
      <p className="text-xs text-faint">
        {account.signature
          ? "Below what you send from this account, after a “-- ” line."
          : "Mail from this account goes unsigned."}
      </p>
      <ErrorMessage>{error}</ErrorMessage>
    </div>
  );
}

/** A refused account takes its password again, and keeps the rest. */
function Reconnect({ account }: { account: MailAccount }) {
  const { workspace } = useWorkspace();
  const [password, setPassword] = useState("");
  const send = async () => {
    const request: MailAccountConnect = {
      name: account.name,
      address: account.address,
      host: account.host,
      port: account.port,
      username: account.username,
      password,
    };
    await api("POST", `/api/w/${workspace.id}/accounts`, request);
  };
  return (
    <ConnectForm label="Reconnect" busy="Reconnecting…" onSubmit={send}>
      <TextField
        label="Password"
        type="password"
        autoComplete="current-password"
        required
        value={password}
        onChange={(e) => setPassword(e.target.value)}
      />
    </ConnectForm>
  );
}

function AddAccount({ first }: { first: boolean }) {
  const { workspace } = useWorkspace();
  const empty = { name: "", address: "", host: "", port: "", password: "" };
  const [form, setForm] = useState(empty);
  const field = (key: keyof typeof empty) => ({
    value: form[key],
    onChange: (e: { target: { value: string } }) => setForm({ ...form, [key]: e.target.value }),
  });
  const send = async () => {
    const request: MailAccountConnect = {
      name: form.name,
      address: form.address,
      host: form.host,
      port: form.port ? Number(form.port) : undefined,
      password: form.password,
    };
    await api("POST", `/api/w/${workspace.id}/accounts`, request);
    setForm(empty);
  };
  return (
    <div className="flex flex-col gap-3">
      {!first && <h3 className="text-xs font-medium">Add an account</h3>}
      {first && (
        <p className="text-muted">
          Postrider reads an account's Inbox over IMAP, and keeps its password in this Mac's
          keychain.
        </p>
      )}
      <ConnectForm label="Connect" busy="Connecting…" onSubmit={send}>
        <div className="grid gap-3 sm:grid-cols-2">
          <TextField label="Name" placeholder="Work" required {...field("name")} />
          <TextField
            label="Address"
            type="email"
            placeholder="you@example.com"
            autoComplete="email"
            required
            {...field("address")}
          />
          <TextField
            label="IMAP server"
            placeholder="imap.example.com"
            autoCapitalize="off"
            spellCheck={false}
            required
            {...field("host")}
          />
          <TextField
            label="Port"
            inputMode="numeric"
            placeholder="993"
            hint="Over TLS."
            {...field("port")}
          />
        </div>
        <TextField
          label="Password"
          type="password"
          autoComplete="current-password"
          required
          {...field("password")}
        />
      </ConnectForm>
    </div>
  );
}

function CalendarRow({ calendar }: { calendar: Calendar }) {
  const { workspace } = useWorkspace();
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const remove = async () => {
    setRemoving(true);
    setError(null);
    try {
      await api("DELETE", `/api/w/${workspace.id}/calendar`);
    } catch (problem) {
      setError(messageOf(problem));
      setRemoving(false);
    }
  };
  return (
    <div className="flex flex-col gap-3 rounded-md bg-surface px-4 py-3">
      <div className="flex items-center gap-3">
        <div className="flex min-w-0 flex-col">
          <span className="truncate font-medium">{new URL(calendar.url).host}</span>
          <span className="truncate text-xs text-muted">Signed in as {calendar.username}.</span>
        </div>
        <Button variant="secondary" className="ml-auto" disabled={removing} onClick={remove}>
          {removing ? "Removing…" : "Remove"}
        </Button>
      </div>
      <ErrorMessage>{error}</ErrorMessage>
      <Calendars calendar={calendar} />
    </div>
  );
}

/**
 * The server's calendars, each wearing an account's colour: the one it's
 * named after, or at the server's domain, until the person picks another.
 */
function Calendars({ calendar }: { calendar: Calendar }) {
  const { workspace, place, sync } = useWorkspace();
  const accounts = useAccounts();
  const [listed, setListed] = useState<{ calendars?: ServerCalendar[]; error?: string }>();

  useEffect(() => {
    const stop = new AbortController();
    api<ServerCalendar[]>(
      "GET",
      `/api/w/${workspace.id}/calendars`,
      undefined,
      place,
      stop.signal,
    ).then(
      (calendars) => setListed({ calendars }),
      (problem: unknown) => {
        if (!stop.signal.aborted) setListed({ error: messageOf(problem) });
      },
    );
    return () => stop.abort();
  }, [workspace.id, place, calendar.id]);

  if (!listed) return <p className="text-xs text-muted">Asking the calendar server…</p>;
  if (listed.error) return <ErrorMessage>{listed.error}</ErrorMessage>;
  if (!listed.calendars?.length) return <p className="text-xs text-muted">It has no calendars.</p>;

  // A copy synced before Postrider kept choices has none until it's fresh.
  const chosenFor: Record<string, string> = calendar.accounts ?? {};
  const wears = (found: ServerCalendar) => {
    const chosen = chosenFor[found.href];
    return (
      accounts.find((a) => a.id === chosen) ?? accounts.find((a) => a.id === found.default_account)
    );
  };
  const choose = (found: ServerCalendar, accountId: string) => {
    const chosen = { ...chosenFor };
    // The account it would wear anyway isn't kept as a choice.
    if (accountId === found.default_account) delete chosen[found.href];
    else chosen[found.href] = accountId;
    sync.update("Calendar", calendar.id, { accounts: chosen });
  };

  return (
    <ul className="flex flex-col border-t border-line pt-2">
      {listed.calendars.map((found) => {
        const account = wears(found);
        return (
          <li key={found.href} className="flex items-center gap-3 py-1.5">
            <span className="swatch" style={hue(account?.hue ?? 1)} aria-hidden="true" />
            <div className="flex min-w-0 flex-col">
              <span className="truncate">{calendarName(found)}</span>
              <span className="truncate text-xs text-muted">{nextEvent(found)}</span>
            </div>
            {accounts.length > 0 && (
              <select
                aria-label={`Whose colour ${calendarName(found)} wears`}
                value={account?.id ?? ""}
                onChange={(event) => choose(found, event.target.value)}
                className="ui-field ml-auto h-7 shrink-0 rounded-md px-2 text-xs"
              >
                {accounts.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                  </option>
                ))}
              </select>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** A calendar's name; one named only by an id, as some apps make them, has none. */
function calendarName(found: ServerCalendar): string {
  return /^[0-9a-f-]{20,}$/i.test(found.name) ? "A calendar without a name" : found.name;
}

const weekday = new Intl.DateTimeFormat("en", { weekday: "short" });
const date = new Intl.DateTimeFormat("en", { weekday: "short", month: "short", day: "numeric" });

/** "Next: Zahnarzt, Mon 15:00", to tell a calendar by. */
function nextEvent(found: ServerCalendar): string {
  if (!found.next) return "Nothing in the next four weeks.";
  const start = new Date(found.next.start);
  const now = new Date();
  const days = Math.round(
    (new Date(start.getFullYear(), start.getMonth(), start.getDate()).getTime() -
      new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()) /
      86_400_000,
  );
  const day =
    days <= 0
      ? "today"
      : days === 1
        ? "tomorrow"
        : days < 7
          ? weekday.format(start)
          : date.format(start);
  const when = found.next.all_day ? day : `${day} ${clockTime(start)}`;
  return `Next: ${found.next.title}, ${when}`;
}

function AddCalendar() {
  const { workspace } = useWorkspace();
  const empty: CalendarConnect = { url: "", username: "", password: "" };
  const [form, setForm] = useState(empty);
  const field = (key: keyof CalendarConnect) => ({
    value: form[key],
    onChange: (e: { target: { value: string } }) => setForm({ ...form, [key]: e.target.value }),
  });
  return (
    <div className="flex flex-col gap-3">
      <p className="text-muted">
        Postrider reads today's events over CalDAV, from a server such as Radicale.
      </p>
      <ConnectForm
        label="Connect"
        busy="Connecting…"
        onSubmit={() => api("POST", `/api/w/${workspace.id}/calendar`, form)}
      >
        <TextField
          label="Address"
          placeholder="https://calendar.example.com"
          autoCapitalize="off"
          spellCheck={false}
          required
          {...field("url")}
        />
        <div className="grid gap-3 sm:grid-cols-2">
          <TextField
            label="User name"
            autoComplete="username"
            autoCapitalize="off"
            required
            {...field("username")}
          />
          <TextField
            label="Password"
            type="password"
            autoComplete="current-password"
            required
            {...field("password")}
          />
        </div>
      </ConnectForm>
    </div>
  );
}

/**
 * A form that only works as a whole: one button, busy while the server
 * signs in, and the server's answer under it when it can't.
 */
function ConnectForm({
  label,
  busy,
  onSubmit,
  children,
}: {
  label: string;
  busy: string;
  onSubmit: () => Promise<unknown>;
  children: ReactNode;
}) {
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setSending(true);
    setError(null);
    try {
      await onSubmit();
    } catch (problem) {
      setError(messageOf(problem));
    } finally {
      setSending(false);
    }
  };
  return (
    <form className="flex flex-col gap-3" onSubmit={submit}>
      {children}
      <ErrorMessage>{error}</ErrorMessage>
      <div>
        <Button type="submit" disabled={sending}>
          {sending ? busy : label}
        </Button>
      </div>
    </form>
  );
}
