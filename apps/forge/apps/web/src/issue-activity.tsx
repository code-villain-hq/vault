// An issue's activity: what changed, who changed it and when, from the
// server's change log. Needs a connection; comments show without one.
import type { Issue, IssueEvent, User } from "@forge/types";
import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { byId, formatDate, identifier, PRIORITY_LABELS, timeAgo, useStates } from "./data";
import { Avatar, StateIcon } from "./icons";
import { useRecords, useWorkspace } from "./workspace";

/**
 * The issue's history, refreshed whenever the server has a new version of it.
 * Not fetched while `enabled` is false.
 */
export function useIssueHistory(issue: Issue, enabled = true): IssueEvent[] | undefined {
  const { workspace } = useWorkspace();
  const [events, setEvents] = useState<IssueEvent[]>();
  useEffect(() => {
    // Re-run when the server's copy changes, so the new change is in the log.
    void issue.updated_at;
    if (!enabled) return;
    let current = true;
    api<IssueEvent[]>("GET", `/api/w/${workspace.id}/issues/${issue.id}/history`)
      .then((history) => current && setEvents(history))
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [workspace.id, issue.id, issue.updated_at, enabled]);
  return events;
}

export function EventLine({ event, users }: { event: IssueEvent; users: Map<string, User> }) {
  const actor = users.get(event.actor_id);
  return (
    <div className="flex items-center gap-2 py-1 pl-0.5 text-xs text-muted">
      <Avatar user={actor} size="size-4" />
      <p className="min-w-0">
        <span className="font-medium text-fg">{actor?.name ?? "Someone"}</span>{" "}
        <Change event={event} users={users} />
        <span className="text-faint">
          {" · "}
          <time dateTime={event.at} title={new Date(event.at).toLocaleString()}>
            {timeAgo(event.at)}
          </time>
        </span>
      </p>
    </div>
  );
}

function Change({ event, users }: { event: IssueEvent; users: Map<string, User> }) {
  const states = useStates();
  const labels = byId(useRecords("Label"));
  const projects = byId(useRecords("Project"));
  const services = byId(useRecords("Service"));
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const issues = byId(useRecords("Issue"));
  const name = (s: string | undefined) => (
    <span className="text-fg">{s ?? "something deleted"}</span>
  );

  switch (event.field) {
    case "created":
      return <>created the issue</>;
    case "title":
      return <>renamed the issue to {name(`“${event.to}”`)}</>;
    case "description":
      return <>updated the description</>;
    case "state": {
      const to = states.get(event.to);
      const from = states.get(event.from);
      return (
        <>
          moved it from {name(from?.name)} to{" "}
          {to && (
            <span className="inline-flex translate-y-0.5">
              <StateIcon state={to} />
            </span>
          )}{" "}
          {name(to?.name)}
        </>
      );
    }
    case "priority":
      return event.to === 0 ? (
        <>removed the priority</>
      ) : (
        <>set the priority to {name(PRIORITY_LABELS[event.to])}</>
      );
    case "assignee":
      if (!event.to) return <>unassigned {name(users.get(event.from ?? "")?.name ?? "someone")}</>;
      if (event.to === event.actor_id) return <>self-assigned the issue</>;
      return <>assigned it to {name(users.get(event.to)?.name ?? "someone")}</>;
    case "reviewer":
      if (!event.to)
        return <>removed {name(users.get(event.from ?? "")?.name ?? "someone")} as reviewer</>;
      if (event.to === event.actor_id) return <>took the review</>;
      return <>asked {name(users.get(event.to)?.name ?? "someone")} to review it</>;
    case "labels": {
      const list = (ids: string[]) =>
        ids.map((id) => labels.get(id)?.name ?? "a deleted label").join(", ");
      return (
        <>
          {event.added.length > 0 && <>added {name(list(event.added))}</>}
          {event.added.length > 0 && event.removed.length > 0 && " and "}
          {event.removed.length > 0 && <>removed {name(list(event.removed))}</>}
        </>
      );
    }
    case "project":
      return event.to ? (
        <>added it to {name(projects.get(event.to)?.name)}</>
      ) : (
        <>removed it from {name(projects.get(event.from ?? "")?.name)}</>
      );
    case "parent": {
      const parent = issues.get(event.to ?? event.from ?? "");
      const label = parent ? `${identifier(parent, teamsById)} ${parent.title}` : undefined;
      return event.to ? (
        <>made it a sub-issue of {name(label)}</>
      ) : (
        <>removed the parent {name(label)}</>
      );
    }
    case "service":
      return event.to ? (
        <>set the service to {name(services.get(event.to)?.name)}</>
      ) : (
        <>removed the service {name(services.get(event.from ?? "")?.name)}</>
      );
    case "due_date":
      return event.to ? (
        <>set the due date to {name(formatDate(event.to))}</>
      ) : (
        <>removed the due date</>
      );
    case "team":
      return (
        <>
          moved it from {name(teamsById.get(event.from)?.name)} to{" "}
          {name(teamsById.get(event.to)?.name)}
        </>
      );
  }
}
