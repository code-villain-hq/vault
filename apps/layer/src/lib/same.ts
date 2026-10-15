/**
 * Keeping what a refresh did not change.
 *
 * Every poll hands back fresh objects, equal or not, and a component that
 * selects one re-renders when its reference changes. A PR sweep or a ticket
 * list that found nothing new still redrew the open task — its header, its
 * terminals, every pane — every couple of minutes, for as long as the app
 * ran. A refresh goes through these so that only what changed is new.
 */

/**
 * Each object's JSON, worked out once. What the store holds is never edited
 * in place, so the text of something kept is the same at every comparison:
 * a poll stringifies only what it brought back, not the old list as well.
 */
const texts = new WeakMap<object, string>();

function text(v: unknown): string {
  if (typeof v !== "object" || v === null) return JSON.stringify(v) ?? "";
  let t = texts.get(v);
  if (t === undefined) {
    t = JSON.stringify(v);
    texts.set(v, t);
  }
  return t;
}

/** Whether two values from the backend say the same thing. */
export function sameJson<T>(a: T, b: T): boolean {
  return a === b || text(a) === text(b);
}

/** `next`, or `prev` itself when it says the same thing. */
export function keep<T>(prev: T, next: T): T {
  return sameJson(prev, next) ? prev : next;
}

/**
 * `next`, with each item that is unchanged since `prev` (by `key`) replaced
 * by the one already held, and `prev` itself when nothing at all changed —
 * so a component showing one item is left alone when another one moves.
 */
export function reuse<T>(
  prev: readonly T[],
  next: readonly T[],
  key: (t: T) => string,
  same: (a: T, b: T) => boolean = sameJson,
): T[] {
  const held = new Map(prev.map((t) => [key(t), t]));
  let changed = prev.length !== next.length;
  const out = next.map((t, i) => {
    const old = held.get(key(t));
    if (old !== undefined && same(old, t)) {
      if (prev[i] !== old) changed = true;
      return old;
    }
    changed = true;
    return t;
  });
  return changed ? out : (prev as T[]);
}

/**
 * `rec` without the keys that are not `live`, and `rec` itself when every
 * key is: per-task state is pruned on every task list that changed, and a
 * new object each time would redraw whatever shows it.
 */
export function only<T>(rec: Record<string, T>, live: ReadonlySet<string>): Record<string, T> {
  const keys = Object.keys(rec);
  if (keys.every((k) => live.has(k))) return rec;
  return Object.fromEntries(keys.filter((k) => live.has(k)).map((k) => [k, rec[k]]));
}
