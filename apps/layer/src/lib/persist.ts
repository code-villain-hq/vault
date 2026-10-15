/**
 * Small guarded wrapper over localStorage, used for the bits of layout that
 * should survive a restart: which task was open, which tab, which pane.
 *
 * Every access is wrapped because the webview can refuse storage outright, and
 * a half-written value should cost a preference rather than the whole app.
 */
const PREFIX = "villain.";

export function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

/** Like `read`, but a value that is no longer one of `allowed` is discarded. */
export function readOneOf<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  const v = read<T>(key, fallback);
  return allowed.includes(v) ? v : fallback;
}

export function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // Storage disabled or full: the preference simply does not stick.
  }
}

/** The part of `Storage` that `forgetAllBut` uses, so a test can hand it a map. */
type Keys = Pick<Storage, "length" | "key" | "removeItem">;

/**
 * Drop every `<name>.<id>` key, for each of `names`, whose id is not in
 * `live`. Per-task layout (the pane last open, whether the browser was) was
 * written for each task and never removed with it, so every task ever made
 * stayed in storage for good.
 */
export function forgetAllBut(names: readonly string[], live: ReadonlySet<string>, store?: Keys): void {
  try {
    const s = store ?? localStorage;
    const gone: string[] = [];
    for (let i = 0; i < s.length; i++) {
      const k = s.key(i);
      if (k === null || !k.startsWith(PREFIX)) continue;
      const rest = k.slice(PREFIX.length);
      const dot = rest.indexOf(".");
      if (dot < 0 || !names.includes(rest.slice(0, dot))) continue;
      if (!live.has(rest.slice(dot + 1))) gone.push(k);
    }
    // Not while walking it: removing shifts every index after it.
    for (const k of gone) s.removeItem(k);
  } catch {
    // Storage disabled: there is nothing in it to forget.
  }
}
