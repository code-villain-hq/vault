import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { taskUsesTracker } from "../lib/tracker";
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
import { Confirm, ErrorMessage, Field, useFailure } from "./ui";

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
        <GithubIssuesSettings toCode={toCode} />
      ) : (
        <JiraSettings />
      )}
    </>
  );
}

function GithubIssuesSettings({ toCode }: { toCode: () => void }) {
  const settings = useStore((s) => s.settings);
  const tasks = useStore((s) => s.tasks);
  const refresh = useStore((s) => s.refreshSettings);
  const workspace = useWorkspace();
  const failure = useFailure();
  const [confirm, setConfirm] = useState<string | null>(null);
  if (!settings) return null;
  const here = settings;
  const selected = here.github_issue_source;
  const sources = here.github_sources.filter((g) => g.shared);
  const live = here.github_sources.find((g) => g.key === selected)?.status === "connected";
  const keeping = selected ? tasks.filter((t) => taskUsesTracker(here, t)).length : 0;
  async function choose(source: string) {
    failure.clear();
    try {
      await api.setTracker("github", source);
      await refresh();
      setConfirm(null);
    } catch (error) {
      failure.fail(error);
    }
  }
  return (
    <>
      <p className="muted">
        Tickets use one GitHub source. Other connected hosts still provide pull requests and
        reviews.
      </p>
      <Field
        label="GitHub Issues source"
        hint={workspace?.admin === false ? "Admins change this for the workspace." : undefined}
      >
        {workspace?.admin === false ? (
          <span>{selected ?? "Not selected"}</span>
        ) : (
          <select
            className="ui-field"
            value={selected ?? ""}
            onChange={(event) => {
              const source = event.target.value;
              if (keeping > 0) setConfirm(source);
              else void choose(source);
            }}
          >
            <option value="" disabled>
              Choose a source
            </option>
            {selected && !sources.some((s) => s.key === selected) && (
              <option value={selected} disabled>
                {selected} (unavailable)
              </option>
            )}
            {sources.map((source) => (
              <option key={source.key} value={source.key}>
                {source.key}
              </option>
            ))}
          </select>
        )}
      </Field>
      {!live && (
        <button className="ui-button ui-button-primary" onClick={toCode}>
          {selected ? `Connect ${selected}` : "Manage GitHub sources"}
        </button>
      )}
      {confirm && (
        <Confirm
          title={`Take tickets from ${confirm}?`}
          body={`${keeping} existing task${keeping === 1 ? "" : "s"} will keep the original ticket link. Layer will no longer read or move those tickets.`}
          confirmLabel={`Use ${confirm}`}
          danger={false}
          onConfirm={() => choose(confirm)}
          onCancel={() => setConfirm(null)}
        />
      )}
      <ErrorMessage>{failure.error}</ErrorMessage>
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
