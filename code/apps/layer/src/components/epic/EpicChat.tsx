import { useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { api } from "../../lib/api";
import { useStore } from "../../store";
import type { TaskView } from "../../lib/types";
import { PaneView } from "../AcpPane";
import { PaneExited, useRestart } from "../Restart";
import { ErrorMessage, useFailure } from "../ui";

/**
 * The epic's chat (EPIC-11): one conversation over ACP in the epic's folder,
 * whose context file says it orchestrates the sub-tasks and writes no code.
 * Its tools are EPIC-12's.
 */
export function EpicChat({ epic }: { epic: TaskView }) {
  const panes = useStore(useShallow((s) => s.panes.filter((p) => p.task_id === epic.id && p.kind === "agent")));
  const agents = useStore((s) => s.agents);
  const refreshPanes = useStore((s) => s.refreshPanes);
  const failure = useFailure();
  const [starting, setStarting] = useState(false);
  const { restart, restarting } = useRestart(() => {});
  // The one running, else the last: an exited one says so and offers Resume.
  const pane = panes.find((p) => p.running) ?? panes[panes.length - 1];
  const agent = agents.find((a) => a.installed && a.acp);

  async function start() {
    if (!agent || starting) return;
    setStarting(true);
    failure.clear();
    try {
      await api.spawnAgent(
        epic.id, agent.id, null,
        `You are the chat of the epic ${epic.name}. Call epic_subtasks, then say in a few lines where its sub-tasks stand and which can start next.`,
        false, true,
      );
      await refreshPanes();
    } catch (e) {
      failure.fail(e);
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="epic-chat">
      <div className="epic-chat-head">
        <b>Epic chat</b>
        <span>starts and unblocks sub-tasks · writes no code</span>
      </div>
      {pane ? (
        <>
          <PaneExited pane={pane} busy={restarting.has(pane.id)} onResume={restart} />
          <div className="pane-stack">
            <PaneView pane={pane} visible />
          </div>
        </>
      ) : (
        <div className="epic-chat-empty">
          <p className="muted">
            It reads the epic's spec and its sub-tasks, starts them in order, and brings
            their questions to you.
          </p>
          <button className="ui-button ui-button-primary" disabled={!agent || starting} onClick={() => void start()}>
            {starting ? "Starting…" : agent ? `Start the chat with ${agent.name}` : "No agent that talks over ACP"}
          </button>
          <ErrorMessage>{failure.error}</ErrorMessage>
        </div>
      )}
    </div>
  );
}
