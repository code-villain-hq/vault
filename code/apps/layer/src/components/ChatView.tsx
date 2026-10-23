import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import { useFocusedPane } from "../lib/goto";
import { usePanesOf } from "../lib/panes";
import { read, write } from "../lib/persist";
import { useSlackBot } from "../lib/slackBot";
import { useStore } from "../store";
import { CHAT_TASK_ID, showsFolded } from "../lib/derive";
import { PaneView } from "./AcpPane";
import { BrowserSide, BrowserToggle, useBrowserSide } from "./BrowserSide";
import { PlusIcon } from "./icons";
import { ContextMenu, Confirm, ErrorMessage, useFailure } from "./ui";
import { PaneExited, useRestart } from "./Restart";
import { WaitingChats } from "./WaitingPanes";
import { ChatItem, SlackChats, useSlackFold } from "./ChatList";
import { SidebarGrip } from "./sidebar/SidebarGrip";
import type { MenuItem } from "./ui";
import type { PaneInfo } from "../lib/types";

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
  // Starting or closing a chat fails under the list's head, where + is, or
  // under the start buttons while there are no chats (NOTE-6).
  const failure = useFailure();
  // A chat the Slack bot opened links to its thread (BOT-11), and is listed
  // apart from yours (CHAT-5).
  const slack = useSlackBot();
  const rooms = useMemo(() => new Map((slack?.threads ?? []).map((t) => [t.room, t])), [slack]);
  const isSlack = (p: { cwd: string | null }) => p.cwd !== null && rooms.has(p.cwd);
  const mine = panes.filter((p) => !isSlack(p));
  const bots = panes.filter(isSlack);
  const lastMine = mine.length ? mine[mine.length - 1].id : null;
  const [slackOpen, toggleSlack] = useSlackFold();

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
      // Your newest chat, not the Slack bot's (CHAT-5): which those are is
      // known once the bot's threads have been read.
      if (!slack) return;
      const newest = lastMine ? ids.indexOf(lastMine) : ids.length - 1;
      const want = read<number>("activeChat", newest);
      const i = Number.isInteger(want) && want >= 0 && want < ids.length
        ? want
        : newest;
      setActive(ids[i]);
    }
  }, [ids, active, slack, lastMine]);

  useFocusedPane(ids, setActive);

  useEffect(() => {
    const i = active ? ids.indexOf(active) : -1;
    if (i >= 0) write("activeChat", i);
  }, [active, ids]);

  // Ctrl+Tab moves between chats, as it does between terminals, in the
  // list's order and past none it hides under the folded Slack heading.
  const order = [...mine, ...bots.filter((p) => slackOpen || showsFolded(p, active))].map((p) => p.id);
  const orderKey = order.join("\n");
  useEffect(() => {
    const shown = orderKey ? orderKey.split("\n") : [];
    if (shown.length < 2) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab" || !e.ctrlKey || e.metaKey || e.altKey) return;
      e.preventDefault();
      e.stopPropagation();
      const at = active ? shown.indexOf(active) : -1;
      const from = at < 0 ? 0 : at;
      setActive(shown[(from + (e.shiftKey ? -1 : 1) + shown.length) % shown.length]);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [orderKey, active]);

  // One start at a time: a second click while the first was spawning
  // opened a second chat.
  const starting = useRef(false);
  async function start(agentId: string, acp = false) {
    if (starting.current) return;
    starting.current = true;
    failure.clear();
    try {
      const pane = await api.spawnChat(agentId, null, acp);
      await refreshPanes();
      setActive(pane.id);
    } catch (e) {
      failure.fail(e);
    } finally {
      starting.current = false;
    }
  }

  async function close(id: string) {
    failure.clear();
    try {
      await api.closePane(id);
      await refreshPanes();
    } catch (e) {
      failure.fail(e);
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

  const item = (p: PaneInfo) => (
    <ChatItem
      key={p.id}
      pane={p}
      active={p.id === active}
      thread={p.cwd ? rooms.get(p.cwd) : undefined}
      restarting={restarting.has(p.id)}
      onSelect={setActive}
      onRestart={restart}
      onClose={setClosing}
    />
  );

  return (
    <div className="chat">
      <div className="chat-list">
        <div className="chat-list-head">
          Chats
          <div className="spacer" />
          <BrowserToggle open={browserOpen} view={browser} whose="the chats'" onToggle={() => showBrowser(!browserOpen)} />
          <button
            ref={addRef}
            className="ui-button ui-icon-button btn-add"
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
        {panes.length > 0 && <ErrorMessage>{failure.error}</ErrorMessage>}

        <div className="chat-list-body">
          {mine.map(item)}
          {panes.length === 0 && (
            <div className="chat-list-empty">No chats open.</div>
          )}
          <WaitingChats keep={(w) => !isSlack(w)} />
          <SlackChats open={slackOpen} onToggle={toggleSlack} panes={bots} rooms={rooms} active={active} item={item} />
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
            <ErrorMessage>{failure.error}</ErrorMessage>
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
              {" This can't be undone."}
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
