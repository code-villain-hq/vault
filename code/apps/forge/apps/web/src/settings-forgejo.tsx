import { type Confirmation, ConfirmDialog } from "@code-villain/web";
import { useCallback, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { timeAgo } from "./data";
import { PageHeader } from "./layout";
import { Button, ErrorMessage, TextField } from "./ui";
import { useWorkspace } from "./workspace";

/** The workspace's Forgejo connection, as anyone in it may see it: never the token. */
export interface ForgejoStatus {
  connected: boolean;
  server: string | null;
  repositories: string[];
  connected_at: string | null;
  /** When a check last worked. */
  checked_at: string | null;
  /** When the last check failed, if it did, and why. */
  failed_at: string | null;
  check_error: string | null;
  up_to_date: boolean;
}

/** The workspace's Forgejo connection; asked again every `every` ms, if given. */
export function useForgejoStatus(every?: number) {
  const { workspace } = useWorkspace();
  const [status, setStatus] = useState<ForgejoStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async () => {
    try {
      setStatus(await api<ForgejoStatus>("GET", `/api/w/${workspace.id}/forgejo`));
      setError(null);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [workspace.id]);
  useEffect(() => {
    void reload();
    if (!every) return;
    const timer = setInterval(() => void reload(), every);
    return () => clearInterval(timer);
  }, [reload, every]);
  return { status, error, reload };
}

export function ForgejoSettingsPage() {
  const { workspace } = useWorkspace();
  const admin = workspace.role === "admin";
  const { status, error, reload } = useForgejoStatus(60_000);
  const [editing, setEditing] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  const disconnect = () =>
    setConfirmation({
      title: "Disconnect Forgejo?",
      body: "Forge stops checking its pull requests. Issues keep the links they have.",
      action: "Disconnect",
      onConfirm: () => {
        void api("DELETE", `/api/w/${workspace.id}/forgejo`)
          .catch(() => {})
          .then(reload);
      },
    });

  return (
    <>
      <PageHeader title="Forgejo" />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-8 px-6 py-8">
          <ErrorMessage>{error}</ErrorMessage>
          <section>
            <h2 className="mb-1 font-medium">Connect Forgejo</h2>
            <p className="text-muted">
              With Forgejo connected, Forge checks its pull requests about once a minute and links
              those that mention an issue. One that says "Fixes FOR-12" (or has FOR-12 in its branch
              name) moves FOR-12 to review once it can be merged, and to done once it's merged and
              no other one is open.
            </p>
          </section>

          {status && (!status.connected || editing) && admin && (
            <ConnectForm
              status={status}
              onSaved={() => {
                setEditing(false);
                void reload();
              }}
              onCancel={status.connected ? () => setEditing(false) : undefined}
            />
          )}
          {status && !status.connected && !admin && (
            <p className="text-muted">Ask a workspace admin to connect Forgejo.</p>
          )}

          {status?.connected && !editing && (
            <section className="flex flex-col gap-4">
              <div className="flex flex-col gap-1">
                <span className="font-medium">{status.server}</span>
                <CheckLine status={status} />
              </div>
              <ul className="-mx-3 flex flex-col gap-0.5">
                {status.repositories.map((repository) => (
                  <li key={repository} className="rounded-md px-3 py-1.5 hover:bg-hover">
                    {repository}
                  </li>
                ))}
              </ul>
              {admin && (
                <div className="flex gap-2">
                  <Button variant="secondary" onClick={() => setEditing(true)}>
                    Change…
                  </Button>
                  <Button variant="secondary" onClick={disconnect}>
                    Disconnect
                  </Button>
                </div>
              )}
            </section>
          )}
        </div>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

/** When Forge last checked, and why the last check failed if it did. */
function CheckLine({ status }: { status: ForgejoStatus }) {
  const failed =
    status.failed_at &&
    (!status.checked_at || Date.parse(status.failed_at) > Date.parse(status.checked_at));
  return (
    <span className={`text-xs ${failed ? "text-danger-ink" : "text-faint"}`}>
      {status.checked_at ? `Checked ${timeAgo(status.checked_at)}` : "Not checked yet"}
      {failed && status.check_error && ` · Last check failed: ${status.check_error}`}
    </span>
  );
}

function ConnectForm({
  status,
  onSaved,
  onCancel,
}: {
  status: ForgejoStatus;
  onSaved: () => void;
  onCancel?: () => void;
}) {
  const { workspace } = useWorkspace();
  const [server, setServer] = useState(status.server ?? "");
  const [repositories, setRepositories] = useState(status.repositories.join("\n"));
  const [token, setToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The saved token only ever goes to the server it was given for.
  const keepsToken = status.connected && server.trim().replace(/\/+$/, "") === status.server;

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await api("PUT", `/api/w/${workspace.id}/forgejo`, {
        server,
        repositories: repositories
          .split(/[\s,]+/)
          .map((r) => r.trim())
          .filter(Boolean),
        token: token.trim() || null,
      });
      onSaved();
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <form onSubmit={save} className="flex flex-col gap-4">
      <ErrorMessage>{error}</ErrorMessage>
      <TextField
        label="Server"
        placeholder="https://git.example.com"
        value={server}
        onChange={(e) => setServer(e.target.value)}
        required
      />
      <div className="flex flex-col gap-1.5">
        <label htmlFor="forgejo-repositories" className="text-xs font-medium text-muted">
          Repositories
        </label>
        <textarea
          id="forgejo-repositories"
          placeholder="owner/name, one per line"
          value={repositories}
          onChange={(e) => setRepositories(e.target.value)}
          rows={3}
          required
          className="ui-field resize-none rounded-md px-3 py-2 font-mono placeholder:text-faint"
        />
      </div>
      <TextField
        label="Access token"
        type="password"
        autoComplete="off"
        placeholder={keepsToken ? "Leave empty to keep the saved one" : ""}
        hint="A token that can read the repositories (read:repository). It stays on the server."
        value={token}
        onChange={(e) => setToken(e.target.value)}
        required={!keepsToken}
      />
      <div className="flex gap-2">
        <Button type="submit" disabled={saving}>
          {saving ? "Connecting…" : status.connected ? "Reconnect" : "Connect"}
        </Button>
        {onCancel && (
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
        )}
      </div>
    </form>
  );
}
