import type { Ticket } from "./types";

/** The epic a ticket belongs to: its key, and its title when Jira sent one. */
export interface EpicOf {
  key: string;
  summary: string;
}

/** One heading of a grouped list, and what is under it. */
export interface Group<T> {
  /** Stable across redraws: what remembers a group's fold. `""` is the "none" group. */
  id: string;
  label: string;
  /** A Jira key shown beside the label, for an epic. */
  key?: string;
  items: T[];
}

/**
 * Put `items` under headings, in the order the first of each appears, so a
 * list sorted before grouping stays sorted inside and between groups. Items
 * with no heading go last, under `none`.
 */
export function groupBy<T>(
  items: T[],
  of: (item: T) => { id: string; label: string; key?: string } | null,
  none: string,
): Group<T>[] {
  const groups = new Map<string, Group<T>>();
  const rest: T[] = [];
  for (const item of items) {
    const g = of(item);
    if (!g) {
      rest.push(item);
      continue;
    }
    const group = groups.get(g.id) ?? { ...g, items: [] };
    group.items.push(item);
    groups.set(g.id, group);
  }
  const out = [...groups.values()];
  if (rest.length > 0) out.push({ id: "", label: none, items: rest });
  return out;
}

/** A group per epic, labelled with its title, or its key when that is all there is. */
export function byEpic<T>(items: T[], epicOf: (item: T) => EpicOf | null | undefined): Group<T>[] {
  return groupBy(items, (item) => {
    const epic = epicOf(item);
    return epic ? { id: epic.key, label: epic.summary || epic.key, key: epic.summary ? epic.key : undefined } : null;
  }, "No epic");
}

/**
 * The Jira key in a branch name or a PR title, as `ticket_key_in` in
 * `pr_task.rs` finds it: the first `ABC-123` that is a whole word, or
 * begins one (`ABC-123-fix-login`).
 */
export function ticketKeyIn(text: string): string | null {
  for (const word of text.split(/[^A-Za-z0-9_-]+/)) {
    const [project, number] = word.split("-");
    if (number === undefined) continue;
    if (/^[A-Z][A-Z0-9_]*$/.test(project) && /^[0-9]+$/.test(number)) return `${project}-${number}`;
  }
  return null;
}

/**
 * Which epic each ticket is in, from tickets Jira has sent. An epic-level
 * ticket is its own epic: work on the epic itself belongs under it, not
 * under "No epic". `null` is a ticket known to have none.
 */
export function epicsOf(issues: Ticket[], isEpic: (type: string) => boolean): Record<string, EpicOf | null> {
  const titles = new Map<string, string>();
  for (const i of issues) {
    if (i.epic_key && i.epic_summary) titles.set(i.epic_key, i.epic_summary);
    if (isEpic(i.issue_type)) titles.set(i.key, i.summary);
  }
  const out: Record<string, EpicOf | null> = {};
  for (const i of issues) {
    const key = i.epic_key ?? (isEpic(i.issue_type) ? i.key : null);
    out[i.key] = key ? { key, summary: titles.get(key) ?? "" } : null;
  }
  return out;
}

/** How a review list orders its pull requests (REV-11). */
export const SORTS = ["newest", "oldest", "smallest"] as const;
export type Sort = (typeof SORTS)[number];

export const SORT_LABEL: Record<Sort, string> = {
  newest: "Newest",
  oldest: "Oldest",
  smallest: "Smallest",
};

/**
 * Newest and oldest by last update; smallest by lines changed, for the
 * quick ones first. A PR with no size (one of yours) sorts as newest.
 */
export function sortPrs<T extends { updated_at: string; additions?: number; deletions?: number }>(
  prs: T[],
  sort: Sort,
): T[] {
  const at = (p: T) => Date.parse(p.updated_at) || 0;
  const size = (p: T) => (p.additions ?? 0) + (p.deletions ?? 0);
  const out = [...prs];
  if (sort === "oldest") out.sort((a, b) => at(a) - at(b));
  else if (sort === "smallest") out.sort((a, b) => size(a) - size(b) || at(b) - at(a));
  else out.sort((a, b) => at(b) - at(a));
  return out;
}
