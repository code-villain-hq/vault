import { memo, useMemo, useState } from "react";
import { goTo } from "../../lib/goto";
import { formatDuration, formatSpan, formatTokens } from "../../lib/runs";
import { sortRows, type SortKey, type TaskRow, type TaskState } from "../../lib/usage";

/** Rows drawn before "Show all". */
const FIRST = 25;

const STATE: Record<TaskState, { label: string; tone: string; hint: string }> = {
  open: { label: "open", tone: "ui-chip", hint: "Still in the app: its length is how long it has been open" },
  finished: { label: "finished", tone: "ui-status ui-status-ok", hint: "Finished with Finish task: start to finish" },
  deleted: { label: "deleted", tone: "ui-chip", hint: "Deleted: start to deletion" },
  removed: { label: "removed", tone: "ui-chip", hint: "Removed before tasks were kept as they ended, so its length is not known" },
  chat: { label: "chats", tone: "ui-chip", hint: "Every chat, together: they belong to no task" },
};

const COLUMNS: { key: SortKey; label: string; hint?: string }[] = [
  { key: "name", label: "Task" },
  { key: "took", label: "Took", hint: "From the task's start to its end, or until now while it is open" },
  { key: "worked", label: "Agents working", hint: "The time its agents spent working, where they say" },
  { key: "tokens", label: "Tokens" },
  { key: "asks", label: "Asked you", hint: "Times its agents stopped on something only you could answer" },
];

/**
 * Every task the period touched, with how long it took and what its agents
 * used (RUN-9). Sorted by any column, the most tokens first; an open task's
 * name opens it.
 */
export const TaskTable = memo(function TaskTable({ rows }: { rows: TaskRow[] }) {
  const [by, setBy] = useState<SortKey>("tokens");
  const [all, setAll] = useState(false);
  const sorted = useMemo(() => sortRows(rows, by), [rows, by]);
  const shown = all ? sorted : sorted.slice(0, FIRST);

  return (
    <section className="usage-tasks">
      <h3>Tasks</h3>
      <table className="run-table">
        <thead>
          <tr>
            {COLUMNS.map((c) => (
              <th key={c.key} title={c.hint} aria-sort={by === c.key ? (c.key === "name" ? "ascending" : "descending") : undefined}>
                <button type="button" className={`sort${by === c.key ? " on" : ""}`} onClick={() => setBy(c.key)}>
                  {c.label}
                </button>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map((r) => (
            <tr key={r.id}>
              <td className="usage-task">
                {r.state === "open" ? (
                  <button type="button" className="ui-link" title="Open this task" onClick={() => goTo(`task:${r.id}`)}>
                    {r.name}
                  </button>
                ) : (
                  <span>{r.name}</span>
                )}
                {r.key && !r.name.includes(r.key) && <span className="muted"> {r.key}</span>}
                <span className={STATE[r.state].tone} title={STATE[r.state].hint}>{STATE[r.state].label}</span>
              </td>
              <td title={STATE[r.state].hint}>
                {r.tookMs === null ? "—" : r.state === "open" ? <span className="muted">{formatSpan(r.tookMs)} so far</span> : formatSpan(r.tookMs)}
              </td>
              <td>{r.workedSecs ? formatDuration(r.workedSecs * 1000) : "—"}</td>
              <td>{r.tokens ? formatTokens(r.tokens) : "—"}</td>
              <td>{r.asks || "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {sorted.length > FIRST && (
        <div className="run-more">
          <button type="button" className="ui-button" onClick={() => setAll((a) => !a)}>
            {all ? `Show the first ${FIRST}` : `Show all ${sorted.length}`}
          </button>
        </div>
      )}
    </section>
  );
});
