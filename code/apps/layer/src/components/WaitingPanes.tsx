import { useMemo, useState } from "react";
import { api } from "../lib/api";
import { CHAT_TASK_ID } from "../lib/derive";
import { goTo } from "../lib/goto";
import type { Pins } from "../lib/pins";
import { menuKeys, pressKeys } from "../lib/press";
import type { WaitingPane } from "../lib/types";
import { useStore } from "../store";
import { ContextMenu } from "./menu";
import { Confirm, ErrorMessage, useFailure } from "./ui";

const NONE: WaitingPane[] = [];

/** The waiting panes of one task, or of the chats. */
function useWaiting(taskId: string): WaitingPane[] {
  const all = useStore((s) => s.waiting);
  return useMemo(() => {
    const mine = all.filter((w) => w.task_id === taskId);
    return mine.length ? mine : NONE;
  }, [all, taskId]);
}

/**
 * Reopen and Forget for a pane the last launch did not put back (PANE-7).
 * They used to be forgotten at launch, and a chat with a day's work in it
 * went with them: nothing in the app could open it again.
 */
function useActions(taskId: string, onForgot?: (w: WaitingPane) => void) {
  // Fails under the list it was done from (NOTE-6).
  const failure = useFailure();
  const [busy, setBusy] = useState<string | null>(null);
  const [forgetting, setForgetting] = useState<WaitingPane | null>(null);
  const refresh = () =>
    Promise.all([useStore.getState().refreshPanes(), useStore.getState().refreshWaiting()]);

  async function reopen(w: WaitingPane) {
    if (busy) return;
    setBusy(w.id);
    failure.clear();
    try {
      const pane = await api.reopenWaitingPane(w.id);
      await refresh();
      goTo(`pane:${taskId}:${pane.id}`);
    } catch (e) {
      failure.fail(e);
      void refresh().catch(() => {});
    } finally {
      setBusy(null);
    }
  }

  async function forget(w: WaitingPane) {
    failure.clear();
    try {
      await api.forgetWaitingPane(w.id);
      onForgot?.(w);
      await useStore.getState().refreshWaiting();
    } catch (e) {
      failure.fail(e);
    }
  }

  const confirm = forgetting && (
    <Confirm
      title={forgetting.kind === "shell" ? "Forget this shell?" : "Forget this conversation?"}
      confirmLabel="Forget"
      body={
        forgetting.kind === "shell"
          ? "It will not be listed here again. This can't be undone."
          : "It will not be listed here again. Its conversation stays in the agent's own history on disk. This can't be undone."
      }
      onCancel={() => setForgetting(null)}
      onConfirm={async () => {
        await forget(forgetting);
        setForgetting(null);
      }}
    />
  );
  const problem = <ErrorMessage>{failure.error}</ErrorMessage>;
  return { busy, reopen, ask: setForgetting, confirm, problem };
}

interface WaitingChatsProps {
  /** Which of them this list holds: the Slack bot's go under its heading (CHAT-5). */
  keep: (w: WaitingPane) => boolean;
  /** Pinned from its right-click menu, and unpinned when forgotten (CHAT-6). */
  pins: Pins;
  heading?: boolean;
  /** What to call one its CLI has not named. */
  label?: (w: WaitingPane) => string | undefined;
  sub?: string | ((w: WaitingPane) => string);
}

/** Chats the last launch did not put back, under the open ones in the Chat view. */
export function WaitingChats({
  keep,
  pins,
  heading = true,
  label,
  sub = "not reopened at launch · click to reopen",
}: WaitingChatsProps) {
  const waiting = useWaiting(CHAT_TASK_ID).filter(keep);
  const { busy, reopen, ask, confirm, problem } = useActions(CHAT_TASK_ID, (w) => pins.drop(w.cwd));
  const [menu, setMenu] = useState<{ x: number; y: number; w: WaitingPane } | null>(null);
  if (waiting.length === 0) return null;
  return (
    <>
      {heading && (
        <div
          className="chat-list-waiting"
          title="At most 12 panes come back at launch. These wait for you."
        >
          Not reopened
        </div>
      )}
      {waiting.map((w) => (
        // A div, not a button: it holds the ✕.
        <div
          key={w.id}
          role="button"
          tabIndex={0}
          className="chat-item waiting"
          onClick={() => void reopen(w)}
          onKeyDown={menuKeys(pressKeys(() => void reopen(w)))}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu({ x: e.clientX, y: e.clientY, w });
          }}
          title="Reopen this chat where it left off"
        >
          <span className="dot" />
          <span className="chat-item-text">
            <span className="chat-item-title">{w.title ?? label?.(w) ?? "Chat"}</span>
            <span className="chat-item-sub">
              {busy === w.id ? "Reopening…" : typeof sub === "string" ? sub : sub(w)}
            </span>
          </span>
          <span
            role="button"
            tabIndex={0}
            className="x"
            title="Forget this chat"
            onClick={(e) => {
              e.stopPropagation();
              ask(w);
            }}
            onKeyDown={pressKeys(() => ask(w))}
          >
            ✕
          </span>
        </div>
      ))}
      {problem}
      {confirm}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={[
            {
              label: pins.has(menu.w) ? "Unpin" : "Pin",
              onSelect: () => menu.w.cwd !== null && pins.toggle(menu.w.cwd),
              disabled: menu.w.cwd === null,
            },
            { label: "Reopen", onSelect: () => void reopen(menu.w), disabled: busy !== null },
            { label: "Forget…", onSelect: () => ask(menu.w), danger: true, separated: true },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </>
  );
}

/** A task's panes the last launch did not put back, at the top of its Agents tab. */
export function WaitingTaskPanes({ taskId }: { taskId: string }) {
  const waiting = useWaiting(taskId);
  const agents = useStore((s) => s.agents);
  const { busy, reopen, ask, confirm, problem } = useActions(taskId);
  if (waiting.length === 0) return null;
  return (
    <>
      <div className="limit-banner waiting-banner">
        <span>
          {waiting.length === 1 ? "A pane was" : `${waiting.length} panes were`} not reopened at
          launch (12 at most):
        </span>
        {waiting.map((w) => (
          <span key={w.id} className="waiting-pane">
            <b>
              {w.kind === "shell"
                ? "Shell"
                : (agents.find((a) => a.id === w.agent_id)?.name ?? w.agent_id ?? "Agent")}
            </b>
            {w.title && ` · ${w.title}`}
            <button className="ui-button" disabled={busy !== null} onClick={() => void reopen(w)}>
              {busy === w.id ? "Reopening…" : "Reopen"}
            </button>
            <button className="ui-button" title="Do not list it again" onClick={() => ask(w)}>
              Forget
            </button>
          </span>
        ))}
        {confirm}
      </div>
      {problem}
    </>
  );
}
