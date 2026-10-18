// A team's issues as a board: a column per status, in workflow order. Drag a
// card, or press Shift with ← or →, to move it to another status. The arrow
// keys (or h j k l) move between cards; Enter opens one.
import type { Issue, WorkflowState } from "@forge/types";
import { useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { byId, byPriority, formatDate, identifier } from "./data";
import { StateIcon } from "./icons";
import { AssigneePicker, LabelChip, PriorityPicker } from "./issue-fields";
import { isTyping, useNewIssue } from "./new-issue";
import { useIsPending, useRecords, useWorkspace } from "./workspace";

const DRAG_TYPE = "application/x-forge-issue";

export function IssueBoard({ issues, states }: { issues: Issue[]; states: WorkflowState[] }) {
  const { sync, workspace } = useWorkspace();
  const navigate = useNavigate();
  const teams = useRecords("Team");
  const teamsById = useMemo(() => byId(teams), [teams]);
  const columns = useMemo(
    () =>
      states.map((state) => ({
        state,
        issues: issues.filter((i) => i.state_id === state.id).sort(byPriority),
      })),
    [issues, states],
  );
  const [focus, setFocus] = useState<{ column: number; row: number }>({ column: 0, row: 0 });
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const boardRef = useRef<HTMLDivElement>(null);
  const focused = columns[focus.column]?.issues[focus.row];

  const open = (issue: Issue | undefined) => {
    if (!issue?.number) return;
    void navigate({
      to: "/$workspace/issue/$identifier",
      params: { workspace: workspace.url_key, identifier: identifier(issue, teamsById) },
    });
  };

  const move = (issue: Issue, state: WorkflowState) => {
    if (issue.state_id !== state.id) sync.update("Issue", issue.id, { state_id: state.id });
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (isTyping(event.target) || event.metaKey || event.ctrlKey || event.altKey) return;
      if (document.querySelector("[role=dialog],[role=alertdialog],[role=menu]")) return;
      const step = (column: number) => {
        const next = Math.max(0, Math.min(columns.length - 1, column));
        const length = columns[next]?.issues.length ?? 0;
        return { column: next, row: Math.max(0, Math.min(focus.row, length - 1)) };
      };
      // Letters whatever the case (Caps Lock, Shift), as the shortcuts list shows them.
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      if (event.shiftKey && (key === "ArrowLeft" || key === "ArrowRight")) {
        const target = columns[focus.column + (key === "ArrowLeft" ? -1 : 1)];
        if (!focused || !target) return;
        event.preventDefault();
        move(focused, target.state);
        // Follow the card to its new column.
        const row = [...target.issues, focused].sort(byPriority).indexOf(focused);
        setFocus({ column: focus.column + (key === "ArrowLeft" ? -1 : 1), row });
      } else if (key === "ArrowLeft" || key === "h") {
        event.preventDefault();
        setFocus(step(focus.column - 1));
      } else if (key === "ArrowRight" || key === "l") {
        event.preventDefault();
        setFocus(step(focus.column + 1));
      } else if (key === "ArrowDown" || key === "j") {
        event.preventDefault();
        const length = columns[focus.column]?.issues.length ?? 0;
        setFocus({ ...focus, row: Math.min(focus.row + 1, length - 1) });
      } else if (key === "ArrowUp" || key === "k") {
        event.preventDefault();
        setFocus({ ...focus, row: Math.max(focus.row - 1, 0) });
      } else if (key === "Enter" && event.target === document.body) {
        event.preventDefault();
        open(focused);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  useEffect(() => {
    boardRef.current
      ?.querySelector(`[data-card="${focus.column}-${focus.row}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [focus]);

  return (
    <div ref={boardRef} className="flex h-full gap-3 overflow-x-auto p-3">
      {columns.map(({ state, issues: cards }, column) => (
        <BoardColumn
          key={state.id}
          state={state}
          count={cards.length}
          highlighted={dropTarget === state.id}
          onDragOver={(over) => setDropTarget(over ? state.id : null)}
          onDrop={(id) => {
            setDropTarget(null);
            const issue = issues.find((i) => i.id === id);
            if (issue) move(issue, state);
          }}
        >
          {cards.map((issue, row) => (
            <Card
              key={issue.id}
              issue={issue}
              position={`${column}-${row}`}
              identifier={identifier(issue, teamsById)}
              focused={focus.column === column && focus.row === row}
              onFocus={() => setFocus({ column, row })}
              onOpen={() => open(issue)}
            />
          ))}
        </BoardColumn>
      ))}
    </div>
  );
}

function BoardColumn({
  state,
  count,
  highlighted,
  onDragOver,
  onDrop,
  children,
}: {
  state: WorkflowState;
  count: number;
  highlighted: boolean;
  onDragOver: (over: boolean) => void;
  onDrop: (issueId: string) => void;
  children: React.ReactNode;
}) {
  const newIssue = useNewIssue();
  return (
    <section
      aria-label={state.name}
      onDragOver={(event) => {
        if (!event.dataTransfer.types.includes(DRAG_TYPE)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "move";
        onDragOver(true);
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onDragOver(false);
      }}
      onDrop={(event) => {
        event.preventDefault();
        const id = event.dataTransfer.getData(DRAG_TYPE);
        if (id) onDrop(id);
      }}
      className={`flex w-72 shrink-0 flex-col rounded-lg ${highlighted ? "bg-highlight" : ""}`}
    >
      <h2 className="flex h-9 shrink-0 items-center gap-2 px-3 text-xs">
        <StateIcon state={state} />
        <span className="font-medium">{state.name}</span>
        {count > 0 && <span className="text-faint">{count}</span>}
        <button
          type="button"
          aria-label={`New issue in ${state.name}`}
          title={`New issue in ${state.name}`}
          onClick={() => newIssue({ teamId: state.team_id, stateId: state.id })}
          className="ui-button ui-icon-button ml-auto"
        >
          <Plus size={15} strokeWidth={1.75} aria-hidden="true" />
        </button>
      </h2>
      <div className="flex min-h-16 flex-1 flex-col gap-2 overflow-y-auto px-2 pb-2">
        {children}
      </div>
    </section>
  );
}

function Card({
  issue,
  position,
  identifier,
  focused,
  onFocus,
  onOpen,
}: {
  issue: Issue;
  position: string;
  identifier: string;
  focused: boolean;
  onFocus: () => void;
  onOpen: () => void;
}) {
  const pending = useIsPending("Issue", issue.id);
  const labels = useRecords("Label");
  const chosen = labels.filter((l) => issue.label_ids.includes(l.id));
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: cards are reached with the arrow keys and opened with Enter
    <article
      data-card={position}
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData(DRAG_TYPE, issue.id);
        event.dataTransfer.effectAllowed = "move";
      }}
      onClick={onOpen}
      onMouseMove={onFocus}
      className={`flex cursor-default flex-col gap-1.5 rounded-md p-2.5 ${focused ? "bg-selected" : "bg-surface hover:bg-hover"}`}
    >
      <div className="flex items-center gap-2 text-xs text-muted">
        <span className="font-mono">{identifier}</span>
        {pending && <span className="text-faint">Saving…</span>}
        <span className="ml-auto">
          <AssigneePicker issue={issue} />
        </span>
      </div>
      <p className="line-clamp-3 leading-snug">{issue.title}</p>
      <div className="flex flex-wrap items-center gap-1.5">
        <PriorityPicker issue={issue} />
        {chosen.slice(0, 3).map((l) => (
          <LabelChip key={l.id} name={l.name} color={l.color} />
        ))}
        {issue.due_date && (
          <span className="text-xs text-muted" title="Due date">
            {formatDate(issue.due_date)}
          </span>
        )}
      </div>
    </article>
  );
}
