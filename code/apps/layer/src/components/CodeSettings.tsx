import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { githubRefusal } from "../lib/derive";
import { useStore } from "../store";
import {
  ChangeRow,
  ConnectRow,
  heldText,
  IN_KEYCHAIN,
  Theirs,
  useFold,
  useHeld,
  useTheirsLocked,
  Yours,
} from "./SharedSettings";
import { ErrorMessage, Field, useFailure } from "./ui";

/**
 * Settings → Code: GitHub's connection, for pull requests and their checks.
 * Connected, it is text, and the form opens behind Change… (SET-14).
 */
export function CodeSettings() {
  const settings = useStore((s) => s.settings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const refreshPrs = useStore((s) => s.refreshPrs);
  const ghRefusal = useStore((s) => githubRefusal(s.prs));
  // Connecting fails under its button, disconnecting under the connected
  // row (NOTE-6).
  const github = useFailure();
  const unlink = useFailure();
  const [busy, setBusy] = useState(false);
  const connected = settings?.github_connected ?? false;
  const { folded, change, fold } = useFold(connected);
  const locked = useTheirsLocked("github");

  const [ghApi, setGhApi] = useState("https://api.github.com");
  const [ghWeb, setGhWeb] = useState("https://github.com");
  const [ghTeam, setGhTeam] = useState("");
  const [ghToken, setGhToken] = useState("");

  const heldGithub = useHeld("github");
  // The form follows what is stored for it, and only when that changes
  // (as on Tickets).
  const loaded = settings !== null;
  const g = settings?.github;
  const ghApiNow = g ? g.api_url : heldText(heldGithub, "api_url");
  const ghWebNow = g ? g.web_url : heldText(heldGithub, "web_url");
  const ghTeamNow = g ? g.review_team : heldText(heldGithub, "review_team");
  useEffect(() => {
    if (!loaded) return;
    setGhApi(ghApiNow ?? "https://api.github.com");
    setGhWeb(ghWebNow ?? "https://github.com");
    setGhTeam(ghTeamNow ?? "");
  }, [loaded, ghApiNow, ghWebNow, ghTeamNow]);

  function cancel() {
    setGhApi(ghApiNow ?? "https://api.github.com");
    setGhWeb(ghWebNow ?? "https://github.com");
    setGhTeam(ghTeamNow ?? "");
    setGhToken("");
    github.clear();
    fold();
  }

  async function connectGithub() {
    setBusy(true);
    github.clear();
    try {
      await api.githubConnect(ghApi.trim(), ghWeb.trim(), ghToken.trim(), ghTeam.trim() || null);
      setGhToken("");
      await refreshSettings();
      fold();
      // The PR tabs still hold what the old token was told.
      void refreshPrs();
    } catch (e) {
      github.fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function disconnect() {
    unlink.clear();
    try {
      await api.disconnect("github");
      await refreshSettings();
    } catch (e) {
      unlink.fail(e);
    }
  }

  return (
    <>
      <p className="muted" style={{ marginTop: 0 }}>
        Pull requests, their checks and reviews come from GitHub, or GitHub Enterprise Server.
      </p>
      {connected && (
        <div className="row" style={{ marginBottom: 14 }}>
          {ghRefusal === "rejected" ? (
            <span className="ui-status ui-status-danger">token rejected</span>
          ) : ghRefusal === "sso" ? (
            <span className="ui-status ui-status-danger">needs single sign-on</span>
          ) : (
            <span className="ui-status">connected</span>
          )}
          <span className="muted">{settings?.github?.api_url}</span>
          <div className="spacer" />
          <button className="ui-button ui-button-danger" onClick={() => void disconnect()}>
            Disconnect
          </button>
        </div>
      )}
      <ErrorMessage>{unlink.error}</ErrorMessage>
      <Theirs
        kind="github"
        what="GitHub"
        rows={[
          ["API URL", ghApiNow],
          ["Web URL", ghWebNow],
          ["Review team", ghTeamNow],
        ]}
        folded={folded}
      >
        <Field
          label="API URL"
          hint="github.com uses https://api.github.com. Enterprise Server uses https://ghe.example.com/api/v3."
        >
          <input className="ui-field" value={ghApi} onChange={(e) => setGhApi(e.target.value)} />
        </Field>
        <Field label="Web URL" hint="For links that open in the browser.">
          <input className="ui-field" value={ghWeb} onChange={(e) => setGhWeb(e.target.value)} />
        </Field>
        <Field
          label="Review team"
          hint="Optional. A second list of pull requests asked of this team: org/fe, or @fe with the read:org scope."
        >
          <input
            className="ui-field"
            value={ghTeam}
            onChange={(e) => setGhTeam(e.target.value)}
            placeholder="@fe"
          />
        </Field>
      </Theirs>
      <Yours rows={[["Personal access token", IN_KEYCHAIN]]} folded={folded}>
        <Field label="Personal access token" hint="Needs repo scope. Stored in your keychain.">
          <input
            className="ui-field"
            type="password"
            value={ghToken}
            onChange={(e) => setGhToken(e.target.value)}
            placeholder={connected ? "•••••••• (leave blank to keep)" : ""}
          />
        </Field>
      </Yours>
      {folded ? (
        <ChangeRow
          note={
            locked
              ? "Your token: it is tried before it is kept."
              : "The addresses, the review team or the token: they are tried together."
          }
          onClick={change}
        />
      ) : (
        <ConnectRow
          connected={connected}
          busy={busy}
          disabled={!ghApi.trim() || (!ghToken.trim() && !connected)}
          onConnect={() => void connectGithub()}
          onCancel={cancel}
        />
      )}
      <ErrorMessage>{github.error}</ErrorMessage>
    </>
  );
}
