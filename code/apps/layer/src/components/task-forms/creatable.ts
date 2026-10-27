import type { TicketType } from "../../lib/types";

/**
 * Types a new ticket can be: a plain issue, never an epic or a sub-task —
 * one would be a sibling of the parent, the other needs a parent of its own.
 */
export function creatableTypes(issueTypes: TicketType[]): TicketType[] {
  return issueTypes.filter((t) => t.hierarchy_level === 0 && !t.subtask);
}

/** Default to whatever the site calls a plain issue, without assuming "Task". */
export function preferredCreatable(creatable: TicketType[]): TicketType | undefined {
  return creatable.find((t) => t.name.toLowerCase() === "task") ?? creatable[0];
}

/** Types at epic level and above — names differ per site; hierarchy does not. */
export function epicTypes(issueTypes: TicketType[]): TicketType[] {
  return issueTypes.filter((t) => t.hierarchy_level >= 1);
}

/** Prefer the site's "Epic" when present; otherwise the first level-1 type. */
export function preferredEpic(types: TicketType[]): TicketType | undefined {
  return types.find((t) => /^epic$/i.test(t.name)) ?? types[0];
}
