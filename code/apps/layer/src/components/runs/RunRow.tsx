import { memo } from "react";
import { durationMs, formatDuration, formatTokens, resultOf, totalTokens, type RunResult } from "../../lib/runs";
import type { Run } from "../../lib/types-runs";
import { Ago } from "../Ago";

const RESULT: Record<RunResult, { mark: string; label: string }> = {
  passed: { mark: "✓", label: "Its loop passed" },
  gave_up: { mark: "✗", label: "Its loop gave up" },
  failed: { mark: "✗", label: "Failed" },
  stopped: { mark: "■", label: "Stopped" },
  exited: { mark: "●", label: "Exited on its own" },
};

/**
 * One run in the list, and what it was when opened (RUN-3). Memoised, with
 * its "ago" on a clock of its own: the view's clock redrew every row every
 * 30 seconds, and a refetched log every row again, though a run once ended
 * never changes.
 */
export const RunRow = memo(function RunRow({ run, agentName, open, onToggle }: {
  run: Run;
  agentName: string;
  open: boolean;
  onToggle: (id: string) => void;
}) {
  const result = resultOf(run);
  return (
    <div className={`run-row ${result}`}>
      <button type="button" className="run-line" onClick={() => onToggle(run.id)} aria-expanded={open}>
        <span className="run-mark" title={RESULT[result].label}>{RESULT[result].mark}</span>
        <span className="run-what"><b>{agentName}</b> · {run.task || "a task since removed"}</span>
        <span className="chips">
          {run.repos.map((p) => <span key={p.id} className="ui-chip">{p.name}</span>)}
          {run.asks > 0 && (
            <span className="ui-chip" title={`Waited on you ${formatDuration(run.waited_secs * 1000)}`}>
              asked {run.asks}×
            </span>
          )}
          {run.limited && <span className="ui-status ui-status-warn">usage limit</span>}
          {run.loop && <span className="ui-chip">{run.loop.used}/{run.loop.rounds} rounds</span>}
        </span>
        <span className="spacer" />
        {run.tokens && <span className="run-tokens" title="Tokens">{formatTokens(totalTokens(run.tokens))}</span>}
        {/* Working, where it says: open, a chat ran for days. */}
        <span className="run-took" title={run.worked_secs === null ? "How long it ran" : `Working; it ran ${formatDuration(durationMs(run))}`}>
          {formatDuration((run.worked_secs ?? durationMs(run) / 1000) * 1000)}
        </span>
        <span className="run-when"><Ago since={run.ended_at} everyMs={30_000} /></span>
      </button>
      {open && (
        <dl className="run-detail">
          <dt>Ended</dt>
          <dd>{ending(run)}</dd>
          <dt>Turns</dt>
          <dd>
            {run.turns}
            {run.worked_secs !== null && `, working ${formatDuration(run.worked_secs * 1000)} of the ${formatDuration(durationMs(run))} it ran`}
          </dd>
          <dt>Asked you</dt>
          <dd>
            {run.asks === 0
              ? "never"
              : `${run.asks} ${run.asks === 1 ? "time" : "times"}, waiting ${formatDuration(run.waited_secs * 1000)} in all`}
            {run.limited && ", and hit its usage limit"}
          </dd>
          <dt>Tool calls</dt>
          <dd>
            {run.tools
              ? `${run.tools.calls}, ${run.tools.failed} failed`
              : "not said: Claude Code, Copilot and agents over ACP say"}
          </dd>
          {run.loop && (
            <>
              <dt>Loop</dt>
              <dd>
                {run.loop.end === "passed" ? "passed" : run.loop.end === "gave_up" ? "gave up" : "stopped"},{" "}
                {run.loop.used} of {run.loop.rounds} rounds used
              </dd>
            </>
          )}
          <dt>Ran</dt>
          <dd>
            {new Date(run.started_at).toLocaleString()} – {new Date(run.ended_at).toLocaleTimeString()},{" "}
            {run.acp ? "as a conversation (ACP)" : "in a terminal"}
          </dd>
          <dt>Tokens</dt>
          <dd>
            {run.tokens
              ? `${formatTokens(run.tokens.input)} in · ${formatTokens(run.tokens.output)} out · ${formatTokens(run.tokens.cached_read)} read from cache · ${formatTokens(run.tokens.cached_write)} written to cache`
              : run.acp
                ? "not said: this agent puts no usage on its answers"
                : "not said: of the agents in a terminal, only Claude Code does"}
          </dd>
        </dl>
      )}
    </div>
  );
// By id: a refetched log is all new objects, but a run is never changed once
// recorded (`runs.rs` only adds and clears).
}, (a, b) => a.run.id === b.run.id && a.open === b.open && a.agentName === b.agentName && a.onToggle === b.onToggle);

function ending(run: Run): string {
  switch (run.end) {
    case "stopped":
      return "stopped: by you, a restart, its task closing or the app quitting";
    case "exited":
      return "on its own, exit code 0";
    case "failed":
      return run.code === null ? "failed, with no exit code" : `failed, exit code ${run.code}`;
  }
}
