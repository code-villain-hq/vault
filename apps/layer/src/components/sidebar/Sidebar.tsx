import { Fragment, useCallback, useState, type ReactNode } from "react";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, errMessage } from "../../lib/api";
import { copyAndSay } from "../../lib/clipboard";
import { useStore } from "../../store";
import { taskReview } from "../../lib/derive";
import { useFleetAgents } from "../../lib/panes";
import type { JiraTransition, TaskView } from "../../lib/types";
import { BusyOverlay, Confirm, ContextMenu, Field, Modal, type MenuItem } from "../ui";
import { RepoPicker } from "../RepoPicker";
import { FinishTask } from "../FinishTask";
import { DeleteTask } from "./DeleteTask";
import { TaskRow } from "./TaskRow";
import { ProfileMenu } from "./ProfileMenu";

/**
 * The overview is a place, not merely what you see before choosing.
 * Selecting a task is remembered across restarts, so without a way back the
 * first task ever clicked is the last view the app offers.
 *
 * Its own component, so that the counts it shows are all that redraws it.
 */
function FleetRow({ selected, onSelect }: { selected: boolean; onSelect: () => void }) {
  // The same agents AgentsView counts, so the row and the view it opens
  // agree; and the waiting ones are what the dock icon counts, said where
  // the count can be explained.
  const { running, waiting } = useFleetAgents();
  return (
    <div className={`ws${selected ? " active" : ""}`} onClick={onSelect}>
      <div className="ws-title">
        <span
          className={`dot ${waiting ? "idle" : running ? "live" : ""}`}
          title={
            waiting
              ? `${waiting} agent${waiting === 1 ? " is" : "s are"} asking for something, or finished and not looked at yet`
              : undefined
          }
        />
        <span className="label">All agents</span>
      </div>
      <div className="ws-meta">
        {waiting > 0 ? (
          <span style={{ color: "var(--amber)" }}>
            {waiting} need{waiting === 1 ? "s" : ""} you
          </span>
        ) : (
          <span>across every task</span>
        )}
        <div className="spacer" />
        {running > 0 && <span>{running}▶</span>}
      </div>
    </div>
  );
}

export function Sidebar() {
  const projects = useStore((s) => s.projects);
  const tasks = useStore((s) => s.tasks);
  const prs = useStore((s) => s.prs);
  const selected = useStore((s) => s.selectedTask);
  const select = useStore((s) => s.select);
  const setView = useStore((s) => s.setView);
  const showIssue = useStore((s) => s.showIssue);
  const refreshTasks = useStore((s) => s.refreshTasks);
  const refreshPanes = useStore((s) => s.refreshPanes);
  const refreshIssues = useStore((s) => s.refreshIssues);
  const toast = useStore((s) => s.toast);
  const fail = useStore((s) => s.fail);
  const cursorIde = useStore((s) => s.cursorIde);

  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [addingRepoTo, setAddingRepoTo] = useState<TaskView | null>(null);
  const [adding, setAdding] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; task: TaskView } | null>(null);
  const [confirming, setConfirming] = useState<{
    title: string;
    body: ReactNode;
    label: string;
    /** A `run` that returns a promise keeps the dialog up and spinning. */
    busyLabel?: string;
    run: () => void | Promise<unknown>;
  } | null>(null);
  const [deleting, setDeleting] = useState<{ id: string; name: string } | null>(null);
  /** The task whose delete dialog is open. */
  const [asking, setAsking] = useState<TaskView | null>(null);
  const [finishing, setFinishing] = useState<TaskView | null>(null);

  function askDeleteTask(task: TaskView) {
    if (deleting) return;
    setAsking(task);
  }

  /// Add every repo picked, reporting per repo rather than stopping at the
  /// first failure: one worktree that will not create is no reason to skip the
  /// rest, and the user needs to know which one it was.
  async function addRepos(task: TaskView) {
    setBusy(true);
    const failed: string[] = [];
    let told = 0;
    for (const id of adding) {
      try {
        told = Math.max(told, (await api.addCheckout(task.id, id)).told);
      } catch (e) {
        failed.push(`${projects.find((p) => p.id === id)?.name ?? id}: ${errMessage(e)}`);
      }
    }
    await Promise.all([refreshTasks(), refreshPanes()]);
    setBusy(false);
    setAddingRepoTo(null);
    setAdding([]);
    setExpanded((e) => ({ ...e, [task.id]: true }));

    const added = adding.length - failed.length;
    if (added > 0) {
      toast(
        "success",
        `${added} worktree${added === 1 ? "" : "s"} on ${task.branch}` +
          // Agents already running cannot see a new sibling folder, so they are
          // told. Say so rather than reaching into their session invisibly.
          (told > 0 ? ` · told ${told} running agent${told === 1 ? "" : "s"}` : ""),
      );
    }
    if (failed.length) toast("error", failed.join("\n"));
  }

  /// git refuses to remove a worktree with uncommitted or untracked files. Say
  /// so and offer to force, rather than leaving an orphan nobody can see.
  async function runDelete(task: TaskView, force: boolean, ticket: JiraTransition | null = null) {
    setConfirming(null);
    setDeleting({ id: task.id, name: task.name });
    try {
      const results = await api.deleteTask(task.id, force);
      const stuck = results.filter((r) => !r.ok);
      // Deleting a task stops its panes too, so the pane list is stale as well.
      await Promise.all([refreshTasks(), refreshPanes()]);

      if (stuck.length === 0) {
        if (selected === task.id) select(null);
        // Only once the task is gone: a ticket closed for a task that was
        // then kept would be the board saying something untrue.
        let moved = "";
        if (ticket && task.issue_key) {
          try {
            await api.jiraTransition(task.issue_key, ticket.id);
            moved = ` · ${task.issue_key} moved to ${ticket.to_status}`;
            void refreshIssues({ quiet: true });
          } catch (e) {
            toast("error", `${task.issue_key} could not be moved: ${errMessage(e)}`);
          }
        }
        toast("success", `Deleted ${task.name}${moved}`);
        return;
      }
      setConfirming({
        title: "Some worktrees could not be removed",
        label: "Force delete",
        body: (
          <>
            git refused to remove {stuck.length} worktree{stuck.length === 1 ? "" : "s"},
            so the task has been kept rather than leaving them orphaned:
            <div className="confirm-detail">
              {stuck.map((r) => (
                <div key={r.checkout_id}>
                  <b>{r.repo}</b> — {r.detail}
                </div>
              ))}
            </div>
            <div style={{ marginTop: 10 }}>
              Forcing deletes those worktrees and anything uncommitted in them.
            </div>
          </>
        ),
        run: () => void runDelete(task, true, ticket),
      });
    } catch (e) {
      fail(e);
    } finally {
      setDeleting(null);
    }
  }

  // Stable, like every callback a row is handed, or each row redraws
  // whenever the sidebar does.
  const onMenu = useCallback((x: number, y: number, task: TaskView) => setMenu({ x, y, task }), []);
  const onToggle = useCallback(
    (id: string) => setExpanded((x) => ({ ...x, [id]: !(x[id] ?? false) })),
    [],
  );
  const askRemoveRepo = useCallback((checkoutId: string, repoName: string, dirty: number, last: boolean) => {
    setConfirming({
      title: "Remove repository from task",
      label: "Remove",
      body: (
        <>
          Remove <b>{repoName}</b> from this task and delete its worktree?
          <div className="muted" style={{ marginTop: 8 }}>
            Any terminal running inside it is stopped. Panes started at the task
            root, which see every repo, are left alone.
          </div>
          {last && (
            <div className="confirm-detail">
              This is the only repository in the task. Removing it leaves nothing to
              work in — add another, or delete the task instead.
            </div>
          )}
          {dirty > 0 && (
            <div className="confirm-detail">
              {dirty} uncommitted change{dirty === 1 ? "" : "s"} in that worktree will be
              lost.
            </div>
          )}
        </>
      ),
      // Awaited by the dialog, so the spinner is up for the whole thing —
      // stopping the panes alone waits two seconds on them, and `git worktree
      // remove` is as slow here as it is when a whole task goes.
      busyLabel: "Removing…",
      run: async () => {
        try {
          await api.removeCheckout(checkoutId, dirty > 0);
          // Panes rooted in that worktree are stopped with it, so refresh both
          // or the terminals stay on screen until the next poll happens to run.
          await Promise.all([refreshTasks(), refreshPanes()]);
        } catch (e) {
          fail(e);
        }
      },
    });
  }, [refreshTasks, refreshPanes, fail]);

  function taskMenu(task: TaskView): MenuItem[] {
    const items: MenuItem[] = [
      { label: "Open", onSelect: () => select(task.id) },
    ];
    if (task.issue_url) {
      items.push({
        label: `Open ${task.issue_key} in Jira`,
        onSelect: () => void openUrl(task.issue_url!),
      });
    }
    if (task.issue_key) {
      items.push({
        label: "Show in Tickets",
        onSelect: () => showIssue(task.issue_key!),
      });
    }
    items.push(
      {
        label: "Copy branch name",
        onSelect: () => copyAndSay(task.branch, task.branch),
      },
      {
        label: "Reveal task folder",
        onSelect: () => void revealItemInDir(task.root).catch(fail),
      },
    );
    if (cursorIde) {
      items.push({
        label: "Open in Cursor",
        onSelect: () => void api.openInCursor(task.root).catch(fail),
      });
    }
    if (available(task).length > 0) {
      items.push({
        label: "Add a repository…",
        separated: true,
        onSelect: () => setAddingRepoTo(task),
      });
    }
    const landed = taskReview(prs[task.id] ?? []) === "merged";
    if (landed) {
      items.push({
        label: "Finish task…",
        separated: available(task).length === 0,
        onSelect: () => setFinishing(task),
      });
    }
    items.push({
      label: "Delete task…",
      danger: true,
      separated: available(task).length === 0 && !landed,
      onSelect: () => askDeleteTask(task),
    });
    return items;
  }

  const available = (task: TaskView) =>
    projects.filter((p) => !task.checkouts.some((c) => c.project_id === p.id));

  // Three lists, because they are three questions. Writing, waiting on a
  // reviewer, and landed (update the ticket) are not the same job — and a
  // merged task in the first list looks like unfinished work.
  const review = new Map(tasks.map((t) => [t.id, taskReview(prs[t.id] ?? [])]));
  const done = tasks.filter((t) => review.get(t.id) === "merged");
  const reviewing = tasks.filter(
    (t) =>
      review.get(t.id) !== "merged" &&
      (prs[t.id] ?? []).some((r) => r.pr && r.pr.state === "open"),
  );
  const elsewhere = new Set([...done, ...reviewing].map((t) => t.id));
  const working = tasks.filter((t) => !elsewhere.has(t.id));
  const ordered = [...working, ...reviewing, ...done];

  return (
    <div className="sidebar">
      <div className="sidebar-body" style={{ paddingTop: 6 }}>
        <FleetRow selected={selected === null} onSelect={() => select(null)} />

        <div className="section-head">
          Tasks
          <span className="count">{working.length}</span>
        </div>

        {tasks.length === 0 && (
          <div className="muted" style={{ padding: "4px 8px 10px", lineHeight: 1.5 }}>
            {projects.length === 0 ? (
              <>
                No repositories yet —{" "}
                <a
                  href="#"
                  style={{ color: "var(--accent)" }}
                  onClick={(e) => { e.preventDefault(); setView("repos"); }}
                >
                  add some in the Repos tab
                </a>
                .
              </>
            ) : (
              <>
                No tasks yet. A task starts from a ticket —{" "}
                <a
                  href="#"
                  style={{ color: "var(--accent)" }}
                  onClick={(e) => { e.preventDefault(); setView("tickets"); }}
                >
                  pick one in Tickets
                </a>
                .
              </>
            )}
          </div>
        )}

        {ordered.map((task, i) => (
          <Fragment key={task.id}>
            {i === working.length && reviewing.length > 0 && (
              <div className="section-head">
                In review
                <span className="count">{reviewing.length}</span>
              </div>
            )}
            {i === working.length + reviewing.length && done.length > 0 && (
              <div className="section-head">
                Done
                <span className="count">{done.length}</span>
              </div>
            )}
            <TaskRow
              task={task}
              selected={task.id === selected}
              vanishing={deleting?.id === task.id}
              open={expanded[task.id] ?? false}
              review={review.get(task.id) ?? "none"}
              canAdd={available(task).length > 0}
              onSelect={select}
              onMenu={onMenu}
              onToggle={onToggle}
              onAddRepo={setAddingRepoTo}
              onRemoveRepo={askRemoveRepo}
            />
          </Fragment>
        ))}

      </div>
      <ProfileMenu />

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={taskMenu(menu.task)}
        />
      )}

      {confirming && (
        <Confirm
          title={confirming.title}
          body={confirming.body}
          confirmLabel={confirming.label}
          busyLabel={confirming.busyLabel}
          onConfirm={confirming.run}
          onCancel={() => setConfirming(null)}
        />
      )}

      {asking && (
        <DeleteTask
          task={asking}
          onCancel={() => setAsking(null)}
          onDelete={(force, ticket) => {
            setAsking(null);
            void runDelete(asking, force, ticket);
          }}
        />
      )}
      {finishing && <FinishTask task={finishing} onClose={() => setFinishing(null)} />}

      {deleting && (
        <BusyOverlay
          title={`Deleting ${deleting.name}`}
          detail="Stopping agents and removing worktrees…"
        />
      )}

      {addingRepoTo && (
        <Modal
          title={`Add repositories to ${addingRepoTo.issue_key ?? addingRepoTo.branch}`}
          onClose={() => { setAddingRepoTo(null); setAdding([]); }}
          footer={
            <>
              <button
                className="btn"
                onClick={() => { setAddingRepoTo(null); setAdding([]); }}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                disabled={busy || adding.length === 0}
                onClick={() => void addRepos(addingRepoTo)}
              >
                {busy
                  ? "Creating…"
                  : `Add ${adding.length} repo${adding.length === 1 ? "" : "s"}`}
              </button>
            </>
          }
        >
          <Field
            label="Repositories"
            hint={`A worktree on ${addingRepoTo.branch} is created for each, beside the ones already in this task.`}
          >
            <RepoPicker
              projects={available(addingRepoTo)}
              picked={adding}
              onChange={setAdding}
            />
          </Field>
        </Modal>
      )}
    </div>
  );
}
