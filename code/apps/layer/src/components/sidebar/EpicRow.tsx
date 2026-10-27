import { memo, useEffect } from "react";
import { taskReview } from "../../lib/derive";
import { openWaits, STATE_COLOR, subTaskState, useEpicViews } from "../../lib/epicState";
import { useTaskAgents } from "../../lib/panes";
import { useStore } from "../../store";
import type { SubTaskView, TaskView } from "../../lib/types";
import { ChevronIcon } from "../icons";

/**
 * An epic in the sidebar's Epics section (EPIC-9): how far its sub-tasks
 * are, and under it each filed one on a line of its own. Its sub-tasks are
 * not listed again under Tasks.
 */
export const EpicRow = memo(function EpicRow({
  epic,
  subTasks,
  selected,
  open,
  onSelect,
  onToggle,
  onMenu,
}: {
  epic: TaskView;
  /** Its filed sub-tasks' tasks. */
  subTasks: TaskView[];
  selected: string | null;
  open: boolean;
  onSelect: (taskId: string) => void;
  onToggle: (taskId: string) => void;
  onMenu: (x: number, y: number, task: TaskView) => void;
}) {
  const view = useEpicViews((s) => s.byEpic[epic.id]);
  const load = useEpicViews((s) => s.load);
  const prs = useStore((s) => s.prs);
  const { waiting, busy } = useTaskAgents(epic.id);
  // Read again when a sub-task is filed or goes, which the task list says.
  useEffect(() => { void load(epic.id).catch(() => {}); }, [epic.id, subTasks.length, load]);

  const all = view?.sub_tasks ?? [];
  const filed = all.filter((s) => s.task_id);
  const merged = filed.filter((s) => taskReview(prs[s.task_id!] ?? []) === "merged").length;
  const toFile = all.length - filed.length;

  return (
    <div>
      <div
        className={`ws${selected === epic.id ? " active" : ""}`}
        onClick={() => onSelect(epic.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          onMenu(e.clientX, e.clientY, epic);
        }}
      >
        <div className="ws-title">
          <span
            className={`chev${open ? " open" : ""}`}
            onClick={(e) => {
              e.stopPropagation();
              onToggle(epic.id);
            }}
          >
            <ChevronIcon />
          </span>
          <span className={`dot ${waiting ? "idle" : busy ? "live" : ""}`} />
          <span className="label">{epic.issue_key ? epic.name.replace(`${epic.issue_key} `, "") : epic.name}</span>
        </div>
        <div className="ws-meta">
          {epic.issue_key && <span className="branch">{epic.issue_key}</span>}
          <div className="spacer" />
          {toFile > 0 ? (
            <span>{toFile} to file</span>
          ) : filed.length > 0 ? (
            <span>{merged} of {filed.length} merged</span>
          ) : (
            <span>spec</span>
          )}
        </div>
      </div>
      {open && filed.map((s) => {
        const task = subTasks.find((t) => t.id === s.task_id);
        return task ? (
          <SubLine key={task.id} sub={s} all={all} task={task} selected={selected === task.id} onSelect={onSelect} onMenu={onMenu} />
        ) : null;
      })}
    </div>
  );
});

/** One sub-task, one line: its dot (hollow while it waits), title and state. */
function SubLine({ sub, all, task, selected, onSelect, onMenu }: {
  sub: SubTaskView;
  all: SubTaskView[];
  task: TaskView;
  selected: boolean;
  onSelect: (taskId: string) => void;
  onMenu: (x: number, y: number, task: TaskView) => void;
}) {
  const agents = useTaskAgents(task.id);
  const prs = useStore((s) => s.prs);
  const state = subTaskState(true, taskReview(prs[task.id] ?? []), agents, openWaits(sub, all, prs).length > 0);
  const dot = state === "waits" ? "queued" : agents.waiting ? "idle" : agents.busy ? "live" : "";
  return (
    <div
      className={`sub-row${selected ? " active" : ""}`}
      onClick={() => onSelect(task.id)}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(e.clientX, e.clientY, task);
      }}
    >
      <span className={`dot ${dot}`} />
      <span className="label">{sub.title}</span>
      <span className="meta" style={{ color: STATE_COLOR[state] }}>
        {state === "working" ? `${agents.live}▶` : state === "ready" ? "" : state}
      </span>
    </div>
  );
}
