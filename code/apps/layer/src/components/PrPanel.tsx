import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, failureText } from "../lib/api";
import {
  reviewComments,
  taskReview,
  TOKEN_NEEDS_SSO,
  TOKEN_REJECTED,
  type TaskReview,
} from "../lib/derive";
import { checkRunsView } from "../lib/pr-checks";
import { prPermalinkIdentity } from "../lib/pr-permalink";
import { pressKeys } from "../lib/press";
import { reportRepoResults } from "../lib/report";
import type { CheckoutPr, Review, TaskView } from "../lib/types";
import { useStore } from "../store";
import { CopyButton } from "./CopyButton";
import { FinishTask } from "./FinishTask";
import { ChevronIcon, ExternalIcon, PlusIcon } from "./icons";
import { Outgoing } from "./Outgoing";
import { PrCheckList, PrCheckStatus } from "./PrChecks";
import { PrDescriptionField, usePrDescription } from "./PrDescription";
import { PrFeedback } from "./PrFeedback";
import { PrImageOffer } from "./PrImageOffer";
import { PrReadErrors } from "./PrReadErrors";
import { TaskPrComments } from "./TaskPrComments";
import { BusyLabel, Combo, ErrorMessage, Field, Modal, useFailure } from "./ui";
import { useTaskPrCards } from "./useTaskPrCards";

/**
 * The latest review from each reviewer, which is the one GitHub itself shows.
 *
 * Reviewers come back and change their minds; listing every submission would
 * show a PR as both approved and blocked by the same person.
 */
function latestByAuthor(reviews: Review[]): Review[] {
  const by = new Map<string, Review>();
  for (const r of reviews) {
    // A comment after a decision does not undo it — the rule the verdict on
    // the card follows. Letting it through showed a reviewer who approved and
    // then replied as having only commented, under an "approved" chip.
    const prev = by.get(r.author);
    if ((r.state === "COMMENTED" || r.state === "PENDING") && prev && prev.state !== "COMMENTED") {
      continue;
    }
    by.set(r.author, r);
  }
  return [...by.values()];
}

function reviewColor(state: string) {
  if (state === "APPROVED") return "var(--status-ok-ink)";
  if (state === "CHANGES_REQUESTED") return "var(--status-danger-ink)";
  return "var(--muted)";
}

const REVIEW_WORDS: Record<string, string> = {
  APPROVED: "approved",
  CHANGES_REQUESTED: "requested changes",
  COMMENTED: "commented",
  DISMISSED: "dismissed",
};

/** Where the task as a whole stands, in one line. */
const TASK_REVIEW: Record<TaskReview, { word: string; color: string; hint: string }> = {
  none: { word: "", color: "var(--muted)", hint: "" },
  incomplete: {
    word: "Partly up for review",
    color: "var(--status-warn-ink)",
    hint: "Some repositories with changes have no pull request yet",
  },
  open: {
    word: "In review",
    color: "var(--status-info-ink)",
    hint: "Waiting on reviewers: nobody has decided yet",
  },
  commented: {
    word: "In review, with comments",
    color: "var(--status-info-ink)",
    hint: "Reviewers have commented, but nobody has approved or asked for changes yet",
  },
  changes_requested: {
    word: "Changes requested",
    color: "var(--status-danger-ink)",
    hint: "A reviewer asked for changes on at least one pull request",
  },
  approved: {
    word: "Approved",
    color: "var(--status-ok-ink)",
    hint: "Every pull request is approved",
  },
  merged: { word: "Merged", color: "var(--muted)", hint: "Every pull request has merged" },
};

const NO_ROWS: CheckoutPr[] = [];
const UNAVAILABLE_CHECKS = checkRunsView({ checks: [], checks_state: "unavailable" });

interface PrPanelProps {
  task: TaskView;
  onUpdateFromBase: () => void;
}

export function PrPanel(props: PrPanelProps) {
  const githubConnectionRevision = useStore((s) => s.githubConnectionRevision);
  return <ConnectedPrPanel key={githubConnectionRevision} {...props} />;
}

function ConnectedPrPanel({ task, onUpdateFromBase }: PrPanelProps) {
  const settings = useStore((s) => s.settings);
  const toggleSettings = useStore((s) => s.toggleSettings);
  // The panel's own actions fail under its buttons, the dialog's above its
  // footer (NOTE-6).
  const panel = useFailure();
  const opener = useFailure();
  const dialog = useFailure();
  const description = usePrDescription(task, dialog);
  const fail = panel.fail;

  /**
   * The rows, held in the store rather than here.
   *
   * The background watch writes them for every task on a timer and this panel
   * fetches them for one on demand. Two copies meant every action had to
   * refresh both and one of them always forgot. The store also rejects older
   * snapshots that arrive late.
   */
  const snapshot = useStore((s) => s.prs[task.id]) ?? NO_ROWS;
  const rows = useMemo(
    () => snapshot.filter((r) => task.checkouts.some((c) => c.id === r.checkout_id)),
    [snapshot, task.checkouts],
  );
  const live = useTaskPrCards(rows).map(({ row, identity }) => {
    const permalink = prPermalinkIdentity(identity.url);
    // A checkout can be pointed at another repo with the same PR number.
    const prIdentity = JSON.stringify([
      task.id,
      row.checkout_id,
      identity.number,
      permalink ?? identity.url,
    ]);
    return { row, identity, permalink, prIdentity };
  });
  const listingErrors = new Set(
    live
      .filter(
        ({ row }) => row.error && row.error !== TOKEN_REJECTED && row.error !== TOKEN_NEEDS_SSO,
      )
      .map(({ row }) => row.checkout_id),
  );
  const refreshTaskPrs = useStore((s) => s.refreshTaskPrs);
  /**
   * Whether GitHub has answered yet.
   *
   * Without this the panel renders its empty state first — the form for
   * opening a PR — and then replaces it with the PR that was there all along.
   * An answer that arrives a moment later is not a reason to show the wrong
   * one in the meantime.
   */
  const [askedFor, setAskedFor] = useState<string | null>(null);
  // Per task rather than a plain flag. The panel is remounted per task now
  // (TaskMain is keyed by it), but before that a flag left true from the last
  // task showed this one's empty state while its own rows were in flight.
  const loaded = askedFor === task.id || rows.length > 0;
  const [branches, setBranches] = useState<Record<string, string[]>>({});
  /** Whether the "open a pull request" dialog is up. */
  const [creating, setCreating] = useState(false);
  /** The verified PR whose card opened the feedback picker. */
  const [feedback, setFeedback] = useState<string | null>(null);
  const [finishing, setFinishing] = useState(false);
  /** Which PR cards are expanded, when there are enough to be worth folding. */
  const [cards, setCards] = useState<Record<string, boolean>>({});
  const [showPast, setShowPast] = useState(false);
  const [loading, setLoading] = useState(false);
  const [commentsRefresh, setCommentsRefresh] = useState(0);
  const [readFailure, setReadFailure] = useState<{ revision: number; message: string } | null>(
    null,
  );
  const refreshError = useStore((s) =>
    readFailure?.revision === (s.prReadRevision[task.id] ?? 0) ? readFailure.message : null,
  );
  const feedbackCard =
    feedback && !refreshError
      ? live.find(({ row, prIdentity }) => prIdentity === feedback && row.pr && !row.error)
      : undefined;
  // Hiding the picker alone would reopen it with default picks when the PR returns.
  if (feedback && !feedbackCard) setFeedback(null);

  const loadRequest = useRef<symbol | null>(null);
  const [busy, setBusy] = useState(false);
  const [title, setTitle] = useState(task.name);
  const [draft, setDraft] = useState(true);
  /** Repos the user unticked in the dialog: no push, no PR. Everything starts picked. */
  const [leftOut, setLeftOut] = useState<string[]>([]);
  /** What the review step before pushing asks a second look at (PR-11). */
  const [flags, setFlags] = useState(0);

  const load = useCallback(async () => {
    if (!settings?.github_connected) return;
    const request = Symbol();
    const revision = useStore.getState().prReadRevision[task.id] ?? 0;
    loadRequest.current = request;
    setLoading(true);
    try {
      await refreshTaskPrs(task.id);
    } catch (e) {
      if (loadRequest.current === request) {
        // A newer successful read can supersede the error before it renders.
        setFeedback(null);
        setReadFailure({ revision, message: failureText(e) });
      }
    } finally {
      // An error is an answer too: never setting this would hide the form for
      // good and offer nothing in its place.
      if (loadRequest.current === request) {
        setAskedFor(task.id);
        setLoading(false);
      }
    }
  }, [task.id, settings?.github_connected, refreshTaskPrs]);

  useEffect(() => {
    void load();
    return () => {
      loadRequest.current = null;
    };
  }, [load]);

  // What the base field offers. Keyed on the set of repos rather than the rows
  // themselves, so a sweep landing every ninety seconds does not refetch it.
  const checkoutIds = rows.map((r) => r.checkout_id).join(",");
  useEffect(() => {
    if (!creating) return;
    let stop = false;
    void (async () => {
      const ids = checkoutIds.split(",").filter(Boolean);
      // Independent of each other, so they are not waited for in turn.
      const lists = await Promise.all(
        ids.map((id) =>
          // A repo whose branches cannot be listed still takes a typed base.
          api.checkoutBranches(id).catch(() => [] as string[]),
        ),
      );
      if (stop) return;
      setBranches(Object.fromEntries(ids.map((id, i) => [id, lists[i]])));
    })();
    return () => {
      stop = true;
    };
  }, [checkoutIds, creating]);

  useEffect(() => {
    setTitle(task.name);
  }, [task.id, task.name]);

  function openPr(url: string) {
    opener.clear();
    void openUrl(url).catch(() => opener.fail("Couldn't open the browser. Try again."));
  }

  /** Where this repo's next PR goes. Changing it leaves an open PR alone. */
  async function setBase(row: CheckoutPr, base: string) {
    if (base.trim() === row.base || !base.trim()) return;
    panel.clear();
    try {
      await api.setCheckoutBase(row.checkout_id, base);
      // The task list carries the base too, and Update from base reads it
      // from there: left alone it said "← origin/main" while the backend
      // merged the new one.
      await Promise.all([
        load(),
        useStore
          .getState()
          .refreshTasks()
          .catch(() => {}),
      ]);
    } catch (e) {
      fail(e);
    }
  }

  async function retarget(row: CheckoutPr) {
    panel.clear();
    try {
      // The row shows the new base once it is read again (NOTE-7).
      await api.githubRetargetPr(row.checkout_id);
      await load();
    } catch (e) {
      fail(e);
    }
  }

  /** Draft to ready for review and back (PR-15); the card shows it once read again. */
  async function markDraft(row: CheckoutPr, draft: boolean) {
    panel.clear();
    try {
      await api.githubSetPrDraft(row.checkout_id, row.pr!.number, draft);
      await load();
    } catch (e) {
      fail(e);
    }
  }

  async function push() {
    setBusy(true);
    panel.clear();
    try {
      reportRepoResults(fail, await api.pushTask(task.id));
      await load();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function openPrs() {
    setBusy(true);
    dialog.clear();
    try {
      const skippedIds = [...leftOut, ...rows.filter((r) => r.error).map((r) => r.checkout_id)];
      const opened = await api.githubOpenPrs(
        task.id,
        title.trim(),
        description.body,
        draft,
        skippedIds,
      );
      const skipped = rows.filter((r) => skippedIds.includes(r.checkout_id)).map((r) => r.repo);
      const results = [...opened, ...(await description.addImages(task.id, opened, skipped))];
      reportRepoResults(dialog.fail, results);
      // A repository that refused keeps the dialog open, its reason in it.
      if (results.length > 0 && results.every((r) => r.ok)) setCreating(false);
      await load();
    } catch (e) {
      dialog.fail(e);
    } finally {
      setBusy(false);
    }
  }

  if (!settings?.github_connected) {
    return (
      <div className="empty">
        <h2>GitHub not connected</h2>
        <p>Connect GitHub or GitHub Enterprise to open pull requests and read CI from here.</p>
        <button className="ui-button ui-button-primary" onClick={() => toggleSettings(true)}>
          Connect GitHub
        </button>
      </div>
    );
  }

  // A PR that has merged or closed no longer covers the repo: work committed
  // after it landed still needs one, and the row keeps the old PR only so the
  // panel can show what became of it.
  const pending = rows.filter(
    (r) => !refreshError && !r.error && (!r.pr || r.pr.state !== "open") && r.changed > 0,
  );
  const picked = pending.filter((r) => !leftOut.includes(r.checkout_id));
  const failed = !!refreshError || rows.some((r) => r.error);
  const review = failed ? "none" : taskReview(rows);
  const said = reviewComments(rows);

  // What the word beside it is about, merged ones too: a finished task's
  // header read "Merged · 0 PRs".
  const counted = rows.filter((r) => r.pr && (r.pr.state === "open" || r.pr.merged)).length;
  const earlier = rows.flatMap((r) => [
    ...(r.pr && r.pr.state !== "open" ? [{ row: r, pr: r.pr }] : []),
    ...r.past.map((pr) => ({ row: r, pr })),
  ]);
  const entries = live.length + earlier.length;
  const checkRefresh = loading ? "loading" : refreshError ? "failed" : undefined;

  const repoRows = (
    <div className="muted" style={{ marginBottom: 10, lineHeight: 1.6 }}>
      {rows.map((r) => (
        <div key={r.checkout_id} className="row">
          {/*
            Only a repo that would get a new PR can be left out. One with a PR
            already open is pushed with the rest, and is listed in the others.
          */}
          <input
            type="checkbox"
            style={{ width: "auto", visibility: pending.includes(r) ? "visible" : "hidden" }}
            title={`Open a pull request in ${r.repo}`}
            checked={pending.includes(r) && !leftOut.includes(r.checkout_id)}
            disabled={!pending.includes(r)}
            onChange={(e) =>
              setLeftOut((out) =>
                e.target.checked
                  ? out.filter((id) => id !== r.checkout_id)
                  : [...out, r.checkout_id],
              )
            }
          />
          <span
            className="dot"
            style={{
              background:
                r.pr && r.pr.state === "open"
                  ? "var(--status-ok-ink)"
                  : r.changed > 0
                    ? "var(--status-warn-ink)"
                    : "var(--faint)",
            }}
          />
          <span
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: "var(--type-ui-small-size)",
              width: 140,
            }}
          >
            {r.repo}
          </span>
          <span>
            {r.pr && r.pr.state === "open"
              ? `PR #${r.pr.number} open`
              : r.changed > 0
                ? `${r.changed} changed file${r.changed === 1 ? "" : "s"}`
                : "no changes"}
          </span>
          <div className="spacer" />
          <span>→</span>
          {/*
            Typing still works: the list is the worktree's remote-tracking
            refs, so a branch pushed since the last fetch is not in it.
          */}
          <Combo
            value={r.base}
            options={branches[r.checkout_id] ?? []}
            width={240}
            title="The branch this repository's pull request is opened against"
            empty="No branch matches"
            onChange={(v) => void setBase(r, v)}
          />
        </div>
      ))}
    </div>
  );

  return (
    <div className="panel-scroll">
      <div className="row" style={{ marginBottom: 12 }}>
        {review !== "none" ? (
          <>
            <span className="row" style={{ gap: 8 }} title={TASK_REVIEW[review].hint}>
              <span className="dot" style={{ background: TASK_REVIEW[review].color }} />
              <b>{TASK_REVIEW[review].word}</b>
            </span>
            <span className="muted">
              {counted} PR{counted === 1 ? "" : "s"}
              {/* A task is only as reviewed as its least reviewed repository. */}
              {review === "incomplete" &&
                `, ${pending.length} repo${pending.length === 1 ? "" : "s"} still without one`}
              {said > 0 && ` · ${said} comment${said === 1 ? "" : "s"}`}
            </span>
          </>
        ) : (
          <b>Pull requests</b>
        )}
        <div className="spacer" />
        <button
          className="ui-button"
          onClick={() => {
            setCommentsRefresh((revision) => revision + 1);
            void load();
          }}
        >
          <BusyLabel busy={loading} idle="Refresh" working="Refreshing…" />
        </button>
        <button className="ui-button" disabled={busy} onClick={() => void push()}>
          Push all
        </button>
        {(entries > 0 || (failed && pending.length > 0)) && (
          <button
            // The primary only while a repository still waits for its pull
            // request; under review, the feedback is what needs acting on.
            className={`ui-button${pending.length > 0 && review === "incomplete" ? " ui-button-primary" : ""}`}
            aria-label="Open a pull request"
            disabled={pending.length === 0}
            title={
              pending.length === 0
                ? "Every repository with changes already has an open pull request"
                : `Open a pull request in ${pending.length} repositor${
                    pending.length === 1 ? "y" : "ies"
                  }`
            }
            onClick={() => {
              setLeftOut([]);
              dialog.clear();
              setCreating(true);
            }}
          >
            <PlusIcon />
          </button>
        )}
      </div>
      <ErrorMessage>{panel.error}</ErrorMessage>
      <ErrorMessage>{opener.error}</ErrorMessage>
      <ErrorMessage>{loading ? null : refreshError}</ErrorMessage>

      {review === "merged" && (
        <div className="card row">
          <span className="dot" style={{ background: "var(--status-ok-ink)" }} />
          <span className="muted" style={{ flex: 1, lineHeight: 1.5 }}>
            Every pull request has merged
            {pending.length > 0
              ? `, but ${pending.map((r) => r.repo).join(", ")} ${
                  pending.length === 1 ? "has" : "have"
                } work since. Finishing keeps ${pending.length === 1 ? "that branch" : "those branches"}.`
              : "."}
          </span>
          <button className="ui-button ui-button-primary" onClick={() => setFinishing(true)}>
            Finish task…
          </button>
        </div>
      )}

      {loaded && !failed && entries === 0 && (
        <div className="card">
          <div className="muted" style={{ lineHeight: 1.6, marginBottom: 12 }}>
            Nothing has been opened from <code>{task.branch}</code> yet.
            {pending.length > 0
              ? ` ${pending.length} repositor${pending.length === 1 ? "y has" : "ies have"} changes ready.`
              : " No repository has changes to open one with."}
          </div>
          <button
            className={`ui-button${pending.length > 0 ? " ui-button-primary" : ""}`}
            disabled={pending.length === 0}
            onClick={() => {
              setLeftOut([]);
              dialog.clear();
              setCreating(true);
            }}
          >
            Open pull request
          </button>
        </div>
      )}

      <PrImageOffer taskId={task.id} fail={fail} onAdded={() => void load()} />

      {live.map(({ row, identity, permalink, prIdentity }) => {
        const pr = row.error || refreshError ? null : row.pr;
        const checks = pr ? checkRunsView(row) : UNAVAILABLE_CHECKS;
        // The picker still loads feedback on its own; this is just its badge.
        const toAnswer = pr
          ? pr.comments +
            pr.review_comments +
            checks.runs.filter((c) => c.outcome === "failing").length
          : 0;
        const key = row.checkout_id;
        // Several fold, so the list reads as a list rather than as a wall of
        // check runs. One on its own is always shown — and stays shown if the
        // others go away while it is folded, which would otherwise leave it
        // collapsed with no chevron left to open it.
        const foldable = live.length > 1;
        const shown = !foldable || (cards[key] ?? false);
        const toggle = () => setCards((c) => ({ ...c, [key]: !shown }));
        // Only a heading that folds is a button; it holds buttons of its own.
        const fold = foldable
          ? {
              role: "button",
              tabIndex: 0,
              "aria-expanded": shown,
              onClick: toggle,
              onKeyDown: pressKeys(toggle),
            }
          : {};
        return (
          <div key={key} className="card">
            <div
              className="row"
              style={{ flexWrap: "wrap", cursor: foldable ? "pointer" : undefined }}
              {...fold}
            >
              {foldable && (
                <span className={`chev${shown ? " open" : ""}`}>
                  <ChevronIcon />
                </span>
              )}
              <h3 style={{ margin: 0, overflowWrap: "anywhere" }}>
                <span style={{ color: "var(--muted)" }}>{row.repo}</span> #{identity.number}{" "}
                {identity.title}
              </h3>
              <div className="spacer" />
              {!pr && <span className="muted">Last known pull request</span>}
              {pr?.draft && <span className="ui-chip">draft</span>}
              {/* The action a draft waits for; going back is the quiet link under the title. */}
              {pr?.draft && (
                <button
                  className="ui-button"
                  title="Mark it ready for review on GitHub"
                  onClick={(e) => {
                    e.stopPropagation();
                    void markDraft(row, false);
                  }}
                >
                  Ready for review
                </button>
              )}
              {/* GitHub's own reading of the branch against its base. */}
              {(pr?.mergeable_state === "dirty" || pr?.mergeable_state === "behind") && (
                <button
                  className={`ui-status ${pr.mergeable_state === "dirty" ? "ui-status-danger" : "ui-status-warn"}`}
                  title={`Bring ${pr.base} into this branch — merge or rebase`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onUpdateFromBase();
                  }}
                >
                  {pr.mergeable_state === "dirty"
                    ? `conflicts with ${pr.base}`
                    : `behind ${pr.base}`}
                </button>
              )}
              {pr && row.verdict === "approved" && (
                <span className="ui-status ui-status-ok">approved</span>
              )}
              {pr && row.verdict === "changes_requested" && (
                <span className="ui-status ui-status-danger">changes requested</span>
              )}
              <PrCheckStatus view={checks} refresh={checkRefresh} />
              {/*
                On the card, not once for the task: one button for every PR
                gave no way to tell whose comments its count was.
              */}
              {pr && (
                <button
                  className="ui-button"
                  title={
                    row.to_reply > 0
                      ? `Pick this PR's review threads, comments and failing checks to hand an agent. ${row.to_reply} wait on your reply.`
                      : "Pick this PR's review threads, comments and failing checks to hand an agent"
                  }
                  onClick={(e) => {
                    e.stopPropagation();
                    setFeedback(prIdentity);
                  }}
                >
                  Feedback → agent
                  {toAnswer > 0 && <span className="badge">{toAnswer}</span>}
                  {row.to_reply > 0 && (
                    <span className="badge badge-reply">· {row.to_reply} to reply</span>
                  )}
                </button>
              )}
              {/* Stopped, or opening the PR would fold the card underneath it. */}
              <button
                className="ui-button ui-icon-button"
                title="Open on GitHub"
                onClick={(e) => {
                  e.stopPropagation();
                  openPr(identity.url);
                }}
              >
                <ExternalIcon />
              </button>
              <CopyButton text={identity.url} label="Copy link" />
            </div>
            {listingErrors.has(key) && <ErrorMessage>{row.error}</ErrorMessage>}

            {shown && (
              <>
                {pr && (
                  <div className="muted" style={{ marginTop: 6 }}>
                    {pr.head} → {pr.base} · opened by {pr.author}
                    {pr.comments + pr.review_comments > 0 && (
                      <>
                        {" · "}
                        {pr.comments + pr.review_comments} comment
                        {pr.comments + pr.review_comments === 1 ? "" : "s"}
                      </>
                    )}
                    {!pr.draft && (
                      <>
                        {" · "}
                        <button
                          className="ui-link"
                          title="Convert it back to a draft on GitHub"
                          onClick={() => void markDraft(row, true)}
                        >
                          Back to draft
                        </button>
                      </>
                    )}
                  </div>
                )}

                {pr && pr.base !== row.base && (
                  <div className="row" style={{ marginTop: 10 }}>
                    <span className="ui-status ui-status-warn">targets {pr.base}</span>
                    <button className="ui-button" onClick={() => void retarget(row)}>
                      Move onto {row.base}
                    </button>
                    {/*
                      The other way round. GitHub moves a PR itself when its
                      base is merged and deleted, and then the only offer
                      was to move it back onto a branch that is gone.
                    */}
                    <button
                      className="ui-button"
                      title={`Update from base and the next PR use ${pr.base} in ${row.repo}`}
                      onClick={() => void setBase(row, pr.base)}
                    >
                      Use {pr.base} here
                    </button>
                  </div>
                )}

                {pr && row.reviews.length > 0 && (
                  <div style={{ marginTop: 10 }}>
                    {latestByAuthor(row.reviews).map((r) => (
                      <div key={r.author} className="check">
                        <span className="dot" style={{ background: reviewColor(r.state) }} />
                        <span className="name">{r.author}</span>
                        <span className="muted">
                          {REVIEW_WORDS[r.state] ?? r.state.toLowerCase()}
                        </span>
                        {r.url && (
                          <button
                            className="ui-button ui-icon-button"
                            title="Open on GitHub"
                            onClick={() => openPr(r.url)}
                          >
                            <ExternalIcon />
                          </button>
                        )}
                      </div>
                    ))}
                  </div>
                )}

                {pr && (checks.runs.length > 0 || !checkRefresh) && (
                  <div style={{ marginTop: 10 }}>
                    <PrCheckList view={checks} onOpen={openPr} />
                  </div>
                )}
                <TaskPrComments
                  key={prIdentity}
                  taskId={task.id}
                  checkoutId={row.checkout_id}
                  number={identity.number}
                  identity={prIdentity}
                  permalink={permalink}
                  refresh={commentsRefresh}
                  onOpen={openPr}
                />
              </>
            )}
          </div>
        );
      })}

      {earlier.length > 0 && (
        <div className="card">
          <div
            role="button"
            tabIndex={0}
            aria-expanded={showPast}
            className="row"
            style={{ cursor: "pointer" }}
            onClick={() => setShowPast((v) => !v)}
            onKeyDown={pressKeys(() => setShowPast((v) => !v))}
          >
            <span className={`chev${showPast ? " open" : ""}`}>
              <ChevronIcon />
            </span>
            <h3 style={{ margin: 0 }}>Earlier pull requests</h3>
            <span className="muted">{earlier.length}</span>
          </div>
          {showPast &&
            earlier.map(({ row, pr }) => (
              <div key={`${row.checkout_id}:${pr.number}`} className="check">
                <span className="dot" style={{ background: "var(--muted)" }} />
                <span className="name">
                  {row.repo} #{pr.number}
                </span>
                <span className="muted">
                  {pr.merged
                    ? `merged into ${pr.base}`
                    : pr.state === "open"
                      ? `still open against ${pr.base}`
                      : "closed without merging"}
                </span>
                <button
                  className="ui-button ui-icon-button"
                  title="Open on GitHub"
                  onClick={() => openPr(pr.url)}
                >
                  <ExternalIcon />
                </button>
                <CopyButton text={pr.url} label="Copy link" />
              </div>
            ))}
        </div>
      )}

      {loaded && <PrReadErrors rows={rows.filter((r) => !listingErrors.has(r.checkout_id))} />}

      {feedbackCard && (
        <PrFeedback
          key={feedbackCard.prIdentity}
          task={task}
          first={{
            checkoutId: feedbackCard.row.checkout_id,
            number: feedbackCard.identity.number,
            permalink: feedbackCard.permalink,
          }}
          onClose={() => setFeedback(null)}
        />
      )}
      {finishing && review === "merged" && (
        <FinishTask task={task} onClose={() => setFinishing(false)} />
      )}

      {creating && (
        <Modal
          title="Open pull request"
          wide
          onClose={() => setCreating(false)}
          footer={
            <>
              <button className="ui-button" onClick={() => setCreating(false)}>
                Cancel
              </button>
              <button
                className="ui-button ui-button-primary"
                disabled={busy || picked.length === 0 || !title.trim()}
                onClick={() => void openPrs()}
              >
                {busy
                  ? "Working…"
                  : `${flags ? "Open anyway: " : "Push & open "}${picked.length} PR${picked.length === 1 ? "" : "s"}`}
              </button>
            </>
          }
        >
          {repoRows}
          <Outgoing taskId={task.id} onFlags={setFlags} />

          {pending.length === 0 ? (
            <div className="muted" style={{ lineHeight: 1.6 }}>
              Every repository with changes already has an open pull request.
            </div>
          ) : (
            <>
              <Field label="Title">
                <input
                  className="ui-field"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                />
              </Field>
              <PrDescriptionField d={description} />
              <div className="row" style={{ marginBottom: 12 }}>
                <label className="row" style={{ gap: 6, cursor: "pointer" }}>
                  <input
                    type="checkbox"
                    style={{ width: "auto" }}
                    checked={draft}
                    onChange={(e) => setDraft(e.target.checked)}
                  />
                  Open as draft
                </label>
              </div>
            </>
          )}

          <div className="muted" style={{ lineHeight: 1.55 }}>
            Opens one PR per ticked repository, all from <code>{task.branch}</code>, with this title
            and description. When there is more than one, each PR lists the others below it.
            {task.issue_key
              ? ` The links are also posted back to ${task.issue_key} as a single comment.`
              : ""}
          </div>
          <ErrorMessage>{dialog.error}</ErrorMessage>
        </Modal>
      )}
    </div>
  );
}
