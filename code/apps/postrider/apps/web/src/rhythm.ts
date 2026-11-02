// The Newsletters page's words for a sender: how often it writes, and how
// much of it you open.

import type { Newsletter } from "@postrider/types";

type Counted = Pick<Newsletter, "sent" | "opened" | "per_week">;

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

/** How much of it you opened: "Never opened", "Opened 3 of 22". */
export function opened({ sent, opened }: Counted): string {
  return opened === 0 ? "Never opened" : `Opened ${opened} of ${sent}`;
}

/** Never opened, and sent more than once: the ones the page leads with. */
export function neverOpened({ sent, opened }: Counted): boolean {
  return opened === 0 && sent >= 2;
}

/** The least read first; of those alike, who sends most. As the server sorts. */
export function leastReadFirst(a: Newsletter, b: Newsletter): number {
  const read = (n: Newsletter) => n.opened / Math.max(n.sent, 1);
  return (
    read(a) - read(b) || b.sent - a.sent || a.name.toLowerCase().localeCompare(b.name.toLowerCase())
  );
}

/** A sender, the same in each list: its account and address. */
export function senderKey(n: { account_id: string; address: string }): string {
  return `${n.account_id}:${n.address}`;
}
