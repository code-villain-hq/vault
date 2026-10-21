import { useState, type MouseEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import { goTo } from "../lib/goto";
import { ago } from "../lib/time";
import { useStore } from "../store";
import type { AuthoredPr, ReviewRequest } from "../lib/types";
import { BusyLabel, ErrorMessage, useFailure } from "./ui";

const NO_BROWSER = "Couldn't open the browser. Try again.";

/** How a banner names a pull request: `owner/repo#n`, as `news.rs` writes it. */
export function identity(pr: { repo: string; number: number }): string {
  return pr.repo ? `${pr.repo}#${pr.number}` : `#${pr.number}`;
}

/**
 * A pull request waiting on your review, and what it asks of you (REV-9).
 * A click opens it on GitHub, where it is reviewed.
 */
export function ReviewCard({
  pr,
  focused,
  asked,
  onContextMenu,
}: {
  pr: ReviewRequest;
  focused: boolean;
  /** Who it was asked of, once the list is not grouped by that (REV-11). */
  asked?: string;
  onContextMenu: (e: MouseEvent) => void;
}) {
  // A card has no room for the browser failing to open: the top bar says it.
  const fail = useStore((s) => s.fail);
  const open = () => void openUrl(pr.url).catch(() => fail(NO_BROWSER));

  return (
    // A div, not a button: it holds buttons of its own.
    <div
      role="button"
      tabIndex={0}
      className={`review-card${focused ? " focused" : ""}`}
      data-review={identity(pr)}
      onClick={open}
      onKeyDown={(e) => {
        if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          open();
        }
      }}
      onContextMenu={onContextMenu}
      title={pr.url}
    >
      <div className="top">
        <span className="repo">{pr.repo || "pull request"}</span>
        <span className="num">#{pr.number}</span>
        {pr.draft && <span className="ui-chip">draft</span>}
        <span className="spacer" />
        <span className="when">{ago(pr.updated_at)}</span>
        <button
          type="button"
          className="ui-link"
          title={pr.url}
          onClick={(e) => { e.stopPropagation(); open(); }}
        >
          GitHub ↗
        </button>
      </div>
      <div className="title">{pr.title}</div>
      <div className="chips standing">
        {pr.author && <span className="by">{pr.author}</span>}
        {asked && <span className="ui-chip">{asked}</span>}
        <span className="ui-chip size" title={`${pr.changed_files} files, opened ${ago(pr.created_at)} ago`}>
          <span className="add">+{pr.additions}</span> <span className="del">−{pr.deletions}</span>
          {" · "}{pr.changed_files} {pr.changed_files === 1 ? "file" : "files"}
        </span>
        {pr.cross_repo && <span className="ui-chip" title="Its branch is on a fork">fork</span>}
        {pr.checks === "failing" && <span className="ui-status ui-status-danger">checks failing</span>}
        {pr.checks === "pending" && <span className="ui-status ui-status-warn">checks running</span>}
        {pr.checks === "passing" && <span className="ui-status ui-status-ok">checks pass</span>}
        <MyReview pr={pr} />
      </div>
    </div>
  );
}

/** Where your own review of it stands (REV-9): what this request asks of you. */
function MyReview({ pr }: { pr: ReviewRequest }) {
  if (pr.new_since_review) {
    return <span className="ui-status ui-status-warn" title="Its author pushed after your last review">new commits since your review</span>;
  }
  switch (pr.my_review) {
    case "approved": return <span className="ui-status ui-status-ok">you approved</span>;
    case "changes_requested": return <span className="ui-status ui-status-danger">you asked for changes</span>;
    case "commented": return <span className="ui-chip">you commented</span>;
    case "dismissed": return <span className="ui-chip">your review was dismissed</span>;
    default: return <span className="ui-status ui-status-warn">not reviewed yet</span>;
  }
}

/**
 * One of your pull requests and where it stands (REV-4), and the way to act
 * on it (REV-7): a click opens its task's Pull requests tab, where feedback
 * goes to an agent; one with no task gets one on its branch. GitHub is the
 * button in the corner. Where it stands is the heading of its group (REV-8).
 */
export function AuthoredCard({
  pr,
  task,
  onContextMenu,
}: {
  pr: AuthoredPr;
  task: { id: string; name: string } | null;
  onContextMenu: (e: MouseEvent) => void;
}) {
  // Opening GitHub fails in the top bar, like the review card; Start task
  // under the card's chips, by its button (NOTE-6).
  const fail = useStore((s) => s.fail);
  const failure = useFailure();
  const refreshTasks = useStore((s) => s.refreshTasks);
  const refreshPrs = useStore((s) => s.refreshPrs);
  const [starting, setStarting] = useState(false);
  const who = (names: string[]) => (names.length ? ` by ${names.join(", ")}` : "");
  const openGitHub = () => void openUrl(pr.url).catch(() => fail(NO_BROWSER));
  const open = () => (task ? goTo(`pr:${task.id}`) : openGitHub());

  async function start() {
    setStarting(true);
    failure.clear();
    try {
      const made = await api.taskForPr(pr.repo, pr.head, pr.base, pr.title);
      // In the list before going to it: a task the store has not seen yet
      // routes to the overview instead.
      await refreshTasks();
      goTo(`pr:${made.id}`);
      void refreshPrs();
    } catch (e) {
      failure.fail(e);
    } finally {
      setStarting(false);
    }
  }

  return (
    // A div, not a button: it holds buttons of its own.
    <div
      role="button"
      tabIndex={0}
      className="review-card"
      data-review={identity(pr)}
      // Selecting a failure's text is not a click on the card.
      onClick={(e) => { if (!(e.target as Element).closest(".error-message")) open(); }}
      onKeyDown={(e) => {
        if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          open();
        }
      }}
      onContextMenu={onContextMenu}
      title={task ? `Open ${task.name}` : pr.url}
    >
      <div className="top">
        <span className="repo">{pr.repo}</span>
        <span className="num">#{pr.number}</span>
        <span className="spacer" />
        <span className="when">{ago(pr.updated_at)}</span>
        <button
          type="button"
          className="ui-link"
          title={pr.url}
          onClick={(e) => { e.stopPropagation(); openGitHub(); }}
        >
          GitHub ↗
        </button>
      </div>
      <div className="title">{pr.title}</div>
      <div className="chips standing">
        {pr.checks === "failing" && <span className="ui-status ui-status-danger">checks failing</span>}
        {pr.checks === "pending" && <span className="ui-status ui-status-warn">checks running</span>}
        {pr.checks === "passing" && <span className="ui-status ui-status-ok">checks pass</span>}
        {pr.review === "changes_requested" && <span className="ui-status ui-status-danger">changes requested{who(pr.changes_by)}</span>}
        {pr.review === "approved" && <span className="ui-status ui-status-ok">approved{who(pr.approved_by)}</span>}
        {pr.review === "review_required" && pr.waiting_on.length === 0 && <span className="ui-status ui-status-warn">needs a review</span>}
        {pr.waiting_on.length > 0 && <span className="ui-chip">waiting on {pr.waiting_on.join(", ")}</span>}
        {pr.unresolved > 0 && (
          <span className="ui-status ui-status-warn">{pr.unresolved}{pr.unresolved_more ? "+" : ""} unresolved</span>
        )}
        {pr.conflicts && <span className="ui-status ui-status-danger">conflicts</span>}
        {task && <span className="ui-status ui-status-info">{task.name}</span>}
        {!task && (
          <button
            type="button"
            className="ui-button start"
            disabled={starting}
            title={`A task on ${pr.head}, to work on it here`}
            onClick={(e) => { e.stopPropagation(); void start(); }}
          >
            <BusyLabel busy={starting} idle="Start task" working="Starting…" />
          </button>
        )}
      </div>
      <ErrorMessage>{failure.error}</ErrorMessage>
    </div>
  );
}
