import { expect, test } from "bun:test";
import type {
  Decision,
  Issue,
  IssueEvent,
  IssueLink,
  Spec,
  Suggestion,
  Team,
  User,
} from "@forge/types";
import {
  adoptedDecision,
  agentWork,
  branchName,
  decisionProposal,
  decisionsAsked,
  duration,
  issueKnowledge,
  mergeBlocker,
  pullRequestsToMerge,
  sharedFields,
} from "./data";

const teams = new Map([["t", { id: "t", key: "FOR" } as Team]]);

test("what every issue in a list has in common is told apart from what differs", () => {
  const row = (fields: Partial<Issue>) =>
    ({ project_id: "m1", service_id: null, label_ids: [], ...fields }) as Issue;
  expect(
    sharedFields([
      row({ service_id: "web", label_ids: ["bug", "ui"] }),
      row({ service_id: "api", label_ids: ["ui"] }),
    ]),
  ).toEqual({ project: "m1", service: undefined, labels: ["ui"] });
  // None has a service: that's shared too.
  expect(sharedFields([row({}), row({})]).service).toBeNull();
  // A single issue has nothing to be told apart from.
  expect(sharedFields([row({})])).toEqual({ labels: [] });
});
const issue = (title: string) => ({ team_id: "t", number: 12, title }) as Issue;

test("branch names start with the identifier and slug the title", () => {
  expect(branchName(issue("Fix the login — again!"), teams)).toBe("for-12-fix-the-login-again");
  expect(branchName(issue("Crème brûlée"), teams)).toBe("for-12-creme-brulee");
  expect(branchName(issue("one two three four five six seven"), teams)).toBe(
    "for-12-one-two-three-four-five-six",
  );
  expect(branchName(issue("!!!"), teams)).toBe("for-12");
});

test("pull requests to merge are the open ones that work on the issue, oldest first", () => {
  const link = (id: string, fields: Partial<IssueLink>) =>
    ({
      id,
      issue_id: "i",
      kind: "pull_request",
      state: "open",
      works_on: true,
      created_at: `2026-10-14T08:0${id}:00Z`,
      ...fields,
    }) as IssueLink;
  const links = [
    link("3", {}),
    link("1", {}),
    link("2", { state: "merged" }),
    link("4", { state: "closed" }),
    link("5", { kind: "commit", state: "pushed" }),
    link("6", { issue_id: "other" }),
    link("7", { works_on: false }),
  ];
  expect(pullRequestsToMerge(links, "i").map((l) => l.id)).toEqual(["1", "3"]);
  expect(pullRequestsToMerge(links, "none")).toEqual([]);
});

test("an open pull request's chip says what keeps it from being merged", () => {
  const link = (fields: Partial<IssueLink>) =>
    ({ kind: "pull_request", state: "open", blocker: null, ...fields }) as IssueLink;
  expect(mergeBlocker(link({ blocker: "conflicts" }))).toBe("has conflicts");
  expect(mergeBlocker(link({ blocker: "checks_failed" }))).toBe("checks failed");
  expect(mergeBlocker(link({ blocker: "checks_running" }))).toBe("checks running");
  expect(mergeBlocker(link({}))).toBeNull();
  // Older records, from before Forge asked GitHub.
  expect(mergeBlocker(link({ blocker: undefined }))).toBeNull();
  // Once it's merged or closed, it no longer matters.
  expect(mergeBlocker(link({ blocker: "conflicts", state: "merged" }))).toBeNull();
});

test("an issue's knowledge is its service's and project's, plus workspace-wide decisions", () => {
  const spec = (id: string, fields: Partial<Spec>) =>
    ({ id, title: id, status: "active", service_ids: [], project_id: null, ...fields }) as Spec;
  const decision = (id: string, fields: Partial<Decision>) =>
    ({
      id,
      title: id,
      status: "accepted",
      service_id: null,
      project_id: null,
      decided_on: "2026-10-01",
      created_at: "2026-10-01T00:00:00Z",
      ...fields,
    }) as Decision;
  const specs = [
    spec("web b", { service_ids: ["web"] }),
    spec("web a", { service_ids: ["sync", "web"], status: "draft" }),
    spec("web old", { service_ids: ["web"], status: "deprecated" }),
    spec("milestone", { project_id: "m1" }),
    spec("sync", { service_ids: ["sync"] }),
    spec("unattached", {}),
  ];
  const decisions = [
    decision("everywhere", { decided_on: "2026-10-14" }),
    decision("web", { service_id: "web", decided_on: "2026-10-02" }),
    decision("proposed", { service_id: "web", status: "proposed", decided_on: null }),
    decision("superseded", { service_id: "web", status: "superseded" }),
    decision("overridden", { status: "overridden" }),
    decision("sync", { service_id: "sync" }),
    decision("milestone", { project_id: "m1" }),
  ];
  const ids = (records: { id: string }[]) => records.map((r) => r.id);

  const onWeb = issueKnowledge({ service_id: "web", project_id: "m1" } as Issue, specs, decisions);
  expect(ids(onWeb.specs)).toEqual(["milestone", "web a", "web b"]);
  expect(ids(onWeb.decisions)).toEqual(["everywhere", "web", "milestone", "proposed"]);

  const bare = issueKnowledge({ service_id: null, project_id: null } as Issue, specs, decisions);
  expect(ids(bare.specs)).toEqual([]);
  expect(ids(bare.decisions)).toEqual(["everywhere"]);
});

test("the decisions asked are the proposed decisions and split, nothing else", () => {
  const description = [
    "The PO's text.",
    "",
    "## Decisions (proposed by Claude Code)",
    "- Keep it short.",
    "### Detail",
    "Still part of it.",
    "",
    "## Notes",
    "Not asked.",
    "## Proposed split",
    "1. First part",
  ].join("\n");
  expect(decisionsAsked(description)).toBe(
    [
      "## Decisions (proposed by Claude Code)",
      "- Keep it short.",
      "### Detail",
      "Still part of it.",
      "",
      "## Proposed split",
      "1. First part",
    ].join("\n"),
  );
  expect(decisionsAsked("Just a description.\n## Notes\nNothing to decide.")).toBe("");
});

test("the agent's own open proposal on a triaged issue is the decision it asks for", () => {
  const users = [
    { id: "agent", kind: "agent" } as User,
    { id: "other-agent", kind: "agent" } as User,
    { id: "ada", kind: "person" } as User,
  ];
  const proposal = (id: string, fields: Partial<Suggestion>) =>
    ({
      id,
      kind: "description",
      status: "open",
      issue_id: "i",
      created_by: "agent",
      created_at: `2026-10-14T08:0${id}:00Z`,
      ...fields,
    }) as Suggestion;
  const suggestions = [
    proposal("1", {}),
    proposal("2", {}),
    proposal("3", { created_by: "other-agent" }),
    proposal("4", { status: "dismissed" }),
    proposal("5", { issue_id: "j" }),
    proposal("6", { kind: "drift" }),
  ];
  const assigned = (assignee_id: string | null) => ({ id: "i", assignee_id }) as Issue;

  // The newest of the agent's own open description proposals.
  expect(decisionProposal(assigned("agent"), "triage", users, suggestions)?.id).toBe("2");
  // Outside Triage, or not assigned to that agent, nothing is being asked.
  expect(decisionProposal(assigned("agent"), "started", users, suggestions)).toBeUndefined();
  expect(decisionProposal(assigned("ada"), "triage", users, suggestions)).toBeUndefined();
  expect(decisionProposal(assigned(null), "triage", users, suggestions)).toBeUndefined();
  expect(decisionProposal(assigned("other-agent"), "triage", users, suggestions)?.id).toBe("3");
});

test("a decision adopted with Edit first is still asked until the issue moves on", () => {
  const users = [{ id: "agent", kind: "agent" } as User, { id: "ada", kind: "person" } as User];
  const issue = (assignee_id: string | null) =>
    ({ id: "i", assignee_id, created_at: "2026-10-14T08:00:00Z" }) as Issue;
  const proposal = (id: string, fields: Partial<Suggestion>) =>
    ({
      id,
      kind: "description",
      status: "accepted",
      issue_id: "i",
      created_by: "agent",
      created_at: `2026-10-14T09:0${id}:00Z`,
      resolved_at: "2026-10-14T10:00:00Z",
      ...fields,
    }) as Suggestion;
  const toTriage = (at: string) =>
    ({ at, actor_id: "agent", field: "state", from: "todo", to: "triage" }) as IssueEvent;
  const accepted = [proposal("1", {})];
  const sentToTriage = [toTriage("2026-10-14T09:30:00Z")];

  // Adopted after the agent sent it to Triage: still the agent's question.
  expect(adoptedDecision(issue("agent"), "triage", users, accepted, sentToTriage)?.id).toBe("1");
  // Also while the history is still loading, and for an issue filed in Triage.
  expect(adoptedDecision(issue("agent"), "triage", users, accepted, undefined)?.id).toBe("1");
  expect(adoptedDecision(issue("agent"), "triage", users, accepted, [])?.id).toBe("1");
  // Accepted before the issue last came back to Triage: an old answer.
  const backLater = [toTriage("2026-10-14T11:00:00Z")];
  expect(adoptedDecision(issue("agent"), "triage", users, accepted, backLater)).toBeUndefined();
  // A newer proposal that's open or dismissed is what counts.
  const newer = (status: Suggestion["status"]) => [
    ...accepted,
    proposal("2", { status, resolved_at: null }),
  ];
  expect(adoptedDecision(issue("agent"), "triage", users, newer("open"), [])).toBeUndefined();
  expect(adoptedDecision(issue("agent"), "triage", users, newer("dismissed"), [])).toBeUndefined();
  // Outside Triage, or not assigned to the agent, nothing is being asked.
  expect(adoptedDecision(issue("agent"), "unstarted", users, accepted, [])).toBeUndefined();
  expect(adoptedDecision(issue("ada"), "triage", users, accepted, [])).toBeUndefined();
});

test("an agent's work runs from when it started, and stops after an hour of silence", () => {
  const now = Date.parse("2026-10-14T12:00:00Z");
  const agent = (seenAt: string | null) =>
    ({
      working_on_id: seenAt ? "i" : null,
      working_since: seenAt ? "2026-10-14T10:55:00Z" : null,
      working_seen_at: seenAt,
    }) as User;

  expect(agentWork(agent(null), now)).toBeUndefined();
  expect(agentWork(undefined, now)).toBeUndefined();
  expect(agentWork(agent("2026-10-14T11:30:00Z"), now)).toMatchObject({
    issueId: "i",
    stopped: false,
  });
  expect(agentWork(agent("2026-10-14T10:59:00Z"), now)?.stopped).toBe(true);

  expect(duration("2026-10-14T11:48:00Z", now)).toBe("12 min");
  expect(duration("2026-10-14T10:55:00Z", now)).toBe("1 h 5 min");
  expect(duration("2026-10-14T10:00:00Z", now)).toBe("2 h");
});
