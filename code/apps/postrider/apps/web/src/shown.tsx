// What a list shows: all of it, what's unread or what's read. Each list
// keeps its own while the app runs: the Inbox, each folder, each sender's
// mail, Flagged, the search and the Newsletters page's senders. F, or the control in
// the list's top bar, goes from one to the next. What a list showed since
// its filter was picked stays while you're on it, so mail you open in
// Unread doesn't leave under you.
import { shortcutFree } from "@code-villain/web";
import { useEffect, useSyncExternalStore } from "react";
import type { From } from "./inbox";
import { Button, Empty } from "./ui";

export type Shown = "all" | "unread" | "read";

const ORDER: Shown[] = ["all", "unread", "read"];

let shown = new Map<string, Shown>();
/** The rows each list showed since its filter was picked, by their key. */
const stayed = new Map<string, Set<string>>();
const watchers = new Set<() => void>();

function watch(watcher: () => void) {
  watchers.add(watcher);
  return () => void watchers.delete(watcher);
}

/** Shows `to` in the list `list`, from now on. */
export function show(list: string, to: Shown) {
  shown = new Map(shown).set(list, to);
  stayed.delete(list);
  for (const watcher of watchers) watcher();
}

/** The list a mail list is, by where it was opened from. */
export function listOf(from: From): string {
  if (from.flagged) return "flagged";
  if (from.q !== undefined) return "search";
  if (from.sender !== undefined) return `sender:${from.sender.toLowerCase()}`;
  if (from.folder !== undefined) return `folder:${from.folder.toLowerCase()}`;
  return "inbox";
}

/** What the list `list` shows. */
export function useShown(list: string): Shown {
  return useSyncExternalStore(watch, () => shown.get(list) ?? "all");
}

/**
 * The rows of `list` its filter shows: those unread or read as it asks,
 * those it showed already, and the one `open`.
 */
export function pick<T>(
  list: string,
  to: Shown,
  rows: T[],
  keyOf: (row: T) => string,
  unread: (row: T) => boolean,
  open: (row: T) => boolean = () => false,
): T[] {
  if (to === "all") return rows;
  const was = stayed.get(list) ?? new Set<string>();
  stayed.set(list, was);
  return rows.filter((row) => {
    const key = keyOf(row);
    if (unread(row) !== (to === "unread") && !was.has(key) && !open(row)) return false;
    was.add(key);
    return true;
  });
}

const NAMES: Record<Shown, string> = { all: "All", unread: "Unread", read: "Read" };

const TITLES: Record<"mail" | "senders", Record<Shown, string>> = {
  mail: { all: "All mail", unread: "Only unread mail", read: "Only read mail" },
  senders: {
    all: "Every sender",
    unread: "Only senders with unread mail",
    read: "Only senders you read all of",
  },
};

/**
 * All, Unread and Read, in a list's top bar, and F to go to the next. What
 * the list showed stays until you leave it.
 */
export function ShownControl({ list, of = "mail" }: { list: string; of?: "mail" | "senders" }) {
  const to = useShown(list);
  useEffect(() => () => void stayed.delete(list), [list]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !shortcutFree(event) || event.key.toLowerCase() !== "f") {
        return;
      }
      event.preventDefault();
      const now = shown.get(list) ?? "all";
      show(list, ORDER[(ORDER.indexOf(now) + 1) % ORDER.length] ?? "all");
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [list]);
  return (
    <fieldset className="ui-segmented shrink-0" aria-label="Show">
      {ORDER.map((option) => (
        <button
          key={option}
          type="button"
          className="ui-segment"
          aria-pressed={to === option}
          title={`${TITLES[of][option]} (F)`}
          onClick={() => show(list, option)}
        >
          {NAMES[option]}
        </button>
      ))}
    </fieldset>
  );
}

/** A list its filter left empty: says so, with the way back to all of it. */
export function NoneShown({ list, of = "mail" }: { list: string; of?: "mail" | "senders" }) {
  const to = useShown(list);
  const title =
    of === "mail"
      ? to === "unread"
        ? "No unread mail"
        : "No read mail"
      : to === "unread"
        ? "No sender has unread mail"
        : "No sender is read in full";
  return (
    <Empty title={title}>
      <Button variant="secondary" onClick={() => show(list, "all")}>
        {of === "mail" ? "Show all mail" : "Show every sender"}
      </Button>
    </Empty>
  );
}
