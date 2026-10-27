import { useState } from "react";
import { api } from "../../lib/api";
import { taskReview } from "../../lib/derive";
import { openWaits, STATE_COLOR, subTaskState, type SubTaskState } from "../../lib/epicState";
import { useTaskAgents } from "../../lib/panes";
import { trackerOfKey } from "../../lib/tracker";
import { useStore } from "../../store";
import type { EpicView, SubTaskView, TaskView } from "../../lib/types";
import { Confirm, ErrorMessage, Spinner, useFailure } from "../ui";
import { FileSubTasks } from "./FileSubTasks";

/**
 * The Sub-tasks tab (EPIC-7, EPIC-8): what the approved spec makes, filed or
 * not, in the order they can start, grouped by the repository each starts
 * in. Filing leads while there is something to file; after that each row's
 * own action does.
 */
export function SubTasks({ epic, view }: { epic: TaskView; view: EpicView | null }) {
  const setTab = useStore((s) => s.setTab);
  const [filing, setFiling] = useState(false);

  if (!view) return <div className="epic-subs"><Spinner /></div>;
  // An epic made from tasks already under way has them and no spec yet (EPIC-13).
  if (!view.planned && view.sub_tasks.length === 0) {
    return (
      <div className="epic-subs">
        <div className="empty">
          <h2>No sub-tasks yet</h2>
          <p>
            The epic's spec says what they are. Approve its tasks on the Spec tab:
            each group of steps becomes a sub-task, filed under the epic.
          </p>
          <button className="ui-button ui-button-primary" onClick={() => setTab("spec")}>Open the spec</button>
        </div>
      </div>
    );
  }

  const all = view.sub_tasks;
  const toFile = all.filter((s) => !s.task_id).length;
  const repos = [...new Set(all.map((s) => s.repos[0] ?? ""))];

  return (
    <div className="epic-subs">
      {toFile > 0 && (
        <div className="epic-lead">
          <div className="what">
            <b>The approved spec makes {all.length === toFile ? "" : `${toFile} more `}{toFile === 1 ? "a sub-task" : `${toFile} sub-tasks`}.</b>{" "}
            <span className="muted">
              Each is {epic.issue_key ? `filed in ${trackerOfKey(epic.issue_key)} under ${epic.issue_key} and ` : ""}given its own task here.
            </span>
          </div>
          <button className="ui-button ui-button-primary" onClick={() => setFiling(true)}>
            File {toFile} sub-task{toFile === 1 ? "" : "s"}
          </button>
        </div>
      )}
      {repos.map((repo) => (
        <div key={repo}>
          {repos.length > 1 && <div className="epic-group">{repo}</div>}
          {all.filter((s) => (s.repos[0] ?? "") === repo).map((s) => (
            <SubTaskRow key={s.title} sub={s} all={all} />
          ))}
        </div>
      ))}
      {filing && <FileSubTasks epic={epic} count={toFile} onClose={() => setFiling(false)} />}
    </div>
  );
}

/** One sub-task: ticket, title, steps, requirements, and state with its action. */
function SubTaskRow({ sub, all }: { sub: SubTaskView; all: SubTaskView[] }) {
  const prs = useStore((s) => s.prs);
  const agents = useTaskAgents(sub.task_id ?? "");
  const waits = openWaits(sub, all, prs);
  const state = subTaskState(!!sub.task_id, taskReview(sub.task_id ? prs[sub.task_id] ?? [] : []), agents, waits.length > 0);
  const select = useStore((s) => s.select);
  const dot = state === "waits" ? "queued" : agents.waiting ? "idle" : agents.busy ? "live" : "";
  const open = () => { if (sub.task_id) select(sub.task_id); };

  return (
    <div className={`epic-row${sub.task_id ? "" : " proposed"}${state === "waits" ? " queued" : ""}`} onClick={open}>
      <span className="key">{sub.key ?? "new"}</span>
      <span className="title"><span className={`dot ${dot}`} />{sub.title}</span>
      <span className="steps" title={sub.steps ? `${sub.done} of ${sub.steps} steps ticked` : "No steps in its spec"}>
        {sub.steps ? `${sub.done}/${sub.steps}` : "—"}
      </span>
      <span className="steps">{sub.serves.join(", ")}</span>
      <span className="state" style={{ color: STATE_COLOR[state] }}>{stateText(state, sub, waits)}</span>
      <span className="act">
        {(state === "ready" || state === "waits") && <StartSubTask sub={sub} waits={waits} />}
      </span>
    </div>
  );
}

function stateText(state: SubTaskState, sub: SubTaskView, waits: string[]): string {
  switch (state) {
    case "to file":
      return sub.existing ? "exists · moves under the epic" : sub.waits_for.length > 0 ? `after ${sub.waits_for.join(", ")}` : "starts first";
    case "changes": return "changes requested";
    case "asking": return "asking you";
    case "waits": return `waits for ${waits.join(", ")}`;
    default: return state;
  }
}

/**
 * Start (EPIC-8): the first installed agent on the sub-task's spec, as
 * Start agent does. One that waits asks first, naming what it waits for.
 */
function StartSubTask({ sub, waits }: { sub: SubTaskView; waits: string[] }) {
  const agents = useStore((s) => s.agents);
  const refreshPanes = useStore((s) => s.refreshPanes);
  const refreshIssues = useStore((s) => s.refreshIssues);
  const failure = useFailure();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const agent = agents.find((a) => a.installed);

  async function start(anyway: boolean) {
    if (!agent || !sub.task_id) return;
    await api.startSubTask(sub.task_id, agent.id, anyway);
    await Promise.all([refreshPanes(), refreshIssues()]);
  }

  async function startNow() {
    if (busy) return;
    setBusy(true);
    failure.clear();
    try {
      await start(false);
    } catch (e) {
      failure.fail(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <span onClick={(e) => e.stopPropagation()}>
      <button
        className="ui-button"
        disabled={!agent || busy}
        title={agent ? `Start ${agent.name} on its spec` : "No agent CLI installed"}
        onClick={() => (waits.length > 0 ? setAsking(true) : void startNow())}
      >
        {busy ? "Starting…" : "Start"}
      </button>
      <ErrorMessage>{failure.error}</ErrorMessage>
      {asking && (
        <Confirm
          title={`Start “${sub.title}” now?`}
          body={<>It waits for {waits.join(", ")}, not merged yet. Its agent may build on work that is still changing.</>}
          confirmLabel="Start anyway"
          busyLabel="Starting…"
          danger={false}
          onConfirm={() => start(true)}
          onCancel={() => setAsking(false)}
        />
      )}
    </span>
  );
}
