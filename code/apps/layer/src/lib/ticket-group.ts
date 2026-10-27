import type { Ticket } from "./types";

/** How Mine is grouped (TKT-14). A ticket has one epic and one status, so it is listed once. */
export const TICKET_GROUPS = ["epic", "status", "none"] as const;
export type TicketGroup = (typeof TICKET_GROUPS)[number];

export const TICKET_GROUP_LABEL: Record<TicketGroup, string> = {
  epic: "Epic",
  status: "Status",
  none: "None",
};

/** Work under way first, then what waits, then what is finished. Categories, not names: those differ per site. */
const CATEGORY_ORDER = ["indeterminate", "new", "done"];

/**
 * Grouped by status name, so "In Progress" in two Jira projects is one group.
 * `issues` comes in sorted (TKT-13), and each group keeps that order; groups
 * in one category follow the order of their top ticket.
 */
export function groupByStatus(issues: Ticket[]) {
  const buckets = new Map<string, { key: string; name: string; category: string; issues: Ticket[] }>();
  for (const issue of issues) {
    const key = `status:${issue.status}`;
    const bucket = buckets.get(key) ?? { key, name: issue.status, category: issue.status_category, issues: [] };
    bucket.issues.push(issue);
    buckets.set(key, bucket);
  }
  const rank = (c: string) => {
    const i = CATEGORY_ORDER.indexOf(c);
    return i < 0 ? CATEGORY_ORDER.length : i;
  };
  return [...buckets.values()].sort((a, b) => rank(a.category) - rank(b.category));
}
