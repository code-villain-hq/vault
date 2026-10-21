import { memo, useState } from "react";
import { formatDuration, formatTokens } from "../../lib/runs";
import type { UsageDay } from "../../lib/usage";

type Measure = "tokens" | "worked" | "finished";

const MEASURES: { id: Measure; label: string }[] = [
  { id: "tokens", label: "Tokens" },
  { id: "worked", label: "Agents working" },
  { id: "finished", label: "Tasks finished" },
];

const value = (m: Measure, d: UsageDay) => (m === "tokens" ? d.tokens : m === "worked" ? d.workedSecs : d.finished);

function say(m: Measure, n: number): string {
  if (m === "tokens") return `${formatTokens(n)} tokens`;
  if (m === "worked") return `agents worked ${formatDuration(n * 1000)}`;
  return `${n} ${n === 1 ? "task" : "tasks"} finished`;
}

/** "Thu 8 Oct", from `YYYY-MM-DD`, read as the Mac's own day. */
function dayName(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d, 12).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

/**
 * A bar a day of the period: tokens, agents' working time or tasks finished
 * (RUN-4). Hovering a day says what it came to; otherwise the line above
 * says what the period did.
 */
export const UsageChart = memo(function UsageChart({ days }: { days: UsageDay[] }) {
  const [measure, setMeasure] = useState<Measure>("tokens");
  const [hover, setHover] = useState<number | null>(null);
  const peak = Math.max(1, ...days.map((d) => value(measure, d)));
  const total = days.reduce((n, d) => n + value(measure, d), 0);
  const at = hover === null ? null : days[hover];

  return (
    <div className="run-chart">
      <div className="run-chart-head">
        <div className="ui-tabs sub-tabs" role="tablist">
          {MEASURES.map((m) => (
            <button key={m.id} type="button" className="ui-tab" role="tab" aria-selected={measure === m.id} onClick={() => setMeasure(m.id)}>
              {m.label}
            </button>
          ))}
        </div>
        <span className="run-chart-readout" aria-live="polite">
          {at ? `${dayName(at.day)}: ${say(measure, value(measure, at))}` : `${say(measure, total)} in ${days.length} days`}
        </span>
      </div>
      <div
        className="run-days"
        role="img"
        aria-label={`${MEASURES.find((m) => m.id === measure)?.label} on each of the last ${days.length} days`}
        onMouseLeave={() => setHover(null)}
      >
        {days.map((d, i) => (
          <div key={d.day} className={`run-day${hover === i ? " hover" : ""}`} onMouseEnter={() => setHover(i)}>
            <div className="bar" style={{ height: value(measure, d) ? `${(value(measure, d) / peak) * 100}%` : 0 }} />
          </div>
        ))}
      </div>
      <div className="run-days-axis">
        <span>{days.length ? dayName(days[0].day) : ""}</span>
        <span>today</span>
      </div>
    </div>
  );
});
