import { messageOf } from "@code-villain/web";
import type {
  AccountState,
  Calendar,
  CalendarConnect,
  MailAccount,
  MailAccountConnect,
} from "@postrider/types";
import { type FormEvent, type ReactNode, useState } from "react";
import { api } from "./api";
import { PageBody, PageHeader } from "./layout";
import { hue, useAccounts, useAccountStatus } from "./mail";
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
  const { workspace } = useWorkspace();
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

  return (
    <li className="flex flex-col gap-3 rounded-md bg-surface px-4 py-3">
      <div className="flex items-center gap-3">
        <span className="av" style={hue(account.hue)} aria-hidden="true">
          {account.name.charAt(0).toUpperCase()}
        </span>
        <div className="flex min-w-0 flex-col">
          <span className="truncate font-medium">{account.name}</span>
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
      <ErrorMessage>{error}</ErrorMessage>
    </li>
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
          <span className="truncate text-xs text-muted">
            Signed in as {calendar.username}. Each calendar wears the colour of the account it's
            named after.
          </span>
        </div>
        <Button variant="secondary" className="ml-auto" disabled={removing} onClick={remove}>
          {removing ? "Removing…" : "Remove"}
        </Button>
      </div>
      <ErrorMessage>{error}</ErrorMessage>
    </div>
  );
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
