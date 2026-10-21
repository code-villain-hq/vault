// What agents are working on (FOR-24): on the issue, on the assignee's
// avatar in lists and on the board, and under Settings → Agents.
import type { Issue, User } from "@forge/types";
import { Link } from "@tanstack/react-router";
import { useMemo } from "react";
import { type AgentWork, agentWork, byId, duration, identifier, timeAgo, useNow } from "./data";
import { useRecords, useWorkspace } from "./workspace";

function workText(name: string, work: AgentWork, now: number) {
  return work.stopped
    ? `${name} stopped working on this: nothing heard from it since ${timeAgo(work.seenAt, now)}`
    : `${name} is working on this · ${duration(work.since, now)}`;
}

/** A dot: pulsing while the agent works, amber once its run stopped. */
function WorkDot({ stopped, className = "" }: { stopped: boolean; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={`size-2 shrink-0 rounded-full ${stopped ? "bg-warn" : "animate-pulse bg-signal"} ${className}`}
    />
  );
}

/** The agents working on an issue, with how long; or that their run stopped. */
export function AgentWorkLine({ issue }: { issue: Issue }) {
  const users = useRecords("User");
  const now = useNow();
  const working = users
    .map((user) => ({ user, work: agentWork(user, now) }))
    .filter(({ work }) => work?.issueId === issue.id);
  if (working.length === 0) return null;
  return (
    <div className="flex flex-col gap-1">
      {working.map(({ user, work }) =>
        work ? (
          <p
            key={user.id}
            className={`flex items-center gap-2 text-xs ${work.stopped ? "text-warn-ink" : "text-muted"}`}
          >
            <WorkDot stopped={work.stopped} />
            {workText(user.name, work, now)}
          </p>
        ) : null,
      )}
    </div>
  );
}

/**
 * A dot on the assignee's avatar while that agent works on the issue. The
 * avatar itself goes inside.
 */
export function AvatarWorkDot({ issue, assignee }: { issue: Issue; assignee: User | undefined }) {
  const now = useNow();
  const work = agentWork(assignee, now);
  if (!assignee || work?.issueId !== issue.id) return null;
  return (
    <span className="absolute -right-0.5 -bottom-0.5 flex rounded-full ring-2 ring-panel">
      <span className="sr-only">{workText(assignee.name, work, now)}</span>
      <WorkDot stopped={work.stopped} className="size-1.5" />
    </span>
  );
}

/** Under Settings → Agents: what the agent is working on, or that it's free. */
export function AgentWorkStatus({ agentId }: { agentId: string }) {
  const { workspace } = useWorkspace();
  const users = useRecords("User");
  const issues = useRecords("Issue");
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const now = useNow();
  const work = agentWork(
    users.find((u) => u.id === agentId),
    now,
  );
  const issue = work && issues.find((i) => i.id === work.issueId);
  if (!work || !issue) return <p className="ml-9 text-xs text-muted">Free</p>;
  const id = identifier(issue, teamsById);
  const link = (
    <Link
      to="/$workspace/issue/$identifier"
      params={{ workspace: workspace.url_key, identifier: id }}
      className="ui-link"
    >
      {id} {issue.title}
    </Link>
  );
  return (
    <p className="ml-9 flex min-w-0 items-center gap-2 text-xs text-muted">
      <WorkDot stopped={work.stopped} />
      {work.stopped ? (
        <span className="truncate">
          Free · stopped working on {link} {timeAgo(work.seenAt, now)}
        </span>
      ) : (
        <span className="truncate">
          Working on {link} · {duration(work.since, now)}
        </span>
      )}
    </p>
  );
}
