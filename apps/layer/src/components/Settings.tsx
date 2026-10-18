import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import { useStore } from "../store";
import { Field, Modal } from "./ui";
import { TicketFlowSettings } from "./TicketFlow";
import { PhoneSettings } from "./PhoneSettings";
import { BrowserSettings } from "./BrowserSettings";
import { ObservabilitySettings } from "./ObservabilitySettings";
import { SlackSettings } from "./SlackSettings";
import { AppearanceSettings } from "./AppearanceSettings";

type Section = "appearance" | "jira" | "github" | "slack" | "observability" | "phone" | "browser" | "general";

export function Settings() {
  const settings = useStore((s) => s.settings);
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => { void getVersion().then(setVersion).catch(() => {}); }, []);
  const agents = useStore((s) => s.agents);
  const toggleSettings = useStore((s) => s.toggleSettings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const refreshIssues = useStore((s) => s.refreshIssues);
  const toast = useStore((s) => s.toast);
  const fail = useStore((s) => s.fail);

  const [section, setSection] = useState<Section>(
    () => (useStore.getState().settingsSection as Section | null) ?? "appearance",
  );
  const [busy, setBusy] = useState(false);

  const [jiraUrl, setJiraUrl] = useState("");
  const [jiraEmail, setJiraEmail] = useState("");
  const [jiraToken, setJiraToken] = useState("");
  const [jiraProject, setJiraProject] = useState("");
  const [jiraJql, setJiraJql] = useState("");

  const [ghApi, setGhApi] = useState("https://api.github.com");
  const [ghWeb, setGhWeb] = useState("https://github.com");
  const [ghTeam, setGhTeam] = useState("");
  const [ghToken, setGhToken] = useState("");

  const [worktreeRoot, setWorktreeRoot] = useState("");

  // Each form follows what is stored for it, and only when that changes.
  // Keyed on the whole settings object, every save anywhere — a switch on
  // Appearance — refreshed it and reset every field, so a JQL half typed on
  // the Jira page was gone by the time you came back to it.
  const loaded = settings !== null;
  const j = settings?.jira;
  useEffect(() => {
    if (!loaded) return;
    setJiraUrl(j?.base_url ?? "");
    setJiraEmail(j?.email ?? "");
    setJiraProject(j?.project_key ?? "");
    setJiraJql(j?.jql ?? "");
  }, [loaded, j?.base_url, j?.email, j?.project_key, j?.jql]);
  const g = settings?.github;
  useEffect(() => {
    if (!loaded) return;
    setGhApi(g?.api_url ?? "https://api.github.com");
    setGhWeb(g?.web_url ?? "https://github.com");
    setGhTeam(g?.review_team ?? "");
  }, [loaded, g?.api_url, g?.web_url, g?.review_team]);
  const rootSaved = settings ? (settings.worktree_root_is_default ? "" : settings.worktree_root) : null;
  useEffect(() => {
    if (rootSaved !== null) setWorktreeRoot(rootSaved);
  }, [rootSaved]);

  async function connectJira() {
    setBusy(true);
    try {
      const who = await api.jiraConnect(
        jiraUrl.trim(), jiraEmail.trim(), jiraToken.trim(),
        jiraProject.trim() || null, jiraJql.trim() || null,
      );
      setJiraToken("");
      toast("success", `Jira connected as ${who}`);
      await refreshSettings();
      await refreshIssues();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function connectGithub() {
    setBusy(true);
    try {
      const login = await api.githubConnect(
        ghApi.trim(), ghWeb.trim(), ghToken.trim(), ghTeam.trim() || null,
      );
      setGhToken("");
      toast("success", `GitHub connected as ${login}`);
      await refreshSettings();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function disconnect(which: "jira" | "github") {
    try {
      await api.disconnect(which);
      await refreshSettings();
      toast("info", `${which} disconnected`);
    } catch (e) {
      fail(e);
    }
  }

  async function saveWorktreeRoot(path: string) {
    try {
      await api.setWorktreeRoot(path.trim() || null);
      await refreshSettings();
    } catch (e) {
      fail(e);
    }
  }

  async function pickWorktreeRoot() {
    const picked = await openDialog({ directory: true, title: "Task folder location" });
    if (typeof picked === "string") {
      setWorktreeRoot(picked);
      await saveWorktreeRoot(picked);
    }
  }

  return (
    <Modal
      title="Settings"
      wide
      tall
      onClose={() => toggleSettings(false)}
      toolbar={
        <div className="ui-tabs section-tabs" role="tablist">
          {/* Named for what each does, not the product behind it (SET-6). */}
          {([
            ["appearance", "Appearance"],
            ["jira", "Tickets"],
            ["github", "Code"],
            ["slack", "Messaging"],
            ["observability", "Observability"],
            ["phone", "Remote access"],
            ["browser", "Browser"],
            ["general", "General"],
          ] as [Section, string][]).map(([id, label]) => (
            <button
              key={id}
              className="ui-tab" role="tab" aria-selected={section === id}
              onClick={() => setSection(id)}
            >
              {label}
            </button>
          ))}
        </div>
      }
    >

      {section === "appearance" && <AppearanceSettings />}

      {section === "slack" && <SlackSettings />}

      {section === "jira" && (
        <>
          <p className="muted" style={{ marginTop: 0 }}>
            Tickets come from Jira: the ones assigned to you, each started as a task, and moved along as the
            work goes.
          </p>
          {settings?.jira_connected && (
            <div className="row" style={{ marginBottom: 14 }}>
              <span className="ui-status ui-status-ok">connected</span>
              <span className="muted">{settings.jira?.email}</span>
              <div className="spacer" />
              <button className="ui-button ui-button-danger" onClick={() => void disconnect("jira")}>
                Disconnect
              </button>
            </div>
          )}
          <Field label="Site URL">
            <input className="ui-field"
              value={jiraUrl}
              onChange={(e) => setJiraUrl(e.target.value)}
              placeholder="https://your-team.atlassian.net"
            />
          </Field>
          <Field label="Account email">
            <input className="ui-field" value={jiraEmail} onChange={(e) => setJiraEmail(e.target.value)} />
          </Field>
          <Field
            label="API token"
            hint={
              <>
                Stored in your macOS keychain, never in config files.{" "}
                <a
                  className="ui-link"
                  href="#"
                 
                  onClick={(e) => {
                    e.preventDefault();
                    void openUrl("https://id.atlassian.com/manage-profile/security/api-tokens");
                  }}
                >
                  Create one ↗
                </a>
              </>
            }
          >
            <input className="ui-field"
              type="password"
              value={jiraToken}
              onChange={(e) => setJiraToken(e.target.value)}
              placeholder={settings?.jira_connected ? "•••••••• (leave blank to keep)" : ""}
            />
          </Field>
          <Field label="Project key" hint="Optional. Scopes the default task query.">
            <input className="ui-field"
              value={jiraProject}
              onChange={(e) => setJiraProject(e.target.value.toUpperCase())}
              placeholder="ACME"
            />
          </Field>
          <Field label="JQL" hint="Optional. Overrides the default 'assigned to me, not done' query.">
            <input className="ui-field"
              value={jiraJql}
              onChange={(e) => setJiraJql(e.target.value)}
              placeholder="assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
            />
          </Field>
          <button
            className="ui-button ui-button-primary"
            // A blank token keeps the stored one, so once connected the
            // project key and JQL can be changed without pasting it again.
            disabled={
              busy || !jiraUrl.trim() || !jiraEmail.trim() ||
              (!jiraToken.trim() && !settings?.jira_connected)
            }
            onClick={() => void connectJira()}
          >
            {busy ? "Verifying…" : settings?.jira_connected ? "Save & reconnect" : "Connect"}
          </button>
          {settings?.jira_connected && <TicketFlowSettings />}
        </>
      )}

      {section === "github" && (
        <>
          <p className="muted" style={{ marginTop: 0 }}>
            Pull requests, their checks and reviews come from GitHub, or GitHub Enterprise Server.
          </p>
          {settings?.github_connected && (
            <div className="row" style={{ marginBottom: 14 }}>
              <span className="ui-status ui-status-ok">connected</span>
              <span className="muted">{settings.github?.api_url}</span>
              <div className="spacer" />
              <button className="ui-button ui-button-danger" onClick={() => void disconnect("github")}>
                Disconnect
              </button>
            </div>
          )}
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
            <input className="ui-field"
              value={ghTeam}
              onChange={(e) => setGhTeam(e.target.value)}
              placeholder="@fe"
            />
          </Field>
          <Field
            label="Personal access token"
            hint="Needs repo scope. Stored in your keychain."
          >
            <input className="ui-field"
              type="password"
              value={ghToken}
              onChange={(e) => setGhToken(e.target.value)}
              placeholder={settings?.github_connected ? "•••••••• (leave blank to keep)" : ""}
            />
          </Field>
          <button
            className="ui-button ui-button-primary"
            disabled={
              busy || !ghApi.trim() || (!ghToken.trim() && !settings?.github_connected)
            }
            onClick={() => void connectGithub()}
          >
            {busy ? "Verifying…" : settings?.github_connected ? "Save & reconnect" : "Connect"}
          </button>
        </>
      )}

      {section === "observability" && <ObservabilitySettings />}
      {section === "phone" && <PhoneSettings />}
      {section === "browser" && <BrowserSettings />}

      {section === "general" && (
        <>
          <Field
            label="Task folder location"
            hint={
              <>
                Each task gets a folder here, holding one worktree per repository
                it touches. Currently <code>{settings?.worktree_root}</code>
                {settings?.worktree_root_is_default ? " (default)" : ""}.
              </>
            }
          >
            <div className="row">
              <input className="ui-field"
                value={worktreeRoot}
                placeholder="(default: ~/.villain-worktrees)"
                onChange={(e) => setWorktreeRoot(e.target.value)}
                onBlur={() => void saveWorktreeRoot(worktreeRoot)}
              />
              <button className="ui-button" onClick={() => void pickWorktreeRoot()}>Browse…</button>
            </div>
          </Field>
          <Field
            label="Version"
            hint={
              <>
                Name this in a bug report. A crash is written to{" "}
                <code>~/Library/Logs/villain-layer/panic.log</code>.
              </>
            }
          >
            <div style={{ fontFamily: "var(--font-mono)", fontSize: "var(--type-ui-small-size)" }}>Layer {version ?? "…"}</div>
          </Field>
          <Field label="Detected agents">
            <div className="col">
              {agents.map((a) => (
                <div key={a.id} className="row" style={{ fontSize: "var(--type-ui-small-size)" }}>
                  <span className={`dot ${a.installed ? "live" : ""}`} />
                  <span style={{ width: 110 }}>{a.name}</span>
                  <span className="muted" style={{ fontFamily: "var(--font-mono)", fontSize: "var(--type-ui-small-size)" }}>
                    {a.path ?? "not found on PATH"}
                  </span>
                </div>
              ))}
            </div>
          </Field>
        </>
      )}
    </Modal>
  );
}
