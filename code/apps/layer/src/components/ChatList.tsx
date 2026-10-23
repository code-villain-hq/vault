import { useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { canRestart, paneName, paneState, showsFolded } from "../lib/derive";
import { read, write } from "../lib/persist";
import { useStore } from "../store";
import type { PaneInfo, SlackThreadView } from "../lib/types";
import { ChevronIcon } from "./icons";
import { RestartButton } from "./Restart";
import { WaitingChats } from "./WaitingPanes";

function started(unix: string): string {
  const d = new Date(unix);
  return Number.isNaN(d.getTime())
    ? ""
    : d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

interface ItemProps {
  pane: PaneInfo;
  active: boolean;
  /** The Slack thread its replies go to (BOT-11). */
  thread: SlackThreadView | undefined;
  restarting: boolean;
  onSelect: (id: string) => void;
  onRestart: (pane: PaneInfo) => void;
  onClose: (id: string) => void;
}

/** One open chat in the Chat view's list. */
export function ChatItem({ pane: p, active, thread, restarting, onSelect, onRestart, onClose }: ItemProps) {
  const agents = useStore((s) => s.agents);
  return (
    <div className={`chat-item${active ? " active" : ""}`} onClick={() => onSelect(p.id)}>
      <span className={`dot ${paneState(p).dot}`} title={paneState(p).label} />
      <span className="chat-item-text">
        <span className="chat-item-title" title={paneName(p)}>{paneName(p)}</span>
        <span className="chat-item-sub">
          {/* First, where a narrow list cannot cut it off. */}
          {thread && (
            <>
              <span
                className="ui-link chat-item-slack"
                title={`Its replies go to the Slack thread in ${thread.label}`}
                onClick={(e) => { e.stopPropagation(); if (thread.link) void openUrl(thread.link); }}
              >
                {thread.label} ↗
              </span>
              {" · "}
            </>
          )}
          {p.topic && `${agents.find((a) => a.id === p.agent_id)?.name ?? p.title} · `}
          {restarting ? "restarting…" : p.running ? started(p.started_at) : "ended"}
        </span>
      </span>
      {canRestart(p, agents) && !restarting && <RestartButton pane={p} onRestart={onRestart} />}
      <span
        className="x"
        title="Close this chat"
        onClick={(e) => { e.stopPropagation(); onClose(p.id); }}
      >
        ✕
      </span>
    </div>
  );
}

/** Whether the Slack heading is open, remembered across launches (CHAT-5). */
export function useSlackFold(): [boolean, () => void] {
  const [open, setOpen] = useState(() => read<boolean>("slackChatsOpen", false) === true);
  const toggle = () => setOpen((was) => { write("slackChatsOpen", !was); return !was; });
  return [open, toggle];
}

interface SlackProps {
  open: boolean;
  onToggle: () => void;
  /** The Slack bot's open chats. */
  panes: PaneInfo[];
  /** The rooms of every thread handed over (BOT-4). */
  rooms: Map<string, SlackThreadView>;
  active: string | null;
  item: (p: PaneInfo) => ReactNode;
}

/**
 * The Slack bot's chats (§24), under their own heading below yours
 * (CHAT-5). Each thread the bot was mentioned in keeps a chat, so mixed in
 * with yours they buried the chats you started.
 */
export function SlackChats({ open, onToggle, panes, rooms, active, item }: SlackProps) {
  const waiting = useStore((s) => s.waiting).filter((w) => w.cwd !== null && rooms.has(w.cwd));
  const count = panes.length + waiting.length;
  if (count === 0) return null;
  const asking = panes.filter((p) => p.running && p.activity === "asking").length;
  const shown = open ? panes : panes.filter((p) => showsFolded(p, active));
  return (
    <>
      <button
        type="button"
        className="chat-list-section"
        aria-expanded={open}
        title={open ? "Fold the Slack bot's chats" : "Show every chat the Slack bot opened"}
        onClick={onToggle}
      >
        <span className={`chev${open ? " open" : ""}`}><ChevronIcon /></span>
        <span>Slack · {count}</span>
        <span className="spacer" />
        {asking > 0 && <span className="chat-list-asking">{asking} asking</span>}
      </button>
      {shown.map(item)}
      {open && (
        <WaitingChats
          keep={(w) => w.cwd !== null && rooms.has(w.cwd)}
          heading={false}
          label={(w) => rooms.get(w.cwd ?? "")?.label}
          sub="stopped while idle · click to reopen"
        />
      )}
    </>
  );
}
