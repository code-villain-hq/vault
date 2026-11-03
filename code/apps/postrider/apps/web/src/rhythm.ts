// The Newsletters page's words for a sender: how often it writes, and how
// much of it you open; and the orders and the finding its list offers.

import type { Newsletter } from "@postrider/types";

type Counted = Pick<Newsletter, "sent" | "opened" | "per_week" | "recent">;

/** How often it writes: "Daily", "3 a week", "Weekly". */
export function rhythm({ sent, per_week: perWeek }: Counted): string {
  if (sent <= 1) return "Once";
  if (perWeek >= 5) return "Daily";
  if (perWeek >= 1.5) return `${Math.round(perWeek)} a week`;
  if (perWeek >= 0.75) return "Weekly";
  if (perWeek >= 0.35) return "Every 2 weeks";
  if (perWeek >= 0.15) return "Monthly";
  return "Now and then";
}

/** How many of its mails you haven't opened yet. */
export function unread({ sent, opened }: Counted): number {
  return Math.max(sent - opened, 0);
}

/** How much of it you opened: "Never opened", "Opened 3 of 22". */
export function opened({ sent, opened }: Counted): string {
  return opened === 0 ? "Never opened" : `Opened ${opened} of ${sent}`;
}

/**
 * Never opened, sent more than once, and still writing (in the last month):
 * the ones the page leads with.
 */
export function neverOpened({ sent, opened, recent }: Counted): boolean {
  return opened === 0 && sent >= 2 && recent > 0;
}

/** The least read first; of those alike, who sends most. As the server sorts. */
export function leastReadFirst(a: Newsletter, b: Newsletter): number {
  const read = (n: Newsletter) => n.opened / Math.max(n.sent, 1);
  return (
    read(a) - read(b) || b.sent - a.sent || a.name.toLowerCase().localeCompare(b.name.toLowerCase())
  );
}

/** What the senders can be sorted by, each by its column's head. */
export type SortBy = "name" | "newest" | "often" | "opened";

/** An order, the way its head first sorts it, or the other way round. */
export type Sort = { by: SortBy; reversed: boolean };

/** How the page first sorts: the least read first. */
export const FIRST_SORT: Sort = { by: "opened", reversed: false };

const byName = (a: Newsletter, b: Newsletter) =>
  a.name.localeCompare(b.name, undefined, { sensitivity: "base" });

/** How often it writes, as `rhythm` says it: once is least often. */
const often = (n: Newsletter) => (n.sent <= 1 ? 0 : n.per_week);

/**
 * Each head's order: the names from A, the newest mail first, who writes
 * most often first, the least read first.
 */
const ORDERS: Record<SortBy, (a: Newsletter, b: Newsletter) => number> = {
  name: (a, b) => byName(a, b) || a.address.localeCompare(b.address),
  newest: (a, b) => Date.parse(b.newest) - Date.parse(a.newest) || byName(a, b),
  often: (a, b) => often(b) - often(a) || b.sent - a.sent || byName(a, b),
  opened: leastReadFirst,
};

/**
 * The senders in `sort`'s order, those with mail you haven't opened first,
 * whatever the order: where the sidebar's count leads.
 */
export function sorted(senders: Newsletter[], sort: Sort): Newsletter[] {
  const order = ORDERS[sort.by];
  const waiting = (n: Newsletter) => (unread(n) > 0 ? 0 : 1);
  return [...senders].sort(
    (a, b) => waiting(a) - waiting(b) || (sort.reversed ? order(b, a) : order(a, b)),
  );
}

/** Lower case, without accents: "Zoë" is found by "zoe". */
function plain(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

/**
 * Whether a sender has every word of `text` in a name its mail gave or in
 * its address: "dimitrios" finds LinkedIn's messages, whoever wrote last.
 */
export function finds(sender: Newsletter, text: string): boolean {
  const words = plain(text).split(/\s+/).filter(Boolean);
  const said = plain([sender.name, ...sender.names, sender.address].join(" "));
  return words.every((word) => said.includes(word));
}

/** A sender, the same in each list: its account and address. */
export function senderKey(n: { account_id: string; address: string }): string {
  return `${n.account_id}:${n.address}`;
}
