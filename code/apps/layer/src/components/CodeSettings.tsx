import { useState } from "react";
import { api } from "../lib/api";
import { githubRefusal, TOKEN_NEEDS_SSO, TOKEN_REJECTED } from "../lib/derive";
import type { GithubSource } from "../lib/types";
import { useStore } from "../store";
import { ConnectRow, IN_KEYCHAIN, Theirs, Yours } from "./SharedSettings";
import { SpecSettings } from "./SpecSettings";
import { ErrorMessage, Field, useFailure } from "./ui";

export function CodeSettings() {
  const sources = useStore((s) => s.settings?.github_sources);
  const [editing, setEditing] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  if (!sources) return null;

  const close = () => {
    setEditing(null);
    setAdding(false);
  };
  return (
    <>
      <p className="muted" style={{ marginTop: 0 }}>
        Connect GitHub.com and any GitHub Enterprise Server you use. Each has its own token.
      </p>
      {sources.map((source) => (
        <SourceRow
          key={source.key}
          source={source}
          editing={editing === source.key}
          onEdit={() => {
            setAdding(false);
            setEditing(source.key);
          }}
          onDone={close}
        />
      ))}
      {adding ? (
        <GithubForm onDone={close} />
      ) : (
        <button
          type="button"
          className={`ui-button${sources.length === 0 ? " ui-button-primary" : ""}`}
          onClick={() => {
            setEditing(null);
            setAdding(true);
          }}
        >
          Add GitHub source
        </button>
      )}
      <SpecSettings />
    </>
  );
}

function SourceRow({
  source,
  editing,
  onEdit,
  onDone,
}: {
  source: GithubSource;
  editing: boolean;
  onEdit: () => void;
  onDone: () => void;
}) {
  const failure = useFailure();
  const refresh = useStore((s) => s.refreshSettings);
  const refusal = useStore((s) => {
    const queue = s.reviewQueue?.sources.find((q) => q.source === source.key);
    const errors = [queue?.mine_error, queue?.team?.error, queue?.authored?.error];
    if (errors.includes(TOKEN_REJECTED)) return "rejected";
    return githubRefusal(s.prs, source.key) ?? (errors.includes(TOKEN_NEEDS_SSO) ? "sso" : null);
  });
  const [busy, setBusy] = useState(false);
  const connected = source.status === "connected";
  const reconnect = source.status === "reconnect_required";
  async function disconnect() {
    failure.clear();
    setBusy(true);
    try {
      await api.disconnect("github", source.key);
      await refresh({ githubDisconnected: true });
    } catch (error) {
      failure.fail(error);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="settings-block" aria-label={source.key}>
      <div className="row">
        <h3 className="settings-block-title">{source.key}</h3>
        <span className={`ui-status${reconnect || refusal ? " ui-status-danger" : ""}`}>
          {refusal === "rejected"
            ? "token rejected"
            : refusal === "sso"
              ? "needs single sign-on"
              : connected
                ? "connected"
                : reconnect
                  ? "reconnect required"
                  : "not connected"}
        </span>
        <div className="spacer" />
        {!editing && (
          <button type="button" className="ui-button" onClick={onEdit} disabled={busy}>
            {connected ? "Change…" : reconnect ? "Reconnect…" : "Connect…"}
          </button>
        )}
        {source.status !== "disconnected" && !editing && (
          <button
            type="button"
            className="ui-button ui-button-danger"
            disabled={busy}
            onClick={() => void disconnect()}
          >
            Disconnect
          </button>
        )}
      </div>
      {editing ? (
        <GithubForm
          key={`${source.key}:${source.revision}:${source.api_url}`}
          source={source}
          onDone={onDone}
        />
      ) : (
        <Theirs kind="github" what={source.key} shared={source.shared} rows={rows(source)} folded>
          {null}
        </Theirs>
      )}
      <ErrorMessage>{failure.error}</ErrorMessage>
    </section>
  );
}

function rows(source?: GithubSource): [string, string | null | undefined][] {
  return [
    ["API URL", source?.api_url],
    ["Web URL", source?.web_url],
    ["Review team", source?.review_team],
  ];
}

function GithubForm({ source, onDone }: { source?: GithubSource; onDone: () => void }) {
  const refresh = useStore((s) => s.refreshSettings);
  const failure = useFailure();
  const [apiUrl, setApiUrl] = useState(source?.api_url ?? "");
  const [webUrl, setWebUrl] = useState(source?.web_url ?? "");
  const [team, setTeam] = useState(source?.review_team ?? "");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const connected = source?.status === "connected";
  const canReuse =
    connected && apiUrl.trim() === source.api_url && webUrl.trim() === source.web_url;
  async function connect() {
    setBusy(true);
    failure.clear();
    try {
      const api_url = apiUrl.trim();
      const web_url = webUrl.trim();
      const typed = token.trim();
      // A new token or address is a new connection; a team alone is not.
      const githubReconnected =
        !connected || !!typed || api_url !== source.api_url || web_url !== source.web_url;
      // Match the native save's normalization: blank and a lone @ mean no team.
      const rawTeam = team.trim();
      const reviewTeam = rawTeam.replace(/^@+/, "").trim() || null;
      await api.githubConnect(api_url, web_url, typed, rawTeam || null, source?.key ?? null);
      setToken("");
      await refresh(
        githubReconnected || !source
          ? { githubReconnected: true }
          : {
              githubReconnected: false,
              githubReviewTeam: { source: source.key, team: reviewTeam },
            },
      );
      onDone();
    } catch (error) {
      failure.fail(error);
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <Theirs
        kind="github"
        what={source?.key ?? "GitHub"}
        shared={source?.shared ?? false}
        rows={rows(source)}
      >
        <Field
          label="API URL"
          hint="The token goes here. GitHub.com uses https://api.github.com. Enterprise Server usually ends in /api/v3."
        >
          <input
            className="ui-field"
            placeholder="https://api.github.com"
            value={apiUrl}
            onChange={(e) => setApiUrl(e.target.value)}
          />
        </Field>
        <Field label="Web URL" hint="The host your repositories call origin.">
          <input
            className="ui-field"
            placeholder="https://github.com"
            value={webUrl}
            onChange={(e) => setWebUrl(e.target.value)}
          />
        </Field>
        <Field label="Review team" hint="Optional: org/team or @team with read:org scope.">
          <input className="ui-field" value={team} onChange={(e) => setTeam(e.target.value)} />
        </Field>
      </Theirs>
      <Yours>
        <Field
          label="Personal access token"
          hint={
            canReuse
              ? `The current token is ${IN_KEYCHAIN}.`
              : "Needs repo scope. Stored in your keychain."
          }
        >
          <input
            className="ui-field"
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder={canReuse ? "Leave blank to keep the token" : ""}
          />
        </Field>
      </Yours>
      <ConnectRow
        connected={source?.status === "connected" || source?.status === "reconnect_required"}
        busy={busy}
        disabled={!apiUrl.trim() || !webUrl.trim() || (!canReuse && !token.trim())}
        onConnect={() => void connect()}
        onCancel={onDone}
        cancel
      />
      <ErrorMessage>{failure.error}</ErrorMessage>
    </>
  );
}
