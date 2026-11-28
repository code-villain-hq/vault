import { useEffect, useState } from "react";
import { api, errMessage } from "../../lib/api";
import { taskTotals } from "../../lib/derive";
import { trackerOfKey } from "../../lib/tracker";
import type { JiraTransition, TaskView } from "../../lib/types";
import { useStore } from "../../store";
import { flowOf, mergedTransition } from "../TicketFlow";
import { Modal } from "../ui";

/**
 * Delete a task, and say what becomes of its ticket (TKT-9). Deleting means
 * abandoned as often as done, so the ticket is left as it is, unless a pull
 * request of the task already merged, when closing it is the likely wish.
 * Either way the choice is on screen. Deleting used to leave tickets In
 * Progress for work merged weeks before.
 */
export function DeleteTask({
  task,
  onCancel,
  onDelete,
}: {
  task: TaskView;
  onCancel: () => void;
  onDelete: (force: boolean, ticket: JiraTransition | null) => void;
}) {
  const settings = useStore((s) => s.settings);
  const rows = useStore((s) => s.prs[task.id]);
  const issue = useStore((s) => issueForTask(s.issues, task));
  // They keep the epic in the list while they sit under its ticket (EPIC-13).
  const subs = useStore((s) => s.tasks.filter((t) => t.sub_task?.epic === task.id).length);
  // Only Jira has transitions to offer (TKT-18). An epic's ticket is never
  // moved from here (EPIC-15): it is only taken off the list.
  const key = settings?.tracker === "jira" && !task.epic ? task.issue_key : null;
  const merged = (rows ?? []).some((r) => r.pr?.merged);
  const [transitions, setTransitions] = useState<JiraTransition[] | null>(key ? null : []);
  const [ticketError, setTicketError] = useState<string | null>(null);
  const [pick, setPick] = useState("");

  // Once, as it opens: what merged and what was chosen are read then.
  useEffect(() => {
    if (!key) return;
    let live = true;
    api.jiraTransitions(key).then(
      (all) => {
        if (!live) return;
        setTransitions(all);
        if (merged) setPick(mergedTransition(all, flowOf(settings, key))?.id ?? "");
      },
      (e) => {
        if (!live) return;
        setTransitions([]);
        setTicketError(errMessage(e));
      },
    );
    return () => {
      live = false;
    };
    // oxlint-disable-next-line react/exhaustive-deps -- `merged` and `settings` as it opens
  }, [key]);

  // Staged counts too: git refuses those as surely as unstaged ones, and
  // deleting without force then stopped and failed on a staged-only repo.
  const { dirty: unstaged, staged } = taskTotals(task);
  const dirty = unstaged + staged;
  const repos = task.checkouts.length;
  const chosen = transitions?.find((t) => t.id === pick) ?? null;

  return (
    <Modal
      title={task.epic ? "Remove from list" : "Delete task"}
      onClose={onCancel}
      footer={
        <>
          <button className="ui-button" onClick={onCancel}>
            Cancel
          </button>
          <button
            className="ui-button ui-button-danger"
            autoFocus
            disabled={transitions === null}
            onClick={() => onDelete(dirty > 0, chosen)}
          >
            {task.epic ? "Remove" : "Delete task"}
          </button>
        </>
      }
    >
      <div style={{ lineHeight: 1.6 }}>
        {task.epic ? (
          <>
            Remove <b>{task.name}</b> from the list?
            <div className="muted" style={{ marginTop: 8 }}>
              {task.issue_key ? (
                <>
                  {task.issue_key} is not changed in {trackerOfKey(task.issue_key)}.{" "}
                </>
              ) : null}
              Its spec is kept.{" "}
              {subs === 1 &&
                "Its sub-task stays as a task, still under the epic until it goes too. "}
              {subs > 1 &&
                `Its ${subs} sub-tasks stay as tasks, still under the epic until they go too. `}
              {repos === 1 ? "Its worktree goes" : `Its ${repos} worktrees go`} from disk; branch{" "}
              <code>{task.branch}</code> is left alone.
            </div>
          </>
        ) : (
          <>
            Delete <b>{task.name}</b> and remove {repos} worktree{repos === 1 ? "" : "s"} from disk?
            <div className="muted" style={{ marginTop: 8 }}>
              Branch <code>{task.branch}</code> is left alone, in the repositories and on any
              remote.
            </div>
          </>
        )}
        {dirty > 0 && (
          <div className="confirm-detail">
            {dirty} uncommitted change{dirty === 1 ? "" : "s"} will be lost. This can't be undone.
          </div>
        )}
        {key && (
          <div className="field" style={{ marginTop: 14, marginBottom: 0 }}>
            <label>
              Ticket {key}
              {issue ? ` · ${issue.status}` : ""}
            </label>
            {transitions === null ? (
              <div className="muted">Asking Jira where it can go…</div>
            ) : (
              <select className="ui-field" value={pick} onChange={(e) => setPick(e.target.value)}>
                <option value="">Leave it as it is</option>
                {transitions.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name} → {t.to_status}
                  </option>
                ))}
              </select>
            )}
            {ticketError && (
              <div className="hint" style={{ color: "var(--status-danger-ink)" }}>
                {ticketError}
              </div>
            )}
            {merged && !ticketError && (
              <div className="hint">A pull request of this task has merged.</div>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}
import { issueForTask } from "../../lib/tracker";
