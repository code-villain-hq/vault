import type { Notification } from "@forge/types";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useRef } from "react";
import { byId, identifier, timeAgo, useUsers } from "./data";
import { Avatar } from "./icons";
import { PageHeader } from "./layout";
import { notify, setBadge } from "./platform";
import { useRecords, useSyncStatus, useWorkspace } from "./workspace";

const VERBS: Record<Notification["type"], string> = {
  assigned: "assigned you",
  comment: "commented on",
  completed: "completed",
  suggestion: "suggests a change to",
  mention: "mentioned you in",
  spec_conflict: "wants to change a requirement your issue changes, in",
};

/** Newest first. */
export function useNotifications() {
  const notifications = useRecords("Notification");
  return useMemo(
    () => [...notifications].sort((a, b) => b.created_at.localeCompare(a.created_at)),
    [notifications],
  );
}

/** Notifications about the same issue (or suggestion) share one row in the Inbox. */
function rowKey(n: Notification) {
  return n.issue_id ?? n.suggestion_id ?? n.id;
}

/** How many Inbox rows have something unread. */
function unreadRows(notifications: Notification[]) {
  return new Set(notifications.filter((n) => !n.read_at).map(rowKey)).size;
}

export function useUnreadCount() {
  return unreadRows(useNotifications());
}

/** One line describing a notification, e.g. "Ada assigned you FOR-12 Fix login". */
function useDescribe() {
  const users = useUsers();
  const issues = useRecords("Issue");
  const teams = useRecords("Team");
  const suggestions = useRecords("Suggestion");
  return useMemo(() => {
    const issuesById = byId(issues);
    const teamsById = byId(teams);
    const suggestionsById = byId(suggestions);
    return (n: Notification) => {
      const actor = users.find((u) => u.id === n.actor_id);
      const issue = n.issue_id ? issuesById.get(n.issue_id) : undefined;
      const suggestion = n.suggestion_id ? suggestionsById.get(n.suggestion_id) : undefined;
      const issueLabel = issue
        ? `${identifier(issue, teamsById)} ${issue.title}`
        : "a deleted issue";
      return {
        actor,
        issue,
        /** Drift suggestions open in Triage, where they're accepted or dismissed. */
        triage: suggestion?.kind === "drift",
        // A suggestion about an issue says which issue; one without, its title.
        issueLabel: suggestion && !issue ? suggestion.title : issueLabel,
        text: suggestion
          ? `${actor?.name ?? "Forge AI"} suggests:`
          : `${actor?.name ?? "Someone"} ${VERBS[n.type]}`,
        identifier: issue ? identifier(issue, teamsById) : undefined,
      };
    };
  }, [users, issues, teams, suggestions]);
}

/** What happened, said under the issue's title in the Inbox. */
const ROW_VERBS: Record<Notification["type"], string> = {
  assigned: "assigned you",
  comment: "commented",
  completed: "completed it",
  suggestion: "suggests a change",
  mention: "mentioned you",
  spec_conflict: "wants to change a requirement your issue changes",
};

/**
 * The Inbox shows one row per issue (or per suggestion without one): what
 * happened last and how much more, so five comments on one issue don't
 * fill the page.
 */
export function InboxPage() {
  const { sync, workspace } = useWorkspace();
  const navigate = useNavigate();
  const notifications = useNotifications();
  const describe = useDescribe();
  const unread = notifications.filter((n) => !n.read_at);
  // Newest first already, so each group's first is its latest.
  const groups = useMemo(() => {
    const map = new Map<string, Notification[]>();
    for (const n of notifications) {
      map.set(rowKey(n), [...(map.get(rowKey(n)) ?? []), n]);
    }
    return [...map.entries()];
  }, [notifications]);

  const markRead = (group: Notification[]) => {
    const now = new Date().toISOString();
    const changes = group
      .filter((n) => !n.read_at)
      .map((n) => ({
        action: "update" as const,
        model: "Notification" as const,
        id: n.id,
        data: { read_at: now },
      }));
    if (changes.length > 0) sync.transact(changes);
  };

  return (
    <>
      <PageHeader title="Inbox">
        {unread.length > 0 && (
          <button
            type="button"
            className="h-7 rounded-md px-2 text-xs text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent"
            onClick={() => markRead(unread)}
          >
            Mark all read
          </button>
        )}
      </PageHeader>
      <main className="min-h-0 flex-1 overflow-y-auto">
        {notifications.length === 0 && (
          <p className="px-4 py-20 text-center text-muted">
            You're all caught up. Assignments, comments and completed issues show up here.
          </p>
        )}
        <ul className="flex flex-col gap-0.5 px-2 py-2">
          {groups.map(([key, group]) => {
            const latest = group[0];
            if (!latest) return null;
            const d = describe(latest);
            const isUnread = group.some((n) => !n.read_at);
            const more = group.length - 1;
            const actor = latest.suggestion_id
              ? (d.actor?.name ?? "Forge AI")
              : (d.actor?.name ?? "Someone");
            return (
              <li
                key={key}
                className="group flex items-center gap-3 rounded-lg px-3 py-2 hover:bg-hover"
              >
                <span
                  className={`size-1.5 shrink-0 rounded-full ${isUnread ? "bg-accent" : ""}`}
                  aria-hidden="true"
                />
                {isUnread && <span className="sr-only">Unread:</span>}
                <Avatar user={d.actor} size="size-6" />
                <button
                  type="button"
                  className="flex min-w-0 flex-1 flex-col text-left outline-none focus-visible:underline"
                  onClick={() => {
                    markRead(group);
                    if (d.triage) {
                      void navigate({
                        to: "/$workspace/triage",
                        params: { workspace: workspace.url_key },
                      });
                    } else if (d.identifier) {
                      void navigate({
                        to: "/$workspace/issue/$identifier",
                        params: { workspace: workspace.url_key, identifier: d.identifier },
                      });
                    }
                  }}
                >
                  <span className={`truncate ${isUnread ? "font-medium" : "text-muted"}`}>
                    {d.issue && d.identifier ? (
                      <>
                        <span className="mr-2 font-mono text-xs font-normal text-faint">
                          {d.identifier}
                        </span>
                        {d.issue.title}
                      </>
                    ) : (
                      d.issueLabel
                    )}
                  </span>
                  <span className="truncate text-xs text-faint">
                    {actor} {ROW_VERBS[latest.type]}
                    {more > 0 && ` · ${more} more`}
                  </span>
                </button>
                <time
                  className="shrink-0 text-xs text-faint group-hover:hidden"
                  dateTime={latest.created_at}
                  title={new Date(latest.created_at).toLocaleString(undefined, {
                    month: "short",
                    day: "numeric",
                    hour: "numeric",
                    minute: "2-digit",
                  })}
                >
                  {timeAgo(latest.created_at)}
                </time>
                <button
                  type="button"
                  className="hidden h-6 rounded px-1.5 text-xs text-faint outline-none group-hover:block hover:bg-panel hover:text-fg focus-visible:block focus-visible:ring-2 focus-visible:ring-accent"
                  onClick={() =>
                    sync.transact(
                      group.map((n) => ({
                        action: "delete" as const,
                        model: "Notification" as const,
                        id: n.id,
                      })),
                    )
                  }
                >
                  Archive
                </button>
              </li>
            );
          })}
        </ul>
      </main>
    </>
  );
}

/**
 * On desktop: a native notification for each new one, and the unread count
 * on the dock icon. Notifications already there when the app opened don't pop up.
 */
export function useDesktopNotifications() {
  const notifications = useNotifications();
  const describe = useDescribe();
  const { sync } = useWorkspace();
  // Per workspace (each has its own sync client), so switching to another
  // doesn't pop up everything already waiting there.
  const seen = useRef<{ sync: unknown; ids: Set<string> } | null>(null);
  const { ready } = useSyncStatus();

  useEffect(() => {
    void setBadge(unreadRows(notifications));
    if (!ready) return;
    if (seen.current?.sync !== sync) {
      seen.current = { sync, ids: new Set(notifications.map((n) => n.id)) };
      return;
    }
    for (const n of notifications) {
      if (seen.current.ids.has(n.id)) continue;
      seen.current.ids.add(n.id);
      if (!n.read_at) {
        const d = describe(n);
        void notify(d.text, d.issueLabel);
      }
    }
  }, [notifications, describe, ready, sync]);

  // Signed out (or the session ended): no count left on the dock.
  useEffect(() => () => void setBadge(0), []);
}
