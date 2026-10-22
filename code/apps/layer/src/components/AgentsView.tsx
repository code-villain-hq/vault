import { Fragment, memo, useCallback, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { api } from "../lib/api";
import { markStopping, useStore } from "../store";
import { CHAT_TASK_ID, needsYou, paneScope, paneState } from "../lib/derive";
import { Ago } from "./Ago";
import { BusyLabel, ErrorMessage, SidebarToggle, useFailure } from "./ui";

/**
 * Every agent, one card each.
 *
 * The view follows only which agents there are, in what order, and the
 * counts in its heading; each card follows its own agent. Selecting the
 * whole pane list, with a five-second clock for the "ago" labels, redrew
 * every card on each hook any agent fired and again every five seconds.
 */
export function AgentsView() {
  const refreshPanes = useStore((s) => s.refreshPanes);
  // A Stop that failed says so under its card (NOTE-6). Drawn here rather
  // than in the card, which is handed `stop` once and stays as it is.
  const failure = useFailure();
  const [failedOn, setFailedOn] = useState<string | null>(null);
  // What needs you first, so the dock's count is the top of this list.
  const ids = useStore(useShallow((s) =>
    s.panes
      .filter((p) => p.kind === "agent")
      .sort((a, b) => Number(needsYou(b)) - Number(needsYou(a)))
      .map((p) => p.id),
  ));
  const { live, waiting } = useStore(useShallow((s) => ({
    live: s.panes.filter((p) => p.kind === "agent" && p.running).length,
    waiting: s.panes.filter(needsYou).length,
  })));

  /// Stopping asks the agent to exit and waits up to five seconds for it to
  /// save, so the row does not change until then. Say which one is stopping.
  /// Several at once: one id meant stopping B while A was still going gave
  /// A its button back, and A finishing cleared B's spinner.
  const [stopping, setStopping] = useState<Set<string>>(new Set());
  // Read by `stop`, which the cards are handed once rather than anew with
  // each change to `stopping`, which would redraw every card.
  const stoppingNow = useRef(stopping);
  stoppingNow.current = stopping;

  const stop = useCallback(async (paneId: string) => {
    if (stoppingNow.current.has(paneId)) return;
    setStopping((s) => new Set(s).add(paneId));
    markStopping(paneId);
    failure.clear();
    setFailedOn(paneId);
    try {
      await api.killPane(paneId);
      await refreshPanes();
    } catch (e) {
      failure.fail(e);
    } finally {
      setStopping((s) => {
        const next = new Set(s);
        next.delete(paneId);
        return next;
      });
    }
  }, [refreshPanes, failure.clear, failure.fail]);

  return (
    <div className="wide">
      <div className="wide-head">
        <SidebarToggle />
        <h2>Agents</h2>
        <span className="sub">
          {waiting > 0 && (
            <span style={{ color: "var(--status-warn-ink)" }}>{waiting} need{waiting === 1 ? "s" : ""} you · </span>
          )}
          {live} running · {ids.length - live} exited — pick a task
          on the left to work on one
        </span>
        <div className="spacer" />
        <button className="ui-button" onClick={() => void refreshPanes()}>Refresh</button>
      </div>

      {ids.length === 0 && (
        <div className="card">
          <div className="muted" style={{ lineHeight: 1.6 }}>
            Nothing running yet. Pick a task on the left, create one with <b>+</b>, or
            open the Tickets tab and start straight from a Jira issue. A task can span
            as many repositories as the change needs.
          </div>
        </div>
      )}

      {ids.map((id) => (
        <Fragment key={id}>
          <AgentCard id={id} stopping={stopping.has(id)} onStop={stop} />
          {failedOn === id && <ErrorMessage>{failure.error}</ErrorMessage>}
        </Fragment>
      ))}
    </div>
  );
}

const AgentCard = memo(function AgentCard({
  id,
  stopping,
  onStop,
}: {
  id: string;
  stopping: boolean;
  onStop: (paneId: string) => Promise<void>;
}) {
  // The store keeps a pane, its task and its agent while they are unchanged.
  const pane = useStore((s) => s.panes.find((p) => p.id === id));
  const task = useStore((s) => (pane ? s.tasks.find((t) => t.id === pane.task_id) : undefined));
  const agent = useStore((s) => (pane ? s.agents.find((a) => a.id === pane.agent_id) : undefined));
  const select = useStore((s) => s.select);
  const setView = useStore((s) => s.setView);
  // Gone between the list and this card's own read of it.
  if (!pane) return null;

  const isChat = pane.task_id === CHAT_TASK_ID;
  const st = paneState(pane);

  return (
    <div className="agent-card">
      <span className={`dot ${st.dot}`} />
      <div className="who">
        <div className="name" title={pane.topic ?? undefined}>
          {pane.topic ?? agent?.name ?? pane.title}
          {pane.notice && (
            <span style={{ color: "var(--status-warn-ink)", marginLeft: 6 }} title={pane.notice}>
              {pane.notice === "trust_prompt" ? "?" : "⚑"}
            </span>
          )}
        </div>
        <div
          className="where"
          style={pane.notice || needsYou(pane) ? { color: "var(--status-warn-ink)" } : undefined}
        >
          {pane.topic && `${agent?.name ?? pane.title} · `}
          {st.label}
        </div>
      </div>

      <div className="ctx">
        <div className="task">
          {isChat ? "Chat" : task?.name ?? "(task removed)"}
        </div>
        <div className="branch">
          {isChat ? pane.cwd : `${task?.branch ?? "?"} · ${paneScope(pane) ?? ""}`}
        </div>
      </div>

      <span className="when" title="Since it started doing what it is doing now">
        <Ago since={pane.activity_since} />
      </span>

      <button
        className="ui-button"
        onClick={() => {
          if (isChat) setView("chat");
          else if (task) select(task.id);
        }}
      >
        Open
      </button>
      {pane.running && (
        <button
          className="ui-button ui-button-danger"
          disabled={stopping}
          onClick={() => void onStop(pane.id)}
        >
          <BusyLabel busy={stopping} idle="Stop" working="Stopping…" />
        </button>
      )}
    </div>
  );
});
