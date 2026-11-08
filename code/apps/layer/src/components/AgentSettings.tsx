import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { AcpStanding, AcpVersion, AgentStatus } from "../lib/types";
import { useStore } from "../store";
import { BusyLabel, ErrorMessage, Field, Switch, useFailure } from "./ui";
import { useUiPrefs } from "./useUiPrefs";

/** The pane limit's choices (PANE-2): the default, and up to `MAX_PANES` in `pty.rs`. */
const PANE_LIMITS = [32, 48, 64];

/** A pill only where something is wrong (ACP-17): an agent that works stays quiet. */
const PILL: Partial<Record<AcpStanding, [string, string]>> = {
  older: ["Too old", "ui-status-danger"],
  newer: ["Not checked yet", "ui-status-warn"],
  unknown: ["Version unknown", "ui-status-warn"],
};

/** The program an ACP command runs: `claude-agent-acp`, `opencode`. */
const program = (v: AcpVersion) => v.command.split(" ")[0];

/** The line under an agent: whether it runs as a conversation here, and what to do if not. */
function conversation(agent: AgentStatus, v: AcpVersion | undefined): string | null {
  if (!agent.installed) return "Install it to use it.";
  if (!v) return null;
  const at = `${program(v)} ${v.version}`;
  switch (v.standing) {
    case "supported":
      return `Runs as a conversation too (${at}).`;
    case "older":
      return `${at}. As a conversation it needs ${v.checked}: update it first.`;
    case "newer":
      return `${at}, a major release past the ${v.checked} checked: it may go wrong as a conversation.`;
    case "unknown":
      return `${program(v)} said no version. As a conversation it was checked with ${v.checked}.`;
    case "missing":
      return `As a conversation it needs ${program(v)}.`;
  }
}

/**
 * Settings → Agents: the agents on this Mac and where each stands as a
 * conversation (ACP-17), how their terminals behave, and where their tasks'
 * folders go.
 */
export function AgentSettings() {
  const settings = useStore((s) => s.settings);
  const refreshSettings = useStore((s) => s.refreshSettings);
  const { ui, saveUi, failedUnder } = useUiPrefs<"terminals" | "pane-limit">();
  const folder = useFailure();
  const [root, setRoot] = useState("");
  const rootSaved = settings
    ? settings.worktree_root_is_default
      ? ""
      : settings.worktree_root
    : null;
  useEffect(() => {
    if (rootSaved !== null) setRoot(rootSaved);
  }, [rootSaved]);

  async function saveRoot(path: string) {
    folder.clear();
    try {
      await api.setWorktreeRoot(path.trim() || null);
      await refreshSettings();
    } catch (e) {
      folder.fail(e);
    }
  }

  async function pickRoot() {
    const picked = await openDialog({ directory: true, title: "Task folder location" });
    if (typeof picked === "string") {
      setRoot(picked);
      await saveRoot(picked);
    }
  }

  if (!settings || !ui) return null;
  return (
    <>
      <AgentList />

      <Field
        label="Terminals"
        hint="Agents are resumed rather than restarted where their CLI supports it, so the conversation carries on."
      >
        <div className="switch-list">
          <Switch
            label="Put terminals back when the app reopens"
            detail="Panes that were open last time are reopened in the same worktrees."
            checked={ui.restore_panes}
            onChange={(v) => void saveUi({ restore_panes: v }, "terminals")}
          />
          <Switch
            label="Trust the folders this app creates"
            detail="Claude Code asks whether it trusts a folder the first time it starts there, and does nothing until answered — once per task, per repo. This answers it in advance, and only for worktrees and chat folders the app made itself."
            checked={ui.trust_agent_dirs}
            onChange={(v) => void saveUi({ trust_agent_dirs: v }, "terminals")}
          />
          <Switch
            label="Let agents read terminal output"
            detail="Agents can read what a terminal here has printed — a dev server's log, a test run — instead of starting a second copy. A shell's scrollback is a record of everything typed in it, so this stays off until you want it."
            checked={ui.agents_read_panes}
            onChange={(v) => void saveUi({ agents_read_panes: v }, "terminals")}
          />
        </div>
        {failedUnder("terminals")}
      </Field>

      <Field
        label="Most terminals running at once"
        hint="Every agent is a process of its own, and not a small one. The limit stops a bug, or an agent, from starting more than this Mac can hold. Terminals that have ended don't count."
      >
        <div className="ui-segmented">
          {PANE_LIMITS.map((n) => (
            <button
              key={n}
              className="ui-segment"
              aria-pressed={ui.pane_limit === n}
              onClick={() => void saveUi({ pane_limit: n }, "pane-limit")}
            >
              {n}
            </button>
          ))}
        </div>
        {failedUnder("pane-limit")}
      </Field>

      <Field
        label="Task folder location"
        hint={
          <>
            Each task gets a folder here, holding one worktree per repository it touches. Currently{" "}
            <code>{settings.worktree_root}</code>
            {settings.worktree_root_is_default ? " (default)" : ""}.
          </>
        }
      >
        <div className="row">
          <input
            className="ui-field"
            value={root}
            placeholder="(default: ~/.villain-worktrees)"
            onChange={(e) => setRoot(e.target.value)}
            onBlur={() => void saveRoot(root)}
          />
          <button className="ui-button" onClick={() => void pickRoot()}>
            Browse…
          </button>
        </div>
        <ErrorMessage>{folder.error}</ErrorMessage>
      </Field>
    </>
  );
}

/**
 * Every agent Layer knows, where it is on this Mac, and its ACP mode's
 * version against the one checked (ACP-17). The list of agents and the ACP
 * versions were two tabs, General and ACP, naming the same four agents.
 */
function AgentList() {
  const agents = useStore((s) => s.agents);
  const [versions, setVersions] = useState<AcpVersion[] | null>(null);
  const [busy, setBusy] = useState(false);
  const failure = useFailure();

  async function check() {
    setBusy(true);
    failure.clear();
    try {
      setVersions(await api.acpVersions());
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void check();
    // Once, as the tab opens; "Check again" after an update.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Field
      label="Agents on this Mac"
      hint="Each runs in a terminal. One that runs as a conversation (ACP) is checked against the version Layer was tried with."
    >
      <div className="col">
        {agents.map((a) => {
          const v = versions?.find((x) => x.id === a.id);
          const line = conversation(a, v);
          const pill = a.installed && v ? PILL[v.standing] : undefined;
          return (
            <div key={a.id} className="row agent-row">
              <div className="col agent-row-name">
                <span>
                  {a.name}{" "}
                  {a.path ? (
                    <code>{a.path}</code>
                  ) : (
                    <span className="muted">not found on PATH</span>
                  )}
                </span>
                {line && <span className="muted">{line}</span>}
              </div>
              <div className="spacer" />
              {pill && <span className={`ui-status ${pill[1]}`}>{pill[0]}</span>}
            </div>
          );
        })}
        <div className="row">
          <button className="ui-button" disabled={busy} onClick={() => void check()}>
            <BusyLabel busy={busy} idle="Check again" working="Checking…" />
          </button>
        </div>
        <ErrorMessage>{failure.error}</ErrorMessage>
      </div>
    </Field>
  );
}
