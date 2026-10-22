// Derived views of the synced records: lookups, identifiers, ordering.
import { ago } from "@code-villain/web";
import type {
  Decision,
  Issue,
  IssueEvent,
  IssueLink,
  Spec,
  StateType,
  Suggestion,
  Team,
  User,
  WorkflowState,
} from "@forge/types";
import { useEffect, useMemo, useState } from "react";
import { useRecords, useWorkspace } from "./workspace";

export function byId<T extends { id: string }>(records: T[]): Map<string, T> {
  return new Map(records.map((record) => [record.id, record]));
}

export function useTeams(): Team[] {
  const teams = useRecords("Team");
  return useMemo(() => [...teams].sort((a, b) => a.name.localeCompare(b.name)), [teams]);
}

/** The teams you're in, by name: the sidebar's. A guest sees no others. */
export function useMyTeams(): Team[] {
  const { me } = useWorkspace();
  const teams = useTeams();
  const memberships = useRecords("TeamMember");
  return useMemo(() => {
    const mine = new Set(memberships.filter((m) => m.user_id === me.user.id).map((m) => m.team_id));
    return teams.filter((team) => mine.has(team.id));
  }, [teams, memberships, me.user.id]);
}

/** Every workflow state, by id. */
export function useStates(): Map<string, WorkflowState> {
  const states = useRecords("WorkflowState");
  return useMemo(() => byId(states), [states]);
}

/** A team's workflow, in order. */
export function useTeamStates(teamId: string | undefined): WorkflowState[] {
  const states = useRecords("WorkflowState");
  return useMemo(
    () => states.filter((s) => s.team_id === teamId).sort((a, b) => a.position - b.position),
    [states, teamId],
  );
}

/**
 * A team's review status, from its workflow. Teams have no "review" status
 * type, so it's the last of the started ones when there are several (In
 * Progress, then In Review). The server's `review_state` agrees.
 */
export function reviewStateOf(teamStates: WorkflowState[]): WorkflowState | undefined {
  const started = teamStates
    .filter((s) => s.type === "started")
    .sort((a, b) => a.position - b.position);
  return started.length > 1 ? started[started.length - 1] : undefined;
}

/** Every team's review status, by id. */
export function reviewStateIds(states: WorkflowState[]): Set<string> {
  const byTeam = new Map<string, WorkflowState[]>();
  for (const state of states)
    byTeam.set(state.team_id, [...(byTeam.get(state.team_id) ?? []), state]);
  return new Set([...byTeam.values()].flatMap((team) => reviewStateOf(team)?.id ?? []));
}

export function useReviewStateIds(): Set<string> {
  const states = useRecords("WorkflowState");
  return useMemo(() => reviewStateIds(states), [states]);
}

/**
 * What every issue in a list has in common, which tells none of them apart:
 * the project or service when all of them have the same one (`null` when
 * none of them has one), and the labels all of them carry. Only for lists
 * of two or more.
 */
export function sharedFields(issues: Issue[]): {
  project?: string | null;
  service?: string | null;
  labels: string[];
} {
  const [first, ...rest] = issues;
  if (!first || rest.length === 0) return { labels: [] };
  const same = (field: "project_id" | "service_id") =>
    rest.every((i) => i[field] === first[field]) ? first[field] : undefined;
  return {
    project: same("project_id"),
    service: same("service_id"),
    labels: first.label_ids.filter((label) => rest.every((i) => i.label_ids.includes(label))),
  };
}

export function useUsers() {
  const users = useRecords("User");
  return useMemo(() => [...users].sort((a, b) => a.name.localeCompare(b.name)), [users]);
}

export function useLabels() {
  const labels = useRecords("Label");
  return useMemo(() => [...labels].sort((a, b) => a.name.localeCompare(b.name)), [labels]);
}

/** `FOR-12`; `FOR-…` until the server has given the issue a number. */
export function identifier(issue: Issue, teams: Map<string, Team>): string {
  const key = teams.get(issue.team_id)?.key ?? "?";
  return `${key}-${issue.number || "…"}`;
}

/** Finds an issue by its identifier, e.g. from a URL. */
export function useIssueByIdentifier(value: string): Issue | undefined {
  const issues = useRecords("Issue");
  const teams = useRecords("Team");
  return useMemo(() => {
    const match = /^([A-Za-z][A-Za-z0-9]*)-(\d+)$/.exec(value);
    if (!match) return issues.find((issue) => issue.id === value);
    const team = teams.find((t) => t.key === match[1]?.toUpperCase());
    return issues.find((issue) => issue.team_id === team?.id && issue.number === Number(match[2]));
  }, [issues, teams, value]);
}

/** The order groups of issues appear in: work in motion first. */
export const STATE_TYPE_ORDER: StateType[] = [
  "triage",
  "started",
  "unstarted",
  "backlog",
  "completed",
  "canceled",
];

/** Urgent first, "no priority" last, then newest. */
export function byPriority(a: Issue, b: Issue) {
  const rank = (p: number) => (p === 0 ? 5 : p);
  return rank(a.priority) - rank(b.priority) || b.number - a.number || b.id.localeCompare(a.id);
}

export const PRIORITY_LABELS = ["No priority", "Urgent", "High", "Medium", "Low"];

export function formatDate(date: string | null) {
  if (!date) return "";
  return new Date(`${date.slice(0, 10)}T00:00:00`).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

/** "just now", "5 minutes ago", "yesterday", then a date: the kit's words. */
export const timeAgo = ago;

/** "12 min", "2 h 5 min": how long something has been going. */
export function duration(iso: string, now = Date.now()) {
  const minutes = Math.max(0, Math.floor((now - new Date(iso).getTime()) / 60_000));
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return minutes % 60 ? `${hours} h ${minutes % 60} min` : `${hours} h`;
}

/**
 * An hour without a sign of life from an agent means its run stopped. The
 * server's `WORK_LEASE_MINUTES`.
 */
export const WORK_LEASE_MINUTES = 60;

export type AgentWork = {
  issueId: string;
  since: string;
  /** Its latest sign of life. */
  seenAt: string;
  /** No sign of life for an hour: the run died. */
  stopped: boolean;
};

/** What an agent is working on (FOR-24), if anything. */
export function agentWork(user: User | undefined, now = Date.now()): AgentWork | undefined {
  if (!user?.working_on_id || !user.working_since || !user.working_seen_at) return undefined;
  const silent = now - new Date(user.working_seen_at).getTime();
  return {
    issueId: user.working_on_id,
    since: user.working_since,
    seenAt: user.working_seen_at,
    stopped: silent > WORK_LEASE_MINUTES * 60_000,
  };
}

/** The current time, ticking every minute, for durations on screen. */
export function useNow(): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

/**
 * A git branch for an issue, e.g. `for-12-fix-login`. The GitHub connection
 * links pull requests from it to the issue.
 */
export function branchName(issue: Issue, teams: Map<string, Team>): string {
  const slug = issue.title
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .split("-")
    .slice(0, 6)
    .join("-");
  const id = identifier(issue, teams).toLowerCase();
  return slug ? `${id}-${slug}` : id;
}

/**
 * The open pull requests that work on the issue (named in their branch, or
 * after a closing word like "Fixes FOR-12"), oldest first: merging one of
 * them completes it. Those that only mention it don't count.
 */
export function pullRequestsToMerge(links: IssueLink[], issueId: string): IssueLink[] {
  return links
    .filter(
      (l) =>
        l.issue_id === issueId && l.kind === "pull_request" && l.state === "open" && l.works_on,
    )
    .sort((a, b) => a.created_at.localeCompare(b.created_at));
}

/**
 * What keeps an open pull request from being merged, in words for its chip,
 * or null when nothing does (as far as Forge knows).
 */
export function mergeBlocker(link: IssueLink): string | null {
  if (link.kind !== "pull_request" || link.state !== "open") return null;
  switch (link.blocker) {
    case "conflicts":
      return "has conflicts";
    case "checks_failed":
      return "checks failed";
    case "checks_running":
      return "checks running";
    default:
      return null;
  }
}

/** How old Forgejo's last successful check may be for its pull requests to count as up to date. */
export const FORGEJO_UP_TO_DATE_MS = 5 * 60 * 1000;

/** What Forge knows about the workspace's code hosts, as far as the review bar cares. */
export interface CodeHosts {
  /** GitHub is on, with an installation linked to the workspace. */
  github: boolean;
  forgejo: {
    connected: boolean;
    /** When a check of Forgejo last worked. */
    checked_at: string | null;
    /** Why the last check failed; cleared by one that works. */
    check_error: string | null;
  } | null;
}

/**
 * Whether Forgejo's pull requests are up to date: its last successful check
 * is recent, and no check has failed since.
 */
export function forgejoUpToDate(hosts: CodeHosts, now = Date.now()): boolean {
  const forgejo = hosts.forgejo;
  const checked = forgejo?.connected && !forgejo.check_error ? forgejo.checked_at : null;
  return checked !== null && now - Date.parse(checked) < FORGEJO_UP_TO_DATE_MS;
}

/**
 * Whether Forge knows an issue's pull requests: Forgejo is up to date, or
 * GitHub is connected. Otherwise why not, in words for the review bar.
 */
export function pullRequestsKnown(
  hosts: CodeHosts,
  now = Date.now(),
): { known: true } | { known: false; why: string } {
  if (hosts.github || forgejoUpToDate(hosts, now)) return { known: true };
  const forgejo = hosts.forgejo;
  if (!forgejo?.connected) return { known: false, why: "No code host is connected." };
  const since = forgejo.checked_at
    ? `Forge last checked Forgejo at ${clockTime(forgejo.checked_at)}`
    : "Forge hasn't checked Forgejo yet";
  return {
    known: false,
    why: forgejo.check_error ? `${since}: ${forgejo.check_error}.` : `${since}.`,
  };
}

/** A time of day, like 21:40. */
export function clockTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * What the review bar asks for: merging the open pull requests that work
 * on the issue; approving work marked as needing no code; or, without
 * either, sending it back since no pull request was found, or, when Forge
 * can't tell, a choice it can't make for you.
 */
export type ReviewAsk = "merge" | "approve" | "no_pull_request" | "unknown";

export function reviewAsk(openPullRequests: number, noCode: boolean, known: boolean): ReviewAsk {
  if (openPullRequests > 0) return "merge";
  if (noCode) return "approve";
  return known ? "no_pull_request" : "unknown";
}

/**
 * The note on an open pull request's row: what keeps it from being merged;
 * for Forgejo's, "No checks reported" when it has none, and when Forge's
 * last look is too old to vouch for, when that was.
 */
export function pullRequestNote(
  link: IssueLink,
  hosts: CodeHosts,
  now = Date.now(),
): { text: string; tone: "danger" | "muted" } | null {
  if (link.kind !== "pull_request" || link.state !== "open") return null;
  if (link.provider === "forgejo" && !forgejoUpToDate(hosts, now)) {
    const checked = hosts.forgejo?.checked_at;
    return { text: checked ? `last checked ${clockTime(checked)}` : "not checked", tone: "muted" };
  }
  const blocker = mergeBlocker(link);
  if (blocker)
    return { text: blocker, tone: link.blocker === "checks_running" ? "muted" : "danger" };
  if (link.provider === "forgejo" && link.checks === "none")
    return { text: "No checks reported", tone: "muted" };
  return null;
}

/**
 * What an agent asks the PO to decide in a proposed description: its
 * "## Decisions (proposed by …)" and "## Proposed split" sections, each up
 * to the next heading of the same or a higher level.
 */
export function decisionsAsked(description: string): string {
  const sections: string[][] = [];
  let level = 0;
  for (const line of description.split("\n")) {
    const [, hashes, title] = /^(#{1,6})\s+(.*)$/.exec(line) ?? [];
    if (hashes && title !== undefined) {
      if (/^(decisions\b|proposed split\b)/i.test(title.trim())) {
        level = hashes.length;
        sections.push([line]);
        continue;
      }
      if (level && hashes.length <= level) level = 0;
    }
    if (level) sections.at(-1)?.push(line);
  }
  return sections.map((lines) => lines.join("\n").trim()).join("\n\n");
}

/**
 * The proposal an agent asks for a decision with: on an issue in a triage
 * status that's assigned to an agent, that agent's newest open description
 * proposal. The triage bar asks for it, so it's the only place that shows it.
 */
export function decisionProposal(
  issue: Issue,
  stateType: StateType | undefined,
  users: User[],
  suggestions: Suggestion[],
): Suggestion | undefined {
  if (stateType !== "triage") return;
  const assignee = users.find((u) => u.id === issue.assignee_id);
  if (assignee?.kind !== "agent") return;
  return suggestions
    .filter(
      (s) =>
        s.kind === "description" &&
        s.status === "open" &&
        s.issue_id === issue.id &&
        s.created_by === assignee.id,
    )
    .sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
}

/**
 * A decision the PO adopted with "Edit first" and hasn't accepted yet: on an
 * issue in a triage status that's assigned to an agent, that agent's newest
 * description proposal, accepted since the issue last changed status. Saved
 * like any other proposal, so the triage bar keeps asking after a reload.
 * Without the issue's history (still loading) an accepted proposal counts.
 */
export function adoptedDecision(
  issue: Issue,
  stateType: StateType | undefined,
  users: User[],
  suggestions: Suggestion[],
  history: IssueEvent[] | undefined,
): Suggestion | undefined {
  if (stateType !== "triage") return;
  const assignee = users.find((u) => u.id === issue.assignee_id);
  if (assignee?.kind !== "agent") return;
  const newest = suggestions
    .filter(
      (s) => s.kind === "description" && s.issue_id === issue.id && s.created_by === assignee.id,
    )
    .sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
  if (newest?.status !== "accepted" || !newest.resolved_at) return;
  // Accepted before the issue last moved: an old answer, not this question.
  const moved = history
    ?.filter((e) => e.field === "state")
    .reduce((latest, e) => (e.at > latest ? e.at : latest), issue.created_at);
  if (moved && Date.parse(newest.resolved_at) < Date.parse(moved)) return;
  return newest;
}

/**
 * The services a spec belongs to. A copy synced before specs could have
 * several has no list until the app's copy is refreshed, right after it opens.
 */
export function servicesOf(spec: Spec): string[] {
  return spec.service_ids ?? [];
}

/**
 * The specs and decisions an issue is built on: those of its service and its
 * project, plus decisions that hold for the whole workspace (no service, no
 * project). Deprecated specs and decisions no longer in force are left out.
 * Specs by title; decisions newest first.
 */
export function issueKnowledge(
  issue: Issue,
  specs: Spec[],
  decisions: Decision[],
): { specs: Spec[]; decisions: Decision[] } {
  const applies = (record: { service_id: string | null; project_id: string | null }) =>
    (record.service_id !== null && record.service_id === issue.service_id) ||
    (record.project_id !== null && record.project_id === issue.project_id);
  // A spec can belong to several services; it applies if one is the issue's.
  const specApplies = (spec: Spec) =>
    (issue.service_id !== null && servicesOf(spec).includes(issue.service_id)) ||
    (spec.project_id !== null && spec.project_id === issue.project_id);
  return {
    specs: specs
      .filter((s) => s.status !== "deprecated" && specApplies(s))
      .sort((a, b) => a.title.localeCompare(b.title)),
    decisions: decisions
      .filter(
        (d) =>
          (d.status === "accepted" || d.status === "proposed") &&
          (applies(d) || (d.service_id === null && d.project_id === null)),
      )
      .sort((a, b) => decidedOn(b).localeCompare(decidedOn(a)) || a.title.localeCompare(b.title)),
  };
}

/** A proposed decision has no date yet: it counts from when it was written. */
function decidedOn(d: Decision) {
  return d.decided_on ?? d.created_at.slice(0, 10);
}
