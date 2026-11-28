import type { MouseEvent } from "react";
import { goTo } from "../lib/goto";
import { keyProjects, type IssueKeys } from "../lib/issue-keys";
import { taskUsesTracker } from "../lib/tracker";
import type { TaskView, Ticket } from "../lib/types";
import { useStore } from "../store";

let keysSeen: { tasks: TaskView[]; issues: Ticket[]; project: string; keys: IssueKeys } | null =
  null;

/**
 * Which keys are links, and which of them have a task (ACP-18): read once
 * per conversation, not once per link, and worked out again only when the
 * tasks, tickets or Jira project change. A selector runs on every change of
 * anything in the store, and every open chat has one.
 */
export function useIssueKeys(): IssueKeys {
  return useStore((s) => {
    const project = s.settings?.jira?.project_key ?? "";
    const seen = keysSeen;
    if (seen && seen.tasks === s.tasks && seen.issues === s.issues && seen.project === project) {
      return seen.keys;
    }
    const projects = keyProjects([
      ...s.tasks.map((t) => t.issue_key),
      ...s.issues.map((i) => i.key),
      project ? `${project}-1` : null,
    ]);
    const tasked = s.tasks
      .map((t) => t.issue_key)
      .filter((k): k is string => !!k)
      .sort()
      .join("|");
    // The same object while the strings are, so no chat redraws for nothing.
    const keys =
      seen && seen.keys.projects === projects && seen.keys.tasked === tasked
        ? seen.keys
        : { projects, tasked };
    keysSeen = { tasks: s.tasks, issues: s.issues, project, keys };
    return keys;
  });
}

/** A ticket's key: its task in Work, else the ticket in Tickets. */
export function IssueKeyLink({ issueKey, tasked }: { issueKey: string; tasked: boolean }) {
  function open(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    // Asked at the click, so a task made or deleted since is where it goes.
    const { settings, tasks } = useStore.getState();
    const task = tasks.find((t) => t.issue_key === issueKey && taskUsesTracker(settings, t));
    goTo(task ? `task:${task.id}` : `ticket:${issueKey}`);
  }
  return (
    <a
      className="ui-link"
      href={`#${issueKey}`}
      title={tasked ? `Open ${issueKey}'s task in Work` : `Show ${issueKey} in Tickets`}
      onClick={open}
    >
      {issueKey}
    </a>
  );
}
