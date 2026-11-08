import { useMemo } from "react";
import { useEpics } from "../../lib/epics";
import { openJiraEpic } from "../../lib/epicState";
import { reposOf, type EpicOf } from "../../lib/grouping";
import { taskMatches } from "../../lib/task-search";
import type { TaskView } from "../../lib/types";
import { useStore } from "../../store";
import { EpicRow, JiraEpicRow } from "./EpicRow";

export type EpicSections = {
  layerEpics: TaskView[];
  subsOf: Record<string, TaskView[]>;
  jiraEpics: { epic: EpicOf; tasks: TaskView[] }[];
  /** What is left for Tasks. */
  tasks: TaskView[];
  epics: Record<string, EpicOf | null>;
};

/**
 * Epics lead, their sub-tasks under them and not again under Tasks (EPIC-9):
 * those made here, then each Jira epic tasks here sit under (EPIC-13). A
 * sub-task whose epic is gone is an ordinary task, and so is a task on an
 * epic's own ticket.
 *
 * Each task's Jira epic is asked whenever Jira is connected: this section
 * needs it, whatever the list is grouped by (TASK-17).
 *
 * A search (TASK-19) narrows every list: an epic stays when it or one of its
 * tasks matches, and an epic that matches keeps all of its tasks. The epics
 * are still asked for every task, so typing asks the tracker nothing.
 */
export function useEpicSections(everyTask: TaskView[], query = ""): EpicSections {
  const keys = useMemo(
    () => everyTask.flatMap((t) => (!t.epic && t.issue_key ? [t.issue_key] : [])),
    [everyTask],
  );
  const epics = useEpics(keys, true);
  const issues = useStore((s) => s.issues);
  const tickets = useMemo(() => new Map(issues.map((i) => [i.key, i])), [issues]);
  const sections = useMemo(() => {
    const layerEpics = everyTask.filter((t) => t.epic);
    const subsOf: Record<string, TaskView[]> = Object.fromEntries(
      layerEpics.map((e) => [e.id, []]),
    );
    const byTicket = new Map(
      layerEpics.flatMap((e) => (e.issue_key ? [[e.issue_key, e.id] as const] : [])),
    );
    const jira = new Map<string, { epic: EpicOf; tasks: TaskView[] }>();
    const tasks: TaskView[] = [];
    for (const t of everyTask) {
      if (t.epic) continue;
      if (t.sub_task && subsOf[t.sub_task.epic]) {
        subsOf[t.sub_task.epic].push(t);
        continue;
      }
      const of = t.issue_key ? epics[t.issue_key] : null;
      const layer = of ? byTicket.get(of.key) : undefined;
      if (!of || of.key === t.issue_key) tasks.push(t);
      else if (layer) subsOf[layer].push(t);
      else if (jira.has(of.key)) jira.get(of.key)?.tasks.push(t);
      else jira.set(of.key, { epic: of, tasks: [t] });
    }
    return { layerEpics, subsOf, jiraEpics: [...jira.values()], tasks, epics };
  }, [everyTask, epics]);
  return useMemo(() => {
    if (!query.trim()) return sections;
    const hit = (t: TaskView) =>
      taskMatches(
        t,
        query,
        t.issue_key ? epics[t.issue_key] : null,
        t.issue_key ? tickets.get(t.issue_key) : null,
      );
    const subsOf: Record<string, TaskView[]> = {};
    const layerEpics = sections.layerEpics.filter((e) => {
      const subs = sections.subsOf[e.id] ?? [];
      subsOf[e.id] = hit(e) ? subs : subs.filter(hit);
      return hit(e) || subsOf[e.id].length > 0;
    });
    const jiraEpics = sections.jiraEpics.flatMap(({ epic, tasks }) => {
      const whole = taskMatches({ name: epic.summary, branch: "", issue_key: epic.key }, query);
      const kept = whole ? tasks : tasks.filter(hit);
      return kept.length > 0 ? [{ epic, tasks: kept }] : [];
    });
    return { layerEpics, subsOf, jiraEpics, tasks: sections.tasks.filter(hit), epics };
  }, [sections, query, epics, tickets]);
}

/**
 * The sidebar's Epics section. An epic is open until closed: its sub-tasks
 * are what it is. A Jira epic that is not one here yet is made one when
 * clicked, in its tasks' repositories, and opens on Sub-tasks as every epic
 * does, its page showing while it is made (EPIC-13). It used to open Start
 * work, so one click on two epic rows did two different things.
 */
export function EpicsSection({
  sections,
  selected,
  expanded,
  onSelect,
  onToggle,
  onMenu,
}: {
  sections: EpicSections;
  selected: string | null;
  expanded: Record<string, boolean>;
  onSelect: (taskId: string) => void;
  onToggle: (id: string) => void;
  onMenu: (x: number, y: number, task: TaskView) => void;
}) {
  const { layerEpics, subsOf, jiraEpics } = sections;
  const opening = useStore((s) => s.opening);

  if (layerEpics.length + jiraEpics.length === 0) return null;
  return (
    <>
      <div className="section-head">Epics</div>
      {layerEpics.map((epic) => (
        <EpicRow
          key={epic.id}
          epic={epic}
          subTasks={subsOf[epic.id]}
          selected={selected}
          open={!(expanded[epic.id] ?? false)}
          onSelect={onSelect}
          onToggle={onToggle}
          onMenu={onMenu}
        />
      ))}
      {jiraEpics.map(({ epic, tasks }) => (
        <JiraEpicRow
          key={epic.key}
          epic={epic}
          tasks={tasks}
          selected={selected}
          open={!(expanded[`jira:${epic.key}`] ?? false)}
          shown={opening?.epic.key === epic.key}
          opening={opening?.epic.key === epic.key && !opening.error}
          onOpen={(e, t) => void openJiraEpic(e, reposOf(t))}
          onSelect={onSelect}
          onToggle={onToggle}
          onMenu={onMenu}
        />
      ))}
    </>
  );
}
