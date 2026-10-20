import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "./lib/api";
import { selectedTask, useStore, type View } from "./store";
import { CHAT_TASK_ID, needsYou, repoTrouble, reviewCounts, taskTotals } from "./lib/derive";
import type { TaskView } from "./lib/types";
import { Sidebar } from "./components/sidebar/Sidebar";
import { SidebarGrip } from "./components/sidebar/SidebarGrip";
import { Terminals } from "./components/Terminals";
import { DiffView } from "./components/DiffView";
import { PrPanel } from "./components/PrPanel";
import { SpecView } from "./components/SpecView";
import { specProgress, useSpecs } from "./lib/specs";
import { TicketsView } from "./components/tickets/TicketsView";
import { AgentsView } from "./components/AgentsView";
import { ChatView } from "./components/ChatView";
import { ReposView } from "./components/ReposView";
import { ReviewsView } from "./components/ReviewsView";
import { UsageView } from "./components/UsageView";
import { Settings } from "./components/Settings";
import { UpdateFromBase } from "./components/UpdateFromBase";
import { CupIcon, GearIcon } from "./components/icons";
import { PhoneBadge } from "./components/PhoneSettings";
import { statusClass } from "./components/tickets/status";
import { SidebarToggle } from "./components/ui";
import { MessageCenter } from "./components/MessageCenter";
import { FrameStatus } from "./components/FrameStatus";
import { Watchers } from "./Watchers";
import { goTo } from "./lib/goto";

/**
 * The top bar (Layer's exception to design/layouts/app-window.md): the views
 * as tabs, and the tools for the whole app. No wordmark: beside macOS's
 * traffic lights the blackletter clashed with them, and the window already
 * says which app it is. A count
 * shows only where it asks for something: agents waiting on you, reviews
 * asked of you, repositories to fix. A running agent is a live dot.
 */
function TopBar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const toggleSettings = useStore((s) => s.toggleSettings);
  // Counts, not the lists: a number that has not changed is not a redraw.
  // Chats have their own tab, so they must not be counted as work as well.
  const running = useStore(
    (s) => s.panes.some((p) => p.kind === "agent" && p.running && p.task_id !== CHAT_TASK_ID),
  );
  const chatting = useStore((s) => s.panes.some((p) => p.task_id === CHAT_TASK_ID && p.running));
  // The dock icon's number, with a place in the window that says what it is.
  const waiting = useStore((s) => s.panes.filter(needsYou).length);
  const repoTroubles = useStore(
    (s) => s.projects.filter((p) => repoTrouble(p, s.repoHealth[p.id], s.tasks)).length,
  );
  // Two strings rather than one object, so an unchanged count is no redraw.
  const toReview = useStore((s) => reviewCounts(s.reviewQueue).toReview);
  const yours = useStore((s) => reviewCounts(s.reviewQueue).yours);

  const tabs: {
    id: View;
    label: string;
    count?: string | number;
    title?: string;
    /** Whether the count is news (a live dot beside it), or only a number. */
    news?: boolean;
    live?: boolean;
  }[] = [
    {
      id: "work",
      label: "Work",
      count: waiting || undefined,
      title: `${waiting} agent${waiting === 1 ? " needs" : "s need"} you — asking for something, or finished and not looked at yet`,
      news: true,
      live: running,
    },
    { id: "tickets", label: "Tickets" },
    {
      id: "reviews",
      label: "Reviews",
      // Both of the view's counts, in its tabs' order: the first alone made
      // twenty-four PRs of yours read as none. Only the first asks something
      // of you, so only it earns the live dot.
      count: [toReview, yours].some((n) => n && n !== "0")
        ? (yours === undefined ? toReview : `${toReview ?? "–"} · ${yours}`)
        : undefined,
      title: `${toReview ?? "?"} to review · ${yours ?? "?"} opened by you`,
      news: !!toReview && toReview !== "0",
    },
    { id: "chat", label: "Chat", live: chatting },
    { id: "runs", label: "Usage" },
    {
      id: "repos",
      label: "Repos",
      count: repoTroubles || undefined,
      title: "Repositories with a problem the Repos view can fix",
    },
  ];

  return (
    // The native title bar is hidden under this one (`titleBarStyle:
    // Overlay`), so this is what the window is moved by. Tauri's attribute,
    // not CSS: `-webkit-app-region` is Chromium's, and macOS's WebKit
    // ignores it, which left a window nothing could grab. "deep" makes the
    // bar's empty parts drag; its buttons stay buttons.
    <header className="topbar" data-tauri-drag-region="deep">
      <div className="topbar-left" />
      <nav className="topbar-center ui-tabs" aria-label="Views">
        {tabs.map((t) => (
          <button
            key={t.id}
            className="ui-tab viewtab"
            aria-current={view === t.id ? "page" : undefined}
            onClick={() => setView(t.id)}
          >
            <span className="viewtab-label" data-label={t.label}>{t.label}</span>
            {t.count !== undefined ? (
              <span className="viewtab-count" title={t.title}>
                {t.news && <span className="ui-live-dot" aria-hidden />}
                {t.count}
              </span>
            ) : (
              t.live && <span className="ui-live-dot" title="Running" />
            )}
          </button>
        ))}
      </nav>
      <div className="topbar-right">
        <FrameStatus />
        <PhoneBadge />
        <KeepAwake />
        <MessageCenter />
        <button className="ui-button ui-icon-button" title="Settings" aria-label="Settings" onClick={() => toggleSettings(true)}>
          <GearIcon />
        </button>
      </div>
    </header>
  );
}

/**
 * Keep the Mac awake while an agent works (STATE-7), or while any agent runs
 * when a phone's way in is open (PHONE-8). The switch is all this does:
 * `attention.rs` holds the Mac awake, so it holds while the window is away,
 * when this webview's timers may not run at all.
 */
function KeepAwake() {
  const on = useStore((s) => s.settings?.ui.keep_awake ?? false);
  const phone = useStore((s) => s.settings?.phone_open ?? false);
  const working = useStore(
    (s) => s.panes.filter((p) => p.kind === "agent" && p.running && (phone || p.activity === "working")).length,
  );
  const refreshSettings = useStore((s) => s.refreshSettings);
  const fail = useStore((s) => s.fail);

  async function toggle() {
    // The prefs as saved now, so this cannot put back an older value.
    const ui = useStore.getState().settings?.ui;
    if (!ui) return;
    try {
      await api.setUiPrefs({ ...ui, keep_awake: !ui.keep_awake });
      await refreshSettings();
    } catch (e) {
      fail(e);
    }
  }

  const title = !on
    ? "Keep the Mac awake while an agent is working"
    : working
      ? phone
        ? `Keeping the Mac awake for the phone: ${working} agent${working === 1 ? " is" : "s are"} running. Click to let it sleep.`
        : `Keeping the Mac awake: ${working} agent${working === 1 ? " is" : "s are"} working. Click to let it sleep.`
      : phone
        ? "The Mac stays awake whenever an agent is running, for the phone. None is now, so it may sleep. Click to turn off."
        : "The Mac stays awake whenever an agent is working. None is now, so it may sleep. Click to turn off.";
  return (
    <button
      className={`ui-button ui-icon-button awake${on ? " on" : ""}${on && working ? " holding" : ""}`}
      title={title}
      aria-pressed={on}
      onClick={() => void toggle()}
    >
      <CupIcon />
    </button>
  );
}

/** The selected task: its header, its tabs, and whichever tab is open. */
function TaskMain({ task }: { task: TaskView }) {
  const tab = useStore((s) => s.tab);
  const setTab = useStore((s) => s.setTab);
  const cursorIde = useStore((s) => s.cursorIde);
  const fail = useStore((s) => s.fail);
  // Where the board has it, from your ticket list: in step with the work or not.
  const ticket = useStore((s) => s.issues.find((i) => i.key === task.issue_key));
  // The Agents badge counts what is open now. The task's own count comes
  // from the task poll, which can be a minute behind a pane just opened.
  const paneCount = useStore((s) => s.panes.filter((p) => p.task_id === task.id).length);
  const [updating, setUpdating] = useState(false);
  // The Spec tab's badge says how far the work is through its steps, or how
  // many requirements there are before it has any (SPEC-8), so the spec is
  // read with the task.
  const specBadge = useSpecs((s) => specProgress(s.byTask[task.id]?.spec ?? null));
  const loadSpec = useSpecs((s) => s.load);
  useEffect(() => { void loadSpec(task.id).catch(() => {}); }, [task.id, loadSpec]);

  const totals = taskTotals(task);
  // What the Diff tab lists by default: files with uncommitted changes. The
  // sidebar's counts split the same work into staged, unstaged and untracked,
  // and adding those up put a 2 beside a list of forty files.
  const changed = totals.changed;

  return (
    <>
      <div className="ws-header">
        <SidebarToggle />
        <h1>{task.name}</h1>
        <span className="branch">{task.branch}</span>
        <div className="chips">
          {task.checkouts.map((c) => (
            <span
              key={c.id}
              className="ui-chip"
              title={c.broken ?? c.path}
              style={!c.exists || c.broken ? { color: "var(--status-danger-ink)" } : undefined}
            >
              {c.project_name}
            </span>
          ))}
        </div>
        <div className="spacer" />
        <div className="chips">
          {totals.unlinked > 0 && (
            <span
              className="ui-status ui-status-danger"
              title={task.checkouts.filter((c) => c.broken).map((c) => `${c.project_name}: ${c.broken}`).join("\n")}
            >
              {totals.unlinked} worktree{totals.unlinked === 1 ? "" : "s"} unlinked
            </span>
          )}
          {totals.missing > 0 && (
            <span className="ui-status ui-status-warn">
              {totals.missing} worktree{totals.missing === 1 ? "" : "s"} missing
            </span>
          )}
          {totals.ahead > 0 && <span className="ui-chip">↑{totals.ahead}</span>}
          {totals.behind > 0 && <span className="ui-status ui-status-warn">↓{totals.behind}</span>}
          {totals.conflicted > 0 && (
            <button
              className="ui-status ui-status-danger"
              title="Resolve with an agent, or abandon the update"
              onClick={() => setUpdating(true)}
            >
              {totals.conflicted} conflict{totals.conflicted === 1 ? "" : "s"}
            </button>
          )}
          <button
            className="ui-button"
            title="Fetch each repo's base branch and merge or rebase this task's branch onto it"
            onClick={() => setUpdating(true)}
          >
            Update from base
          </button>
        </div>
        {/* Elsewhere: links, not buttons (design/components.md). */}
        {(cursorIde || task.issue_url) && (
          <div className="ws-links">
            {cursorIde && (
              <button
                className="ui-link"
                title={`Open ${task.root} in Cursor`}
                onClick={() => void api.openInCursor(task.root).catch(fail)}
              >
                Cursor ↗
              </button>
            )}
            {task.issue_url && (
              <button
                className="ui-link"
                title={`Open ${task.issue_key} in Jira`}
                onClick={() => void openUrl(task.issue_url!)}
              >
                {task.issue_key} ↗
              </button>
            )}
            {ticket && (
              <span className={`ui-status status-pill ${statusClass(ticket.status_category)}`}>{ticket.status}</span>
            )}
          </div>
        )}
      </div>

      <div className="ui-tabs task-tabs" role="tablist">
        <button className="ui-tab" role="tab" aria-selected={tab === "spec"} onClick={() => setTab("spec")}>
          Spec{specBadge && <span className="ui-tab-count">{specBadge}</span>}
        </button>
        <button
          className="ui-tab"
          role="tab"
          aria-selected={tab === "terminals"}
          onClick={() => setTab("terminals")}
        >
          Agents<span className="ui-tab-count">{paneCount}</span>
        </button>
        <button className="ui-tab" role="tab" aria-selected={tab === "diff"} onClick={() => setTab("diff")}>
          Diff{changed > 0 && <span className="ui-tab-count">{changed}</span>}
        </button>
        <button className="ui-tab" role="tab" aria-selected={tab === "pr"} onClick={() => setTab("pr")}>
          Pull requests
          {task.checkouts.length > 1 && (
            <span className="ui-tab-count">{task.checkouts.length}</span>
          )}
        </button>
      </div>

      <div className="content">
        {/* Terminals stay mounted so xterm state survives tab switches. */}
        <div
          style={{
            display: tab === "terminals" ? "flex" : "none",
            flexDirection: "column",
            flex: 1,
            overflow: "hidden",
          }}
        >
          <Terminals task={task} />
        </div>
        {tab === "spec" && <SpecView task={task} />}
        {tab === "diff" && <DiffView task={task} />}
        {tab === "pr" && <PrPanel task={task} onUpdateFromBase={() => setUpdating(true)} />}
      </div>
      {updating && <UpdateFromBase task={task} onClose={() => setUpdating(false)} />}
    </>
  );
}

function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismissToast = useStore((s) => s.dismissToast);
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`toast ${t.kind}${t.target ? " linked" : ""}`}
          title={t.target ? "Open" : undefined}
          onClick={() => {
            if (t.target) goTo(t.target);
            dismissToast(t.id);
          }}
        >
          {t.text}
        </div>
      ))}
    </div>
  );
}

export default function App() {
  const task = useStore(selectedTask);
  const view = useStore((s) => s.view);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const sidebarHidden = useStore((s) => s.sidebarHidden);
  const scale = useStore((s) => s.settings?.ui.scale ?? 1);

  return (
    <div className="shell" style={{ ["--ui-scale" as string]: scale }}>
      <Watchers />
      <TopBar />

      <div className="views">
        {view === "work" && (
          <div className={`app${sidebarHidden ? " no-sidebar" : ""}`}>
            <Sidebar />
            <SidebarGrip />
            <div className="main">
              {task ? (
                <TaskMain key={task.id} task={task} />
              ) : (
                // With nothing selected, show what every agent is doing rather
                // than an empty panel — the sidebar covers the task list, this
                // covers the fleet.
                <AgentsView />
              )}
            </div>
          </div>
        )}

        {view === "tickets" && <TicketsView />}
        {view === "reviews" && <ReviewsView />}
        {view === "chat" && <ChatView />}
        {view === "runs" && <UsageView />}
        {view === "repos" && <ReposView />}
      </div>

      {settingsOpen && <Settings />}
      <Toasts />
    </div>
  );
}
