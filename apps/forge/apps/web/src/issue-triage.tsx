// The decision an issue in Triage is waiting for, at the top of the issue:
// accept it into the team's workflow, close it as a duplicate, or decline it.
// When the agent it's assigned to sent it here to ask for a decision, the
// bar asks for that decision instead.
import { Menu } from "@base-ui/react/menu";
import type { Issue, StateType, Suggestion, User, WorkflowState } from "@forge/types";
import { ChevronDown } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, messageOf } from "./api";
import {
  adoptedDecision,
  byId,
  decisionProposal,
  decisionsAsked,
  identifier,
  useStates,
  useTeamStates,
  useUsers,
} from "./data";
import { StateIcon } from "./icons";
import { useIssueHistory } from "./issue-activity";
import { Markdown } from "./markdown";
import { isTyping } from "./new-issue";
import { Picker } from "./picker";
import { DescriptionDiff, proposedDescription, TriageSuggestion } from "./suggestions";
import { Button, buttonClass, ErrorMessage } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

const DUPLICATE_TRIGGER = "triage-duplicate";

const menuItem =
  "flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 outline-none select-none data-highlighted:bg-highlight";

/** The first of a team's statuses of a type. */
function firstOf(states: WorkflowState[], type: StateType) {
  return states.find((s) => s.type === type);
}

export function TriageBar({
  issue,
  onEditDescription,
}: {
  issue: Issue;
  /** Opens the description for editing. */
  onEditDescription: () => void;
}) {
  const users = useUsers();
  const suggestions = useRecords("Suggestion");
  const stateType = useStates().get(issue.state_id)?.type;
  const assignee = users.find((u) => u.id === issue.assignee_id);
  const agent = assignee?.kind === "agent" ? assignee : undefined;
  const history = useIssueHistory(issue, agent !== undefined);
  // What the agent asks to decide.
  const proposal = decisionProposal(issue, stateType, users, suggestions);
  // Adopted with "Edit first": still the agent's decision until it's accepted.
  const adopted = adoptedDecision(issue, stateType, users, suggestions, history);

  if (agent && (proposal || adopted)) {
    return (
      <DecisionBar
        issue={issue}
        agent={agent}
        proposal={proposal}
        onEditFirst={onEditDescription}
      />
    );
  }
  return <PlainTriageBar issue={issue} />;
}

function PlainTriageBar({ issue }: { issue: Issue }) {
  const { sync } = useWorkspace();
  const states = useTeamStates(issue.team_id);
  const issues = useRecords("Issue");
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const reporter = useUsers().find((u) => u.id === issue.created_by);

  const first = (type: StateType) => firstOf(states, type);
  // Accepted issues go where the Triage page sends them: the backlog.
  const accept = first("backlog") ?? first("unstarted");
  const decline = first("canceled");
  // Or straight into planned work, or into the first started status ("I'm on it").
  const elsewhere = states.filter(
    (s) =>
      s.id !== accept?.id &&
      (s.type === "backlog" || s.type === "unstarted" || s === first("started")),
  );
  const candidates = issues.filter((i) => i.id !== issue.id && i.team_id === issue.team_id);

  const moveTo = (stateId: string | undefined) => {
    if (stateId) sync.update("Issue", issue.id, { state_id: stateId });
  };
  const markDuplicate = (otherId: string) => {
    sync.create("IssueRelation", {
      issue_id: issue.id,
      related_issue_id: otherId,
      type: "duplicate",
    });
    moveTo(decline?.id);
  };

  // 1 accepts, 2 marks as a duplicate, 3 declines, as in Linear.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
      if (event.key === "1") moveTo(accept?.id);
      else if (event.key === "2") document.getElementById(DUPLICATE_TRIGGER)?.click();
      else if (event.key === "3") moveTo(decline?.id);
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  return (
    <section aria-label="Triage" className="flex flex-col gap-3 rounded-lg bg-surface p-4">
      <div>
        <h2 className="font-medium">Needs triage</h2>
        <p className="text-xs text-muted">
          {reporter ? `${reporter.name} filed this.` : "Someone filed this."} Accept it into the
          workflow, or close it.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex">
          <Button className="rounded-r-none" disabled={!accept} onClick={() => moveTo(accept?.id)}>
            Accept to {accept?.name ?? "Backlog"}
            <Key>1</Key>
          </Button>
          {elsewhere.length > 0 && (
            <Menu.Root>
              <Menu.Trigger
                aria-label="Accept to another status"
                // The other half of the one primary action, not a second one.
                className={`${buttonClass()} ml-px rounded-l-none px-2`}
              >
                <ChevronDown size={15} strokeWidth={1.75} aria-hidden="true" />
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="start">
                  <Menu.Popup className="app-menu min-w-48 p-1 outline-none">
                    {elsewhere.map((state) => (
                      <Menu.Item
                        key={state.id}
                        className={menuItem}
                        onClick={() => moveTo(state.id)}
                      >
                        <StateIcon state={state} />
                        Accept to {state.name}
                      </Menu.Item>
                    ))}
                  </Menu.Popup>
                </Menu.Positioner>
              </Menu.Portal>
            </Menu.Root>
          )}
        </div>
        <Picker
          label="Duplicate of"
          triggerId={DUPLICATE_TRIGGER}
          options={candidates.map((i) => ({
            value: i.id,
            label: `${identifier(i, teamsById)} ${i.title}`,
          }))}
          selected={[]}
          onSelect={markDuplicate}
          triggerClassName={buttonClass("secondary")}
        >
          Duplicate of…
          <Key>2</Key>
        </Picker>
        <Button variant="secondary" disabled={!decline} onClick={() => moveTo(decline?.id)}>
          Decline
          <Key>3</Key>
        </Button>
      </div>
      <div className="pt-3">
        <TriageSuggestion issueId={issue.id} />
      </div>
    </section>
  );
}

/**
 * An agent asked for a decision: it proposed a description with the
 * decisions it recommends and moved the issue here. Accepting adopts the
 * proposal and moves the issue to Todo, which wakes the agent.
 */
function DecisionBar({
  issue,
  agent,
  proposal,
  onEditFirst,
}: {
  issue: Issue;
  agent: User;
  /** Gone once adopted with "Edit first". */
  proposal: Suggestion | undefined;
  onEditFirst: () => void;
}) {
  const { sync, workspace } = useWorkspace();
  const states = useTeamStates(issue.team_id);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The whole proposed description against the current one: the bar is the
  // only place this proposal shows.
  const [fullChange, setFullChange] = useState(false);
  // Where the agent picks up planned work.
  const proceed =
    firstOf(states, "unstarted") ?? firstOf(states, "started") ?? firstOf(states, "backlog");
  const decline = firstOf(states, "canceled");
  const asked = proposal
    ? decisionsAsked(proposedDescription(proposal) ?? "") || proposal.body
    : "";

  const run = async (steps: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await steps();
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  };
  const resolve = (action: "accept" | "dismiss") =>
    proposal
      ? api("POST", `/api/w/${workspace.id}/suggestions/${proposal.id}/${action}`)
      : Promise.resolve();
  const moveTo = (state: WorkflowState | undefined) => {
    if (state) sync.update("Issue", issue.id, { state_id: state.id });
  };

  const acceptAndContinue = () =>
    run(async () => {
      await resolve("accept");
      moveTo(proceed);
    });
  const editFirst = () =>
    run(async () => {
      await resolve("accept");
      onEditFirst();
    });
  // Declining the issue; its proposal goes with it.
  const declineIssue = () =>
    run(async () => {
      await resolve("dismiss");
      moveTo(decline);
    });

  // 1 accepts and continues, 2 edits first, 3 declines.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || isTyping(event.target)) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
      if (event.key === "1") void acceptAndContinue();
      else if (event.key === "2" && proposal) void editFirst();
      else if (event.key === "3") void declineIssue();
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  return (
    <section aria-label="Triage" className="flex flex-col gap-3 rounded-lg bg-surface p-4">
      <div>
        <h2 className="font-medium">{agent.name} needs your decision</h2>
        <p className="text-xs text-muted">
          {proposal
            ? `It proposes these decisions. Accept them and it carries on (the issue moves to ${proceed?.name ?? "Todo"}).`
            : `The proposal is in the description now. Once it says what to build, accept it and ${agent.name} carries on.`}
        </p>
      </div>
      {asked && (
        <figure aria-label="Proposed decisions" className="rounded-md bg-panel px-3 py-2">
          <Markdown text={asked} />
        </figure>
      )}
      {proposal && (
        <div className="flex flex-col gap-2">
          <button
            type="button"
            aria-expanded={fullChange}
            className="self-start text-xs text-muted hover:text-fg"
            onClick={() => setFullChange(!fullChange)}
          >
            {fullChange ? "Hide the full change" : "See the full change"}
          </button>
          {fullChange && <DescriptionDiff issue={issue} suggestion={proposal} />}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Button disabled={busy || !proceed} onClick={() => void acceptAndContinue()}>
          Accept and continue
          <Key>1</Key>
        </Button>
        {proposal && (
          <Button variant="secondary" disabled={busy} onClick={() => void editFirst()}>
            Edit first
            <Key>2</Key>
          </Button>
        )}
        <Button variant="secondary" disabled={busy || !decline} onClick={() => void declineIssue()}>
          Decline
          <Key>3</Key>
        </Button>
      </div>
      <ErrorMessage>{error}</ErrorMessage>
    </section>
  );
}

function Key({ children }: { children: string }) {
  return <kbd className="font-sans text-xs font-normal opacity-60">{children}</kbd>;
}
