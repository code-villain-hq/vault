import { memo } from "react";
import { taskTotals, type TaskReview } from "../../lib/derive";
import { useTaskAgents } from "../../lib/panes";
import type { TaskView } from "../../lib/types";
import { ChevronIcon, CloseIcon } from "../icons";
import { Spinner } from "../ui";

/** What a task out for review is waiting on, in a word. */
const REVIEW_WORD: Partial<Record<TaskReview, { text: string; color: string }>> = {
  changes_requested: { text: "changes", color: "var(--status-danger-ink)" },
  approved: { text: "approved", color: "var(--status-ok-ink)" },
  // Done is calm; approved keeps ok, since it means you can merge.
  merged: { text: "merged", color: "var(--muted)" },
  commented: { text: "comments", color: "var(--status-info-ink)" },
};

/**
 * One task in the sidebar, with its repos when expanded.
 *
 * Its own component, reading only its own task's agents, and memoised: the
 * sidebar used to select every pane and redraw every task on each hook any
 * agent fired. Everything passed in is a plain value, a task the store keeps
 * while it is unchanged, or a callback that does not change.
 */
export const TaskRow = memo(function TaskRow({
  task,
  selected,
  vanishing,
  open,
  review,
  canAdd,
  onSelect,
  onMenu,
  onToggle,
  onAddRepo,
  onRemoveRepo,
}: {
  task: TaskView;
  selected: boolean;
  /** Being deleted: a spinner, and nothing to click. */
  vanishing: boolean;
  open: boolean;
  review: TaskReview;
  /** Some repository is not in the task yet. */
  canAdd: boolean;
  onSelect: (taskId: string) => void;
  onMenu: (x: number, y: number, task: TaskView) => void;
  onToggle: (taskId: string) => void;
  onAddRepo: (task: TaskView) => void;
  onRemoveRepo: (checkoutId: string, repoName: string, dirty: number, last: boolean) => void;
}) {
  const { live, waiting, busy, stuck } = useTaskAgents(task.id);
  const totals = taskTotals(task);
  const multi = task.checkouts.length > 1;
  const verdict = REVIEW_WORD[review];

  return (
    <div>
      <div
        className={`ws${selected ? " active" : ""}${vanishing ? " deleting" : ""}`}
        onClick={() => { if (!vanishing) onSelect(task.id); }}
        onContextMenu={(e) => {
          if (vanishing) return;
          e.preventDefault();
          onMenu(e.clientX, e.clientY, task);
        }}
      >
        <div className="ws-title">
          {/*
            Every task expands, whatever its repo count. Gating this on
            having several meant a task with one repo could not show or
            remove it — and a task that dropped to one lost its rows
            while keeping the "add repo" beneath them, with no way left
            to close it.
          */}
          <span
            className={`chev${open ? " open" : ""}`}
            onClick={(e) => {
              e.stopPropagation();
              if (vanishing) return;
              onToggle(task.id);
            }}
          >
            <ChevronIcon />
          </span>
          {vanishing ? (
            <Spinner />
          ) : (
          <span
            className={`dot ${
              totals.missing || totals.unlinked || stuck ? "gone" : waiting ? "idle" : busy ? "live" : ""
            }`}
            title={waiting ? "An agent here needs you: it is asking, or it finished" : undefined}
          />
          )}
          <span className="label">
            {task.issue_key ? task.name.replace(`${task.issue_key} `, "") : task.name}
          </span>
        </div>
        {/* The ticket key reads here, in the branch the composer starts with it,
            rather than in a box on the title line: there it was hard to read
            and said the same thing twice. */}
        <div className="ws-meta">
          <span className="branch">
            {task.issue_key && !task.branch.includes(task.issue_key)
              ? `${task.issue_key} · ${task.branch}`
              : task.branch}
          </span>
          <div className="spacer" />
          {multi && <span>{task.checkouts.length} repos</span>}
          {live > 0 && (
            <span style={waiting ? { color: "var(--status-warn-ink)" } : undefined}>
              {live}▶
            </span>
          )}
          {totals.dirty > 0 && (
            <span style={{ color: "var(--status-warn-ink)" }}>±{totals.dirty}</span>
          )}
          {verdict && <span style={{ color: verdict.color }}>{verdict.text}</span>}
        </div>
      </div>

      {open && task.checkouts.map((c) => {
        const d = c.status ? c.status.unstaged + c.status.untracked : 0;
        const lose = d + (c.status?.staged ?? 0);
        return (
          <div key={c.id} className="repo-row">
            <span
              className={`dot ${c.exists && !c.broken ? "" : "gone"}`}
              title={c.broken ?? undefined}
            />
            <span className="rname">{c.project_name}</span>
            {d > 0 && <span style={{ color: "var(--status-warn-ink)" }}>±{d}</span>}
            <span
              className="x"
              title="Remove repo from task"
              onClick={(e) => {
                e.stopPropagation();
                onRemoveRepo(c.id, c.project_name, lose, task.checkouts.length === 1);
              }}
            >
              <CloseIcon />
            </span>
          </div>
        );
      })}

      {open && canAdd && (
        <div
          className="repo-row"
          style={{ cursor: "pointer" }}
          onClick={() => onAddRepo(task)}
        >
          <span className="rname">+ add repo</span>
        </div>
      )}
    </div>
  );
});
