import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import { readOneOf, write } from "../lib/persist";
import { byAgent, filterRuns, formatDuration, formatSpan, formatTokens, runChoices, runPage, RUN_PAGE } from "../lib/runs";
import type { RunLog } from "../lib/types-runs";
import { byRepo, periodStart, PERIODS, taskRows, usageDays, usageStats, type LiveTask, type Period } from "../lib/usage";
import { useNow, useStore } from "../store";
import { RunRow } from "./runs/RunRow";
import { TaskTable } from "./runs/TaskTable";
import { UsageBreakdown } from "./runs/UsageBreakdown";
import { UsageChart } from "./runs/UsageChart";
import { Confirm, Spinner } from "./ui";

/** What `runs.rs` keeps of each; said once the log is that full (RUN-5). */
const CAP = 1000;
const PERIOD_CHOICES = PERIODS.map(String) as `${Period}`[];

/**
 * What agents used and what came of it (§22): tokens, their working time,
 * the tasks finished and how long each took, over a period. Each run is
 * still listed, folded away at the foot: a list of a pane's every start and
 * stop said little on its own.
 */
export function UsageView() {
  const agents = useStore((s) => s.agents);
  const tasks = useStore((s) => s.tasks);
  const fail = useStore((s) => s.fail);
  // For the chart's days, which move at midnight, and an open task's age.
  // The rows keep their own "ago" clocks (`RunRow`).
  const now = useNow(30_000);
  const [log, setLog] = useState<RunLog | null>(null);
  const [period, setPeriod] = useState<Period>(() => Number(readOneOf("usagePeriod", PERIOD_CHOICES, "30")) as Period);
  const [agent, setAgent] = useState<string | null>(null);
  const [repo, setRepo] = useState<string | null>(null);
  const [listing, setListing] = useState(false);
  const [pages, setPages] = useState(1);
  const [open, setOpen] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);
  const toggle = useCallback((id: string) => setOpen((o) => (o === id ? null : id)), []);

  useEffect(() => {
    let current = true;
    let asked = 0;
    const load = () => {
      // Agents ending together ask together; only the last answer counts.
      const mine = ++asked;
      api.listRuns().then((l) => { if (current && mine === asked) setLog(l); }).catch(fail);
    };
    load();
    const p = listen("runs:changed", load);
    return () => {
      current = false;
      void p.then((un) => un());
    };
  }, [fail]);

  const today = noonOf(now);
  const since = periodStart(period, today);
  const runs = useMemo(() => log?.runs ?? [], [log]);
  const done = useMemo(() => log?.tasks ?? [], [log]);
  const live = useMemo<LiveTask[]>(
    () => tasks.map((t) => ({
      id: t.id,
      name: t.name,
      issue_key: t.issue_key,
      created_at: t.created_at,
      repos: t.checkouts.map((c) => ({ id: c.project_id, name: c.project_name })),
    })),
    [tasks],
  );
  const choices = useMemo(() => runChoices(runs), [runs]);
  const shown = useMemo(
    () => filterRuns(runs, { agent, repo }).filter((r) => Date.parse(r.ended_at) >= since),
    [runs, agent, repo, since],
  );
  // An open task's age moves with the clock; everything else only with the log.
  const rows = useMemo(
    () => taskRows(shown, done, live, { since, now, repo, byRuns: agent !== null }),
    [shown, done, live, since, now, repo, agent],
  );
  const stats = useMemo(() => usageStats(shown, rows), [shown, rows]);
  const days = useMemo(() => usageDays(shown, rows, period, today), [shown, rows, period, today]);
  const perRepo = useMemo(() => byRepo(shown, rows), [shown, rows]);
  const perAgent = useMemo(() => byAgent(shown), [shown]);
  const page = useMemo(() => runPage(shown, pages), [shown, pages]);
  const agentName = useCallback(
    (id: string) => agents.find((a) => a.id === id)?.name ?? (id || "an agent"),
    [agents],
  );
  // A different filter is a different list, read from its top again.
  const filter = (set: (v: string | null) => void) => (e: { target: { value: string } }) => {
    set(e.target.value || null);
    setPages(1);
  };
  const empty = runs.length === 0 && done.length === 0;

  return (
    <div className="wide runs">
      <div className="wide-head">
        <h2>Usage</h2>
        <span className="sub">
          {runs.length >= CAP ? `the newest ${CAP} runs are kept` : "kept on this Mac only"}
        </span>
        <div className="spacer" />
        {!empty && (
          <>
            <select className="ui-field"
              value={period}
              aria-label="Period"
              onChange={(e) => {
                const p = Number(e.target.value) as Period;
                setPeriod(p);
                write("usagePeriod", String(p));
                setPages(1);
              }}
            >
              {PERIODS.map((p) => <option key={p} value={p}>Last {p} days</option>)}
            </select>
            <select className="ui-field" value={agent ?? ""} onChange={filter(setAgent)} aria-label="Agent">
              <option value="">Every agent</option>
              {choices.agents.map((a) => <option key={a} value={a}>{agentName(a)}</option>)}
            </select>
            <select className="ui-field" value={repo ?? ""} onChange={filter(setRepo)} aria-label="Repository">
              <option value="">Every repository</option>
              {choices.repos.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
            </select>
            <button type="button" className="ui-button ui-button-danger" onClick={() => setClearing(true)}>
              Clear…
            </button>
          </>
        )}
      </div>

      {log === null ? (
        <div className="empty"><Spinner /></div>
      ) : empty ? (
        <div className="empty">
          <h2>Nothing used yet</h2>
          <p>
            Each agent is recorded here when it ends, and each task when it is finished or deleted: the tokens agents
            used, how long they worked and waited on you, and how long each task took. Nothing leaves this Mac.
          </p>
        </div>
      ) : (
        <>
          <div className="run-stats">
            <Stat
              label="Tokens"
              value={stats.withTokens ? formatTokens(stats.tokens) : "—"}
              note={
                stats.withTokens
                  ? stats.withTokens < stats.runs ? `said by ${stats.withTokens} of ${stats.runs} runs` : `over ${stats.runs} runs`
                  : "only Claude Code and agents over ACP say"
              }
            />
            <Stat
              label="Agents working"
              value={stats.withWorked ? formatDuration(stats.workedSecs * 1000) : "—"}
              note={
                stats.withWorked
                  ? stats.withWorked < stats.runs ? `said by ${stats.withWorked} of ${stats.runs} runs` : `over ${stats.runs} runs`
                  : "counted for runs that end from 0.9 on"
              }
            />
            <Stat
              label="Tasks finished"
              value={String(stats.finished)}
              note={`${stats.open} open${stats.deleted ? ` · ${stats.deleted} deleted` : ""}`}
            />
            <Stat
              label="Typical task"
              value={stats.finished ? formatSpan(stats.leadMedianMs) : "—"}
              note={
                stats.finished
                  ? `start to finish; the slowest tenth over ${formatSpan(stats.leadP90Ms)}`
                  : "once a task is finished"
              }
            />
            <Stat
              label="Waited on you"
              value={formatDuration(stats.waitedSecs * 1000)}
              note={`asked ${stats.asks} ${stats.asks === 1 ? "time" : "times"}`}
            />
          </div>

          <UsageChart days={days} />
          <TaskTable rows={rows} />
          <UsageBreakdown repos={perRepo} agents={perAgent} agentName={agentName} />

          <button type="button" className="run-list-title ui-link" aria-expanded={listing} onClick={() => setListing((l) => !l)}>
            {listing ? "▾" : "▸"} Every run in these days ({shown.length})
          </button>
          {listing && (
            <>
              <div className="run-list">
                {page.rows.map((r) => (
                  <RunRow key={r.id} run={r} agentName={agentName(r.agent)} open={open === r.id} onToggle={toggle} />
                ))}
              </div>
              {page.left > 0 && (
                <div className="run-more">
                  <span>{page.left} older {page.left === 1 ? "run" : "runs"} not shown</span>
                  <button type="button" className="ui-button" onClick={() => setPages((n) => n + 1)}>
                    Show {Math.min(RUN_PAGE, page.left)} more
                  </button>
                </div>
              )}
            </>
          )}
        </>
      )}

      {clearing && (
        <Confirm
          title="Clear the run log?"
          body="Every run and finished task kept here is forgotten. Agents, tasks and their work are not touched."
          confirmLabel="Clear"
          onConfirm={() =>
            api.clearRuns().then(() => {
              setAgent(null);
              setRepo(null);
              setPages(1);
            }).catch(fail)
          }
          onCancel={() => setClearing(false)}
        />
      )}
    </div>
  );
}

/** Midday of `now`'s day: the same number all day, so the chart is not redrawn on every tick. */
function noonOf(now: number): number {
  const d = new Date(now);
  d.setHours(12, 0, 0, 0);
  return d.getTime();
}

function Stat({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="run-stat">
      <div className="label">{label}</div>
      <div className="value">{value}</div>
      {note && <div className="note">{note}</div>}
    </div>
  );
}
