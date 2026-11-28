import { useState } from "react";
import type { CheckoutPr, PullRequest } from "../lib/types";

interface TaskPrCard {
  row: CheckoutPr;
  identity: Pick<PullRequest, "number" | "title" | "url">;
}

function openCards(rows: CheckoutPr[], previous: TaskPrCard[]): TaskPrCard[] {
  return rows.flatMap((row) => {
    if (!row.error && row.pr?.state === "open") {
      const { number, title, url } = row.pr;
      return [{ row, identity: { number, title, url } }];
    }
    if (!row.error || row.pr) return [];
    const last = previous.find((card) => card.row.checkout_id === row.checkout_id);
    return last ? [{ row, identity: last.identity }] : [];
  });
}

/** A failed listing cannot disprove the last PR's identity. Retain only the
 * reader's identity, not its verdict, CI, or actions. A successful listing
 * (including one with no PR) or a removed checkout supersedes it. */
export function useTaskPrCards(rows: CheckoutPr[]): TaskPrCard[] {
  const [held, setHeld] = useState(() => ({ rows, cards: openCards(rows, []) }));
  if (held.rows === rows) return held.cards;
  const cards = openCards(rows, held.cards);
  setHeld({ rows, cards });
  return cards;
}
