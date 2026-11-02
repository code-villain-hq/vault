// Newsletters: every sender whose mail carries a link to unsubscribe, the
// least read first, led by the ones you never open. Unsubscribing asks
// nothing: it archives what the sender sent, and the request waits 10
// seconds on the server, so Undo really undoes it. A sender that takes
// unsubscribing only on its page gets the page opened, or among several,
// a link to it.

import { messageOf, shortcutFree } from "@code-villain/web";
import type {
  KeptInInbox,
  MailAccount,
  Newsletter,
  Newsletters,
  SenderProblem,
  SettingAside,
  UnsubscribeSent,
  Unsubscribed,
} from "@postrider/types";
import { Link, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, RequestError } from "./api";
import { AccountProblems, initial } from "./inbox";
import { PageBody, PageHeader } from "./layout";
import { hue, useAccounts, useFolders } from "./mail";
import { isDesktop, openInBrowser } from "./platform";
import { leastReadFirst, neverOpened, opened, rhythm, senderKey } from "./rhythm";
import { Button, Empty, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** What this page did to a sender, by the batch Undo names. */
type Done =
  | { kind: "unsubscribed"; batch: string; archived: number; asked: boolean; page: string | null }
  | { kind: "kept"; batch: string; moved: number };

/** One click's senders: Undo takes them back together. */
type Batch = {
  keys: string[];
  /** Its requests wait to go. */
  waiting: boolean;
  /** Those whose request failed, by sender. */
  failed: Map<string, string>;
  undoing: boolean;
  problem: string;
};

export function NewslettersPage() {
  const { workspace, place } = useWorkspace();
  const accounts = useAccounts();
  const byId = useMemo(() => new Map(accounts.map((a) => [a.id, a])), [accounts]);
  const { moved } = useFolders();
  const navigate = useNavigate();
  const base = `/api/w/${workspace.id}/newsletters`;

  // Read once, and again after Undo: each read asks every account's server
  // for 90 days of headers. The first time an account comes online, its
  // Inbox newsletters move here: until they have, read every 5 seconds.
  const [data, setData] = useState<Newsletters>();
  const [error, setError] = useState("");
  const load = useCallback(() => {
    api<Newsletters>("GET", base, undefined, place).then(
      (read) => {
        setData(read);
        setError("");
      },
      (problem) => setError(messageOf(problem)),
    );
  }, [base, place]);
  useEffect(load, [load]);
  const moving = data?.setting_aside.some((s) => s.problem === null) ?? false;
  useEffect(() => {
    if (!moving) return;
    const timer = setTimeout(load, 5000);
    return () => clearTimeout(timer);
  }, [moving, data, load]);

  // The senders this page unsubscribed from or kept stay in their place,
  // with what happened, though their mail has left Newsletters.
  const [done, setDone] = useState(new Map<string, { row: Newsletter; done: Done }>());
  const [batches, setBatches] = useState(new Map<string, Batch>());
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState("");

  const rows = useMemo(() => {
    const read = (data?.newsletters ?? []).filter((n) => !done.has(senderKey(n)));
    return [...read, ...[...done.values()].map((d) => d.row)].sort(leastReadFirst);
  }, [data, done]);
  const never = rows.filter((n) => neverOpened(n) && !done.has(senderKey(n)));

  const updateBatch = useCallback((id: string, change: Partial<Batch>) => {
    setBatches((all) => {
      const batch = all.get(id);
      return batch ? new Map(all).set(id, { ...batch, ...change }) : all;
    });
  }, []);

  /** Waits for the requests to go, then shows which failed. */
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
          updateBatch(id, { waiting: false, failed: failedBySender(sent.failed) });
          return;
        } catch (problem) {
          // Undone: nothing went.
          if (problem instanceof RequestError && problem.status === 404) return;
        }
      }
    },
    [base, place, updateBatch],
  );

  const unsubscribe = async (senders: Newsletter[]) => {
    if (busy || !senders.length) return;
    setBusy(true);
    setProblem("");
    try {
      const gone = await api<Unsubscribed>(
        "POST",
        `${base}/unsubscribe`,
        { senders: senders.map(({ account_id, address }) => ({ account_id, address })) },
        place,
      );
      const rowOf = new Map(senders.map((n) => [senderKey(n), n]));
      setDone((all) => {
        const next = new Map(all);
        for (const sender of gone.senders) {
          const row = rowOf.get(senderKey(sender));
          if (!row) continue;
          next.set(senderKey(sender), {
            row,
            done: {
              kind: "unsubscribed",
              batch: gone.id,
              archived: sender.archived,
              asked: sender.asked,
              page: sender.page,
            },
          });
        }
        return next;
      });
      setBatches((all) =>
        new Map(all).set(gone.id, {
          keys: gone.senders.map(senderKey),
          waiting: gone.waiting,
          failed: new Map(),
          undoing: false,
          problem: "",
        }),
      );
      // One sender's page opens; for many, each row links to its own.
      const [only] = gone.senders;
      if (gone.senders.length === 1 && only?.page) openPage(only.page);
      moved();
      if (gone.waiting) void follow(gone.id);
    } catch (problem) {
      setProblem(messageOf(problem));
    } finally {
      setBusy(false);
    }
  };

  const keep = async (sender: Newsletter) => {
    if (busy) return;
    setBusy(true);
    setProblem("");
    try {
      const kept = await api<KeptInInbox>(
        "POST",
        `${base}/keep`,
        { senders: [{ account_id: sender.account_id, address: sender.address }] },
        place,
      );
      const key = senderKey(sender);
      setDone((all) =>
        new Map(all).set(key, {
          row: sender,
          done: { kind: "kept", batch: kept.id, moved: kept.moved },
        }),
      );
      setBatches((all) =>
        new Map(all).set(kept.id, {
          keys: [key],
          waiting: false,
          failed: new Map(),
          undoing: false,
          problem: "",
        }),
      );
      moved();
    } catch (problem) {
      setProblem(messageOf(problem));
    } finally {
      setBusy(false);
    }
  };

  const undo = (id: string) => {
    const batch = batches.get(id);
    if (!batch || batch.undoing) return;
    updateBatch(id, { undoing: true, problem: "" });
    api("POST", `${base}/undo/${id}`, undefined, place).then(
      () => {
        setDone((all) => {
          const next = new Map(all);
          for (const key of batch.keys) next.delete(key);
          return next;
        });
        setBatches((all) => {
          const next = new Map(all);
          next.delete(id);
          return next;
        });
        moved();
        load();
      },
      (problem) => updateBatch(id, { undoing: false, problem: messageOf(problem) }),
    );
  };

  // J and K move through the senders; Enter or O opens the newest mail of
  // the one they're on, U unsubscribes from it.
  const [focusedKey, setFocusedKey] = useState<string>();
  const focused = Math.max(
    0,
    rows.findIndex((n) => senderKey(n) === focusedKey),
  );
  const listRef = useRef<HTMLOListElement>(null);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!shortcutFree(event)) return;
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      const step =
        key === "j" || key === "ArrowDown" ? 1 : key === "k" || key === "ArrowUp" ? -1 : 0;
      const row = rows[focused];
      if (step) {
        event.preventDefault();
        const next = rows[Math.min(Math.max(focused + step, 0), rows.length - 1)];
        if (next) setFocusedKey(senderKey(next));
      } else if (row && ((key === "Enter" && event.target === document.body) || key === "o")) {
        event.preventDefault();
        void navigate(newestMail(workspace.url_key, row));
      } else if (row && key === "u" && !done.has(senderKey(row))) {
        event.preventDefault();
        void unsubscribe([row]);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });
  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${focused}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [focused]);

  // The last click that unsubscribed from more than one says so in the lead.
  const bulk = [...batches.entries()].reverse().find(([, b]) => b.keys.length > 1);

  return (
    <>
      <PageHeader title="Newsletters" />
      <PageBody>
        <div className="newsletters">
          {data && data.problems.length > 0 && (
            <AccountProblems accounts={accounts} statuses={data.problems} />
          )}
          {data && (
            <SettingAsideLines
              accounts={accounts}
              settingAside={data.setting_aside}
              showMoving={rows.length > 0}
            />
          )}
          {bulk ? (
            <BulkLead id={bulk[0]} batch={bulk[1]} done={done} onUndo={undo} />
          ) : (
            never.length > 0 && (
              <div className="lead">
                {never.length === 1 ? (
                  <div>
                    <b>You never open {never[0]?.name}.</b>{" "}
                    <span>It sent you {mails(never[0]?.recent ?? 0)} in the last month.</span>
                  </div>
                ) : (
                  <div>
                    <b>You never open {never.length} of these.</b>{" "}
                    <span>
                      They sent you {mails(never.reduce((sum, n) => sum + n.recent, 0))} in the last
                      month.
                    </span>
                  </div>
                )}
                <Button disabled={busy} onClick={() => void unsubscribe(never)}>
                  {never.length === 1
                    ? `Unsubscribe from ${never[0]?.name}`
                    : `Unsubscribe from those ${never.length}`}
                </Button>
              </div>
            )
          )}
          <ErrorMessage>{problem}</ErrorMessage>
          {!data && !error && <p className="px-3 text-muted">Reading your newsletters…</p>}
          {!data && error && <ErrorMessage>{error}</ErrorMessage>}
          {data &&
            rows.length === 0 &&
            (moving ? (
              <Empty title="Moving your newsletters here">
                The first time, Postrider moves the mail in your Inbox with a link to unsubscribe
                here. It shows in a moment.
              </Empty>
            ) : (
              <Empty title="No newsletters yet">
                Mail with a link to unsubscribe comes here, instead of to your Inbox.
              </Empty>
            ))}
          {rows.length > 0 && (
            <ol ref={listRef} aria-label="Newsletters">
              {rows.map((row, index) => {
                const key = senderKey(row);
                const what = done.get(key)?.done;
                return (
                  <SenderRow
                    key={key}
                    row={row}
                    index={index}
                    focused={index === focused}
                    account={byId.get(row.account_id)}
                    done={what}
                    batch={what ? batches.get(what.batch) : undefined}
                    busy={busy}
                    onUnsubscribe={() => void unsubscribe([row])}
                    onKeep={() => void keep(row)}
                    onUndo={undo}
                    onSelect={() => setFocusedKey(key)}
                  />
                );
              })}
            </ol>
          )}
        </div>
      </PageBody>
    </>
  );
}

/** After unsubscribing from several at once: what it did, with Undo. */
function BulkLead({
  id,
  batch,
  done,
  onUndo,
}: {
  id: string;
  batch: Batch;
  done: Map<string, { done: Done }>;
  onUndo: (id: string) => void;
}) {
  let archived = 0;
  let pages = 0;
  for (const key of batch.keys) {
    const what = done.get(key)?.done;
    if (what?.kind !== "unsubscribed") continue;
    archived += what.archived;
    if (what.page) pages += 1;
  }
  return (
    <div className="lead">
      <div>
        <b>Unsubscribed from {batch.keys.length}.</b>{" "}
        {archived > 0 && <span>Archived {mails(archived)}.</span>}
        {pages > 0 && (
          <span>
            {" "}
            {pages === 1 ? "One asks" : `${pages} ask`} you to finish on their page, linked below.
          </span>
        )}
        {batch.problem && <span className="text-danger-ink"> {batch.problem}</span>}
      </div>
      {undoable(batch, batch.keys, done) && (
        <Button variant="secondary" disabled={batch.undoing} onClick={() => onUndo(id)}>
          {batch.undoing ? "Undoing…" : "Undo"}
        </Button>
      )}
    </div>
  );
}

function SenderRow({
  row,
  index,
  focused,
  account,
  done,
  batch,
  busy,
  onUnsubscribe,
  onKeep,
  onUndo,
  onSelect,
}: {
  row: Newsletter;
  index: number;
  focused: boolean;
  account: MailAccount | undefined;
  done: Done | undefined;
  batch: Batch | undefined;
  busy: boolean;
  onUnsubscribe: () => void;
  onKeep: () => void;
  onUndo: (id: string) => void;
  /** A click chooses the row, as J and K do: its buttons show without a hover. */
  onSelect: () => void;
}) {
  const { workspace } = useWorkspace();
  const key = senderKey(row);
  // A row of several unsubscribed at once leaves Undo, and what it says,
  // to the lead.
  const alone = batch !== undefined && batch.keys.length === 1;
  const warning = batch?.failed.get(key) || (alone ? batch.problem : "");
  return (
    <li
      className={`nl ${focused ? "focused" : ""} ${done ? "gone" : ""}`}
      style={hue(account?.hue ?? 1)}
      data-index={index}
      onPointerDown={onSelect}
    >
      <span className="av" aria-hidden="true">
        {initial(row.name)}
      </span>
      <Link
        {...newestMail(workspace.url_key, row)}
        className="name"
        title={account ? `${row.address}, to ${account.name}` : row.address}
      >
        {row.name}
      </Link>
      <span className="rh">{rhythm(row)}</span>
      {done ? (
        <span className={`op note ${warning ? "text-danger-ink" : ""}`}>
          {warning || <DoneWords done={done} name={row.name} />}
        </span>
      ) : (
        <span className="op">{opened(row)}</span>
      )}
      <span className="act">
        {done ? (
          alone &&
          batch &&
          undoable(batch, [key], new Map([[key, { done }]])) && (
            <Button variant="secondary" disabled={batch.undoing} onClick={() => onUndo(done.batch)}>
              {batch.undoing ? "Undoing…" : "Undo"}
            </Button>
          )
        ) : (
          <>
            <Button variant="secondary" disabled={busy} onClick={onKeep}>
              Keep in Inbox
            </Button>
            <Button variant="secondary" disabled={busy} onClick={onUnsubscribe}>
              Unsubscribe
            </Button>
          </>
        )}
      </span>
    </li>
  );
}

/** What happened to a sender, in a sentence or two. */
function DoneWords({ done, name }: { done: Done; name: string }) {
  if (done.kind === "kept") {
    return <>Kept in the Inbox.{done.moved > 0 && ` Moved ${mails(done.moved)} there.`}</>;
  }
  const archived = done.archived > 0 ? ` Archived ${mails(done.archived)}.` : "";
  if (done.asked) return <>Unsubscribed.{archived}</>;
  if (done.page) {
    const page = done.page;
    return (
      <>
        {archived.trim() || "Archived."}{" "}
        <a
          href={page}
          target="_blank"
          rel="noreferrer"
          className="ui-link"
          onClick={(event) => {
            event.preventDefault();
            openPage(page);
          }}
        >
          Unsubscribe on {name}'s page
        </a>
      </>
    );
  }
  return <>{archived.trim() || "Archived."} It gives no way to unsubscribe.</>;
}

/**
 * Whether Undo still undoes something: while a request waits, after one
 * failed, and always for what only moved mail.
 */
function undoable(batch: Batch, keys: string[], done: Map<string, { done: Done }>): boolean {
  if (batch.waiting || batch.failed.size > 0) return true;
  return keys.some((key) => {
    const what = done.get(key)?.done;
    return what?.kind === "kept" || (what?.kind === "unsubscribed" && !what.asked);
  });
}

function failedBySender(failed: SenderProblem[]): Map<string, string> {
  return new Map(failed.map((f) => [senderKey(f), f.message]));
}

/** "1 mail", "23 mails". */
function mails(count: number): string {
  return `${count} ${count === 1 ? "mail" : "mails"}`;
}

/** A sender's newest mail, opened from Newsletters. */
function newestMail(workspace: string, row: Newsletter) {
  return {
    to: "/$workspace/mail/$account/$uid",
    params: { workspace, account: row.account_id, uid: String(row.newest_uid) },
    search: { folder: "newsletters" },
  } as const;
}

/** A sender's page to unsubscribe on, in the browser. */
function openPage(url: string) {
  if (isDesktop) void openInBrowser(url);
  else window.open(url, "_blank", "noopener");
}

/** The accounts whose newsletters aren't here yet: still moving, or why
 * moving them failed. */
function SettingAsideLines({
  accounts,
  settingAside,
  showMoving,
}: {
  accounts: MailAccount[];
  settingAside: SettingAside[];
  showMoving: boolean;
}) {
  const lines = accounts.flatMap((account) => {
    const aside = settingAside.find((s) => s.account_id === account.id);
    return aside && (aside.problem !== null || showMoving)
      ? [{ account, problem: aside.problem }]
      : [];
  });
  if (!lines.length) return null;
  return (
    <ul className="problems">
      {lines.map(({ account, problem }) => (
        <li key={account.id} className={problem ? "text-danger-ink" : ""}>
          <span className="av sm" style={hue(account.hue)} aria-hidden="true">
            {account.name.charAt(0).toUpperCase()}
          </span>
          <span>
            {problem
              ? `${account.name}: its newsletters stay in the Inbox for now. ${problem} Postrider tries again the next time it connects.`
              : `Moving ${account.name}'s newsletters here from its Inbox…`}
          </span>
        </li>
      ))}
    </ul>
  );
}
