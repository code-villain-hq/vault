import type { EpicOf } from "./grouping";
import type { Task, Ticket } from "./types";

/**
 * Whether a task is what the sidebar's search asks for (TASK-19): its
 * ticket's key, its name (the ticket's title), its branch, its epic's key and
 * title, and — when the ticket is on your own list, so nothing more is asked
 * of the tracker — the ticket's title and description. Every word typed must
 * be found somewhere, in any order and case: "dt-48 login" finds DT-4826
 * "Login fails on Safari".
 */
export function taskMatches(
  task: Pick<Task, "name" | "branch" | "issue_key">,
  query: string,
  epic?: EpicOf | null,
  ticket?: Pick<Ticket, "summary" | "description" | "epic_key" | "epic_summary"> | null,
): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const text = [
    task.issue_key,
    task.name,
    task.branch,
    epic?.key,
    epic?.summary,
    ticket?.summary,
    ticket?.description,
    ticket?.epic_key,
    ticket?.epic_summary,
  ]
    .filter(Boolean)
    .join("\n")
    .toLowerCase();
  return words.every((w) => text.includes(w));
}
