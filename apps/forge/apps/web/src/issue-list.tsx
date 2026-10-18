import type { Issue, Service, WorkflowState } from "@forge/types";
import { useNavigate } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { BulkBar } from "./bulk";
import {
  byId,
  byPriority,
  formatDate,
  identifier,
  STATE_TYPE_ORDER,
  sharedFields,
  useStates,
} from "./data";
import { ProjectIcon, ServiceIcon, StateIcon } from "./icons";
import {
  AssigneePicker,
  LabelChip,
  PriorityPicker,
  ProjectPicker,
  ServicePicker,
  StatusPicker,
} from "./issue-fields";
import { isTyping, useNewIssue } from "./new-issue";
import { buttonClass } from "./ui";
import { useIsPending, useRecords, useWorkspace } from "./workspace";

/** What a list's sections are: a status each, or a service each. */
export type GroupBy = "status" | "service";

interface Group {
  key: string;
  heading: ReactNode;
  /** The rows at the top level; sub-issues fold under their parents. */
  issues: Issue[];
}

/** Work in motion first; within a type, later steps first (In Review before In Progress). */
function statusRank(state: WorkflowState | undefined) {
  return state ? STATE_TYPE_ORDER.indexOf(state.type) * 1000 - state.position : 1e9;
}

/**
 * Issues grouped by status, work in motion first, or by service, by name.
 * Statuses with the same name in different teams share a group. A
 * sub-issue whose parent is in the list folds under it, and the parent
 * opens to show it. What every issue in the list has in common (its one
 * project or service) is left out of the rows: show it once by the title
 * with `SharedFields`. j/k or the arrow keys move, → and ← open and close
 * a parent, Enter opens an issue, x selects for changing several at once
 * (Esc clears).
 */
export function IssueList({
  issues,
  empty,
  showProject = true,
  groupBy = "status",
}: {
  issues: Issue[];
  empty: string;
  showProject?: boolean;
  groupBy?: GroupBy;
}) {
  const states = useStates();
  const navigate = useNavigate();
  const { workspace } = useWorkspace();
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const services = useRecords("Service");
  const projects = useRecords("Project");
  const allIssues = useRecords("Issue");
  /** Done and total sub-issues per parent. */
  const subIssues = useMemo(() => {
    const counts = new Map<string, { done: number; total: number }>();
    for (const issue of allIssues) {
      if (!issue.parent_id) continue;
      const count = counts.get(issue.parent_id) ?? { done: 0, total: 0 };
      count.total += 1;
      if (issue.completed_at) count.done += 1;
      counts.set(issue.parent_id, count);
    }
    return counts;
  }, [allIssues]);
  const [focused, setFocused] = useState(0);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const lastPicked = useRef<number | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const shared = useMemo(() => sharedFields(issues), [issues]);
  const projectChips = showProject && shared.project === undefined;
  const serviceChips = groupBy !== "service" && shared.service === undefined;

  const byStatus = useMemo(
    () => (a: Issue, b: Issue) =>
      statusRank(states.get(a.state_id)) - statusRank(states.get(b.state_id)) || byPriority(a, b),
    [states],
  );

  /** Sub-issues in the list, by parent; and the issues without a parent in it. */
  const { folded, top } = useMemo(() => {
    const listed = new Set(issues.map((i) => i.id));
    const folded = new Map<string, Issue[]>();
    const top: Issue[] = [];
    for (const issue of issues) {
      if (issue.parent_id && listed.has(issue.parent_id)) {
        folded.set(issue.parent_id, [...(folded.get(issue.parent_id) ?? []), issue]);
      } else {
        top.push(issue);
      }
    }
    for (const children of folded.values()) children.sort(byStatus);
    return { folded, top };
  }, [issues, byStatus]);

  const groups = useMemo((): Group[] => {
    if (groupBy === "service") {
      const byService = new Map(services.map((s) => [s.id, s]));
      const map = new Map<string, Group>();
      for (const issue of top) {
        const service = issue.service_id ? byService.get(issue.service_id) : undefined;
        const key = service ? service.id : "";
        const group: Group = map.get(key) ?? {
          key,
          heading: <ServiceHeading service={service} />,
          issues: [],
        };
        group.issues.push(issue);
        map.set(key, group);
      }
      // By name, "No service" last.
      const last = (g: Group) => Number(g.key === "");
      const nameOf = (g: Group) => byService.get(g.key)?.name ?? "";
      return [...map.values()]
        .sort((a, b) => last(a) - last(b) || nameOf(a).localeCompare(nameOf(b)))
        .map((g) => ({ ...g, issues: g.issues.sort(byStatus) }));
    }
    const map = new Map<string, Group & { state: WorkflowState }>();
    for (const issue of top) {
      const state = states.get(issue.state_id);
      if (!state) continue;
      const key = `${state.type}:${state.name}`;
      const group: Group & { state: WorkflowState } = map.get(key) ?? {
        key,
        state,
        heading: (
          <>
            <StateIcon state={state} />
            <span className="font-medium">{state.name}</span>
          </>
        ),
        issues: [],
      };
      group.issues.push(issue);
      map.set(key, group);
    }
    return [...map.values()]
      .sort((a, b) => statusRank(a.state) - statusRank(b.state))
      .map((g) => ({ ...g, issues: g.issues.sort(byPriority) }));
  }, [groupBy, top, states, services, byStatus]);

  /** Each group's rows, with how deep each sits under its parent. */
  const sections = useMemo(
    () =>
      groups.map((group) => {
        const rows: { issue: Issue; depth: number }[] = [];
        const add = (issue: Issue, depth: number) => {
          rows.push({ issue, depth });
          if (!expanded.has(issue.id)) return;
          for (const child of folded.get(issue.id) ?? []) add(child, depth + 1);
        };
        for (const issue of group.issues) add(issue, 0);
        // A project every row of the group shares is said once, in its heading.
        const project = sharedFields(rows.map((r) => r.issue)).project;
        return { ...group, rows, project };
      }),
    [groups, folded, expanded],
  );

  const ordered = useMemo(
    () => sections.flatMap((section) => section.rows.map((r) => r.issue)),
    [sections],
  );
  // Only issues still shown stay selected.
  const selected = useMemo(() => ordered.filter((i) => picked.has(i.id)), [ordered, picked]);

  // For key handlers that run before the next render.
  const current = useRef({ ordered, focused });
  current.current = { ordered, focused };

  /** Selects or unselects an issue; with `range`, everything since the last one. */
  const toggle = (index: number, range = false) => {
    const list = current.current.ordered;
    const issue = list[index];
    if (!issue) return;
    const from = range && lastPicked.current !== null ? lastPicked.current : index;
    lastPicked.current = index;
    setPicked((previous) => {
      const next = new Set(previous);
      const on = !next.has(issue.id);
      for (let i = Math.min(from, index); i <= Math.max(from, index); i++) {
        const id = list[i]?.id;
        if (id) on ? next.add(id) : next.delete(id);
      }
      return next;
    });
  };

  /** Opens or closes a parent's sub-issues. */
  const fold = (id: string, open: boolean) =>
    setExpanded((previous) => {
      const next = new Set(previous);
      open ? next.add(id) : next.delete(id);
      return next;
    });

  const open = (issue: Issue | undefined) => {
    if (!issue?.number) return;
    void navigate({
      to: "/$workspace/issue/$identifier",
      params: { workspace: workspace.url_key, identifier: identifier(issue, teamsById) },
    });
  };

  // Keyboard navigation, while no field or menu has focus.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (isTyping(event.target) || event.metaKey || event.ctrlKey || event.altKey) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
      // Letters whatever the case (Caps Lock, Shift).
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      if (key === "j" || event.key === "ArrowDown") {
        event.preventDefault();
        setFocused((i) => (current.current.focused = Math.min(i + 1, ordered.length - 1)));
      } else if (key === "k" || event.key === "ArrowUp") {
        event.preventDefault();
        setFocused((i) => (current.current.focused = Math.max(i - 1, 0)));
      } else if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        const issue = ordered[focused];
        const opening = event.key === "ArrowRight";
        if (!issue || !folded.has(issue.id) || expanded.has(issue.id) === opening) return;
        event.preventDefault();
        fold(issue.id, opening);
      } else if (event.key === "Enter" && event.target === document.body) {
        event.preventDefault();
        open(ordered[focused]);
      } else if (key === "x") {
        event.preventDefault();
        toggle(current.current.focused);
      } else if (event.key === "Escape" && picked.size > 0) {
        event.preventDefault();
        setPicked(new Set());
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${focused}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [focused]);

  if (issues.length === 0) {
    return <EmptyState text={empty} />;
  }

  let index = -1;
  return (
    <div ref={listRef} className="px-2 pb-2">
      {sections.map((group) => {
        const project =
          projectChips && group.project ? projects.find((p) => p.id === group.project) : undefined;
        return (
          <section key={group.key}>
            <h2 className="sticky top-0 z-10 flex h-9 items-center gap-2 bg-band px-4 text-xs">
              {group.heading}
              <span className="text-faint">{group.issues.length}</span>
              {project && (
                <span
                  className="ml-auto flex min-w-0 items-center gap-1.5 pr-8 text-faint"
                  title={`Every issue here is in ${project.name}`}
                >
                  <ProjectIcon />
                  <span className="truncate">{project.name}</span>
                </span>
              )}
            </h2>
            <ul>
              {group.rows.map(({ issue, depth }) => {
                index += 1;
                const position = index;
                const children = folded.get(issue.id);
                return (
                  <IssueRow
                    key={issue.id}
                    issue={issue}
                    index={position}
                    depth={depth}
                    focused={position === focused}
                    identifier={identifier(issue, teamsById)}
                    // A group that shares a project names it once; one where
                    // none has a project still lets each row set one.
                    showProject={projectChips && !group.project}
                    showService={serviceChips}
                    sharedLabels={shared.labels}
                    subIssues={subIssues.get(issue.id)}
                    fold={
                      children
                        ? {
                            open: expanded.has(issue.id),
                            toggle: () => fold(issue.id, !expanded.has(issue.id)),
                          }
                        : undefined
                    }
                    onOpen={() => open(issue)}
                    onFocus={() => setFocused(position)}
                    selected={picked.has(issue.id)}
                    selecting={selected.length > 0}
                    onSelect={(range) => toggle(position, range)}
                  />
                );
              })}
            </ul>
          </section>
        );
      })}
      {selected.length > 0 && <BulkBar issues={selected} onClear={() => setPicked(new Set())} />}
    </div>
  );
}

function ServiceHeading({ service }: { service: Service | undefined }) {
  return (
    <>
      <ServiceIcon service={service} />
      <span className={service ? "font-medium" : "font-medium text-muted"}>
        {service?.name ?? "No service"}
      </span>
    </>
  );
}

/** A small arrow inside the sub-issue chip: right when closed, down when open. */
function Disclosure({ open }: { open: boolean }) {
  return (
    <ChevronRight
      size={12}
      strokeWidth={1.75}
      aria-hidden="true"
      className={`shrink-0 transition-transform ${open ? "rotate-90" : ""}`}
    />
  );
}

function IssueRow({
  issue,
  index,
  depth,
  focused,
  identifier,
  showProject,
  showService,
  sharedLabels,
  subIssues,
  fold,
  onOpen,
  onFocus,
  selected,
  selecting,
  onSelect,
}: {
  issue: Issue;
  index: number;
  /** How far under its parent it sits: 0 for a row of its own. */
  depth: number;
  focused: boolean;
  identifier: string;
  showProject: boolean;
  showService: boolean;
  /** Labels every issue in the list has, shown by the title instead. */
  sharedLabels: string[];
  subIssues: { done: number; total: number } | undefined;
  /** For a parent whose sub-issues are in the list, folded under it. */
  fold: { open: boolean; toggle: () => void } | undefined;
  onOpen: () => void;
  onFocus: () => void;
  selected: boolean;
  /** Whether any issue is selected, which shows every row's checkbox. */
  selecting: boolean;
  /** `range`: select everything since the last one (shift-click). */
  onSelect: (range: boolean) => void;
}) {
  const pending = useIsPending("Issue", issue.id);
  const labels = useRecords("Label");
  const chosen = labels.filter(
    (l) => issue.label_ids.includes(l.id) && !sharedLabels.includes(l.id),
  );
  const progress = subIssues && `${subIssues.done}/${subIssues.total}`;

  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: rows are reached with j/k and opened with Enter
    <li
      data-index={index}
      onClick={onOpen}
      onMouseMove={onFocus}
      style={depth > 0 ? { paddingLeft: `${0.5 + depth * 1.5}rem` } : undefined}
      className={`group flex h-10 cursor-default items-center gap-2 rounded-md pr-4 pl-2 ${selected ? "bg-selected" : focused ? "bg-hover" : ""}`}
    >
      <input
        type="checkbox"
        aria-label={`Select ${identifier}`}
        checked={selected}
        onClick={(e) => {
          e.stopPropagation();
          onSelect(e.shiftKey);
        }}
        onChange={() => {}}
        className={`size-3.5 shrink-0 accent-(--fg) ${selected || selecting ? "" : "opacity-0 group-hover:opacity-100 focus-visible:opacity-100"}`}
      />
      <PriorityPicker issue={issue} />
      <span className="w-16 shrink-0 font-mono text-xs text-muted">{identifier}</span>
      <StatusPicker issue={issue} />
      <span className="min-w-0 truncate">
        <a
          href={`./issue/${identifier}`}
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onOpen();
          }}
          className="outline-none focus-visible:underline"
        >
          {issue.title}
        </a>
      </span>
      {fold ? (
        <button
          type="button"
          aria-expanded={fold.open}
          title={fold.open ? "Hide sub-issues (←)" : "Show sub-issues (→)"}
          onClick={(e) => {
            e.stopPropagation();
            fold.toggle();
          }}
          className="flex h-7 shrink-0 items-center gap-1 rounded-md px-1.5 text-xs text-faint outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent"
        >
          <Disclosure open={fold.open} />
          <span className="sr-only">Sub-issues done:</span>
          {progress}
        </button>
      ) : (
        progress && (
          <span className="shrink-0 text-xs text-faint" title="Sub-issues done">
            {progress}
          </span>
        )
      )}
      {pending && (
        <span className="shrink-0 text-xs text-faint" title="Not yet confirmed by the server">
          Saving…
        </span>
      )}
      <span className="ml-auto flex shrink-0 items-center gap-1.5">
        {showService && <ServicePicker issue={issue} variant="compact" />}
        {chosen.slice(0, 3).map((l) => (
          <LabelChip key={l.id} name={l.name} color={l.color} />
        ))}
        {showProject && <ProjectPicker issue={issue} />}
        {issue.due_date && (
          <span className="text-xs text-muted" title="Due date">
            {formatDate(issue.due_date)}
          </span>
        )}
        <AssigneePicker issue={issue} />
      </span>
    </li>
  );
}

/**
 * What every issue in a list has in common (its one service, its one
 * project, the labels all of them carry), shown once by the list's title
 * since the rows leave it out. Fields the page itself is about go in
 * `except`.
 */
export function SharedFields({
  issues,
  except = [],
}: {
  issues: Issue[];
  except?: ("project" | "service")[];
}) {
  const projects = useRecords("Project");
  const services = useRecords("Service");
  const labels = useRecords("Label");
  const shared = useMemo(() => sharedFields(issues), [issues]);
  const chips: ReactNode[] = [];
  if (shared.service !== undefined && !except.includes("service")) {
    const service = services.find((s) => s.id === shared.service);
    const name = service?.name;
    chips.push(
      <SharedChip
        key="service"
        icon={<ServiceIcon service={service} />}
        name={name ?? "No service"}
        empty={!name}
        title={name ? `Every issue here is about ${name}` : "None of these issues has a service"}
      />,
    );
  }
  if (shared.project !== undefined && !except.includes("project")) {
    const name = projects.find((p) => p.id === shared.project)?.name;
    chips.push(
      <SharedChip
        key="project"
        icon={<ProjectIcon />}
        name={name ?? "No project"}
        empty={!name}
        title={name ? `Every issue here is in ${name}` : "None of these issues is in a project"}
      />,
    );
  }
  for (const label of labels.filter((l) => shared.labels.includes(l.id))) {
    chips.push(<LabelChip key={label.id} name={label.name} color={label.color} />);
  }
  if (chips.length === 0) return null;
  return <span className="flex min-w-0 items-center gap-1.5">{chips}</span>;
}

function SharedChip({
  icon,
  name,
  empty,
  title,
}: {
  icon: ReactNode;
  name: string;
  empty: boolean;
  title: string;
}) {
  return (
    <span
      title={title}
      className={`flex min-w-0 items-center gap-1.5 text-xs ${empty ? "text-faint" : "text-muted"}`}
    >
      {icon}
      <span className="truncate">{name}</span>
    </span>
  );
}

/** `offerCreate`: where a new issue would show up in this list. */
export function EmptyState({ text, offerCreate = true }: { text: string; offerCreate?: boolean }) {
  const newIssue = useNewIssue();
  return (
    <div className="flex flex-col items-center gap-3 px-4 py-20 text-center text-muted">
      <p>{text}</p>
      {offerCreate && (
        <button type="button" onClick={() => newIssue()} className={buttonClass()}>
          Create an issue <kbd className="text-on-primary/70">C</kbd>
        </button>
      )}
    </div>
  );
}
