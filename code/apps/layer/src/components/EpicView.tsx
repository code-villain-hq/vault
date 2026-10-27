import { useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { taskReview } from "../lib/derive";
import { useEpicViews } from "../lib/epicState";
import { specProgress, useSpecs } from "../lib/specs";
import { useStore } from "../store";
import { trackerOfKey } from "../lib/tracker";
import type { TaskView } from "../lib/types";
import { SpecView } from "./SpecView";
import { EpicChat } from "./epic/EpicChat";
import { SubTasks } from "./epic/SubTasks";
import { statusClass } from "./tickets/status";
import { SidebarToggle } from "./ui";

/**
 * An epic's page (§26): its sub-tasks and its spec, and beside them its
 * chat. No Agents, Diff or Pull requests: an epic holds no code (EPIC-1).
 * Spec when the store's tab says so; Sub-tasks for any other, since the
 * tab a task was left on means nothing here.
 */
export function EpicMain({ epic }: { epic: TaskView }) {
  const tab = useStore((s) => s.tab);
  const setTab = useStore((s) => s.setTab);
  const prs = useStore((s) => s.prs);
  const ticket = useStore((s) => s.issues.find((i) => i.key === epic.issue_key));
  const view = useEpicViews((s) => s.byEpic[epic.id]);
  const load = useEpicViews((s) => s.load);
  const specBadge = useSpecs((s) => specProgress(s.byTask[epic.id]?.spec ?? null));
  const loadSpec = useSpecs((s) => s.load);
  useEffect(() => { void loadSpec(epic.id).catch(() => {}); }, [epic.id, loadSpec]);
  // Approving its tasks changes what it plans: read again on coming back.
  useEffect(() => { if (tab !== "spec") void load(epic.id).catch(() => {}); }, [epic.id, tab, load]);

  const all = view?.sub_tasks ?? [];
  const filed = all.filter((s) => s.task_id);
  const merged = filed.filter((s) => taskReview(prs[s.task_id!] ?? []) === "merged").length;
  const onSpec = tab === "spec";

  return (
    <>
      <div className="ws-header">
        <SidebarToggle />
        <h1>{epic.name}</h1>
        <div className="chips">
          <span className="ui-chip">epic</span>
          {epic.checkouts.map((c) => (
            <span key={c.id} className={!c.exists || c.broken ? "ui-status ui-status-danger" : "ui-chip"} title={c.broken ?? c.path}>
              {c.project_name}
            </span>
          ))}
        </div>
        <div className="spacer" />
        {epic.issue_url && (
          <div className="ws-links">
            <button className="ui-link" title={`Open ${epic.issue_key} in ${trackerOfKey(epic.issue_key ?? "")}`} onClick={() => void openUrl(epic.issue_url!)}>
              {epic.issue_key} ↗
            </button>
            {ticket && <span className={`ui-status status-pill ${statusClass(ticket.status_category)}`}>{ticket.status}</span>}
          </div>
        )}
      </div>

      <div className="ui-tabs task-tabs" role="tablist">
        <button className="ui-tab" role="tab" aria-selected={!onSpec} onClick={() => setTab("terminals")}>
          Sub-tasks
          {all.length > 0 && (
            <span className="ui-tab-count">{filed.length > 0 ? `${merged}/${filed.length}` : all.length}</span>
          )}
        </button>
        <button className="ui-tab" role="tab" aria-selected={onSpec} onClick={() => setTab("spec")}>
          Spec{specBadge && <span className="ui-tab-count">{specBadge}</span>}
        </button>
      </div>

      <div className="content epic">
        <div className="epic-main">
          {onSpec ? <SpecView task={epic} /> : <SubTasks epic={epic} view={view ?? null} />}
        </div>
        <EpicChat epic={epic} />
      </div>
    </>
  );
}
