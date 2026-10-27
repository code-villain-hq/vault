import { useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { branchStem } from "../../lib/tracker";
import { useStore } from "../../store";
import { useSpecs } from "../../lib/specs";
import type { AgentStatus, Ticket, Project, TaskView } from "../../lib/types";
import { Combo, ErrorMessage, Field, Modal, useFailure } from "../ui";
import { RepoPicker } from "../RepoPicker";
import { suggestBase } from "../../lib/derive";
import { read, write } from "../../lib/persist";
import { IssueTypeIcon, isEpicType, type TypeMap } from "../IssueType";

export function StartWorkDialog({
  issue,
  projects,
  agents,
  types,
  subTasks = [],
  onClose,
}: {
  issue: Ticket;
  projects: Project[];
  agents: AgentStatus[];
  types: TypeMap;
  /** An epic's tasks here from before it was one (EPIC-13): they become its sub-tasks. */
  subTasks?: TaskView[];
  onClose: () => void;
}) {
  const refreshIssues = useStore((s) => s.refreshIssues);
  const refreshTasks = useStore((s) => s.refreshTasks);
  const refreshPanes = useStore((s) => s.refreshPanes);
  const select = useStore((s) => s.select);
  const setTab = useStore((s) => s.setTab);
  const setPendingSpec = useSpecs((s) => s.setPending);
  // Starting fails in the dialog, above its buttons (NOTE-6).
  const failure = useFailure();

  const [picked, setPicked] = useState<string[]>([]);
  const [reason, setReason] = useState<string | null>(null);
  /** Null until a default is picked; "" is the choice of no agent at all. */
  const [agentId, setAgentId] = useState<string | null>(null);
  const [suffix, setSuffix] = useState("");
  const [base, setBase] = useState("");
  const [baseOptions, setBaseOptions] = useState<string[]>([]);
  /** The base this dialog last filled in itself, as opposed to one typed. */
  const autoBase = useRef("");
  const [starting, setStarting] = useState(false);
  /** SPEC-18: start the agent from the Spec tab, once its spec is agreed. */
  const [specFirst, setSpecFirst] = useState(() => read("specFirst", true));

  const installed = agents.filter((a) => a.installed);
  // An epic-level ticket is made an epic: its spec makes the sub-tasks, and
  // no agent starts on it (EPIC-2).
  const epic = isEpicType(types, issue.issue_type);
  // Its work is already under way: it opens on those tasks, with no spec drafted.
  const adopting = epic && subTasks.length > 0;
  const theirRepos = [...new Set(subTasks.flatMap((t) => t.checkouts.map((c) => c.project_id)))].join(",");
  const selected = projects.filter((p) => picked.includes(p.id));

  // A default, once. Re-filled whenever the value was empty — and on every
  // render, since `installed` is a new list each time — this put the first
  // agent back the moment "No agent" was picked, and Start work launched one.
  const firstInstalled = installed[0]?.id;
  useEffect(() => {
    if (agentId === null && firstInstalled) setAgentId(firstInstalled);
  }, [firstInstalled, agentId]);

  useEffect(() => {
    failure.clear();
    setReason(null);
    setSuffix("");
    setBase("");
    setBaseOptions([]);
    setPicked([]);
    if (theirRepos) {
      setPicked(theirRepos.split(","));
      setReason(`the repositories its ${subTasks.length === 1 ? "task" : "tasks"} here ${subTasks.length === 1 ? "is" : "are"} in`);
      return;
    }
    let stop = false;
    // A suggestion, so it gives way to what was picked while it was out.
    api.suggestRepos({ issueKey: issue.key, epicKey: issue.epic_key })
      .then((s) => {
        if (stop) return;
        setPicked((p) => (p.length > 0 ? p : s.project_ids));
        setReason(s.reason);
      })
      .catch(() => { if (!stop) setReason(null); });
    return () => { stop = true; };
  }, [issue, theirRepos, failure.clear]);

  // Prefill from the repos' defaults when they agree; load the union of their
  // remote-tracking branches for the picker. Typing still works — a branch
  // pushed since the last fetch will not be in the list.
  useEffect(() => {
    if (selected.length === 0) {
      setBase("");
      setBaseOptions([]);
      return;
    }
    const defaults = [...new Set(selected.map((p) => p.default_branch))];
    // Suggest, do not overwrite: a base typed by hand survives adding another
    // repository. Only a box still showing the last suggestion follows it.
    const last = autoBase.current;
    setBase((cur) => suggestBase(cur, last, defaults));
    autoBase.current = suggestBase("", "", defaults);
    let cancelled = false;
    Promise.all(selected.map((p) => api.projectBranches(p.id).catch(() => [] as string[])))
      .then((lists) => {
        if (cancelled) return;
        const seen = new Set<string>();
        const merged: string[] = [];
        for (const list of lists) {
          for (const b of list) {
            if (!seen.has(b)) {
              seen.add(b);
              merged.push(b);
            }
          }
        }
        // Keep each repo's default near the top even when it is not the newest.
        for (const d of defaults) {
          if (!seen.has(d)) merged.unshift(d);
        }
        setBaseOptions(merged);
      });
    return () => { cancelled = true; };
  }, [picked.join(","), projects]);

  async function startWork() {
    if (picked.length === 0) return;
    setStarting(true);
    failure.clear();
    const spec = (epic && !adopting) || (!epic && specFirst && !!agentId);
    try {
      const task = await api.startWorkOnTicket(
        issue.key,
        picked,
        spec ? null : agentId || null,
        suffix || null,
        base.trim() || null,
        epic,
      );
      // Refresh the issues too: the ticket has usually just moved, and the
      // status on the card is the thing the move was meant to correct.
      await Promise.all([refreshTasks(), refreshPanes(), refreshIssues()]);
      select(task.id);
      if (spec) {
        setPendingSpec({ taskId: task.id, agentId: epic ? null : agentId || null });
        setTab("spec");
      }
      // The task is selected and its ticket's card moved: nothing more to say (NOTE-7).
      onClose();
    } catch (e) {
      failure.fail(e);
    } finally {
      setStarting(false);
    }
  }

  const baseHint = selected.length === 0
    ? "Pick repositories first."
    : selected.length === 1
      ? `New branch starts at this tip in ${selected[0].name}.`
      : "Created from this tip in every repo you picked. Blank uses each repo's own default.";

  return (
    <Modal
      title={`${issue.key} · ${issue.status}`}
      wide
      onClose={onClose}
      footer={
        <>
          <button className="ui-button" onClick={onClose}>Close</button>
          <button
            className="ui-button ui-button-primary"
            disabled={picked.length === 0 || starting}
            onClick={() => void startWork()}
          >
            {starting
              ? "Creating worktrees…"
              : adopting
                ? `Make it an epic in ${picked.length} repo${picked.length === 1 ? "" : "s"}`
                : epic
                ? `Write the epic's spec in ${picked.length} repo${picked.length === 1 ? "" : "s"}`
                : `Start work in ${picked.length} repo${picked.length === 1 ? "" : "s"}`}
          </button>
        </>
      }
    >
      <h3 style={{ margin: "0 0 10px", fontSize: "var(--type-ui-size)", fontWeight: 700, lineHeight: 1.4 }}>{issue.summary}</h3>

      <div className="row" style={{ marginBottom: 14, flexWrap: "wrap" }}>
        <span className="ui-chip chip-type">
          <IssueTypeIcon types={types} name={issue.issue_type} size={13} />
          {issue.issue_type}
        </span>
        {issue.priority && <span className="ui-chip">{issue.priority}</span>}
        {issue.assignee && <span className="ui-chip">{issue.assignee}</span>}
        {issue.epic_key && (
          <span className="ui-chip" title={issue.epic_summary ?? undefined}>
            epic {issue.epic_key}
          </span>
        )}
        {issue.components.map((c) => <span key={c} className="ui-chip">{c}</span>)}
        {issue.labels.map((l) => <span key={l} className="ui-chip">{l}</span>)}
      </div>

      <Field
        label="Repositories"
        hint="One worktree per repo, on the same branch, side by side in one task folder. You can add more later."
      >
        <RepoPicker
          projects={projects}
          picked={picked}
          onChange={setPicked}
          reason={reason}
        />
      </Field>

      <Field
        label="Branch"
        hint={`Created in every repo you picked, and names the task folder. Leave the suffix blank for just ${branchStem(issue.key, issue.summary)}.`}
      >
        <div className="ui-field branch-compose">
          <span className="branch-key">{branchStem(issue.key, issue.summary)}</span>
          <span className="branch-dash">-</span>
          <input className="ui-field"
            value={suffix}
            onChange={(e) => setSuffix(e.target.value)}
            placeholder="optional suffix"
          />
        </div>
      </Field>

      <Field label="Branch from" hint={baseHint}>
        <Combo
          value={base}
          options={baseOptions}
          placeholder={
            selected.length > 1 && !base
              ? "(each repo's default)"
              : selected[0]?.default_branch ?? "main"
          }
          empty="No branch matches"
          onChange={setBase}
        />
      </Field>

      {adopting ? (
        <div className="muted" style={{ marginBottom: 14, lineHeight: 1.6 }}>
          An epic: {subTasks.length === 1 ? "its task here becomes its sub-task" : `its ${subTasks.length} tasks here become its sub-tasks`},
          and its chat runs them. The work is under way, so no spec is drafted; one can still be
          written on its Spec tab.
        </div>
      ) : epic ? (
        <div className="muted" style={{ marginBottom: 14, lineHeight: 1.6 }}>
          An epic: its spec is written here, and its tasks become sub-tasks, each
          filed under {issue.key} and worked in a task of its own.
        </div>
      ) : <>
      <Field
        label="Agent"
        hint={
          picked.length > 1
            ? "Starts at the task root, where all the repos are visible as sibling folders, primed with the ticket and the layout."
            : "Starts in the new worktree with the ticket as its opening prompt."
        }
      >
        <select className="ui-field" value={agentId ?? ""} onChange={(e) => setAgentId(e.target.value)}>
          <option value="">No agent — just the worktrees</option>
          {installed.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
        </select>
      </Field>
      {agentId && (
        <label className="row" style={{ gap: 7, margin: "-6px 0 14px", cursor: "pointer" }}>
          <input
            type="checkbox"
            style={{ width: "auto" }}
            checked={specFirst}
            onChange={(e) => { setSpecFirst(e.target.checked); write("specFirst", e.target.checked); }}
          />
          Write a spec first: the agent starts from the Spec tab once you have agreed it
        </label>
      )}
      </>}

      {issue.description.trim() && (
        <Field label="Description">
          <div
            className="muted"
            style={{
              whiteSpace: "pre-wrap",
              maxHeight: 220,
              overflowY: "auto",
              background: "var(--dialog-field)",
              borderRadius: "var(--radius-md)",
              padding: 10,
              userSelect: "text",
            }}
          >
            {issue.description}
          </div>
        </Field>
      )}
      <ErrorMessage>{failure.error}</ErrorMessage>
    </Modal>
  );
}
