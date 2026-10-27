import type { Ticket } from "./types";

/** The orders Tickets lists in (TKT-13). Find work asks Jira for the same. */
export const TICKET_SORTS = ["updated", "newest", "oldest", "priority"] as const;
export type TicketSort = (typeof TICKET_SORTS)[number];

export const TICKET_SORT_LABEL: Record<TicketSort, string> = {
  updated: "Updated",
  newest: "Newest",
  oldest: "Oldest",
  priority: "Priority",
};

const at = (t: string | null) => (t ? Date.parse(t) || 0 : 0);

/**
 * `priorities` is the site's own list, highest first: names differ per
 * Jira, so none is ranked here. A ticket with no priority, or one the list
 * does not name, goes after every ranked one. Ties go to the most recently
 * updated, and past that the order Jira sent.
 */
export function sortTickets(issues: Ticket[], sort: TicketSort, priorities: readonly string[]): Ticket[] {
  const rank = new Map(priorities.map((p, i) => [p, i]));
  const of = (i: Ticket) => (i.priority !== null ? rank.get(i.priority) : undefined) ?? priorities.length;
  const updated = (a: Ticket, b: Ticket) => at(b.updated) - at(a.updated);
  const by: Record<TicketSort, (a: Ticket, b: Ticket) => number> = {
    updated,
    newest: (a, b) => at(b.created) - at(a.created) || updated(a, b),
    oldest: (a, b) => at(a.created) - at(b.created) || updated(a, b),
    priority: (a, b) => of(a) - of(b) || updated(a, b),
  };
  return [...issues].sort(by[sort]);
}
