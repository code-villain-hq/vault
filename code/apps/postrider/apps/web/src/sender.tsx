// What one sender sent, in Newsletters: with no mail of it open, the
// reading pane leads with unsubscribing from it, and Keep in Inbox beside.
// U and I do the same from its list, a mail open or not. Both work as on
// the Newsletters page: Unsubscribe archives its mail at once and the
// request waits 10 seconds on the server, so Undo really undoes it.

import { messageOf, shortcutFree } from "@code-villain/web";
import type { KeptInInbox, UnsubscribeSent, Unsubscribed } from "@postrider/types";
import { useNavigate, useParams } from "@tanstack/react-router";
import { useCallback, useEffect, useSyncExternalStore } from "react";
import { api, RequestError } from "./api";
import { useListed } from "./folders";
import { PageBody } from "./layout";
import { useFolders } from "./mail";
import { type Done, DoneWords, openPage } from "./newsletters";
import { Button, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** What was done to a sender here, kept while the app runs, by its address. */
type Doing = {
  busy: boolean;
  problem: string;
  /** Its name when it was done: its mail, and its name with it, leave the list. */
  name: string;
  done?: Done;
  /** Its request waits to go. */
  waiting: boolean;
  /** Why its request failed. */
  failed: string;
  undoing: boolean;
};

const IDLE: Doing = {
  busy: false,
  problem: "",
  name: "",
  waiting: false,
  failed: "",
  undoing: false,
};

const doings = new Map<string, Doing>();
const watchers = new Set<() => void>();
let version = 0;

function change(sender: string, to: Partial<Doing>) {
  const key = sender.toLowerCase();
  doings.set(key, { ...(doings.get(key) ?? IDLE), ...to });
  version += 1;
  for (const watcher of watchers) watcher();
}

function forget(sender: string) {
  doings.delete(sender.toLowerCase());
  version += 1;
  for (const watcher of watchers) watcher();
}

function watch(watcher: () => void) {
  watchers.add(watcher);
  return () => void watchers.delete(watcher);
}

/**
 * Unsubscribing from a sender, keeping it in the Inbox, and Undo, for its
 * mail in every account it wrote to, as its list read it.
 */
export function useSender(sender: string) {
  const { workspace, place } = useWorkspace();
  const { moved } = useFolders();
  const listing = useListed({ folder: "newsletters", sender });
  useSyncExternalStore(watch, () => version);
  const doing = doings.get(sender.toLowerCase()) ?? IDLE;
  const base = `/api/w/${workspace.id}/newsletters`;
  const mails = listing?.mails ?? [];
  const name = doing.done ? doing.name : mails[0]?.from_name || sender;
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
    const done = doing.done;
    if (!done || doing.undoing) return;
    change(sender, { undoing: true, problem: "" });
    api("POST", `${base}/undo/${done.batch}`, undefined, place).then(
      () => {
        forget(sender);
        moved();
      },
      (problem) => change(sender, { undoing: false, problem: messageOf(problem) }),
    );
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
 * list, a mail of it open or not: the pane shows what it did, with Undo.
 * Gives what `useSender` does, for the list's name.
 */
export function useSenderKeys(sender: string, reading: boolean) {
  const { workspace } = useWorkspace();
  const navigate = useNavigate();
  const actions = useSender(sender);
  const { ready, unsubscribe, keep } = actions;
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // G then I goes to the Inbox: the shortcuts took that I already.
      if (event.defaultPrevented || !shortcutFree(event)) return;
      const key = event.key.toLowerCase();
      if ((key !== "u" && key !== "i") || !ready) return;
      event.preventDefault();
      if (reading) {
        void navigate({
          to: "/$workspace/newsletters/$sender",
          params: { workspace: workspace.url_key, sender },
        });
      }
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
