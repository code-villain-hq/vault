import { Menu } from "@base-ui/react/menu";
import { type Confirmation, ConfirmDialog, isTyping } from "@code-villain/web";
import type {
  Comment,
  Decision,
  Issue,
  IssueEvent,
  IssueLink,
  IssueRelation,
  RelationType,
} from "@forge/types";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { Ellipsis, Plus, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { AgentWorkLine } from "./agent-work";
import {
  branchName,
  byId,
  identifier,
  issueKnowledge,
  pullRequestNote,
  timeAgo,
  useIssueByIdentifier,
  useStates,
  useUsers,
} from "./data";
import { useDraft } from "./draft";
import { Avatar, StateIcon } from "./icons";
import { EventLine, useIssueHistory } from "./issue-activity";
import {
  AssigneePicker,
  LabelsPicker,
  PriorityPicker,
  ProjectPicker,
  ReviewerPicker,
  ServicePicker,
  StatusPicker,
} from "./issue-fields";
import { ReviewBar, useCodeHosts, useReviewState } from "./issue-review";
import { SpecChangeSummary, useOpenSpecChanges } from "./issue-spec-change";
import { TriageBar } from "./issue-triage";
import { DecisionRow, SpecRow } from "./knowledge";
import { PageHeader } from "./layout";
import { Markdown } from "./markdown";
import { MentionTextarea } from "./mentions";
import { useNewIssue } from "./new-issue";
import { chip, NewButton } from "./page-parts";
import { Picker } from "./picker";
import { describeDelta } from "./spec-change";
import { DescriptionProposals } from "./suggestions";
import { Button, Disclosure } from "./ui";
import { useIsAdmin, useMayDeleteIssue, useRecords, useWorkspace } from "./workspace";

export function IssuePage() {
  const { identifier: id } = useParams({ strict: false }) as { identifier: string };
  const issue = useIssueByIdentifier(id);
  const navigate = useNavigate();

  const teams = useRecords("Team");
  // Esc goes back to where the issue was opened from; ⌘⇧. copies a branch name.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // By key position: with Shift, "." types ">" on many layouts.
      if (issue && event.code === "Period" && event.shiftKey && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        void navigator.clipboard.writeText(branchName(issue, byId(teams)));
        return;
      }
      if (event.key !== "Escape" || isTyping(event.target)) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
      window.history.back();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [issue, teams]);

  if (!issue) {
    return (
      <>
        <PageHeader title={id} />
        <p className="px-6 py-16 text-center text-muted">
          This issue doesn't exist, or was deleted.
        </p>
      </>
    );
  }
  // Keyed, so nothing of one issue (history, drafts) shows on the next.
  return <IssueDetail key={issue.id} issue={issue} onDeleted={() => void navigate({ to: ".." })} />;
}

function IssueDetail({ issue, onDeleted }: { issue: Issue; onDeleted: () => void }) {
  const { workspace, sync } = useWorkspace();
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const issues = useRecords("Issue");
  const parent = issues.find((i) => i.id === issue.parent_id);
  const team = teamsById.get(issue.team_id);
  const states = useStates();
  const review = useReviewState(issue.team_id);
  const users = useRecords("User");
  // Lifted, so a comment can be added to the description and opened there.
  const [editingDescription, setEditingDescription] = useState(false);
  // A first relation is asked for from "⋯"; the section shows once there is one.
  const [relating, setRelating] = useState(false);
  const addToDescription = (comment: Comment) => {
    const author = users.find((u) => u.id === comment.author_id)?.name ?? "a former member";
    const section = `## From ${author}'s comment\n\n${comment.body.trim()}`;
    const current = issue.description.trim();
    sync.update("Issue", issue.id, {
      description: current ? `${current}\n\n${section}` : section,
    });
    setEditingDescription(true);
  };

  return (
    <>
      <PageHeader
        title={identifier(issue, teamsById)}
        headingTitle={issue.title}
        // Where the issue sits: its team, and its parent for a sub-issue.
        parent={
          team && (
            <>
              <Link
                to="/$workspace/team/$team"
                params={{ workspace: workspace.url_key, team: team.key }}
                className="hover:text-fg"
              >
                {team.name}
              </Link>
              {parent && (
                <>
                  <span className="text-faint">›</span>
                  <Link
                    to="/$workspace/issue/$identifier"
                    params={{
                      workspace: workspace.url_key,
                      identifier: identifier(parent, teamsById),
                    }}
                    title={parent.title}
                    className="hover:text-fg"
                  >
                    {identifier(parent, teamsById)}
                  </Link>
                </>
              )}
            </>
          )
        }
      />
      <div className="page-content min-h-0 flex-1 overflow-y-auto">
        <div className="flex flex-col gap-6 px-8 py-8">
          {states.get(issue.state_id)?.type === "triage" && (
            <TriageBar issue={issue} onEditDescription={() => setEditingDescription(true)} />
          )}
          {issue.state_id === review?.id && <ReviewBar issue={issue} />}
          <div className="flex flex-col gap-3">
            <TitleField issue={issue} />
            <Fields issue={issue} onDeleted={onDeleted} onAddRelation={() => setRelating(true)} />
            <AgentWorkLine issue={issue} />
          </div>
          <DescriptionProposals issue={issue} />
          <DescriptionField
            issue={issue}
            editing={editingDescription}
            setEditing={setEditingDescription}
          />
          <Knowledge issue={issue} />
          {issue.state_id !== review?.id && <OpenSpecChange issue={issue} />}
          <AppliedSpecChange issue={issue} />
          <SubIssues issue={issue} />
          <Relations issue={issue} asked={relating} onAsked={() => setRelating(false)} />
          <Links issue={issue} />
          <Activity issue={issue} onAddToDescription={addToDescription} />
        </div>
      </div>
    </>
  );
}

/**
 * The issue's fields as a row of chips under the title. The ones most issues
 * never need (parent, due date, a first sub-issue or relation) and the rare
 * actions sit behind "⋯".
 */
function Fields({
  issue,
  onDeleted,
  onAddRelation,
}: {
  issue: Issue;
  onDeleted: () => void;
  onAddRelation: () => void;
}) {
  const { sync } = useWorkspace();
  const newIssue = useNewIssue();
  const hasChildren = useRecords("Issue").some((i) => i.parent_id === issue.id);
  const hasRelations = useRecords("IssueRelation").some(
    (r) => r.issue_id === issue.id || r.related_issue_id === issue.id,
  );
  const mayDelete = useMayDeleteIssue()(issue);
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  // Chips asked for from "⋯" that have no value yet.
  const [choosingParent, setChoosingParent] = useState(false);
  const [addingDueDate, setAddingDueDate] = useState(false);
  const [copied, setCopied] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const branch = branchName(issue, teamsById);

  const copyBranch = () => {
    void navigator.clipboard.writeText(branch).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <StatusPicker issue={issue} variant="chip" />
      <PriorityPicker issue={issue} variant="chip" />
      <AssigneePicker issue={issue} variant="chip" />
      <ReviewerPicker issue={issue} variant="chip" />
      <ServicePicker issue={issue} />
      <LabelsPicker issue={issue} variant="chip" />
      <ProjectPicker issue={issue} variant="chip" />
      {choosingParent && <ParentPicker issue={issue} onClose={() => setChoosingParent(false)} />}
      {(issue.due_date || addingDueDate) && (
        <DueDate issue={issue} autoOpen={addingDueDate} onDone={() => setAddingDueDate(false)} />
      )}
      {issue.no_code && <NoCodeChip issue={issue} />}
      <Menu.Root>
        <Menu.Trigger
          aria-label="More"
          className="ui-button ui-icon-button data-popup-open:bg-hover"
        >
          <Ellipsis size={15} strokeWidth={1.75} aria-hidden="true" />
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="start">
            <Menu.Popup className="ui-menu min-w-56 p-1 outline-none">
              <Menu.Item className={menuItem} onClick={() => setChoosingParent(true)}>
                {issue.parent_id ? "Change parent issue…" : "Make it a sub-issue of…"}
              </Menu.Item>
              {!issue.due_date && (
                <Menu.Item className={menuItem} onClick={() => setAddingDueDate(true)}>
                  Add a due date
                </Menu.Item>
              )}
              {!issue.no_code && (
                <Menu.Item
                  className={menuItem}
                  onClick={() => sync.update("Issue", issue.id, { no_code: true })}
                >
                  Mark as needing no code
                </Menu.Item>
              )}
              {!hasChildren && (
                <Menu.Item
                  className={menuItem}
                  onClick={() =>
                    newIssue({
                      teamId: issue.team_id,
                      parentId: issue.id,
                      projectId: issue.project_id ?? undefined,
                    })
                  }
                >
                  Add a sub-issue
                </Menu.Item>
              )}
              {!hasRelations && (
                <Menu.Item className={menuItem} onClick={onAddRelation}>
                  Add a relation…
                </Menu.Item>
              )}
              <Menu.Item className={menuItem} onClick={copyBranch}>
                Copy git branch name
                <kbd className="ml-auto text-xs text-faint">⌘⇧.</kbd>
              </Menu.Item>
              {/* Admins delete any issue; others the ones they created. */}
              {mayDelete && (
                <>
                  <Menu.Separator className="my-1 h-px bg-line" />
                  <Menu.Item
                    className={`${menuItem} text-danger-ink`}
                    onClick={() =>
                      setConfirmation({
                        title: `Delete “${issue.title}”?`,
                        body: "It's deleted for everyone, along with its comments. This can't be undone.",
                        action: "Delete issue",
                        onConfirm: () => {
                          sync.delete("Issue", issue.id);
                          onDeleted();
                        },
                      })
                    }
                  >
                    Delete issue
                  </Menu.Item>
                </>
              )}
            </Menu.Popup>
          </Menu.Positioner>
        </Menu.Portal>
      </Menu.Root>
      {copied && (
        <span role="status" className="text-xs text-faint">
          Copied {branch}
        </span>
      )}
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </div>
  );
}

const menuItem =
  "ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none";

/**
 * The No code mark: the work changes no code, so its review needs no pull
 * request. Set from "⋯" or by an agent handing the issue over.
 */
function NoCodeChip({ issue }: { issue: Issue }) {
  const { sync } = useWorkspace();
  return (
    <span className={chip}>
      No code
      <button
        type="button"
        aria-label="Remove the No code mark"
        title="Remove the No code mark"
        onClick={() => sync.update("Issue", issue.id, { no_code: false })}
        className="-mr-1 rounded text-faint outline-none hover:text-fg focus-visible:ring-2 focus-visible:ring-accent"
      >
        <X size={12} aria-hidden="true" />
      </button>
    </span>
  );
}

/** The due date as a chip; opens the calendar when just added from "⋯". */
function DueDate({
  issue,
  autoOpen,
  onDone,
}: {
  issue: Issue;
  autoOpen: boolean;
  onDone: () => void;
}) {
  const { sync } = useWorkspace();
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!autoOpen || !field.current) return;
    field.current.focus();
    try {
      field.current.showPicker();
    } catch {
      // Not every browser lets a page open the calendar; the field has focus.
    }
  }, [autoOpen]);
  return (
    <label className={chip}>
      <span>Due</span>
      <input
        ref={field}
        type="date"
        aria-label="Due date"
        value={issue.due_date ?? ""}
        onChange={(e) => sync.update("Issue", issue.id, { due_date: e.target.value || null })}
        onBlur={onDone}
        className="h-full rounded bg-transparent text-fg outline-none focus-visible:ring-2 focus-visible:ring-accent"
      />
    </label>
  );
}

function TitleField({ issue }: { issue: Issue }) {
  const { sync } = useWorkspace();
  const title = useDraft(issue.title, (value) => {
    if (value.trim()) sync.update("Issue", issue.id, { title: value.trim() });
  });
  return (
    <textarea
      aria-label="Title"
      rows={1}
      value={title.value}
      onChange={(e) => title.onChange(e.target.value.replace(/\n/g, ""))}
      // An emptied title isn't saved, so the field shows the title again.
      onBlur={() => (title.value.trim() ? title.onBlur() : title.commit(issue.title))}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      className="field-sizing-content resize-none overflow-hidden bg-transparent text-title font-bold outline-none"
    />
  );
}

/**
 * The issue's scope, as rendered Markdown; click it to edit the source,
 * saved as you pause.
 */
function DescriptionField({
  issue,
  editing,
  setEditing,
}: {
  issue: Issue;
  editing: boolean;
  setEditing: (editing: boolean) => void;
}) {
  const { sync } = useWorkspace();
  // Saving "@Ada" while someone is still choosing between Ada and Adam
  // would notify the wrong person.
  const [mentioning, setMentioning] = useState(false);
  const description = useDraft(
    issue.description,
    (value) => sync.update("Issue", issue.id, { description: value }),
    mentioning,
  );
  const field = useRef<HTMLTextAreaElement>(null);
  // Opened from the rendered text: carry on typing at the end.
  useEffect(() => {
    if (!editing || !field.current) return;
    field.current.focus();
    field.current.setSelectionRange(field.current.value.length, field.current.value.length);
  }, [editing]);

  if (!editing && description.value.trim()) {
    return (
      // biome-ignore lint/a11y/useKeyWithClickEvents lint/a11y/noStaticElementInteractions: the Edit button below does the same from the keyboard
      <div
        className="group relative cursor-text"
        onClick={(event) => {
          // Links in the text open; anywhere else starts editing.
          if (!(event.target as Element).closest("a")) setEditing(true);
        }}
      >
        <Markdown text={description.value} />
        <button
          type="button"
          onClick={() => setEditing(true)}
          className="absolute -top-1 right-0 rounded px-1.5 text-xs text-faint opacity-0 hover:text-fg focus-visible:opacity-100 group-hover:opacity-100"
        >
          Edit
        </button>
      </div>
    );
  }
  return (
    <MentionTextarea
      issueId={issue.id}
      ref={field}
      aria-label="Description"
      placeholder="Add a description… (Markdown, @ to mention)"
      value={description.value}
      onChange={description.onChange}
      onMentioning={setMentioning}
      onCommit={description.commit}
      // Stay a text field while typing into an empty description.
      onFocus={() => setEditing(true)}
      onBlur={() => {
        description.onBlur();
        setEditing(false);
      }}
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        // Done editing; Escape shouldn't also leave the issue.
        e.stopPropagation();
        e.currentTarget.blur();
      }}
      className="field-sizing-content min-h-24 resize-none bg-transparent leading-relaxed outline-none placeholder:text-faint"
    />
  );
}

/**
 * The specs and decisions the work follows, folded to one line until asked
 * for: they're for looking things up, not for reading every time. The
 * workspace-wide decisions hold for every issue, so they're counted, not
 * listed.
 */
function Knowledge({ issue }: { issue: Issue }) {
  const { workspace } = useWorkspace();
  const allSpecs = useRecords("Spec");
  const allDecisions = useRecords("Decision");
  const { specs, decisions, wide } = useMemo(() => {
    const { specs, decisions } = issueKnowledge(issue, allSpecs, allDecisions);
    const isWide = (d: Decision) => d.service_id === null && d.project_id === null;
    return {
      specs,
      decisions: decisions.filter((d) => !isWide(d)),
      wide: decisions.filter(isWide).length,
    };
  }, [issue, allSpecs, allDecisions]);
  const [open, setOpen] = useRemembered("forge:issue:knowledge-open");
  if (specs.length === 0 && decisions.length === 0) return null;
  const counts = [
    specs.length > 0 && plural(specs.length, "spec"),
    decisions.length > 0 && plural(decisions.length, "decision"),
  ]
    .filter(Boolean)
    .join(" and ");
  return (
    <section className="flex flex-col gap-2">
      <Disclosure open={open} onToggle={() => setOpen(!open)} className="self-start">
        Follows {counts}
      </Disclosure>
      {open && (
        <div className="flex flex-col gap-4">
          {specs.length > 0 && (
            <div>
              <h3 className="text-xs text-faint">Specs</h3>
              <ul className="-mx-3 flex flex-col">
                {specs.map((spec) => (
                  <SpecRow key={spec.id} spec={spec} />
                ))}
              </ul>
            </div>
          )}
          {decisions.length > 0 && (
            <div>
              <h3 className="text-xs text-faint">Decisions</h3>
              <ul className="-mx-3 flex flex-col">
                {decisions.map((decision) => (
                  <DecisionRow key={decision.id} decision={decision} />
                ))}
              </ul>
            </div>
          )}
          {wide > 0 && (
            <Link
              to="/$workspace/decisions"
              params={{ workspace: workspace.url_key }}
              className="ui-link self-start text-xs"
            >
              And the {plural(wide, "decision")} for the whole workspace
            </Link>
          )}
        </div>
      )}
    </section>
  );
}

function plural(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

/** An open/closed state this viewer's browser remembers. */
function useRemembered(key: string): [boolean, (open: boolean) => void] {
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem(key) === "1";
    } catch {
      return false;
    }
  });
  return [
    open,
    (next) => {
      setOpen(next);
      try {
        localStorage.setItem(key, next ? "1" : "0");
      } catch {
        // Not remembered, then; it still opens.
      }
    },
  ];
}

/**
 * What the issue's spec change will do once the issue is done, while it's
 * open; in review, the review bar shows it next to the pull request.
 */
function OpenSpecChange({ issue }: { issue: Issue }) {
  const users = byId(useRecords("User"));
  const states = useStates();
  const open = useOpenSpecChanges().some((c) => c.issue_id === issue.id);
  // Reopened after its spec change took effect: that's history, shown below.
  const applied = useRecords("SpecChange").some((c) => c.issue_id === issue.id && c.applied_at);
  const type = states.get(issue.state_id)?.type;
  // Before work starts, none written yet is the usual; once started, it's worth saying.
  if (applied || (!open && type !== "started")) return null;
  const author = issue.assignee_id ? users.get(issue.assignee_id)?.name : undefined;
  return (
    <Section title="Spec change">
      <SpecChangeSummary issue={issue} verb="Finishing it" author={author} />
    </Section>
  );
}

/**
 * What the issue's spec change did when the issue was done (FOR-74): the
 * specs it changed, the ones that no longer fitted it, and the decisions it
 * replaced. Before that, the review bar shows what it will do.
 */
function AppliedSpecChange({ issue }: { issue: Issue }) {
  const { workspace } = useWorkspace();
  const change = useRecords("SpecChange").find((c) => c.issue_id === issue.id);
  const specs = byId(useRecords("Spec"));
  const decisions = byId(useRecords("Decision"));
  if (!change?.applied_at) return null;
  const replaced = change.replaces_decision_ids.flatMap((id) => decisions.get(id) ?? []);
  const deltas = change.applied_spec_ids.map((id) => change.specs.find((d) => d.spec_id === id));
  // Said once when it rewrote every one of them, instead of on each row.
  const allRewritten = deltas.length > 1 && deltas.every((d) => d?.rewrite);
  return (
    <Section title={allRewritten ? "Spec change · rewrote each of these" : "Spec change"}>
      <ul className="flex flex-col gap-2 text-sm">
        {change.applied_spec_ids.map((id) => {
          const spec = specs.get(id);
          return (
            <li key={id}>
              {spec ? (
                <Link
                  to="/$workspace/spec/$spec"
                  params={{ workspace: workspace.url_key, spec: id }}
                  className="ui-link"
                >
                  {spec.title}
                </Link>
              ) : (
                <span className="font-medium">A spec since deleted</span>
              )}
              {!allRewritten && (
                <span className="text-muted">
                  {" "}
                  · {describeDelta(change.specs.find((d) => d.spec_id === id))}
                </span>
              )}
            </li>
          );
        })}
        {change.not_applied.map((missed) => (
          <li key={missed.spec_id ?? missed.title} className="flex flex-col">
            <span className="font-medium">{missed.title}</span>
            <span className="text-warn-ink">
              Left as it was: {missed.reason} Change it by hand if it should still change.
            </span>
          </li>
        ))}
        {replaced.map((decision) => (
          <li key={decision.id} className="text-muted">
            Replaced the decision “{decision.title}”.
          </li>
        ))}
      </ul>
      <p className="mt-2 text-xs text-faint">
        Applied {timeAgo(change.applied_at)}, when this issue was done.
      </p>
    </Section>
  );
}

function SubIssues({ issue }: { issue: Issue }) {
  const { workspace } = useWorkspace();
  const newIssue = useNewIssue();
  const issues = useRecords("Issue");
  const states = useStates();
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const children = issues
    .filter((i) => i.parent_id === issue.id)
    .sort((a, b) => a.number - b.number);
  const done = children.filter((c) => c.completed_at).length;
  // The first one is added from "⋯".
  if (children.length === 0) return null;

  return (
    <Section
      title={`Sub-issues · ${done}/${children.length}`}
      action={
        <NewButton
          label="Add sub-issue"
          onClick={() =>
            newIssue({
              teamId: issue.team_id,
              parentId: issue.id,
              projectId: issue.project_id ?? undefined,
            })
          }
        />
      }
    >
      {children.map((child) => {
        const state = states.get(child.state_id);
        return (
          <Link
            key={child.id}
            to="/$workspace/issue/$identifier"
            params={{ workspace: workspace.url_key, identifier: identifier(child, teamsById) }}
            className="-mx-2 flex h-8 items-center gap-2 rounded-md px-2 hover:bg-hover"
          >
            {state && <StateIcon state={state} />}
            <span className="w-16 shrink-0 font-mono text-xs text-muted">
              {identifier(child, teamsById)}
            </span>
            <span className="truncate">{child.title}</span>
          </Link>
        );
      })}
    </Section>
  );
}

function Section({
  title,
  action,
  children,
}: {
  title: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="pt-2">
      <div className="mb-1 flex h-7 items-center justify-between">
        <h2 className="text-xs font-medium text-muted">{title}</h2>
        {action}
      </div>
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

/** Adding "blocked by X" stores "X blocks this". */
type NewRelation = RelationType | "blocked_by";

const RELATION_NAMES: Record<RelationType, [outgoing: string, incoming: string]> = {
  blocks: ["Blocks", "Blocked by"],
  related: ["Related to", "Related to"],
  duplicate: ["Duplicate of", "Duplicated by"],
};

/** Shown once there is a relation; `asked`: the first one, from "⋯". */
function Relations({
  issue,
  asked,
  onAsked,
}: {
  issue: Issue;
  asked: boolean;
  onAsked: () => void;
}) {
  const { sync, workspace } = useWorkspace();
  const relations = useRecords("IssueRelation");
  const issues = useRecords("Issue");
  const issuesById = useMemo(() => byId(issues), [issues]);
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const [adding, setAdding] = useState<NewRelation | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  const mine = relations.filter((r) => r.issue_id === issue.id || r.related_issue_id === issue.id);
  const describe = (r: IssueRelation) => {
    const outgoing = r.issue_id === issue.id;
    const other = issuesById.get(outgoing ? r.related_issue_id : r.issue_id);
    return { label: RELATION_NAMES[r.type][outgoing ? 0 : 1], other };
  };

  const candidates = issues.filter(
    (i) => i.id !== issue.id && !mine.some((r) => describe(r).other?.id === i.id),
  );
  if (mine.length === 0 && !adding && !asked) return null;

  return (
    <Section
      title="Relations"
      action={
        <Picker
          label="Add relation"
          options={[
            { value: "blocks", label: "Blocks…" },
            { value: "blocked_by", label: "Blocked by…" },
            { value: "related", label: "Related to…" },
            { value: "duplicate", label: "Duplicate of…" },
          ]}
          selected={[]}
          onSelect={(value) => setAdding(value as NewRelation)}
          defaultOpen={asked}
          onOpenChange={(open) => !open && onAsked()}
          triggerClassName="size-7 justify-center rounded-md text-muted hover:text-fg"
        >
          <Plus size={15} strokeWidth={1.75} aria-hidden="true" />
        </Picker>
      }
    >
      {mine.map((relation) => {
        const { label, other } = describe(relation);
        if (!other) return null;
        return (
          <div
            key={relation.id}
            className="group -mx-2 flex h-8 items-center gap-2 rounded-md px-2 hover:bg-hover"
          >
            <span className="w-24 shrink-0 text-xs text-muted">{label}</span>
            <Link
              to="/$workspace/issue/$identifier"
              params={{ workspace: workspace.url_key, identifier: identifier(other, teamsById) }}
              className="min-w-0 flex-1 truncate"
            >
              <span className="mr-2 font-mono text-xs text-muted">
                {identifier(other, teamsById)}
              </span>
              {other.title}
            </Link>
            <button
              type="button"
              aria-label={`Remove relation to ${other.title}`}
              className="invisible text-xs text-faint group-hover:visible hover:text-danger-ink"
              onClick={() =>
                setConfirmation({
                  title: `Remove “${label} ${identifier(other, teamsById)}”?`,
                  body: "The two issues are no longer related. The issues themselves stay.",
                  action: "Remove relation",
                  onConfirm: () => sync.delete("IssueRelation", relation.id),
                })
              }
            >
              Remove
            </button>
          </div>
        );
      })}
      {adding && (
        <div className="flex items-center gap-2 py-1">
          <span className="text-xs text-muted">
            {adding === "blocked_by" ? "Blocked by" : RELATION_NAMES[adding][0]}
          </span>
          <Picker
            label="Issue"
            options={candidates.map((i) => ({
              value: i.id,
              label: `${identifier(i, teamsById)} ${i.title}`,
            }))}
            selected={[]}
            onSelect={(otherId) => {
              sync.create(
                "IssueRelation",
                adding === "blocked_by"
                  ? { issue_id: otherId, related_issue_id: issue.id, type: "blocks" }
                  : { issue_id: issue.id, related_issue_id: otherId, type: adding },
              );
              setAdding(null);
            }}
            defaultOpen
            triggerClassName={chip}
          >
            Choose an issue…
          </Picker>
          <button
            type="button"
            className="text-xs text-faint hover:text-fg"
            onClick={() => setAdding(null)}
          >
            Cancel
          </button>
        </div>
      )}
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </Section>
  );
}

/** Comments and changes, oldest first, then a box for a new comment. */
function Activity({
  issue,
  onAddToDescription,
}: {
  issue: Issue;
  onAddToDescription: (comment: Comment) => void;
}) {
  const { sync } = useWorkspace();
  const comments = useRecords("Comment");
  const users = useRecords("User");
  const usersById = useMemo(() => byId(users), [users]);
  const history = useIssueHistory(issue);
  const thread = useMemo(() => {
    const items: ({ at: string; comment: Comment } | { at: string; event: IssueEvent })[] = [
      ...comments
        .filter((c) => c.issue_id === issue.id)
        .map((c) => ({ at: c.created_at, comment: c })),
      ...(history ?? []).map((e) => ({ at: e.at, event: e })),
    ];
    return items.sort((a, b) => a.at.localeCompare(b.at));
  }, [comments, history, issue.id]);
  const [body, setBody] = useState("");
  // Sending mid-upload would leave the file out.
  const [uploading, setUploading] = useState(false);

  const send = () => {
    if (!body.trim() || uploading) return;
    sync.create("Comment", { issue_id: issue.id, body: body.trim() });
    setBody("");
  };

  return (
    <Section title="Activity">
      <div className="flex flex-col gap-2">
        {thread.map((item) =>
          "comment" in item ? (
            <CommentView
              key={item.comment.id}
              comment={item.comment}
              onAddToDescription={() => onAddToDescription(item.comment)}
            />
          ) : (
            <EventLine
              key={`${item.at}-${item.event.field}`}
              event={item.event}
              users={usersById}
            />
          ),
        )}
        <div className="rounded-md ui-field">
          <MentionTextarea
            issueId={issue.id}
            aria-label="Write a comment"
            placeholder="Leave a comment… (@ to mention, ⌘↵ to send)"
            value={body}
            onChange={setBody}
            onUploading={setUploading}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                send();
              }
            }}
            className="field-sizing-content min-h-16 w-full resize-none bg-transparent px-3 py-2 outline-none placeholder:text-faint"
          />
          <div className="flex justify-end px-2 pb-2">
            {/* The screen's one primary belongs to its state (Accept, the pull request). */}
            <Button variant="secondary" disabled={!body.trim() || uploading} onClick={send}>
              Comment
            </Button>
          </div>
        </div>
      </div>
    </Section>
  );
}

function CommentView({
  comment,
  onAddToDescription,
}: {
  comment: Comment;
  onAddToDescription: () => void;
}) {
  const { sync, me } = useWorkspace();
  const author = useUsers().find((u) => u.id === comment.author_id);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(comment.body);
  const [uploading, setUploading] = useState(false);
  const mine = comment.author_id === me.user.id;
  // Only its author edits a comment; admins may delete anyone's.
  const isAdmin = useIsAdmin();
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  return (
    <article id={`comment-${comment.id}`} className="group flex scroll-mt-6 gap-3">
      <Avatar user={author} size="size-6" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2 text-xs">
          <span className="font-medium">{author?.name ?? "Former member"}</span>
          {/* Said like the changes around it; the exact time on hover. */}
          <time
            className="text-faint"
            dateTime={comment.created_at}
            title={new Date(comment.created_at).toLocaleString(undefined, {
              month: "short",
              day: "numeric",
              hour: "numeric",
              minute: "2-digit",
            })}
          >
            {timeAgo(comment.created_at)}
          </time>
          {!editing && (
            <span className="invisible ml-auto flex gap-2 group-focus-within:visible group-hover:visible">
              {/* The description is the issue's scope; comments are discussion until adopted. */}
              <button
                type="button"
                className="text-faint hover:text-fg"
                onClick={onAddToDescription}
              >
                Add to description
              </button>
              {mine && (
                <button
                  type="button"
                  className="text-faint hover:text-fg"
                  onClick={() => setEditing(true)}
                >
                  Edit
                </button>
              )}
              {(mine || isAdmin) && (
                <button
                  type="button"
                  className="text-faint hover:text-danger-ink"
                  onClick={() =>
                    setConfirmation({
                      title: "Delete this comment?",
                      body: "It's deleted for everyone. This can't be undone.",
                      action: "Delete comment",
                      onConfirm: () => sync.delete("Comment", comment.id),
                    })
                  }
                >
                  Delete
                </button>
              )}
            </span>
          )}
        </div>
        {editing ? (
          <div className="mt-1 flex flex-col gap-2">
            <MentionTextarea
              issueId={comment.issue_id}
              aria-label="Edit comment"
              value={draft}
              onChange={setDraft}
              onUploading={setUploading}
              className="field-sizing-content min-h-12 resize-none rounded-md ui-field px-2 py-1 outline-none"
            />
            <div className="flex gap-2">
              <Button
                variant="secondary"
                disabled={!draft.trim() || uploading}
                onClick={() => {
                  sync.update("Comment", comment.id, { body: draft.trim() });
                  setEditing(false);
                }}
              >
                Save
              </Button>
              <Button variant="secondary" onClick={() => setEditing(false)}>
                Cancel
              </Button>
            </div>
          </div>
        ) : (
          <div className="mt-1">
            <Markdown text={comment.body} />
          </div>
        )}
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </article>
  );
}

/** Asked for from "⋯": opens at once, and goes away again when closed. */
function ParentPicker({ issue, onClose }: { issue: Issue; onClose: () => void }) {
  const { sync } = useWorkspace();
  const issues = useRecords("Issue");
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const parent = issues.find((i) => i.id === issue.parent_id);
  // Not itself and not one of its own descendants.
  const descendants = new Set<string>([issue.id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const i of issues) {
      if (i.parent_id && descendants.has(i.parent_id) && !descendants.has(i.id)) {
        descendants.add(i.id);
        grew = true;
      }
    }
  }
  return (
    <Picker
      label="Parent issue"
      options={[
        { value: "", label: "No parent" },
        ...issues
          .filter((i) => !descendants.has(i.id))
          .map((i) => ({ value: i.id, label: `${identifier(i, teamsById)} ${i.title}` })),
      ]}
      selected={[issue.parent_id ?? ""]}
      onSelect={(value) => sync.update("Issue", issue.id, { parent_id: value || null })}
      defaultOpen
      onOpenChange={(open) => !open && onClose()}
      triggerClassName={chip}
    >
      <span className={`max-w-48 truncate ${parent ? "" : "text-faint"}`}>
        {parent ? `Sub-issue of ${identifier(parent, teamsById)}` : "Parent issue"}
      </span>
    </Picker>
  );
}

const LINK_STATES: Record<string, { label: string; className: string }> = {
  open: { label: "Open", className: "text-muted" },
  merged: { label: "Merged", className: "text-muted" },
  closed: { label: "Closed", className: "text-faint" },
  pushed: { label: "Commit", className: "text-muted" },
};

/** Pull requests and commits that mention the issue. */
function Links({ issue }: { issue: Issue }) {
  const links = useRecords("IssueLink")
    .filter((l) => l.issue_id === issue.id)
    .sort((a, b) => b.created_at.localeCompare(a.created_at));
  if (links.length === 0) return null;
  return <LinkRows links={links} />;
}

function LinkRows({ links }: { links: IssueLink[] }) {
  // Whether Forge's look at Forgejo is recent enough to vouch for its rows.
  const { hosts, now } = useCodeHosts();
  // Said on each row only when they're in more than one repository.
  const oneRepository = links.every((l) => l.repository === links[0]?.repository);
  return (
    <Section title="Pull requests and commits">
      {links.map((link) => {
        const state = LINK_STATES[link.state];
        const note = hosts && pullRequestNote(link, hosts, now);
        return (
          <a
            key={link.id}
            href={link.url}
            target="_blank"
            rel="noreferrer"
            title={link.repository}
            className="-mx-2 flex h-8 items-center gap-2 rounded-md px-2 hover:bg-hover"
          >
            <span className={`w-14 shrink-0 text-xs ${state?.className ?? ""}`}>
              {state?.label}
            </span>
            <span className="min-w-0 flex-1 truncate">{link.title}</span>
            {note && (
              <span
                className={`shrink-0 text-xs ${note.tone === "danger" ? "text-danger-ink" : "text-muted"}`}
              >
                {note.text}
              </span>
            )}
            <span className="shrink-0 font-mono text-xs text-faint">
              {!oneRepository && link.repository}
              {link.number
                ? `#${link.number}`
                : link.sha
                  ? `${oneRepository ? "" : "@"}${link.sha.slice(0, 7)}`
                  : ""}
            </span>
          </a>
        );
      })}
    </Section>
  );
}
