import type { Issue, StateType } from "@forge/types";
import { Link, useParams } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { byId, identifier, useStates, useTeamStates, useTeams } from "./data";
import { FilterBar, FilterButton } from "./filters";
import { StateIcon } from "./icons";
import { IssueBoard } from "./issue-board";
import { applyFilters, type Filters } from "./issue-filter";
import { EmptyState, type GroupBy, IssueList, SharedFields } from "./issue-list";
import { PageHeader } from "./layout";
import { useNewIssue } from "./new-issue";
import { NewButton } from "./page-parts";
import { Redirect } from "./redirect";
import { SuggestionCard, TriageSuggestion, useOpenSuggestions } from "./suggestions";
import { Button } from "./ui";
import { useRecords, useWorkspace } from "./workspace";

/** `/$workspace`: opens the first team's issues. */
export function WorkspaceHome() {
  const { workspace } = useWorkspace();
  const teams = useTeams();
  const first = teams[0];
  if (!first) return <PageHeader title="Issues" />;
  return (
    <Redirect
      to="/$workspace/team/$team"
      params={{ workspace: workspace.url_key, team: first.key }}
      replace
    />
  );
}

const VIEWS: { id: string; label: string; types: StateType[] }[] = [
  {
    id: "all",
    label: "All issues",
    types: ["backlog", "unstarted", "started", "completed", "canceled"],
  },
  { id: "active", label: "Active", types: ["unstarted", "started"] },
  { id: "backlog", label: "Backlog", types: ["backlog"] },
];

const LAYOUTS = ["list", "board"] as const;
const GROUPINGS: readonly GroupBy[] = ["status", "service"];

export function TeamIssuesPage() {
  const { team: key } = useParams({ strict: false }) as { team: string };
  const team = useTeams().find((t) => t.key === key);
  const issues = useRecords("Issue");
  const states = useStates();
  const newIssue = useNewIssue();
  const [view, setView] = useState("active");
  const [layout, setLayout] = useRemembered("forge:layout", LAYOUTS, "list");
  const [groupBy, setGroupBy] = useRemembered("forge:group-by", GROUPINGS, "status");
  const [filters, setFilters] = useState<Filters>({});
  const types = VIEWS.find((v) => v.id === view)?.types ?? [];
  const teamStates = useTeamStates(team?.id);
  const columns = useMemo(
    () => teamStates.filter((s) => types.includes(s.type)),
    [teamStates, types],
  );

  const shown = useMemo(() => {
    const teamIssues = issues.filter((issue) => issue.team_id === team?.id);
    return applyFilters(
      teamIssues.filter((issue) => {
        const type = states.get(issue.state_id)?.type;
        return type !== undefined && types.includes(type);
      }),
      filters,
    );
  }, [issues, states, team, types, filters]);
  const filtering = Object.values(filters).some((values) => values.length > 0);

  if (!team) return <PageHeader title="Team not found" />;
  return (
    <>
      <PageHeader title={team.name} meta={<SharedFields issues={shown} />}>
        <ViewTabs view={view} onChange={setView} />
        <FilterButton filters={filters} onChange={setFilters} />
        {layout === "list" && (
          <Toggle
            label="Group by"
            showLabel
            options={GROUPINGS}
            value={groupBy}
            onChange={setGroupBy}
          />
        )}
        <Toggle label="Layout" options={LAYOUTS} value={layout} onChange={setLayout} />
        <NewButton label="New issue (C)" onClick={() => newIssue({ teamId: team.id })} />
      </PageHeader>
      <FilterBar filters={filters} onChange={setFilters} />
      {layout === "board" ? (
        <main className="min-h-0 flex-1">
          <IssueBoard issues={shown} states={columns} />
        </main>
      ) : (
        <main className="min-h-0 flex-1 overflow-y-auto">
          <IssueList
            issues={shown}
            groupBy={groupBy}
            empty={
              filtering ? "No issues match these filters." : `No issues in ${team.name} here yet.`
            }
          />
        </main>
      )}
    </>
  );
}

/** A choice remembered on this device; `fallback` until there is one, or without storage. */
function useRemembered<T extends string>(
  key: string,
  choices: readonly T[],
  fallback: T,
): [T, (value: T) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const saved = localStorage.getItem(key);
      return choices.find((choice) => choice === saved) ?? fallback;
    } catch {
      return fallback;
    }
  });
  return [
    value,
    (next) => {
      setValue(next);
      try {
        localStorage.setItem(key, next);
      } catch {
        // Not remembered; fine.
      }
    },
  ];
}

/** One of a few choices, pressed: list or board, grouped by status or service. */
function Toggle<T extends string>({
  label,
  showLabel = false,
  options,
  value,
  onChange,
}: {
  label: string;
  /** Show the label too, where the choices don't say enough by themselves. */
  showLabel?: boolean;
  options: readonly T[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      {showLabel && (
        <span className="text-xs whitespace-nowrap text-muted @max-3xl:hidden">{label}</span>
      )}
      <fieldset className="flex items-center gap-0.5" aria-label={label}>
        {options.map((option) => (
          <button
            key={option}
            type="button"
            aria-pressed={value === option}
            onClick={(event) => {
              onChange(option);
              // Hand the keyboard back to the list or board.
              event.currentTarget.blur();
            }}
            className={`h-6 rounded px-2 text-xs capitalize ${value === option ? "bg-hover text-fg" : "text-muted hover:text-fg"}`}
          >
            {option}
          </button>
        ))}
      </fieldset>
    </div>
  );
}

function ViewTabs({ view, onChange }: { view: string; onChange: (view: string) => void }) {
  return (
    <div role="tablist" className="flex items-center gap-1">
      {VIEWS.map((v) => (
        <button
          key={v.id}
          type="button"
          role="tab"
          aria-selected={view === v.id}
          onClick={() => onChange(v.id)}
          className={`h-7 rounded-md px-2.5 text-xs whitespace-nowrap ${view === v.id ? "bg-hover text-fg" : "text-muted hover:text-fg"}`}
        >
          {v.label}
        </button>
      ))}
    </div>
  );
}

export function MyIssuesPage() {
  const { me } = useWorkspace();
  const issues = useRecords("Issue");
  const states = useStates();
  const [filters, setFilters] = useState<Filters>({});
  const shown = useMemo(
    () =>
      applyFilters(
        issues.filter(
          (i) =>
            i.assignee_id === me.user.id &&
            !["triage", "canceled"].includes(states.get(i.state_id)?.type ?? ""),
        ),
        filters,
      ),
    [issues, states, me.user.id, filters],
  );
  return (
    <>
      <PageHeader title="My issues" meta={<SharedFields issues={shown} />}>
        <FilterButton filters={filters} onChange={setFilters} />
      </PageHeader>
      <FilterBar filters={filters} onChange={setFilters} />
      <main className="min-h-0 flex-1 overflow-y-auto">
        <IssueList
          issues={shown}
          empty={
            Object.values(filters).some((values) => values.length > 0)
              ? "No issues match these filters."
              : "Nothing here."
          }
        />
      </main>
    </>
  );
}

/**
 * New issues that need someone to accept them into a team's workflow, or
 * decline them. Later, AI suggestions about drift arrive here too.
 */
export function TriagePage() {
  const { sync, workspace } = useWorkspace();
  const issues = useRecords("Issue");
  const states = useStates();
  const allStates = useRecords("WorkflowState");
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const waiting = useMemo(
    () =>
      issues
        .filter((i) => states.get(i.state_id)?.type === "triage")
        .sort((a, b) => a.created_at.localeCompare(b.created_at)),
    [issues, states],
  );

  const suggestions = useOpenSuggestions();

  /** The team's first state of a type, e.g. where accepted issues go. */
  const stateOf = (issue: Issue, type: StateType) =>
    allStates
      .filter((s) => s.team_id === issue.team_id && s.type === type)
      .sort((a, b) => a.position - b.position)[0];

  return (
    <>
      <PageHeader title="Triage" />
      <main className="min-h-0 flex-1 overflow-y-auto">
        {suggestions.length > 0 && (
          <section className="flex flex-col gap-3 px-4 py-4">
            <h2 className="text-xs font-medium text-muted">Suggestions from Forge AI</h2>
            {suggestions.map((suggestion) => (
              <SuggestionCard key={suggestion.id} suggestion={suggestion} />
            ))}
          </section>
        )}
        {waiting.length === 0 && suggestions.length === 0 ? (
          // A clear Triage needs nothing; new issues go to Backlog, not here.
          <EmptyState
            offerCreate={false}
            text="Triage is clear. New issues and Forge AI's suggestions show up here."
          />
        ) : (
          <ul className="flex flex-col gap-0.5 px-2 py-2">
            {waiting.map((issue) => {
              const accept = stateOf(issue, "backlog") ?? stateOf(issue, "unstarted");
              const decline = stateOf(issue, "canceled");
              const triage = states.get(issue.state_id);
              return (
                <li
                  key={issue.id}
                  className="flex flex-col gap-2 rounded-lg px-2 py-2 hover:bg-hover"
                >
                  <div className="flex items-center gap-3">
                    {triage && <StateIcon state={triage} />}
                    <span className="w-16 shrink-0 font-mono text-xs text-muted">
                      {identifier(issue, teamsById)}
                    </span>
                    {/* Made offline, it opens once the server has given it a number. */}
                    {issue.number ? (
                      <Link
                        to="/$workspace/issue/$identifier"
                        params={{
                          workspace: workspace.url_key,
                          identifier: identifier(issue, teamsById),
                        }}
                        className="min-w-0 flex-1 truncate hover:underline"
                      >
                        {issue.title}
                      </Link>
                    ) : (
                      <span className="min-w-0 flex-1 truncate">{issue.title}</span>
                    )}
                    <Button
                      variant="secondary"
                      className="h-7 px-2 text-xs"
                      disabled={!accept}
                      onClick={() =>
                        accept && sync.update("Issue", issue.id, { state_id: accept.id })
                      }
                    >
                      Accept
                    </Button>
                    <Button
                      variant="secondary"
                      className="h-7 px-2 text-xs"
                      disabled={!decline}
                      onClick={() =>
                        decline && sync.update("Issue", issue.id, { state_id: decline.id })
                      }
                    >
                      Decline
                    </Button>
                  </div>
                  <div className="pl-7">
                    <TriageSuggestion issueId={issue.id} />
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </main>
    </>
  );
}
