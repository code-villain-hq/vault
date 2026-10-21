// The decision an issue in review is waiting for, at the top of the issue:
// approve it (by merging its pull request, when it has one), or send it back
// saying what needs to change. Without a pull request, what it asks depends
// on the No code mark and on whether Forge can see the issue's pull
// requests at all. It shows what approving also changes in the specs.
import type { Issue, IssueLink, WorkflowState } from "@forge/types";
import { useMemo, useState } from "react";
import {
  byId,
  type CodeHosts,
  identifier,
  pullRequestsKnown,
  pullRequestsToMerge,
  reviewAsk,
  reviewStateOf,
  useNow,
  useTeamStates,
  useUsers,
} from "./data";
import { useIssueHistory } from "./issue-activity";
import { ReviewerPicker } from "./issue-fields";
import { SpecChangeSummary } from "./issue-spec-change";
import { useForgejoStatus } from "./settings-forgejo";
import { useGitHubStatus } from "./settings-github";
import { Button, buttonClass } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/**
 * What Forge knows about the workspace's code hosts (null until it has
 * asked), and the time to judge it by. Forgejo's is asked again every
 * minute, so its last check stays current.
 */
export function useCodeHosts(): { hosts: CodeHosts | null; now: number } {
  const github = useGitHubStatus();
  const forgejo = useForgejoStatus(60_000);
  const now = useNow();
  const hosts = useMemo(() => {
    const githubKnown = github.status !== null || github.error !== null;
    const forgejoKnown = forgejo.status !== null || forgejo.error !== null;
    if (!githubKnown || !forgejoKnown) return null;
    return {
      github: Boolean(github.status?.configured && github.status.installations.length > 0),
      forgejo: forgejo.status,
    };
  }, [github.status, github.error, forgejo.status, forgejo.error]);
  return { hosts, now };
}

/** Where a pull request lives, by name. */
function hostOf(link: IssueLink): string {
  return link.provider === "forgejo" ? "Forgejo" : "GitHub";
}

/** The team's review status (see `reviewStateOf`). */
export function useReviewState(teamId: string): WorkflowState | undefined {
  const states = useTeamStates(teamId);
  return useMemo(() => reviewStateOf(states), [states]);
}

export function ReviewBar({ issue }: { issue: Issue }) {
  const { sync, me } = useWorkspace();
  const states = useTeamStates(issue.team_id);
  const done = states.find((s) => s.type === "completed");
  const working = states.find((s) => s.type === "started");
  const users = useUsers();
  const assignee = users.find((u) => u.id === issue.assignee_id);
  const reviewer = users.find((u) => u.id === issue.reviewer_id);
  const mine = reviewer?.id === me.user.id;
  // The handoff: the latest comment from whoever did the work.
  const notes = useRecords("Comment")
    .filter((c) => c.issue_id === issue.id && c.author_id === issue.assignee_id)
    .sort((a, b) => a.created_at.localeCompare(b.created_at))
    .at(-1);
  // With a pull request, approving means merging it on GitHub, which moves
  // the issue to Done; approving here alone would leave the code unmerged.
  // Pull requests that only mention the issue wouldn't move it, so they're
  // left to "Pull requests and commits".
  const allLinks = useRecords("IssueLink");
  const pullRequests = useMemo(() => pullRequestsToMerge(allLinks, issue.id), [allLinks, issue.id]);
  const { hosts, now } = useCodeHosts();
  const known = hosts ? pullRequestsKnown(hosts, now) : null;
  // Only telling "no pull request" from "can't tell" waits for the code hosts.
  const ask =
    pullRequests.length > 0 || issue.no_code || known
      ? reviewAsk(pullRequests.length, issue.no_code, known?.known ?? true)
      : null;
  // Who said the work needs no code: the latest to set the mark.
  const history = useIssueHistory(issue, issue.no_code);
  const markedBy = users.find(
    (u) => u.id === history?.findLast((e) => e.field === "no_code" && e.to)?.actor_id,
  );
  const teams = useRecords("Team");
  const issueKey = identifier(issue, byId(teams));
  const [sendingBack, setSendingBack] = useState(false);
  const [feedback, setFeedback] = useState("");

  const doneName = done?.name ?? "Done";
  const hostNames = new Set(pullRequests.map(hostOf));
  const host = hostNames.size === 1 ? ` on ${[...hostNames][0]}` : "";
  const allForgejo = pullRequests.every((pr) => pr.provider === "forgejo");
  const approve = () => done && sync.update("Issue", issue.id, { state_id: done.id });

  const sendBack = () => {
    if (!feedback.trim() || !working) return;
    sync.create("Comment", { issue_id: issue.id, body: feedback.trim() });
    sync.update("Issue", issue.id, { state_id: working.id });
  };

  return (
    <section aria-label="Review" className="flex flex-col gap-3 rounded-lg bg-surface p-4">
      <div>
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="font-medium">
            {mine
              ? "Ready for your review"
              : reviewer
                ? `Waiting for ${reviewer.name}'s review`
                : "No reviewer yet: choose one"}
          </h2>
          {!reviewer && <ReviewerPicker issue={issue} variant="chip" />}
        </div>
        <p className="text-xs text-muted">
          {assignee
            ? `${assignee.name} is done and asks ${mine ? "you" : (reviewer?.name ?? "someone")} to check it.`
            : "This is done and waits for someone to check it."}{" "}
          {ask === "merge" &&
            (pullRequests.length === 1
              ? `Review and merge its pull request${host}; Forge moves it to ${doneName} once it's merged. Or send it back with what needs to change.`
              : allForgejo
                ? `Review and merge its pull requests on Forgejo; Forge moves it to ${doneName} once the open ones are merged or closed, with at least one merged. Or send it back with what needs to change.`
                : `Review and merge its pull requests${host}; Forge moves it to ${doneName} once they're merged. Or send it back with what needs to change.`)}
          {ask === "approve" &&
            `${markedBy?.name ?? "Someone"} marked it as needing no code. Approve it to move it to ${doneName}, or send it back with what needs to change.`}
          {ask === "approve" && known && !known.known && " Forge couldn't look for pull requests."}
          {ask === "no_pull_request" &&
            `No pull request was found. Code work needs one that says "Fixes ${issueKey}": send it back, or mark it done without merging if it needed none.`}
          {ask === "unknown" &&
            known &&
            !known.known &&
            `Forge can't check this issue's pull requests right now. ${known.why} Mark it done anyway, or send it back.`}
        </p>
      </div>
      {notes && assignee && (
        <button
          type="button"
          className="ui-link self-start text-xs"
          onClick={() =>
            document
              .getElementById(`comment-${notes.id}`)
              ?.scrollIntoView({ behavior: "smooth", block: "start" })
          }
        >
          Read {assignee.name}'s notes on what changed ↓
        </button>
      )}
      <div className="pt-3">
        <SpecChangeSummary
          issue={issue}
          verb={ask === "merge" ? "Merging" : ask === "approve" ? "Approving" : "Marking it done"}
          author={assignee?.name}
        />
      </div>
      {sendingBack ? (
        <div className="flex flex-col gap-2">
          <textarea
            aria-label="What needs to change"
            // biome-ignore lint/a11y/noAutofocus: opened by pressing Send back
            autoFocus
            placeholder="What needs to change? (⌘↵ to send back)"
            value={feedback}
            onChange={(e) => setFeedback(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                sendBack();
              }
              if (e.key === "Escape") {
                // Done typing; Escape shouldn't also leave the issue.
                e.stopPropagation();
                setSendingBack(false);
              }
            }}
            className="field-sizing-content min-h-16 resize-none rounded-md ui-field px-3 py-2 outline-none placeholder:text-faint"
          />
          <div className="flex gap-2">
            <Button disabled={!feedback.trim() || !working} onClick={sendBack}>
              Send back to {working?.name ?? "In Progress"}
            </Button>
            <Button variant="secondary" onClick={() => setSendingBack(false)}>
              Cancel
            </Button>
          </div>
        </div>
      ) : ask === "merge" ? (
        <div className="flex flex-col gap-3">
          <ul className="flex flex-col gap-2">
            {pullRequests.map((pr, i) => (
              <li key={pr.id} className="flex min-w-0 items-center gap-3">
                {/* One primary action: the first pull request, a button though it leaves for
                    GitHub or Forgejo. Any others are plain links. */}
                <a
                  href={pr.url}
                  target="_blank"
                  rel="noreferrer"
                  className={i === 0 ? buttonClass("primary") : "ui-link ui-control"}
                >
                  Review and merge {pr.number ? `PR #${pr.number}` : "the pull request"} on{" "}
                  {hostOf(pr)} ↗
                </a>
                {pullRequests.length > 1 && (
                  <span className="min-w-0 truncate text-xs text-muted">{pr.title}</span>
                )}
              </li>
            ))}
          </ul>
          <div className="flex flex-wrap items-center gap-3">
            <Button variant="secondary" disabled={!working} onClick={() => setSendingBack(true)}>
              Send back…
            </Button>
            <Button variant="secondary" disabled={!done} onClick={approve}>
              Mark done without merging
            </Button>
          </div>
        </div>
      ) : ask === "approve" ? (
        <div className="flex flex-wrap gap-2">
          <Button disabled={!done} onClick={approve}>
            Approve
          </Button>
          <Button variant="secondary" disabled={!working} onClick={() => setSendingBack(true)}>
            Send back…
          </Button>
        </div>
      ) : ask === "no_pull_request" ? (
        <div className="flex flex-wrap gap-2">
          <Button disabled={!working} onClick={() => setSendingBack(true)}>
            Send back…
          </Button>
          <Button variant="secondary" disabled={!done} onClick={approve}>
            Mark done without merging
          </Button>
        </div>
      ) : ask === "unknown" ? (
        // Forge can't tell whether code is waiting, so no choice is the obvious one.
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" disabled={!done} onClick={approve}>
            Mark done anyway
          </Button>
          <Button variant="secondary" disabled={!working} onClick={() => setSendingBack(true)}>
            Send back…
          </Button>
        </div>
      ) : null}
    </section>
  );
}
