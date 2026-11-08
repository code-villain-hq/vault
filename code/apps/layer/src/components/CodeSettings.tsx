import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { githubRejected } from "../lib/derive";
import { useStore } from "../store";
import { heldText, Theirs, useHeld, Yours } from "./SharedSettings";
import { BusyLabel, ErrorMessage, Field, useFailure } from "./ui";

/** Settings → Code: GitHub's connection, for pull requests and their checks. */
export function CodeSettings() {
  const settings = useStore((s) => s.settings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const refreshPrs = useStore((s) => s.refreshPrs);
  const ghRejected = useStore((s) => githubRejected(s.prs));
  // Connecting fails under its button, disconnecting under the connected
  // row (NOTE-6).
  const github = useFailure();
  const unlink = useFailure();
  const [busy, setBusy] = useState(false);

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

  async function connectGithub() {
    setBusy(true);
    github.clear();
    try {
      await api.githubConnect(ghApi.trim(), ghWeb.trim(), ghToken.trim(), ghTeam.trim() || null);
      setGhToken("");
      await refreshSettings();
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
      {settings?.github_connected && (
        <div className="row" style={{ marginBottom: 14 }}>
          {ghRejected ? (
            <span className="ui-status ui-status-danger">token rejected</span>
          ) : (
            <span className="ui-status">connected</span>
          )}
          <span className="muted">{settings.github?.api_url}</span>
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
          ["API URL", ghApi],
          ["Web URL", ghWeb],
          ["Review team", ghTeam],
        ]}
      >
        <Field
          label="API URL"
          hint="github.com uses https://api.github.com. Enterprise Server uses https://ghe.example.com/api/v3."
        >
          <input className="ui-field" value={ghApi} onChange={(e) => setGhApi(e.target.value)} />
        </Field>
        <Field label="Web URL" hint="Used for building links back to the browser.">
          <input className="ui-field" value={ghWeb} onChange={(e) => setGhWeb(e.target.value)} />
        </Field>
        <Field
          label="Review team"
          hint="Optional. A second list of pull requests requested of this team, as @fe or org/fe. A bare name is looked up from the teams you belong to, which needs the read:org scope; org/fe does not."
        >
          <input
            className="ui-field"
            value={ghTeam}
            onChange={(e) => setGhTeam(e.target.value)}
            placeholder="@fe"
          />
        </Field>
      </Theirs>
      <Yours>
        <Field label="Personal access token" hint="Needs repo scope. Stored in your keychain.">
          <input
            className="ui-field"
            type="password"
            value={ghToken}
            onChange={(e) => setGhToken(e.target.value)}
            placeholder={settings?.github_connected ? "•••••••• (leave blank to keep)" : ""}
          />
        </Field>
      </Yours>
      <button
        className="ui-button ui-button-primary"
        disabled={busy || !ghApi.trim() || (!ghToken.trim() && !settings?.github_connected)}
        onClick={() => void connectGithub()}
      >
        <BusyLabel
          busy={busy}
          idle={settings?.github_connected ? "Reconnect" : "Connect"}
          working="Connecting…"
        />
      </button>
      <ErrorMessage>{github.error}</ErrorMessage>
    </>
  );
}
