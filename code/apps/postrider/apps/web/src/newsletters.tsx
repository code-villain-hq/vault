// Newsletters: every sender whose mail went to many, as its header says or
// Claude Code found reading it, the least read first, led by the ones you
// never open. A sender that writes from several addresses, as the same
// name at the same domain, is one row (the server folds them). Those with
// mail you haven't opened come first, with how much, as the sidebar counts it. Each column's head sorts
// by it, remembered on this device, and / finds a sender by any name it
// wrote as. Unsubscribing asks nothing: it archives what the sender sent,
// and the request waits 10 seconds on the server, so Undo really undoes it.
// A sender that takes unsubscribing only on its page gets the page opened,
// or among several, a link to it. A sender's own view whose mail has left
// comes back here, where it was, to the row below it.

import { messageOf, shortcutFree } from "@code-villain/web";
import type {
  KeptInInbox,
  MailAccount,
  Newsletter,
  NewsletterRef,
  Newsletters,
  SenderProblem,
  SettingAside,
  UnsubscribeSent,
  Unsubscribed,
} from "@postrider/types";
import { Link, useNavigate } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, Search } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, RequestError } from "./api";
import { AccountProblems, initial, useNow } from "./inbox";
import { PageBody, PageHeader, Unread } from "./layout";
import { hue, mailTime, useAccounts, useFolders } from "./mail";
import { isDesktop, openInBrowser } from "./platform";
import {
  FIRST_SORT,
  finds,
  nearest,
  neverOpened,
  opened,
  rhythm,
  type Sort,
  type SortBy,
  senderKey,
  sorted,
  unread,
  writesFrom,
} from "./rhythm";
import { undoSender, useDoings } from "./sender";
import { Button, Empty, ErrorMessage } from "./ui";
import { useWorkspace } from "./workspace";

/** What this page did to a sender, by the batch Undo names. */
export type Done =
  | { kind: "unsubscribed"; batch: string; archived: number; asked: boolean; page: string | null }
  | { kind: "kept"; batch: string; moved: number };

/** One click's senders: Undo takes them back together. */
type Batch = {
  keys: string[];
  /** The sender it was done to in its own view, each of its rows with Undo. */
  sender?: string;
  /** Its requests wait to go. */
  waiting: boolean;
  /** Those whose request failed, by sender. */
  failed: Map<string, string>;
  undoing: boolean;
  problem: string;
};

/** The page as each workspace read it last, shown at once when it opens
 * again: its senders' views name them, and keep their rows, by it. */
const lastRead = new Map<string, Newsletters>();

/** A sender's name as the page shows it, by its address. */
export function pageName(workspace: string, address: string): string | undefined {
  return pageRows(workspace, address)[0]?.name;
}

/** A sender's rows on the page, one per account it wrote to, by any
 * address it writes from. */
export function pageRows(workspace: string, address: string): Newsletter[] {
  return (lastRead.get(workspace)?.newsletters ?? []).filter((n) => writesFrom(n, address));
}

/** The sender whose view emptied and came back here, by its address. */
let cameBackFrom: string | undefined;
/** Where the page was scrolled to when it was left. */
let scrolledTo = 0;

/** Goes back to the page from a sender's view, once its mail has left. */
export function cameBack(address: string) {
  cameBackFrom = address.toLowerCase();
}

export function NewslettersPage() {
  const { workspace, place } = useWorkspace();
  const accounts = useAccounts();
  const byId = useMemo(() => new Map(accounts.map((a) => [a.id, a])), [accounts]);
  const { moved, refresh, folders } = useFolders();
  const navigate = useNavigate();
  const now = useNow();
  const base = `/api/w/${workspace.id}/newsletters`;

  // Read when the page opens, after Undo, and when what's unread in
  // Newsletters changes, as new ones come or you open one elsewhere: the
  // server reads only what came since its last read. The first time an
  // account comes online, its Inbox newsletters move here: until they have,
  // read every 5 seconds.
  const [data, setData] = useState(() => lastRead.get(workspace.id));
  const [error, setError] = useState("");
  const load = useCallback(() => {
    api<Newsletters>("GET", base, undefined, place).then(
      (read) => {
        lastRead.set(workspace.id, read);
        setData(read);
        setError("");
        refresh();
      },
      (problem) => setError(messageOf(problem)),
    );
  }, [workspace.id, base, place, refresh]);
  useEffect(load, [load]);
  const moving = data?.setting_aside.some((s) => s.problem === null) ?? false;
  useEffect(() => {
    if (!moving) return;
    const timer = setTimeout(load, 5000);
    return () => clearTimeout(timer);
  }, [moving, data, load]);
  const counted = folders.find((f) => f.role === "newsletters")?.unread;
  const countedBefore = useRef(counted);
  useEffect(() => {
    const before = countedBefore.current;
    countedBefore.current = counted;
    if (before !== undefined && counted !== before) load();
  }, [counted, load]);

  const [sort, setSort] = useState(storedSort);
  const sortBy = (by: SortBy) => {
    const next = { by, reversed: sort.by === by && !sort.reversed };
    setSort(next);
    try {
      localStorage.setItem(SORT_KEY, JSON.stringify(next));
    } catch {
      // Not kept: it sorts so until the page closes.
    }
  };
  const [find, setFind] = useState("");

  // The senders this page unsubscribed from or kept stay in their place,
  // with what happened, though their mail has left Newsletters; so does
  // the one whose own view came back here, as that did it.
  const [doneHere, setDone] = useState(new Map<string, { row: Newsletter; done: Done }>());
  const [batchesHere, setBatches] = useState(new Map<string, Batch>());
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState("");
  const [back] = useState(() => cameBackFrom);
  useEffect(() => {
    cameBackFrom = undefined;
  }, []);
  const doing = useDoings().get(back ?? "");
  const { done, batches } = useMemo(() => {
    if (!back || !doing?.done) return { done: doneHere, batches: batchesHere };
    const what = doing.done;
    const keys = doing.rows.map(senderKey);
    const failed = new Map(doing.failed ? keys.map((key) => [key, doing.failed]) : []);
    return {
      done: new Map([
        ...doing.rows.map((row) => [senderKey(row), { row, done: what }] as const),
        ...doneHere,
      ]),
      batches: new Map(batchesHere).set(what.batch, {
        keys,
        sender: back,
        waiting: doing.waiting,
        failed,
        undoing: doing.undoing,
        problem: doing.problem,
      }),
    };
  }, [back, doing, doneHere, batchesHere]);

  const all = useMemo(() => {
    const read = (data?.newsletters ?? []).filter((n) => !done.has(senderKey(n)));
    return [...read, ...[...done.values()].map((d) => d.row)];
  }, [data, done]);
  const never = all.filter((n) => neverOpened(n) && !done.has(senderKey(n)));
  const rows = useMemo(
    () =>
      sorted(
        all.filter((n) => finds(n, find)),
        sort,
      ),
    [all, find, sort],
  );

  const updateBatch = useCallback((id: string, change: Partial<Batch>) => {
    setBatches((all) => {
      const batch = all.get(id);
      return batch ? new Map(all).set(id, { ...batch, ...change }) : all;
    });
  }, []);

  /** Waits for the requests to go, then shows which failed. */
  const follow = useCallback(
    async (id: string, rowOf: Map<string, Newsletter>) => {
      for (let tries = 0; tries < 3; tries++) {
        try {
          const sent = await api<UnsubscribeSent>(
            "GET",
            `${base}/unsubscribed/${id}`,
            undefined,
            place,
          );
          updateBatch(id, { waiting: false, failed: failedByRow(sent.failed, rowOf) });
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
        { senders: refs(senders) },
        place,
      );
      const rowOf = byAddress(senders);
      const each = unsubscribedRows(gone, rowOf);
      setDone((all) => new Map([...all, ...each]));
      setBatches((all) =>
        new Map(all).set(gone.id, {
          keys: [...each.keys()],
          waiting: gone.waiting,
          failed: new Map(),
          undoing: false,
          problem: "",
        }),
      );
      // One sender's page opens; for many, each row links to its own.
      const [only, ...more] = each.values();
      if (only?.done.kind === "unsubscribed" && only.done.page && more.length === 0) {
        openPage(only.done.page);
      }
      moved();
      if (gone.waiting) void follow(gone.id, rowOf);
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
        { senders: refs([sender]) },
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
    if (batch.sender) {
      void undoSender(batch.sender, base, place).then((undone) => {
        if (!undone) return;
        moved();
        load();
      });
      return;
    }
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

  // J and K move through the senders; Enter or O opens what the one they're
  // on sent, U unsubscribes from it and I keeps it in the Inbox.
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
        void navigate(senderMail(workspace.url_key, row));
      } else if (row && key === "u" && !done.has(senderKey(row))) {
        event.preventDefault();
        void unsubscribe([row]);
      } else if (row && key === "i" && !done.has(senderKey(row)) && !event.defaultPrevented) {
        // G then I goes to the Inbox: the shortcuts took that I already.
        event.preventDefault();
        void keep(row);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });
  // The page's place shows at once: it moves to a sender only as you do.
  const shown = useRef(focused);
  useEffect(() => {
    if (shown.current === focused) return;
    shown.current = focused;
    listRef.current
      ?.querySelector(`[data-index="${focused}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [focused]);

  // Back from a sender's view, the page is where it was, at the row below
  // the sender's.
  const pageRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const body = pageRef.current?.closest(".page-content");
    if (!body) return;
    if (back) body.scrollTop = scrolledTo;
    const onScroll = () => {
      scrolledTo = body.scrollTop;
    };
    body.addEventListener("scroll", onScroll, { passive: true });
    return () => body.removeEventListener("scroll", onScroll);
  }, [back]);
  const placed = useRef(false);
  useEffect(() => {
    if (!back || placed.current || rows.length === 0) return;
    placed.current = true;
    const next = nearest(rows, back);
    if (next) setFocusedKey(senderKey(next));
  }, [back, rows]);

  // The last click that unsubscribed from more than one says so in the lead.
  const bulk = [...batches.entries()].reverse().find(([, b]) => b.keys.length > 1 && !b.sender);

  return (
    <>
      <PageHeader title="Newsletters">
        <FindSender text={find} onChange={setFind} />
      </PageHeader>
      <PageBody>
        <div ref={pageRef} className="newsletters">
          {data && data.problems.length > 0 && (
            <AccountProblems accounts={accounts} statuses={data.problems} />
          )}
          {data && (
            <SettingAsideLines
              accounts={accounts}
              settingAside={data.setting_aside}
              notSorted={data.not_sorted}
              showMoving={all.length > 0}
            />
          )}
          {bulk ? (
            <BulkLead id={bulk[0]} batch={bulk[1]} done={done} onUndo={undo} />
          ) : (
            // While you look for one sender, the ones you never open wait.
            never.length > 0 &&
            !find.trim() && (
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
            all.length === 0 &&
            (moving ? (
              <Empty title="Moving your newsletters here">
                The first time, Postrider moves the newsletters in your Inbox here. It shows in a
                moment.
              </Empty>
            ) : (
              <Empty title="No newsletters yet">
                Newsletters and offers, mail sent to many, come here instead of to your Inbox.
              </Empty>
            ))}
          {all.length > 0 && (
            <div className="nl-table">
              <SortHeads sort={sort} onSort={sortBy} />
              {rows.length === 0 && (
                <p className="nl-none text-muted">No senders match “{find.trim()}”.</p>
              )}
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
                      now={now}
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
            </div>
          )}
          {data && <Named named={data.named} rows={all} />}
        </div>
      </PageBody>
    </>
  );
}

/** Where the page's order is kept on this device. */
const SORT_KEY = "postrider.newsletters.sort";

/** The order chosen last on this device, else the least read first. */
function storedSort(): Sort {
  try {
    const kept = JSON.parse(localStorage.getItem(SORT_KEY) ?? "null") as Partial<Sort> | null;
    if (kept && kept.by && kept.by in HEADS)
      return { by: kept.by, reversed: kept.reversed === true };
  } catch {
    // Nothing kept, or nothing to keep it in.
  }
  return FIRST_SORT;
}

/** The heads, by what each sorts. */
const HEADS: Record<SortBy, string> = {
  name: "Sender",
  newest: "Last wrote",
  often: "How often",
  opened: "Opened",
};

/** The columns' heads: one clicked sorts by it, clicked again the other way. */
function SortHeads({ sort, onSort }: { sort: Sort; onSort: (by: SortBy) => void }) {
  return (
    <div className="nl-heads" role="toolbar" aria-label="Sort the senders">
      {(Object.entries(HEADS) as [SortBy, string][]).map(([by, label]) => {
        const on = sort.by === by;
        const Arrow = sort.reversed ? ArrowUp : ArrowDown;
        return (
          <button
            key={by}
            type="button"
            className={`sort sort-${by} ${on ? "on" : ""}`}
            aria-pressed={on}
            onClick={() => onSort(by)}
          >
            {label}
            {on && <Arrow size={12} strokeWidth={2} aria-hidden="true" />}
          </button>
        );
      })}
    </div>
  );
}

/**
 * "Find a sender", in the top bar where the other lists search: / goes to
 * it; Enter or ↓ goes on to the senders it found, Esc empties it.
 */
function FindSender({ text, onChange }: { text: string; onChange: (text: string) => void }) {
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "/" || !shortcutFree(event)) return;
      event.preventDefault();
      field.current?.focus();
      field.current?.select();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  return (
    <div role="search" className="search find ui-field">
      <Search size={14} strokeWidth={1.75} aria-hidden="true" />
      <input
        ref={field}
        type="search"
        value={text}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onChange("");
            field.current?.blur();
          } else if (event.key === "Enter" || event.key === "ArrowDown") {
            event.preventDefault();
            field.current?.blur();
          }
        }}
        placeholder="Find a sender"
        aria-label="Find a sender"
        spellCheck={false}
      />
      {!text && (
        <kbd className="ui-kbd" aria-hidden="true">
          /
        </kbd>
      )}
    </div>
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
  now,
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
  now: Date;
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
  const alone = batch !== undefined && (batch.keys.length === 1 || batch.sender !== undefined);
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
      <span className="who">
        <Link
          {...senderMail(workspace.url_key, row)}
          className="name"
          title={`${row.addresses.join(", ")}${account ? `, to ${account.name}` : ""}`}
        >
          {row.name}
        </Link>
        {!done && <Unread count={unread(row)} />}
      </span>
      <span className="lw" title={new Date(row.newest).toLocaleString()}>
        {mailTime(row.newest, now)}
      </span>
      <span className="meta">
        <span className="rh">{rhythm(row)}</span>
        {done ? (
          <span className={`op note ${warning ? "text-danger-ink" : ""}`}>
            {warning || <DoneWords done={done} name={row.name} />}
          </span>
        ) : (
          <span className="op">{opened(row)}</span>
        )}
      </span>
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
export function DoneWords({ done, name }: { done: Done; name: string }) {
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

/** Every address the rows write from, as the server takes senders. */
function refs(rows: Newsletter[]): NewsletterRef[] {
  return rows.flatMap(({ account_id, addresses }) =>
    addresses.map((address) => ({ account_id, address })),
  );
}

/** The rows by each address they write from, keyed as `senderKey` keys. */
function byAddress(rows: Newsletter[]): Map<string, Newsletter> {
  return new Map(
    rows.flatMap((row) =>
      row.addresses.map(
        (address) => [senderKey({ account_id: row.account_id, address }), row] as const,
      ),
    ),
  );
}

/** What unsubscribing did to each row, by its key: its addresses' added up. */
function unsubscribedRows(
  gone: Unsubscribed,
  rowOf: Map<string, Newsletter>,
): Map<string, { row: Newsletter; done: Done }> {
  const each = new Map<string, { row: Newsletter; done: Done }>();
  for (const sender of gone.senders) {
    const row = rowOf.get(senderKey(sender));
    if (!row) continue;
    const had = each.get(senderKey(row))?.done;
    const before = had?.kind === "unsubscribed" ? had : { archived: 0, asked: false, page: null };
    const asked = before.asked || sender.asked;
    each.set(senderKey(row), {
      row,
      done: {
        kind: "unsubscribed",
        batch: gone.id,
        archived: before.archived + sender.archived,
        asked,
        page: asked ? null : (before.page ?? sender.page),
      },
    });
  }
  return each;
}

/** Why each row's request failed, its addresses' reasons together. */
function failedByRow(failed: SenderProblem[], rowOf: Map<string, Newsletter>): Map<string, string> {
  const each = new Map<string, string>();
  for (const f of failed) {
    const row = rowOf.get(senderKey(f));
    const key = senderKey(row ?? f);
    each.set(key, [each.get(key), f.message].filter(Boolean).join(" "));
  }
  return each;
}

/** The senders Postrider's rule names, since their mail doesn't say it went
 * to many but Claude Code found it always does: by the name they write as. */
function Named({ named, rows }: { named: NewsletterRef[]; rows: Newsletter[] }) {
  const names = [
    ...new Set(
      named.map(
        (n) =>
          rows.find((r) => r.account_id === n.account_id && writesFrom(r, n.address))?.name ??
          n.address,
      ),
    ),
  ];
  if (names.length === 0) return null;
  const shown = names.length > 5 ? [...names.slice(0, 4), `${names.length - 4} more`] : names;
  const list = new Intl.ListFormat("en", { type: "conjunction" }).format(shown);
  return (
    <p className="px-3 pt-4 text-muted">
      {names.length === 1
        ? `${list} doesn't say its mail goes to many, so Postrider's rule names it and its ` +
          "mail comes here at once. Keep in Inbox takes it off."
        : `${list} don't say their mail goes to many, so Postrider's rule names them and ` +
          "their mail comes here at once. Keep in Inbox takes one off."}
    </p>
  );
}

/** "1 mail", "23 mails". */
export function mails(count: number): string {
  return `${count} ${count === 1 ? "mail" : "mails"}`;
}

/** What a sender sent, in the list column, to open beside it. */
function senderMail(workspace: string, row: Newsletter) {
  return {
    to: "/$workspace/newsletters/$sender",
    params: { workspace, sender: row.address },
  } as const;
}

/** A sender's page to unsubscribe on, in the browser. */
export function openPage(url: string) {
  if (isDesktop) void openInBrowser(url);
  else window.open(url, "_blank", "noopener");
}

/** The accounts whose newsletters aren't here yet: still moving, or why
 * moving them failed; and those whose mail couldn't be sorted again. */
function SettingAsideLines({
  accounts,
  settingAside,
  notSorted,
  showMoving,
}: {
  accounts: MailAccount[];
  settingAside: SettingAside[];
  notSorted: SettingAside[];
  showMoving: boolean;
}) {
  const lines = accounts.flatMap((account) => {
    const aside = settingAside.find((s) => s.account_id === account.id);
    const unsorted = notSorted.find((s) => s.account_id === account.id)?.problem;
    if (unsorted) return [{ account, problem: unsorted, again: true }];
    return aside && (aside.problem !== null || showMoving)
      ? [{ account, problem: aside.problem, again: false }]
      : [];
  });
  if (!lines.length) return null;
  return (
    <ul className="problems">
      {lines.map(({ account, problem, again }) => (
        <li key={account.id} className={problem ? "text-danger-ink" : ""}>
          <span className="av sm" style={hue(account.hue)} aria-hidden="true">
            {account.name.charAt(0).toUpperCase()}
          </span>
          <span>
            {again
              ? `${account.name}: couldn't sort its newsletters and notices again. ${problem} Postrider tries again the next time it connects.`
              : problem
                ? `${account.name}: its newsletters stay in the Inbox for now. ${problem} Postrider tries again the next time it connects.`
                : `Moving ${account.name}'s newsletters here from its Inbox…`}
          </span>
        </li>
      ))}
    </ul>
  );
}
