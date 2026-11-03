// What one sender sent, in Newsletters: with no mail of it open, the
// reading pane leads with unsubscribing from it, and Keep in Inbox beside.
// U and I do the same from its list; a newsletter open, from here or not,
// has both above it (`message.tsx`), and they come back here to do it.
// Both work as on the Newsletters page: Unsubscribe archives its mail at
// once and the request waits 10 seconds on the server, so Undo really
// undoes it. Once its mail has left, the list goes back to the page, where
// its row says what was done, with Undo.

import { messageOf, shortcutFree } from "@code-villain/web";
import type { KeptInInbox, Newsletter, UnsubscribeSent, Unsubscribed } from "@postrider/types";
import { useParams } from "@tanstack/react-router";
import { useCallback, useEffect, useSyncExternalStore } from "react";
import { api, RequestError } from "./api";
import { useListed } from "./folders";
import { PageBody } from "./layout";
import { useFolders } from "./mail";
import { type Done, DoneWords, openPage, pageName, pageRows } from "./newsletters";
import type { Place } from "./server";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** What was done to a sender here, kept while the app runs, by its address. */
export type Doing = {
  busy: boolean;
  problem: string;
  /** Its name when it was done: its mail, and its name with it, leave the list. */
  name: string;
  done?: Done;
  /** Its rows on the Newsletters page when it was done, which show it there. */
  rows: Newsletter[];
  /** Its request waits to go. */
  waiting: boolean;
  /** Why its request failed. */
  failed: string;
  undoing: boolean;
  /** Asked for with one of its mail open: done once its list is read. */
  wanted?: Wanted;
};

/** What can be asked for a sender. */
export type Wanted = "unsubscribe" | "keep";

const IDLE: Doing = {
  busy: false,
  problem: "",
  name: "",
  rows: [],
  waiting: false,
  failed: "",
  undoing: false,
};

/** A new map at each change, for what watches it. */
let doings = new Map<string, Doing>();
const watchers = new Set<() => void>();

function change(sender: string, to: Partial<Doing>) {
  const key = sender.toLowerCase();
  doings = new Map(doings).set(key, { ...(doings.get(key) ?? IDLE), ...to });
  for (const watcher of watchers) watcher();
}

function forget(sender: string) {
  doings = new Map(doings);
  doings.delete(sender.toLowerCase());
  for (const watcher of watchers) watcher();
}

function watch(watcher: () => void) {
  watchers.add(watcher);
  return () => void watchers.delete(watcher);
}

/**
 * Asks to unsubscribe from a sender, or to keep it in the Inbox, from one
 * of its mail: its own view does it once it has read its list.
 */
export function want(sender: string, wanted: Wanted) {
  change(sender, { wanted, problem: "" });
}

/** What was done to each sender in its own view, by its address. */
export function useDoings(): ReadonlyMap<string, Doing> {
  return useSyncExternalStore(watch, () => doings);
}

/** Undoes what was done to a sender in its own view: whether it did. */
export async function undoSender(sender: string, base: string, place: Place): Promise<boolean> {
  const doing = doings.get(sender.toLowerCase());
  const done = doing?.done;
  if (!done || doing.undoing) return false;
  change(sender, { undoing: true, problem: "" });
  try {
    await api("POST", `${base}/undo/${done.batch}`, undefined, place);
    forget(sender);
    return true;
  } catch (problem) {
    change(sender, { undoing: false, problem: messageOf(problem) });
    return false;
  }
}

/**
 * Unsubscribing from a sender, keeping it in the Inbox, and Undo, for its
 * mail in every account it wrote to, as its list read it.
 */
export function useSender(sender: string) {
  const { workspace, place } = useWorkspace();
  const { moved } = useFolders();
  const listing = useListed({ folder: "newsletters", sender });
  const doing = useDoings().get(sender.toLowerCase()) ?? IDLE;
  const base = `/api/w/${workspace.id}/newsletters`;
  const mails = listing?.mails ?? [];
  // Named as the Newsletters page names it, else as its mail does.
  const name =
    pageName(workspace.id, sender) ?? (doing.done ? doing.name : mails[0]?.from_name || sender);
  const senders = [...new Set(mails.map((mail) => mail.account_id))].map((account_id) => ({
    account_id,
    address: sender,
  }));
  const ready = senders.length > 0 && !doing.busy && !doing.done;

  /** Waits for the request to go, then says whether it failed. */
  const follow = useCallback(
    async (id: string) => {
      for (let tries = 0; tries < 3; tries++) {
        try {
          const sent = await api<UnsubscribeSent>(
            "GET",
            `${base}/unsubscribed/${id}`,
            undefined,
            place,
          );
          change(sender, {
            waiting: false,
            failed: sent.failed.map((f) => f.message).join(" "),
          });
          return;
        } catch (problem) {
          // Undone: nothing went.
          if (problem instanceof RequestError && problem.status === 404) return;
        }
      }
    },
    [base, place, sender],
  );

  const unsubscribe = async () => {
    if (!ready) return;
    change(sender, { busy: true, problem: "" });
    try {
      const gone = await api<Unsubscribed>("POST", `${base}/unsubscribe`, { senders }, place);
      const asked = gone.senders.some((s) => s.asked);
      const page = asked ? null : (gone.senders.find((s) => s.page)?.page ?? null);
      change(sender, {
        busy: false,
        name,
        rows: pageRows(workspace.id, sender),
        done: {
          kind: "unsubscribed",
          batch: gone.id,
          archived: gone.senders.reduce((sum, s) => sum + s.archived, 0),
          asked,
          page,
        },
        waiting: gone.waiting,
        failed: "",
      });
      if (page) openPage(page);
      moved();
      if (gone.waiting) void follow(gone.id);
    } catch (problem) {
      change(sender, { busy: false, problem: messageOf(problem) });
    }
  };

  const keep = async () => {
    if (!ready) return;
    change(sender, { busy: true, problem: "" });
    try {
      const kept = await api<KeptInInbox>("POST", `${base}/keep`, { senders }, place);
      change(sender, {
        busy: false,
        name,
        rows: pageRows(workspace.id, sender),
        done: { kind: "kept", batch: kept.id, moved: kept.moved },
        waiting: false,
        failed: "",
      });
      moved();
    } catch (problem) {
      change(sender, { busy: false, problem: messageOf(problem) });
    }
  };

  const undo = () => {
    void undoSender(sender, base, place).then((undone) => undone && moved());
  };

  // Undo undoes something while the request waits, after it failed, and
  // always for what only moved mail.
  const undoable =
    doing.done !== undefined &&
    (doing.waiting ||
      doing.failed !== "" ||
      doing.done.kind === "kept" ||
      (doing.done.kind === "unsubscribed" && !doing.done.asked));

  return { listing, name, doing, ready, undoable, unsubscribe, keep, undo };
}

/**
 * U unsubscribes from the sender and I keeps it in the Inbox, from its
 * list with none of its mail open (an open one has its own, `message.tsx`):
 * the pane shows what it did, with Undo. What one of its mail asked for
 * is done here once the list is read. Gives what `useSender` does, for the
 * list's name.
 */
export function useSenderKeys(sender: string, reading: boolean) {
  const actions = useSender(sender);
  const { ready, unsubscribe, keep, doing } = actions;
  useEffect(() => {
    if (!doing.wanted) return;
    // Done already, from somewhere else: there's nothing left to ask.
    if (!ready && !doing.done) return;
    const wanted = doing.wanted;
    change(sender, { wanted: undefined });
    if (doing.done) return;
    void (wanted === "unsubscribe" ? unsubscribe() : keep());
  });
  useEffect(() => {
    if (reading) return;
    const onKeyDown = (event: KeyboardEvent) => {
      // G then I goes to the Inbox: the shortcuts took that I already.
      if (event.defaultPrevented || !shortcutFree(event)) return;
      const key = event.key.toLowerCase();
      if ((key !== "u" && key !== "i") || !ready) return;
      event.preventDefault();
      void (key === "u" ? unsubscribe() : keep());
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });
  return actions;
}

/**
 * The reading pane beside a sender's list, with none of its mail open:
 * how much of it you read, Unsubscribe first and Keep in Inbox; once one
 * is done, what it did and Undo.
 */
export function SenderPane() {
  const { sender = "" } = useParams({ strict: false }) as { sender?: string };
  const { listing, name, doing, ready, undoable, unsubscribe, keep, undo } = useSender(sender);
  const mails = listing?.mails ?? [];
  const opened = mails.filter((mail) => mail.seen).length;
  const done = doing.done;
  return (
    <>
      <div className="app-topbar min-h-12 shrink-0" data-tauri-drag-region="deep" />
      <PageBody>
        {(done || mails.length > 0) && (
          <div className="mx-auto flex max-w-md flex-col items-center gap-3 px-4 py-20 text-center">
            <p className="font-medium">{name}</p>
            {done ? (
              <p className={doing.failed ? "text-danger-ink" : "text-muted"}>
                {doing.failed || <DoneWords done={done} name={name} />}
              </p>
            ) : (
              <p className="text-muted">
                {opened === 0
                  ? `You never opened ${mails.length === 1 ? "this mail" : `any of these ${mails.length}`}.`
                  : `You opened ${opened} of these ${mails.length}.`}
              </p>
            )}
            {done ? (
              undoable && (
                <Button variant="secondary" disabled={doing.undoing} onClick={undo}>
                  {doing.undoing ? "Undoing…" : "Undo"}
                </Button>
              )
            ) : (
              <div className="flex flex-wrap justify-center gap-2">
                <Button
                  disabled={!ready}
                  onClick={() => void unsubscribe()}
                  title="Unsubscribe (U)"
                >
                  Unsubscribe
                </Button>
                <Button
                  variant="secondary"
                  disabled={!ready}
                  onClick={() => void keep()}
                  title="Keep in Inbox (I)"
                >
                  Keep in Inbox
                </Button>
              </div>
            )}
            <ErrorMessage>{doing.problem}</ErrorMessage>
          </div>
        )}
      </PageBody>
    </>
  );
}
