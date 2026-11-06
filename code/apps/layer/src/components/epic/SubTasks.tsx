import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { taskReview } from "../../lib/derive";
import { openWaits, STATE_COLOR, subTaskState, type SubTaskState } from "../../lib/epicState";
import { useTaskAgents } from "../../lib/panes";
import { pressKeys } from "../../lib/press";
import { trackerOfKey } from "../../lib/tracker";
import type { EpicView, SubTaskView, TaskView, Ticket, TicketPage } from "../../lib/types";
import { useStore } from "../../store";
import { typeMap } from "../IssueType";
import { StartWorkDialog } from "../tickets/StartWorkDialog";
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
  const [starting, setStarting] = useState<Ticket | null>(null);
  const under = useTicketsUnder(epic, view);
  const startWork = starting && <StartWork ticket={starting} onClose={() => setStarting(null)} />;

  if (!view)
    return (
      <div className="epic-subs">
        <Spinner />
      </div>
    );
  const waiting = under.page?.issues ?? [];
  const tracker = epic.issue_key ? trackerOfKey(epic.issue_key) : "";
  // Tickets already under the epic, and no plan yet: the spec shares the
  // steps out among them (EPIC-14). "No sub-tasks yet" over six of them in
  // Jira led to a plan that filed each again.
  if (!view.planned && view.sub_tasks.length === 0 && waiting.length > 0) {
    return (
      <div className="epic-subs">
        <div className="epic-lead">
          <div className="what">
            <b>
              {waiting.length === 1 ? "A ticket is" : `${waiting.length} tickets are`} under{" "}
              {epic.issue_key} in {tracker}.
            </b>{" "}
            <span className="muted">
              Start work on one now, or let the spec's tasks share the steps out among them and file
              a new ticket only for work none of them covers.
            </span>
          </div>
          <button className="ui-button ui-button-primary" onClick={() => setTab("spec")}>
            Open the spec
          </button>
        </div>
        <NotHereYet
          tickets={waiting}
          more={!!under.page?.more}
          tracker={tracker}
          onStart={setStarting}
        />
        {startWork}
      </div>
    );
  }
  // An epic made from tasks already under way has them and no spec yet (EPIC-13).
  if (!view.planned && view.sub_tasks.length === 0) {
    return (
      <div className="epic-subs">
        <div className="empty">
          <h2>No sub-tasks yet</h2>
          <p>
            The epic's spec says what they are. Approve its tasks on the Spec tab: each group of
            steps becomes a sub-task, filed under the epic.
          </p>
          <button className="ui-button ui-button-primary" onClick={() => setTab("spec")}>
            Open the spec
          </button>
          <ErrorMessage>{under.error}</ErrorMessage>
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
            <b>
              The approved spec makes {all.length === toFile ? "" : `${toFile} more `}
              {toFile === 1 ? "a sub-task" : `${toFile} sub-tasks`}.
            </b>{" "}
            <span className="muted">
              Each is{" "}
              {epic.issue_key
                ? `filed in ${trackerOfKey(epic.issue_key)} under ${epic.issue_key} and `
                : ""}
              given its own task here.
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
          {all
            .filter((s) => (s.repos[0] ?? "") === repo)
            .map((s) => (
              <SubTaskRow key={s.title} sub={s} all={all} />
            ))}
        </div>
      ))}
      {waiting.length > 0 && (
        <>
          <div className="epic-group">
            Under {epic.issue_key} in {tracker}, not in the spec
          </div>
          <NotHereYet
            tickets={waiting}
            more={!!under.page?.more}
            tracker={tracker}
            onStart={setStarting}
          />
        </>
      )}
      <ErrorMessage>{under.error}</ErrorMessage>
      {filing && <FileSubTasks epic={epic} count={toFile} onClose={() => setFiling(false)} />}
      {startWork}
    </div>
  );
}

/**
 * Start work on a ticket under the epic (EPIC-14), as from Tickets (TKT-1).
 * The task it makes sits under the epic's ticket, so it becomes the epic's
 * sub-task (EPIC-13) and leaves the tickets not here yet.
 */
function StartWork({ ticket, onClose }: { ticket: Ticket; onClose: () => void }) {
  const projects = useStore((s) => s.projects);
  const agents = useStore((s) => s.agents);
  const issueTypes = useStore((s) => s.issueTypes);
  const types = useMemo(() => typeMap(issueTypes), [issueTypes]);
  return (
    <StartWorkDialog
      issue={ticket}
      projects={projects}
      agents={agents}
      types={types}
      onClose={onClose}
    />
  );
}

/**
 * The tickets under the epic's that are no sub-task here yet (EPIC-14), read
 * again whenever its plan or its sub-tasks' tickets change.
 */
function useTicketsUnder(epic: TaskView, view: EpicView | null) {
  const [page, setPage] = useState<TicketPage | null>(null);
  const failure = useFailure();
  const { fail, clear } = failure;
  const known = view ? `${view.planned} ${view.sub_tasks.map((s) => s.key ?? "").join(" ")}` : null;
  useEffect(() => {
    if (!epic.issue_key || known === null) return;
    let live = true;
    api
      .epicTickets(epic.id)
      .then((p) => {
        if (!live) return;
        setPage(p);
        clear();
      })
      .catch((e) => {
        if (live) fail(e);
      });
    return () => {
      live = false;
    };
  }, [epic.id, epic.issue_key, known, fail, clear]);
  return { page, error: failure.error };
}

/**
 * Tickets under the epic with no sub-task here: their key, summary and
 * status, and Start work. Before, the row had no action, and starting one
 * meant finding it again under Tickets.
 */
function NotHereYet({
  tickets,
  more,
  tracker,
  onStart,
}: {
  tickets: Ticket[];
  more: boolean;
  tracker: string;
  onStart: (ticket: Ticket) => void;
}) {
  return (
    <>
      {tickets.map((t) => (
        <div key={t.key} className="epic-row proposed">
          <span className="key">{t.key}</span>
          <span className="title">
            <span className="dot" />
            {t.summary}
          </span>
          <span className="steps">—</span>
          <span className="steps" />
          <span className="state">{t.status}</span>
          <span className="act">
            <button className="ui-button" onClick={() => onStart(t)}>
              Start work
            </button>
          </span>
        </div>
      ))}
      {more && <div className="epic-group">More in {tracker} than one page shows</div>}
    </>
  );
}

/** One sub-task: ticket, title, steps, requirements, and state with its action. */
function SubTaskRow({ sub, all }: { sub: SubTaskView; all: SubTaskView[] }) {
  const prs = useStore((s) => s.prs);
  const agents = useTaskAgents(sub.task_id ?? "");
  const waits = openWaits(sub, all, prs);
  const state = subTaskState(
    !!sub.task_id,
    taskReview(sub.task_id ? (prs[sub.task_id] ?? []) : []),
    agents,
    waits.length > 0,
  );
  const select = useStore((s) => s.select);
  const dot = state === "waits" ? "queued" : agents.waiting ? "idle" : agents.busy ? "live" : "";
  // One filed opens its task: a div, not a button, as it holds Start.
  const id = sub.task_id;
  const opens = id
    ? {
        role: "button",
        tabIndex: 0,
        onClick: () => select(id),
        onKeyDown: pressKeys(() => select(id)),
      }
    : {};

  return (
    <div
      className={`epic-row${sub.task_id ? "" : " proposed"}${state === "waits" ? " queued" : ""}`}
      {...opens}
    >
      <span className="key">{sub.key ?? "new"}</span>
      <span className="title">
        <span className={`dot ${dot}`} />
        {sub.title}
      </span>
      <span
        className="steps"
        title={sub.steps ? `${sub.done} of ${sub.steps} steps ticked` : "No steps in its spec"}
      >
        {sub.steps ? `${sub.done}/${sub.steps}` : "—"}
      </span>
      <span className="steps">{sub.serves.join(", ")}</span>
      <span className="state" style={{ color: STATE_COLOR[state] }}>
        {stateText(state, sub, waits)}
      </span>
      <span className="act">
        {(state === "ready" || state === "waits") && <StartSubTask sub={sub} waits={waits} />}
      </span>
    </div>
  );
}

function stateText(state: SubTaskState, sub: SubTaskView, waits: string[]): string {
  switch (state) {
    case "to file":
      return sub.existing
        ? "exists · moves under the epic"
        : sub.waits_for.length > 0
          ? `after ${sub.waits_for.join(", ")}`
          : "starts first";
    case "changes":
      return "changes requested";
    case "asking":
      return "asking you";
    case "waits":
      return `waits for ${waits.join(", ")}`;
    default:
      return state;
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
    // oxlint-disable-next-line jsx-a11y/no-static-element-interactions, jsx-a11y/click-events-have-key-events -- not a control: it keeps Start's clicks, and its confirm's, from opening the row
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
          body={
            <>
              It waits for {waits.join(", ")}, not merged yet. Its agent may build on work that is
              still changing.
            </>
          }
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
