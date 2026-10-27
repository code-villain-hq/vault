import { useMemo, useState } from "react";
import { api } from "../../lib/api";
import { useEpics } from "../../lib/epics";
import type { EpicOf } from "../../lib/grouping";
import type { Ticket, TaskView } from "../../lib/types";
import { useStore } from "../../store";
import { trackerOfKey } from "../../lib/tracker";
import { typeMap } from "../IssueType";
import { StartWorkDialog } from "../tickets/StartWorkDialog";
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
 */
export function useEpicSections(everyTask: TaskView[]): EpicSections {
  const keys = useMemo(() => everyTask.flatMap((t) => (!t.epic && t.issue_key ? [t.issue_key] : [])), [everyTask]);
  const epics = useEpics(keys, true);
  return useMemo(() => {
    const layerEpics = everyTask.filter((t) => t.epic);
    const subsOf: Record<string, TaskView[]> = Object.fromEntries(layerEpics.map((e) => [e.id, []]));
    const byTicket = new Map(layerEpics.flatMap((e) => (e.issue_key ? [[e.issue_key, e.id] as const] : [])));
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
}

/**
 * The sidebar's Epics section. An epic is open until closed: its sub-tasks
 * are what it is. A Jira epic that is not one here yet opens Start work on
 * its ticket, with its tasks' repositories picked (EPIC-13).
 */
export function EpicsSection({ sections, selected, expanded, onSelect, onToggle, onMenu }: {
  sections: EpicSections;
  selected: string | null;
  expanded: Record<string, boolean>;
  onSelect: (taskId: string) => void;
  onToggle: (id: string) => void;
  onMenu: (x: number, y: number, task: TaskView) => void;
}) {
  const { layerEpics, subsOf, jiraEpics } = sections;
  const projects = useStore((s) => s.projects);
  const agents = useStore((s) => s.agents);
  const issueTypes = useStore((s) => s.issueTypes);
  const fail = useStore((s) => s.fail);
  const types = useMemo(() => typeMap(issueTypes), [issueTypes]);
  const [making, setMaking] = useState<{ issue: Ticket; tasks: TaskView[] } | null>(null);

  async function open(epic: EpicOf, tasks: TaskView[]) {
    try {
      const issue = useStore.getState().issues.find((i) => i.key === epic.key)
        ?? (await api.ticketsByKey([epic.key]))[0];
      if (issue) setMaking({ issue, tasks });
      else fail(`${trackerOfKey(epic.key)} did not return ${epic.key}.`);
    } catch (e) {
      fail(e);
    }
  }

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
          onOpen={(e, t) => void open(e, t)}
          onSelect={onSelect}
          onToggle={onToggle}
          onMenu={onMenu}
        />
      ))}
      {making && (
        <StartWorkDialog
          issue={making.issue}
          projects={projects}
          agents={agents}
          types={types}
          subTasks={making.tasks}
          onClose={() => setMaking(null)}
        />
      )}
    </>
  );
}
