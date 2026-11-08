import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";
import {
  ChangeRow,
  ConnectRow,
  heldText,
  IN_KEYCHAIN,
  Theirs,
  useFold,
  useHeld,
  useWorkspace,
  Yours,
} from "./SharedSettings";
import { TicketFlowSettings } from "./TicketFlow";
import { TrackerChoice } from "./TrackerChoice";
import { ErrorMessage, Field, useFailure } from "./ui";

const ATLASSIAN_TOKENS = "https://id.atlassian.com/manage-profile/security/api-tokens";

/**
 * Settings → Tickets: which tracker tickets come from (TRK-1), and Jira's
 * connection when it is Jira. GitHub Issues use the token under Code, so
 * that tracker only sends you there.
 */
export function TicketsSettings({ toCode }: { toCode: () => void }) {
  const settings = useStore((s) => s.settings);
  return (
    <>
      <TrackerChoice />
      {settings?.tracker_chosen === "github" ? (
        <>
          <p className="muted">
            Issues come from the workspace's repositories on GitHub: the open ones assigned to you,
            each started as a task, and closed when its pull request merges. They use the token
            under Code.
          </p>
          {settings.github_connected ? (
            <div className="row">
              <span className="ui-status">connected</span>
              <span className="muted">{settings.github?.web_url}</span>
            </div>
          ) : (
            <button className="ui-button ui-button-primary" onClick={toCode}>
              Connect GitHub
            </button>
          )}
        </>
      ) : (
        <JiraSettings />
      )}
    </>
  );
}

/**
 * Jira's connection, and how tickets follow the work once connected.
 * Connected, the connection is text, and its form opens behind Change…
 * (SET-14); how tickets follow the work stays out, being used day to day.
 */
function JiraSettings() {
  const settings = useStore((s) => s.settings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const refreshIssues = useStore((s) => s.refreshIssues);
  // Connecting fails under its button, disconnecting under the connected
  // row (NOTE-6).
  const jira = useFailure();
  const unlink = useFailure();
  const [busy, setBusy] = useState(false);
  const connected = settings?.jira_connected ?? false;
  const { folded, change, fold } = useFold(connected);

  const [jiraUrl, setJiraUrl] = useState("");
  const [jiraEmail, setJiraEmail] = useState("");
  const [jiraToken, setJiraToken] = useState("");
  const [jiraProject, setJiraProject] = useState("");
  const [jiraJql, setJiraJql] = useState("");

  const workspace = useWorkspace();
  const heldJira = useHeld("jira");
  const flowLocked = workspace?.admin === false && heldJira !== null;

  // The form follows what is stored for it, and only when that changes.
  // Keyed on the whole settings object, every save anywhere — a switch on
  // Appearance — refreshed it and reset every field, so a JQL half typed on
  // the Jira page was gone by the time you came back to it.
  const loaded = settings !== null;
  const j = settings?.jira;
  // Not connected here: the workspace's settings, so connecting takes only
  // yours (SET-8), and a Disconnect leaves them showing (SET-9).
  const jiraUrlNow = j ? j.base_url : heldText(heldJira, "base_url");
  const jiraProjectNow = j ? j.project_key : heldText(heldJira, "project_key");
  const jiraJqlNow = j ? j.jql : heldText(heldJira, "jql");
  useEffect(() => {
    if (!loaded) return;
    setJiraUrl(jiraUrlNow ?? "");
    setJiraEmail(j?.email ?? "");
    setJiraProject(jiraProjectNow ?? "");
    setJiraJql(jiraJqlNow ?? "");
  }, [loaded, jiraUrlNow, j?.email, jiraProjectNow, jiraJqlNow]);

  function cancel() {
    setJiraUrl(jiraUrlNow ?? "");
    setJiraEmail(j?.email ?? "");
    setJiraProject(jiraProjectNow ?? "");
    setJiraJql(jiraJqlNow ?? "");
    setJiraToken("");
    jira.clear();
    fold();
  }

  async function connectJira() {
    setBusy(true);
    jira.clear();
    try {
      // The connected row names who it signed in as: no toast (studio/design/behaviour.md).
      await api.jiraConnect(
        jiraUrl.trim(),
        jiraEmail.trim(),
        jiraToken.trim(),
        jiraProject.trim() || null,
        jiraJql.trim() || null,
      );
      setJiraToken("");
      await refreshSettings();
      fold();
      await refreshIssues();
    } catch (e) {
      jira.fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function disconnect() {
    unlink.clear();
    try {
      await api.disconnect("jira");
      await refreshSettings();
    } catch (e) {
      unlink.fail(e);
    }
  }

  return (
    <>
      <p className="muted">
        Tickets come from Jira: the ones assigned to you, each started as a task, and moved along as
        the work goes.
      </p>
      {connected && (
        <div className="row" style={{ marginBottom: 14 }}>
          <span className="ui-status">connected</span>
          <span className="muted">{settings?.jira?.email}</span>
          <div className="spacer" />
          <button className="ui-button ui-button-danger" onClick={() => void disconnect()}>
            Disconnect
          </button>
        </div>
      )}
      <ErrorMessage>{unlink.error}</ErrorMessage>
      <Theirs
        kind="jira"
        what="Jira"
        rows={[
          ["Site", jiraUrlNow],
          ["Project", jiraProjectNow],
          ["Query", jiraJqlNow, true],
        ]}
        folded={folded}
      >
        <Field label="Site URL">
          <input
            className="ui-field"
            value={jiraUrl}
            onChange={(e) => setJiraUrl(e.target.value)}
            placeholder="https://your-team.atlassian.net"
          />
        </Field>
        <Field label="Project key" hint="Optional. Scopes the default task query.">
          <input
            className="ui-field"
            value={jiraProject}
            onChange={(e) => setJiraProject(e.target.value.toUpperCase())}
            placeholder="ACME"
          />
        </Field>
        <Field label="JQL" hint="Optional. Overrides the default 'assigned to me, not done' query.">
          <input
            className="ui-field"
            value={jiraJql}
            onChange={(e) => setJiraJql(e.target.value)}
            placeholder="assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
          />
        </Field>
      </Theirs>
      <Yours
        rows={[
          ["Account email", j?.email],
          ["API token", IN_KEYCHAIN],
        ]}
        folded={folded}
      >
        <Field label="Account email">
          <input
            className="ui-field"
            value={jiraEmail}
            onChange={(e) => setJiraEmail(e.target.value)}
          />
        </Field>
        <Field
          label="API token"
          hint={
            <>
              Stored in your macOS keychain, never in config files.{" "}
              <a
                className="ui-link"
                href={ATLASSIAN_TOKENS}
                onClick={(e) => {
                  e.preventDefault();
                  void openUrl(ATLASSIAN_TOKENS);
                }}
              >
                Create one ↗
              </a>
            </>
          }
        >
          <input
            className="ui-field"
            type="password"
            value={jiraToken}
            onChange={(e) => setJiraToken(e.target.value)}
            placeholder={connected ? "•••••••• (leave blank to keep)" : ""}
          />
        </Field>
      </Yours>
      {folded ? (
        <ChangeRow
          note={
            flowLocked
              ? "Your email or the token: they are tried together."
              : "The site, project, query, your email or the token: they are tried together."
          }
          onClick={change}
        />
      ) : (
        <ConnectRow
          connected={connected}
          busy={busy}
          // A blank token keeps the stored one, so the project key and JQL
          // can be changed without pasting it again.
          disabled={!jiraUrl.trim() || !jiraEmail.trim() || (!jiraToken.trim() && !connected)}
          onConnect={() => void connectJira()}
          onCancel={cancel}
        />
      )}
      <ErrorMessage>{jira.error}</ErrorMessage>
      {connected && (
        // Where tickets go is the workspace's too (SET-7).
        <fieldset className="settings-locked" disabled={flowLocked}>
          <TicketFlowSettings />
          {flowLocked && (
            <p className="settings-alone">
              How tickets follow the work is {workspace.name}'s: admins change it.
            </p>
          )}
        </fieldset>
      )}
    </>
  );
}
