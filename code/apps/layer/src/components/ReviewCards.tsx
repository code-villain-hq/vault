import { openUrl } from "@tauri-apps/plugin-opener";
import { useRef, useState, type MouseEvent, type PointerEvent } from "react";
import { api } from "../lib/api";
import { githubPrKey, githubPrRepo, githubRepoLabel } from "../lib/github-source";
import { goTo } from "../lib/goto";
import { menuKeys, pressKeys } from "../lib/press";
import type { ReviewPrReaders } from "../lib/review-pr-readers";
import { age, ago } from "../lib/time";
import type { AuthoredPr, ReviewRequest, Reviewer } from "../lib/types";
import { useStore } from "../store";
import { ReviewPrDetails } from "./ReviewPrDetails";
import { BusyLabel, ErrorMessage, useFailure } from "./ui";

const NO_BROWSER = "Couldn't open the browser. Try again.";

function useCardActivation(open: () => void) {
  const fromReader = useRef(false);
  return {
    onPointerDownCapture: (e: PointerEvent) => {
      if (e.isPrimary && e.button === 0) {
        fromReader.current =
          (e.target as Element).closest(".review-pr-details, .review-toggles") !== null;
      }
    },
    // A selection drag ending on padding sends the click to the card itself.
    // The next pointer press resets the guard; keyboard clicks bypass it.
    onClick: (e: MouseEvent) => {
      const reading = e.detail > 0 && fromReader.current;
      fromReader.current = false;
      if (!reading && !(e.target as Element).closest(".error-message")) open();
    },
    onKeyDown: menuKeys(pressKeys(open)),
  };
}

export const identity = githubPrKey;

/** A wait this long is called out: the author has been blocked for days. */
const LONG_WAIT_MS = 2 * 86_400_000;

function waitedLong(at: string): boolean {
  const t = Date.parse(at);
  return !Number.isNaN(t) && Date.now() - t >= LONG_WAIT_MS;
}

/** The repository's own name; its owner and host are on hover, and in its group. */
function RepoNumber({ pr }: { pr: ReviewRequest | AuthoredPr }) {
  return (
    <span className="repo" title={githubRepoLabel(pr)}>
      {pr.repo.split("/").pop()} #{pr.number}
    </span>
  );
}

function GitHubLink({ url, open }: { url: string; open: () => void }) {
  return (
    <button
      type="button"
      className="ui-link"
      title={url}
      onClick={(e) => {
        e.stopPropagation();
        open();
      }}
    >
      GitHub ↗
    </button>
  );
}

/**
 * A pull request waiting on your review, and what it asks of you (REV-9).
 * A click opens it on GitHub, where it is reviewed.
 */
export function ReviewCard({
  pr,
  focused,
  asked,
  readers,
  onContextMenu,
}: {
  pr: ReviewRequest;
  focused: boolean;
  /** Who it was asked of, once the list is not grouped by that (REV-11). */
  asked?: string;
  readers: ReviewPrReaders;
  onContextMenu: (e: MouseEvent) => void;
}) {
  // A card has no room for the browser failing to open: the top bar says it.
  const fail = useStore((s) => s.fail);
  const open = () => void openUrl(pr.url).catch(() => fail(NO_BROWSER));
  const activation = useCardActivation(open);
  const when = pr.asked_at || pr.updated_at;

  return (
    // A div, not a button: it holds buttons of its own.
    <div
      role="button"
      tabIndex={0}
      className={`review-card${focused ? " focused" : ""}`}
      data-review={identity(pr)}
      {...activation}
      onContextMenu={onContextMenu}
      title={pr.url}
    >
      <div className="head">
        <span className="title">{pr.title}</span>
        <span
          className={`when${pr.asked_at && waitedLong(pr.asked_at) ? " long" : ""}`}
          title={`${pr.asked_at ? "Asked" : "Updated"} ${ago(when)}; opened ${ago(pr.created_at)}`}
        >
          {pr.asked_at ? "asked " : ""}
          {age(when)}
        </span>
      </div>
      <div className="meta">
        <RepoNumber pr={pr} />
        {pr.author && <span>{pr.author}</span>}
        <span>
          <span className="add">+{pr.additions}</span> <span className="del">−{pr.deletions}</span>
          {" · "}
          {pr.changed_files} {pr.changed_files === 1 ? "file" : "files"}
        </span>
        {pr.cross_repo && <span title="Its branch is on a fork">fork</span>}
        {pr.draft && <span>draft</span>}
        {asked && <span>for {asked}</span>}
        <span className="spacer" />
        <GitHubLink url={pr.url} open={open} />
      </div>
      <ReviewPrDetails pr={pr} readers={readers} checks={pr.checks}>
        <MyReview pr={pr} />
      </ReviewPrDetails>
    </div>
  );
}

/**
 * Where your own review of it stands (REV-9). Not reviewed yet says
 * nothing: that is what its group, Review, already says.
 */
function MyReview({ pr }: { pr: ReviewRequest }) {
  if (pr.new_since_review) {
    return (
      <span className="said warn" title="Its author pushed after your last review">
        New commits since your review
      </span>
    );
  }
  switch (pr.my_review) {
    case "approved":
      return <span className="said ok">You approved</span>;
    case "changes_requested":
      return <span className="said">You asked for changes</span>;
    case "commented":
      return <span className="said">You commented</span>;
    case "dismissed":
      return <span className="said">Your review was dismissed</span>;
    default:
      return null;
  }
}

const MARK: Record<Reviewer["state"], { mark: string; tone: string; says: string }> = {
  waiting: { mark: "◷", tone: "warn", says: "asked" },
  approved: { mark: "✓", tone: "ok", says: "approved" },
  changes_requested: { mark: "✕", tone: "danger", says: "asked for changes" },
};

/**
 * Who it waits on and who has answered (REV-4): a wait of days stands out,
 * and an answer says what it was.
 */
function Reviewers({ pr }: { pr: AuthoredPr }) {
  return (
    <>
      {pr.reviewers.map((r) => {
        const { mark, tone, says } = MARK[r.state];
        const long = r.state === "waiting" && waitedLong(r.at);
        return (
          <span
            key={r.who}
            className="reviewer"
            title={`${r.who} ${says}${r.at ? ` ${ago(r.at)}` : ""}`}
          >
            <span className={`mark ${tone}`}>{mark}</span>
            <span className="who">{r.who}</span>
            {r.state === "waiting" && r.at && (
              <span className={long ? "said warn" : "said"}>{age(r.at)}</span>
            )}
            {r.state === "changes_requested" && <span className="said">asked for changes</span>}
          </span>
        );
      })}
    </>
  );
}

/**
 * One of your pull requests and where it stands (REV-4), and the way to act
 * on it (REV-7): a click opens its task's Pull requests tab, where feedback
 * goes to an agent. One with no task opens GitHub and offers Start task.
 * Where it stands is the heading of its group (REV-8).
 */
export function AuthoredCard({
  pr,
  task,
  readers,
  onContextMenu,
}: {
  pr: AuthoredPr;
  task: { id: string; name: string } | null;
  readers: ReviewPrReaders;
  onContextMenu: (e: MouseEvent) => void;
}) {
  // Opening GitHub fails in the top bar, like the review card; Start task
  // under the card, by its link (NOTE-6).
  const fail = useStore((s) => s.fail);
  const failure = useFailure();
  const refreshTasks = useStore((s) => s.refreshTasks);
  const refreshPrs = useStore((s) => s.refreshPrs);
  const [starting, setStarting] = useState(false);
  const openGitHub = () => void openUrl(pr.url).catch(() => fail(NO_BROWSER));
  const open = () => (task ? goTo(`pr:${task.id}`) : openGitHub());
  const activation = useCardActivation(open);

  async function start() {
    setStarting(true);
    failure.clear();
    try {
      const made = await api.taskForPr(githubPrRepo(pr), pr.head, pr.base, pr.title, pr.url);
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

  const unresolved =
    pr.unresolved > 0 ? `${pr.unresolved}${pr.unresolved_more ? "+" : ""}` : undefined;
  const asksNobody =
    pr.review === "review_required" && !pr.reviewers.some((r) => r.state === "waiting");

  return (
    // A div, not a button: it holds buttons of its own.
    <div
      role="button"
      tabIndex={0}
      className="review-card"
      data-review={identity(pr)}
      {...activation}
      onContextMenu={onContextMenu}
      title={task ? `Open ${task.name}` : pr.url}
    >
      <div className="head">
        <span className="title">{pr.title}</span>
        <span
          className="when"
          title={`Updated ${ago(pr.updated_at)}; opened ${ago(pr.created_at)}`}
        >
          {age(pr.updated_at)}
        </span>
      </div>
      <div className="meta">
        <RepoNumber pr={pr} />
        {task && (
          <span className="task" title={`Task: ${task.name}`}>
            {task.name}
          </span>
        )}
        <span className="spacer" />
        {!task && (
          <button
            type="button"
            className="ui-link"
            disabled={starting}
            title={`A task on ${pr.head}, to work on it here`}
            onClick={(e) => {
              e.stopPropagation();
              void start();
            }}
          >
            <BusyLabel busy={starting} idle="Start task" working="Starting…" />
          </button>
        )}
        <GitHubLink url={pr.url} open={openGitHub} />
      </div>
      <ErrorMessage>{failure.error}</ErrorMessage>
      <ReviewPrDetails pr={pr} readers={readers} checks={pr.checks} unresolved={unresolved}>
        {pr.conflicts && <span className="said danger">Conflicts</span>}
        {asksNobody && <span className="said warn">Needs a reviewer</span>}
        <Reviewers pr={pr} />
      </ReviewPrDetails>
    </div>
  );
}
