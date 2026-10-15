import { mergeEntries } from "./acpEntries";
import type { AcpEntry, AcpView } from "./types";

/**
 * A conversation as the window last had it, kept for as long as its pane is
 * open, as `Terminal.tsx` keeps a pane's xterm.
 *
 * It used to live in the pane's component, and that unmounts whenever the
 * task changes or the Chat view is left. Coming back then asked from no
 * revision, so the backend sent everything again: up to 2,000 entries, each
 * up to 64KB, across IPC, and every one of them drawn again from scratch.
 * Kept here, a pane coming back shows what it had at once and asks only for
 * what changed since (ACP-7).
 */
export interface HeldConversation {
  entries: AcpEntry[];
  /** The revision the entries are as of; null until first asked. */
  rev: number | null;
  /** The rest of the last view (busy, settings, …), without its entries. */
  view: AcpView | null;
}

const EMPTY: HeldConversation = { entries: [], rev: null, view: null };

const pool = new Map<string, HeldConversation>();

/** What `paneId` was last seen holding, or nothing yet. */
export function heldConversation(paneId: string): HeldConversation {
  return pool.get(paneId) ?? EMPTY;
}

/**
 * `view` merged into what `paneId` holds. A reset from the backend (a
 * conversation replaced, or a revision it cannot have seen) replaces it, as
 * `mergeEntries` does.
 */
export function holdConversation(paneId: string, view: AcpView): HeldConversation {
  const prev = heldConversation(paneId);
  const next: HeldConversation = {
    entries: mergeEntries(prev.entries, view),
    rev: view.rev,
    // A reset's entries are the whole conversation; held twice they would
    // be kept alive by the view as well.
    view: { ...view, entries: [] },
  };
  pool.set(paneId, next);
  return next;
}

/** Forget every pane not in `live`: it has been closed. */
export function pruneConversations(live: Iterable<string>) {
  const keep = new Set(live);
  for (const id of pool.keys()) if (!keep.has(id)) pool.delete(id);
}
