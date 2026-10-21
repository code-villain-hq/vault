import { Dialog } from "@base-ui/react/dialog";
import type {
  AgentInfo,
  ApiTokenInfo,
  CreatedToken,
  RunnerInfo,
  RunnerSecret,
  WakeupEvent,
  WakeupInfo,
} from "@forge/types";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { AgentWorkStatus } from "./agent-work";
import { api, messageOf } from "./api";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { timeAgo } from "./data";
import { Avatar } from "./icons";
import { PageHeader } from "./layout";
import { CreateDialog, MoreMenu, NewButton } from "./page-parts";
import { publicUrl } from "./server";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** A key used this recently is probably what something is running on right now. */
const IN_USE_MS = 15 * 60 * 1000;

/** How a key is told apart: `forge_…a1b2`. */
function keyName(hint: string | null) {
  return hint ? `forge_…${hint}` : "forge_…";
}

function inUse(token: ApiTokenInfo) {
  return (
    token.last_used_at !== null && Date.now() - new Date(token.last_used_at).getTime() < IN_USE_MS
  );
}

/**
 * Agents: AI clients that use Forge through the MCP server, with their own
 * keys. Everyone sees them; admins create them and handle their keys and
 * runners.
 */
export function AgentsSettingsPage() {
  const { workspace } = useWorkspace();
  const base = `/api/w/${workspace.id}`;
  const isAdmin = workspace.role === "admin";
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  // Null until typed in: "Claude Code", unless an agent already has that name.
  const [typed, setTyped] = useState<string | null>(null);
  const [created, setCreated] = useState<{ name: string; token: string } | null>(null);
  const [secret, setSecret] = useState<{ name: string; secret: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [creating, setCreating] = useState(false);

  const reload = useCallback(async () => {
    try {
      setAgents(await api<AgentInfo[]>("GET", `${base}/agents`));
    } catch (e) {
      setError(messageOf(e));
    }
  }, [base]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const run = async (action: () => Promise<void>) => {
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(messageOf(e));
    }
    await reload();
  };

  const name =
    typed ?? (agents.some((a) => a.name.toLowerCase() === "claude code") ? "" : "Claude Code");
  // Usually someone typing an existing name wants another key for that agent.
  const taken = agents.find((a) => a.name.toLowerCase() === name.trim().toLowerCase());

  const create = (event: FormEvent) => {
    event.preventDefault();
    if (taken) return;
    void run(async () => {
      const result = await api<CreatedToken>("POST", `${base}/agents`, { name: name.trim() });
      setCreated({ name: name.trim(), token: result.token });
    });
  };

  const newKey = (agent: AgentInfo) =>
    void run(async () => {
      const result = await api<CreatedToken>("POST", `${base}/agents/${agent.id}/tokens`);
      setCreated({ name: agent.name, token: result.token });
    });

  const setRunner = (agent: AgentInfo, url: string) =>
    run(async () => {
      const result = await api<RunnerSecret | null>("PUT", `${base}/agents/${agent.id}/runner`, {
        url,
      });
      if (result) setSecret({ name: agent.name, secret: result.secret });
    });

  const newSecret = (agent: AgentInfo) =>
    setConfirmation({
      title: `Make a new runner secret for “${agent.name}”?`,
      body: "The old one stops working at once: its runner refuses wake-ups until it has the new one.",
      action: "New secret",
      onConfirm: () =>
        void run(async () => {
          const result = await api<RunnerSecret>(
            "POST",
            `${base}/agents/${agent.id}/runner/secret`,
          );
          setSecret({ name: agent.name, secret: result.secret });
        }),
    });

  return (
    <>
      <PageHeader title="Agents">
        {isAdmin && (
          <>
            <NewButton label="New agent" onClick={() => setCreating(true)} />
            <CreateDialog open={creating} onClose={() => setCreating(false)}>
              <form
                onSubmit={(event) => {
                  create(event);
                  if (!taken) setCreating(false);
                }}
              >
                <div className="flex items-center gap-2 px-4 pt-4 text-xs text-muted">
                  <span>Agents</span>
                  <span>›</span>
                  <Dialog.Title className="font-normal">New agent</Dialog.Title>
                </div>
                <div className="flex flex-col gap-2 px-4 py-3">
                  <input
                    // biome-ignore lint/a11y/noAutofocus: the dialog opens to type a name
                    autoFocus
                    value={name}
                    onChange={(e) => setTyped(e.target.value)}
                    placeholder="Agent name, such as Claude Code (laptop)"
                    aria-label="Agent name"
                    className="bg-transparent text-title font-bold outline-none placeholder:text-faint"
                  />
                  {taken ? (
                    <p className="text-warn-ink">
                      There's already an agent called “{taken.name}”. To connect it somewhere else,
                      give it a new key from its ⋯ menu, or give this agent another name.
                    </p>
                  ) : (
                    <p className="text-muted">
                      Name it after where it runs, so you can tell agents apart. It gets its own
                      key, and everything it changes is marked as its work.
                    </p>
                  )}
                </div>
                <div className="flex items-center justify-end gap-3 px-4 py-3">
                  <Button type="submit" disabled={!name.trim() || Boolean(taken)}>
                    Create agent
                  </Button>
                </div>
              </form>
            </CreateDialog>
          </>
        )}
      </PageHeader>
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-10 px-6 py-8">
          {(agents.length === 0 || !isAdmin) && (
            <p className="text-muted">
              Agents read and change issues, specs and decisions through Forge's MCP server, so
              specs don't need to live in your repositories. Each agent gets its own identity:
              everything it changes is marked as its work.
              {!isAdmin &&
                " Workspace admins create agents and their keys; ask one to connect yours."}
            </p>
          )}
          <ErrorMessage>{error}</ErrorMessage>
          {created && <TokenNotice name={created.name} token={created.token} />}
          {secret && <SecretNotice name={secret.name} secret={secret.secret} />}

          <section>
            <ul className="-mx-3 flex flex-col gap-1">
              {agents.length === 0 && <li className="px-3 py-3 text-muted">No agents yet.</li>}
              {agents.map((agent) => (
                <AgentRow
                  key={agent.id}
                  agent={agent}
                  isAdmin={isAdmin}
                  onNewKey={() => newKey(agent)}
                  onSetRunner={(url) => setRunner(agent, url)}
                  onNewSecret={() => newSecret(agent)}
                  onRemoveRunner={() =>
                    setConfirmation({
                      title: `Stop waking “${agent.name}”?`,
                      body: "Forge stops sending wake-ups to its runner, and forgets the ones it sent. The agent keeps its keys.",
                      action: "Stop waking it",
                      onConfirm: () =>
                        void run(() => api("DELETE", `${base}/agents/${agent.id}/runner`)),
                    })
                  }
                  onRename={(next) =>
                    run(() => api("PATCH", `${base}/agents/${agent.id}`, { name: next }))
                  }
                  onRemove={() =>
                    setConfirmation({
                      title: `Remove the agent “${agent.name}”?`,
                      body: "Its keys stop working at once, and it's unassigned from its issues. What it did stays in history.",
                      action: "Remove agent",
                      onConfirm: () => void run(() => api("DELETE", `${base}/members/${agent.id}`)),
                    })
                  }
                  onRevoke={(token) => {
                    const recent = inUse(token) && token.last_used_at;
                    setConfirmation({
                      title: `Revoke ${token.hint ? keyName(token.hint) : "this older key"} of “${agent.name}”?`,
                      body: `Whatever uses it loses access right away.${
                        recent
                          ? ` It was used ${timeAgo(recent)}, so something is probably using it now.`
                          : ""
                      }`,
                      action: "Revoke key",
                      onConfirm: () => void run(() => api("DELETE", `${base}/tokens/${token.id}`)),
                    });
                  }}
                />
              ))}
            </ul>
          </section>
        </div>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

function AgentRow({
  agent,
  isAdmin,
  onNewKey,
  onRename,
  onRemove,
  onRevoke,
  onSetRunner,
  onNewSecret,
  onRemoveRunner,
}: {
  agent: AgentInfo;
  isAdmin: boolean;
  onNewKey: () => void;
  onRename: (name: string) => Promise<void>;
  onRemove: () => void;
  onRevoke: (token: ApiTokenInfo) => void;
  onSetRunner: (url: string) => Promise<void>;
  onNewSecret: () => void;
  onRemoveRunner: () => void;
}) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const save = () => {
    const next = renaming?.trim();
    setRenaming(null);
    if (next && next !== agent.name) void onRename(next);
  };

  return (
    <li className="flex flex-col gap-2 rounded-md px-3 py-3 hover:bg-hover">
      <div className="flex items-center gap-3">
        <Avatar user={{ ...agent, kind: "agent" }} size="size-6" />
        {renaming === null ? (
          <span className="flex-1 font-medium">{agent.name}</span>
        ) : (
          <input
            aria-label="Agent name"
            // biome-ignore lint/a11y/noAutofocus: opened by pressing Rename
            autoFocus
            value={renaming}
            onChange={(e) => setRenaming(e.target.value)}
            onBlur={save}
            onKeyDown={(e) => {
              if (e.key === "Enter") save();
              if (e.key === "Escape") setRenaming(null);
            }}
            className="h-7 flex-1 rounded-md ui-field px-2 font-medium outline-none"
          />
        )}
        {isAdmin && renaming === null && (
          <MoreMenu
            label={`Actions for ${agent.name}`}
            items={[
              { label: "New key", onSelect: onNewKey },
              { label: "Rename", onSelect: () => setRenaming(agent.name) },
              { label: "Remove agent", danger: true, onSelect: onRemove },
            ]}
          />
        )}
      </div>
      <AgentWorkStatus agentId={agent.id} />
      {agent.tokens.length === 0 && (
        <p className="ml-9 text-xs text-muted">No keys: it can't connect until it gets one.</p>
      )}
      {agent.tokens.map((token) => (
        <div key={token.id} className="ml-9 flex items-center gap-3 text-xs">
          {token.hint ? (
            <span className="w-24 font-mono">{keyName(token.hint)}</span>
          ) : (
            // Made before keys kept a hint.
            <span className="w-24 text-faint">older key</span>
          )}
          {inUse(token) ? (
            <span className="flex items-center gap-1.5 text-ok-ink">
              <span className="size-1.5 rounded-full bg-ok-ink" aria-hidden="true" />
              In use · {timeAgo(token.last_used_at ?? "")}
            </span>
          ) : (
            <span className="text-muted">
              {token.last_used_at ? `Last used ${timeAgo(token.last_used_at)}` : "Never used"}
            </span>
          )}
          <span className="text-faint">made {timeAgo(token.created_at)}</span>
          {isAdmin && (
            <button
              type="button"
              className="ml-auto text-faint hover:text-danger-ink"
              onClick={() => onRevoke(token)}
            >
              Revoke
            </button>
          )}
        </div>
      ))}
      <Runner
        runner={agent.runner}
        isAdmin={isAdmin}
        onSet={onSetRunner}
        onNewSecret={onNewSecret}
        onRemove={onRemoveRunner}
      />
    </li>
  );
}

const EVENTS: Record<WakeupEvent, string> = {
  assigned: "assigned",
  comment: "new comment",
  sent_back: "sent back",
  mention: "mentioned",
  accepted: "accepted out of triage",
  pr_blocked: "pull request needs fixing",
};

/** How a wake-up went, in words. */
function outcome(wakeup: WakeupInfo) {
  const tries = wakeup.attempts === 1 ? "1 try" : `${wakeup.attempts} tries`;
  switch (wakeup.status) {
    case "delivered":
      return {
        text: `Delivered ${timeAgo(wakeup.delivered_at ?? wakeup.created_at)}`,
        tone: "text-muted",
      };
    case "waiting":
      return { text: "Waiting until it's free", tone: "text-muted" };
    case "pending":
      return wakeup.last_error
        ? { text: `Retrying: ${wakeup.last_error} (${tries})`, tone: "text-warn-ink" }
        : { text: "Sending…", tone: "text-muted" };
    case "failed":
      return {
        text: `Gave up: ${wakeup.last_error ?? "no answer"} (${tries})`,
        tone: "text-danger-ink",
      };
  }
}

/**
 * Where Forge wakes the agent when there's work for it, and how the latest
 * wake-ups went. Admins set it up; everyone can see whether it's working.
 */
function Runner({
  runner,
  isAdmin,
  onSet,
  onNewSecret,
  onRemove,
}: {
  runner: RunnerInfo | null;
  isAdmin: boolean;
  onSet: (url: string) => Promise<void>;
  onNewSecret: () => void;
  onRemove: () => void;
}) {
  const [editing, setEditing] = useState<string | null>(null);
  const save = (event: FormEvent) => {
    event.preventDefault();
    const url = editing?.trim();
    if (!url) return;
    void onSet(url).then(() => setEditing(null));
  };

  if (editing !== null) {
    return (
      <form onSubmit={save} className="ml-9 flex items-center gap-2 text-xs">
        <input
          aria-label="Runner URL"
          // biome-ignore lint/a11y/noAutofocus: opened by pressing a button
          autoFocus
          placeholder="http://localhost:4100/wake"
          value={editing}
          onChange={(e) => setEditing(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setEditing(null);
          }}
          className="h-7 flex-1 rounded-md ui-field px-2 font-mono outline-none"
        />
        <Button type="submit" className="h-7 px-2 text-xs" disabled={!editing.trim()}>
          Save
        </Button>
        <button type="button" className="text-faint hover:text-fg" onClick={() => setEditing(null)}>
          Cancel
        </button>
      </form>
    );
  }

  if (!runner) {
    return isAdmin ? (
      <div className="ml-9 flex items-center gap-3 text-xs">
        <span className="text-muted">Not woken: it finds its work by checking in.</span>
        <button type="button" className="ui-link" onClick={() => setEditing("")}>
          Wake it when there's work
        </button>
      </div>
    ) : null;
  }

  return (
    <div className="ml-9 flex flex-col gap-1.5 text-xs">
      <div className="flex items-center gap-3">
        <span className="text-muted">Woken at</span>
        <span className="min-w-0 truncate font-mono" title={runner.url}>
          {runner.url}
        </span>
        {isAdmin && (
          <span className="ml-auto flex shrink-0 gap-3">
            <button
              type="button"
              className="text-faint hover:text-fg"
              onClick={() => setEditing(runner.url)}
            >
              Change
            </button>
            <button type="button" className="text-faint hover:text-fg" onClick={onNewSecret}>
              New secret
            </button>
            <button type="button" className="text-faint hover:text-danger-ink" onClick={onRemove}>
              Stop
            </button>
          </span>
        )}
      </div>
      {runner.wakeups.length === 0 ? (
        <p className="text-faint">
          No wake-ups yet. Assigning it an issue, commenting on one of its issues, sending one back
          or mentioning it wakes it.
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {runner.wakeups.map((wakeup) => {
            const { text, tone } = outcome(wakeup);
            return (
              <li key={wakeup.id} className="flex items-center gap-3">
                <span className="w-16 font-mono">{wakeup.issue}</span>
                <span className="w-24 text-muted">{EVENTS[wakeup.event]}</span>
                <span className={tone}>{text}</span>
                <span className="ml-auto text-faint">{timeAgo(wakeup.created_at)}</span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/** Shown once: the secret a runner checks wake-ups with. */
function SecretNotice({ name, secret }: { name: string; secret: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="mt-4 flex flex-col gap-3 rounded-lg bg-surface p-4">
      <p>
        The runner secret for <span className="font-medium">{name}</span>. Copy it now: it won't be
        shown again.
      </p>
      <code className="block break-all rounded-md bg-panel p-3 font-mono text-xs">{secret}</code>
      <p className="text-xs text-muted">
        Forge POSTs JSON like{" "}
        <span className="font-mono">
          {'{"event": "assigned", "issue": "FOR-12", "workspace_id": …}'}
        </span>{" "}
        and signs it: <span className="font-mono">X-Forge-Signature: sha256=…</span> is the
        HMAC-SHA256 of the body with this secret. The runner answers 2xx once it has it; otherwise
        Forge tries again later.
      </p>
      <Button
        variant="secondary"
        className="self-start"
        onClick={async () => {
          await navigator.clipboard.writeText(secret).catch(() => {});
          setCopied(true);
        }}
      >
        {copied ? "Copied" : "Copy secret"}
      </Button>
    </div>
  );
}

/** Shown once: the key, and the command that connects Claude Code. */
function TokenNotice({ name, token }: { name: string; token: string }) {
  const url = publicUrl("/mcp");
  const command = `claude mcp add --transport http forge ${url} --header "Authorization: Bearer ${token}"`;
  const [copied, setCopied] = useState(false);
  return (
    <div className="mt-4 flex flex-col gap-3 rounded-lg bg-surface p-4">
      <p>
        A new key for <span className="font-medium">{name}</span>,{" "}
        <span className="font-mono">{keyName(token.slice(-4))}</span>. Copy it now: it won't be
        shown again.
      </p>
      <p className="text-xs text-muted">To connect Claude Code, run:</p>
      <code className="block break-all rounded-md bg-panel p-3 font-mono text-xs">{command}</code>
      <p className="text-xs text-muted">
        Other MCP clients: server <span className="font-mono">{url}</span>, header{" "}
        <span className="font-mono">Authorization: Bearer {"<key>"}</span>.
      </p>
      <Button
        variant="secondary"
        className="self-start"
        onClick={async () => {
          await navigator.clipboard.writeText(command).catch(() => {});
          setCopied(true);
        }}
      >
        {copied ? "Copied" : "Copy command"}
      </Button>
    </div>
  );
}
