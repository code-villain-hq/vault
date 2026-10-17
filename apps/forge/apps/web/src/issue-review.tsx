// The decision an issue in review is waiting for, at the top of the issue:
// approve it (by merging its pull request, when it has one), or send it back
// saying what needs to change. It shows what approving also changes in the
// specs.
import type { Issue, WorkflowState } from "@forge/types";
import { useMemo, useState } from "react";
import { pullRequestsToMerge, reviewStateOf, useTeamStates, useUsers } from "./data";
import { SpecChangeSummary } from "./issue-spec-change";
import { Button, buttonClass } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/** The team's review status (see `reviewStateOf`). */
export function useReviewState(teamId: string): WorkflowState | undefined {
  const states = useTeamStates(teamId);
  return useMemo(() => reviewStateOf(states), [states]);
}

export function ReviewBar({ issue }: { issue: Issue }) {
  const { sync } = useWorkspace();
  const states = useTeamStates(issue.team_id);
  const done = states.find((s) => s.type === "completed");
  const working = states.find((s) => s.type === "started");
  const assignee = useUsers().find((u) => u.id === issue.assignee_id);
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
  const [sendingBack, setSendingBack] = useState(false);
  const [feedback, setFeedback] = useState("");

  const doneName = done?.name ?? "Done";
  const approve = () => done && sync.update("Issue", issue.id, { state_id: done.id });

  const sendBack = () => {
    if (!feedback.trim() || !working) return;
    sync.create("Comment", { issue_id: issue.id, body: feedback.trim() });
    sync.update("Issue", issue.id, { state_id: working.id });
  };

  return (
    <section aria-label="Review" className="flex flex-col gap-3 rounded-lg bg-surface p-4">
      <div>
        <h2 className="font-medium">Ready for your review</h2>
        <p className="text-xs text-muted">
          {assignee
            ? `${assignee.name} is done and asks you to check it.`
            : "This is done and waits for someone to check it."}{" "}
          {pullRequests.length === 0
            ? `Approve it to move it to ${doneName}, or send it back with what needs to change.`
            : pullRequests.length === 1
              ? `Review and merge its pull request on GitHub; Forge moves it to ${doneName} once it's merged. Or send it back with what needs to change.`
              : `Review and merge its pull requests on GitHub; Forge moves it to ${doneName} once they're merged. Or send it back with what needs to change.`}
        </p>
      </div>
      {notes && assignee && (
        <button
          type="button"
          className="self-start text-xs font-bold text-fg underline decoration-fg/45 underline-offset-4 hover:decoration-fg"
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
          verb={pullRequests.length > 0 ? "Merging" : "Approving"}
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
      ) : pullRequests.length > 0 ? (
        <div className="flex flex-col gap-3">
          <ul className="flex flex-col gap-2">
            {pullRequests.map((pr) => (
              <li key={pr.id} className="flex min-w-0 items-center gap-3">
                <a href={pr.url} target="_blank" rel="noreferrer" className={buttonClass()}>
                  Review and merge {pr.number ? `PR #${pr.number}` : "the pull request"} on GitHub ↗
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
            <button
              type="button"
              disabled={!done}
              onClick={approve}
              className="text-xs text-muted hover:text-fg hover:underline disabled:opacity-50"
            >
              Mark done without merging
            </button>
          </div>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2">
          <Button disabled={!done} onClick={approve}>
            Approve
          </Button>
          <Button variant="secondary" disabled={!working} onClick={() => setSendingBack(true)}>
            Send back…
          </Button>
        </div>
      )}
    </section>
  );
}
