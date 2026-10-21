import type { AcpEntry, AcpView } from "./types";

/**
 * A conversation's entries after `view` (ACP-7), in index order, keeping
 * every entry it did not change as the same object, so only those redraw.
 *
 * It used to copy every entry into a new array and sort it on each update,
 * and a reply streams in as an update per few tokens: in a conversation
 * hundreds of entries long, each token cost a sort of all of them. The
 * changes are almost always the last entry or a new one after it, so each
 * is put in place by a binary search and the list is otherwise only cut
 * where `base` says.
 */
export function mergeEntries(prev: readonly AcpEntry[], view: Pick<AcpView, "base" | "reset" | "entries">): AcpEntry[] {
  const from = view.reset ? prev.length : firstAt(prev, view.base);
  if (view.entries.length === 0 && from === 0) return prev as AcpEntry[];
  const out = view.reset ? [] : prev.slice(from);
  for (const e of view.entries) {
    if (e.index < view.base) continue;
    const at = firstAt(out, e.index);
    if (out[at]?.index === e.index) out[at] = e;
    else out.splice(at, 0, e);
  }
  return out;
}

/** Where the first entry at or after `index` is, or the end. */
function firstAt(list: readonly AcpEntry[], index: number): number {
  let lo = 0;
  let hi = list.length;
  // The last is the usual answer, so it is asked first.
  if (hi > 0 && list[hi - 1].index < index) return hi;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (list[mid].index < index) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}
