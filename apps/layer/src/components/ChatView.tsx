import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import { useFocusedPane } from "../lib/goto";
import { usePanesOf } from "../lib/panes";
import { read, write } from "../lib/persist";
import { useSlackBot } from "../lib/slackBot";
import { useStore } from "../store";
import { CHAT_TASK_ID, canRestart, paneName, paneState } from "../lib/derive";
import { PaneView } from "./AcpPane";
import { BrowserSide, BrowserToggle, useBrowserSide } from "./BrowserSide";
import { PlusIcon } from "./icons";
import { ContextMenu, Confirm } from "./ui";
import { PaneExited, RestartButton, useRestart } from "./Restart";
import { WaitingChats } from "./WaitingPanes";
import { SidebarGrip } from "./sidebar/SidebarGrip";
import type { MenuItem } from "./ui";

function started(unix: string): string {
  const d = new Date(unix);
  return Number.isNaN(d.getTime())
    ? ""
    : d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

/**
 * A standing agent with no worktree: for questions, drafting tickets, and
 * pulling context from whatever MCP servers the user's own CLI is configured
 * with. It runs in a scratch folder so it can keep notes between sessions.
 *
 * Conversations live down the left rather than in a tab strip: they are long
 * lived and there is no obvious limit on how many you might keep, which is the
 * case tabs handle worst.
 */
export function ChatView() {
  const { panes, ids } = usePanesOf(CHAT_TASK_ID);
  const agents = useStore((s) => s.agents);
  const refreshPanes = useStore((s) => s.refreshPanes);
  const fail = useStore((s) => s.fail);
  // A chat the Slack bot opened links to its thread (BOT-11).
  const slack = useSlackBot();

  const [active, setActive] = useState<string | null>(null);
  const { restarting, restart } = useRestart(setActive);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [closing, setClosing] = useState<string | null>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const installed = agents.filter((a) => a.installed);
  // One browser for every chat, as a task's agents share the task's (BRW-2).
  const { open: browserOpen, show: showBrowser, view: browser } = useBrowserSide(CHAT_TASK_ID);

  // Chats are given fresh ids every launch, so what survives a restart is the
  // position in the list, not the identity of the pane.
  useEffect(() => {
    if (ids.length === 0) { setActive(null); return; }
    if (!active || !ids.includes(active)) {
      const want = read<number>("activeChat", ids.length - 1);
      const i = Number.isInteger(want) && want >= 0 && want < ids.length
        ? want
        : ids.length - 1;
      setActive(ids[i]);
    }
  }, [ids, active]);

  useFocusedPane(ids, setActive);

  useEffect(() => {
    const i = active ? ids.indexOf(active) : -1;
    if (i >= 0) write("activeChat", i);
  }, [active, ids]);

  // Ctrl+Tab moves between chats, as it does between terminals.
  useEffect(() => {
    if (ids.length < 2) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab" || !e.ctrlKey || e.metaKey || e.altKey) return;
      e.preventDefault();
      e.stopPropagation();
      const at = active ? ids.indexOf(active) : -1;
      const from = at < 0 ? 0 : at;
      setActive(ids[(from + (e.shiftKey ? -1 : 1) + ids.length) % ids.length]);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [ids, active]);

  // One start at a time: a second click while the first was spawning
  // opened a second chat.
  const starting = useRef(false);
  async function start(agentId: string, acp = false) {
    if (starting.current) return;
    starting.current = true;
    try {
      const pane = await api.spawnChat(agentId, null, acp);
      await refreshPanes();
      setActive(pane.id);
    } catch (e) {
      fail(e);
    } finally {
      starting.current = false;
    }
  }

  async function close(id: string) {
    try {
      await api.closePane(id);
      await refreshPanes();
    } catch (e) {
      fail(e);
    }
  }

  const addItems: MenuItem[] = installed.length
    ? [
        ...installed.map((a) => ({ label: a.name, onSelect: () => void start(a.id) })),
        // The same agents as a conversation the app draws (§20).
        ...installed.filter((a) => a.acp).map((a, i) => ({
          label: `${a.name} · conversation (ACP)`,
          onSelect: () => void start(a.id, true),
          separated: i === 0,
        })),
      ]
    : [{ label: "No agent CLIs on your PATH", onSelect: () => {}, disabled: true }];

  const doomed = panes.find((p) => p.id === closing);

  return (
    <div className="chat">
      <div className="chat-list">
        <div className="chat-list-head">
          Chats
          <div className="spacer" />
          <BrowserToggle open={browserOpen} view={browser} whose="the chats'" onToggle={() => showBrowser(!browserOpen)} />
          <button
            ref={addRef}
            className="ui-button btn-add"
            title="New chat"
            onClick={() =>
              setMenu((open) => {
                if (open) return null;
                const r = addRef.current?.getBoundingClientRect();
                return r ? { x: r.right, y: r.bottom + 4 } : null;
              })
            }
          >
            <PlusIcon />
          </button>
        </div>

        <div className="chat-list-body">
          {panes.map((p) => (
            <div
              key={p.id}
              className={`chat-item${p.id === active ? " active" : ""}`}
              onClick={() => setActive(p.id)}
            >
              <span className={`dot ${paneState(p).dot}`} title={paneState(p).label} />
              <span className="chat-item-text">
                <span className="chat-item-title" title={paneName(p)}>{paneName(p)}</span>
                <span className="chat-item-sub">
                  {(() => {
                    // First, where a narrow list cannot cut it off.
                    const thread = slack?.threads.find((t) => t.room === p.cwd);
                    return thread && (
                      <>
                        <span
                          className="chat-item-slack"
                          title={`Its replies go to the Slack thread in ${thread.label}`}
                          onClick={(e) => { e.stopPropagation(); if (thread.link) void openUrl(thread.link); }}
                        >
                          Slack ↗
                        </span>
                        {" · "}
                      </>
                    );
                  })()}
                  {p.topic && `${agents.find((a) => a.id === p.agent_id)?.name ?? p.title} · `}
                  {restarting.has(p.id) ? "restarting…" : p.running ? started(p.started_at) : "ended"}
                </span>
              </span>
              {canRestart(p, agents) && !restarting.has(p.id) && <RestartButton pane={p} onRestart={restart} />}
              <span
                className="x"
                title="Close this chat"
                onClick={(e) => { e.stopPropagation(); setClosing(p.id); }}
              >
                ✕
              </span>
            </div>
          ))}
          {panes.length === 0 && (
            <div className="chat-list-empty">No chats open.</div>
          )}
          <WaitingChats />
        </div>
      </div>
      <SidebarGrip side="chats" />

      <div className="chat-main">
      <PaneExited pane={panes.find((p) => p.id === active)} busy={restarting.has(active ?? "")} onResume={restart} />
      <div className="pane-stack">
        {panes.map((p) => (
          <PaneView key={p.id} pane={p} visible={p.id === active} />
        ))}
        {panes.length === 0 && (
          <div className="empty">
            <h2>Chat</h2>
            <p>
              An agent with no worktree attached — for asking questions, drafting a
              ticket before it exists, or pulling context together from elsewhere.
            </p>
            <p style={{ color: "var(--faint)" }}>
              It starts with a <code>CLAUDE.md</code> describing your repos, the tasks
              in flight and which integrations the app is connected to — so it knows
              what you are working on without being told.
            </p>
            <p style={{ color: "var(--faint)" }}>
              It also reaches Jira, Slack and GitHub through this app's own
              connections, so it can search tickets, file one, or start work on it
              without you configuring anything separately.
            </p>
            <div className="row">
              {installed.length === 0 && <span>No agent CLIs found on your PATH.</span>}
              {/* One primary: the first agent; the others are a choice beside it. */}
              {installed.map((a, i) => (
                <button key={a.id} className={`ui-button${i === 0 ? " ui-button-primary" : ""}`} onClick={() => void start(a.id)}>
                  Start {a.name}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
      </div>
      {browserOpen && <BrowserSide taskId={CHAT_TASK_ID} visible onClose={() => showBrowser(false)} />}

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          ignore={addRef}
          items={addItems}
          onClose={() => setMenu(null)}
        />
      )}

      {doomed && (
        <Confirm
          title="Close this chat?"
          confirmLabel="Close"
          body={
            <>
              <b>{doomed.title}</b> will be stopped and its scrollback discarded.
              {doomed.running && " Anything it is part-way through is lost."}
            </>
          }
          onCancel={() => setClosing(null)}
          // Returned, not fired and forgotten: stopping a chat agent gives it
          // five seconds to save, and the dialog now waits that out instead of
          // vanishing while the tab stays put.
          busyLabel="Closing…"
          onConfirm={() => close(doomed.id)}
        />
      )}
    </div>
  );
}
