import { useEffect, useState } from "react";
import { create } from "zustand";
import { api, errMessage, failureText } from "./lib/api";
import { CHAT_TASK_ID } from "./lib/derive";
import type { EpicOf } from "./lib/grouping";
import {
  detailsVersions,
  itemDetailsActions,
  samePane,
  type ItemDetailsState,
} from "./lib/item-details-store";
import { forgetAllBut, read, readOneOf, write } from "./lib/persist";
import { keep, only, reuse, sameJson } from "./lib/same";
import { useSpecs } from "./lib/specs";
import { acceptTaskPrReads, type TaskPrSnapshots } from "./lib/task-pr-snapshots";
import type {
  AgentStatus,
  Ticket,
  TicketType,
  Message,
  MessageKind,
  PaneInfo,
  Project,
  RepoHealth,
  ReviewQueue,
  Settings,
  Target,
  TaskView,
  TicketMove,
  WaitingPane,
} from "./lib/types";

export const TABS = ["terminals", "spec", "diff", "pr"] as const;
export const VIEWS = [
  "henchman",
  "work",
  "tickets",
  "chat",
  "routines",
  "repos",
  "reviews",
  "runs",
] as const;
export type Tab = (typeof TABS)[number];
export type View = (typeof VIEWS)[number];

interface Toast {
  id: number;
  /** News or a done thing. A failure is never a toast (NOTE-6): it is `fail`. */
  kind: "info" | "success";
  text: string;
  /** Where a click on it goes (NOTE-4); without one a click only dismisses. */
  target?: Target;
}

interface ToastOpts {
  target?: Target;
  /**
   * Keep it in the message center as this kind (MSG-1). Failures are kept
   * unless this says otherwise; anything else only when it says so. False
   * for news the backend has recorded already (MSG-3).
   */
  record?: MessageKind | false;
}

/**
 * A note on a line of the Diff tab (DIFF-8). Held here rather than in the
 * tab, so switching to another tab loses none.
 */
export interface ReviewNote {
  id: number;
  checkoutId: string;
  repo: string;
  path: string;
  /** The last line it is on; a range's first is `startLine`. */
  line: number;
  /** A removed line is on the old side ("LEFT"); everything else the new. */
  side?: "LEFT" | "RIGHT";
  startLine?: number;
  body: string;
  code: string;
}

export interface State extends ItemDetailsState, TaskPrSnapshots {
  projects: Project[];
  /**
   * How each repository is doing, by project id (REPO-5). Read at launch,
   * when the Repos view opens, and after anything there changes it.
   */
  repoHealth: Record<string, RepoHealth>;
  tasks: TaskView[];
  panes: PaneInfo[];
  /** Panes the last launch did not put back (PANE-7). */
  waiting: WaitingPane[];
  agents: AgentStatus[];
  /** Pull requests waiting on you, and on the configured review team. */
  reviewQueue: ReviewQueue | null;
  reviewQueueLoading: boolean;
  reviewQueueError: string | null;
  settings: Settings | null;
  /** Replacing a token may leave the visible GitHub settings unchanged. */
  githubConnectionRevision: number;
  issues: Ticket[];
  issueTypes: TicketType[];
  /** The site's priorities, highest first, for sorting by them (TKT-13). */
  priorities: string[];
  issuesLoading: boolean;
  /** A fetch has completed, so the list — even an empty one — is a real snapshot. */
  issuesLoaded: boolean;
  /** Jira had more than the app asked for, so the list below is not all of it. */
  issuesTruncated: boolean;

  selectedTask: string | null;
  /** A Jira epic a click is making one, shown until it is or anything is picked (EPIC-13). */
  opening: { epic: EpicOf; repos: string[]; error: string | null } | null;
  view: View;
  tab: Tab;
  sidebarHidden: boolean;
  settingsOpen: boolean;
  /** The Settings tab to open on, when something asked for one ("phone"). */
  settingsSection: string | null;
  /** Ticket key to scroll to and highlight in the Tickets view, then clear. */
  focusIssueKey: string | null;
  /** Pane to select once its task or chat is on screen, then clear (NOTE-4). */
  focusPane: string | null;
  /** Review request (`owner/repo#n`) to scroll to and highlight, then clear. */
  focusReview: string | null;
  toasts: Toast[];
  /** The failure the top bar's status shows (WIN-2), for six seconds. */
  failure: { id: number; text: string } | null;
  /** Background task/pane polls have failed repeatedly — not a toast every few seconds. */
  watchFailing: boolean;
  /** Cursor IDE (the editor app / `cursor` CLI), not the cursor-agent agent. */
  cursorIde: boolean;
  /**
   * False while the window is unfocused or minimized. Polls pause and hidden
   * terminals stop painting; coming back flips this and catches up once.
   */
  appActive: boolean;

  select: (id: string | null) => void;
  setView: (v: View) => void;
  setTab: (t: Tab) => void;
  toggleSidebar: () => void;
  toggleSettings: (open?: boolean, section?: string) => void;
  /** Jump to Tickets and highlight this key — for a task that already has one. */
  showIssue: (key: string) => void;
  clearFocusIssue: () => void;
  showPane: (paneId: string) => void;
  clearFocusPane: () => void;
  /** Jump to Reviews and highlight this pull request. */
  showReview: (id: string) => void;
  clearFocusReview: () => void;
  setAppActive: (active: boolean) => void;

  toast: (kind: Toast["kind"], text: string, opts?: ToastOpts) => void;
  dismissToast: (id: number) => void;
  /**
   * A failure in the background, or of a tool with no place of its own:
   * the top bar's status (WIN-2). One the user can see where they acted
   * goes there instead, through `useFailure` (NOTE-6).
   */
  fail: (e: unknown, opts?: ToastOpts) => void;

  /**
   * The first `refreshAll` has come back, or failed. Until then the window
   * is Layer's mark and nothing else (WIN-4), not views of empty lists.
   */
  booted: boolean;
  refreshAll: () => Promise<void>;
  refreshRepos: () => Promise<void>;
  refreshRepoHealth: () => Promise<void>;
  /**
   * `poll` marks a timer tick: one that lands while another is still out
   * reuses its answer. An explicit refresh — after a delete, a commit, a
   * spawn — never does: it is asking about the world after something
   * changed, and the call already in flight was asked before it did.
   */
  refreshTasks: (opts?: { poll?: boolean }) => Promise<void>;
  refreshPanes: (opts?: { poll?: boolean }) => Promise<void>;
  refreshWaiting: () => Promise<void>;
  refreshPrs: () => Promise<void>;
  refreshTaskPrs: (taskId: string) => Promise<void>;
  /** `quiet` is a timer tick: no toast, and no spinner if a queue is already shown. */
  refreshReviewQueue: (opts?: { quiet?: boolean }) => Promise<void>;
  /** The message center's log (MSG-1), newest first. */
  messages: Message[];
  refreshMessages: () => Promise<void>;
  /** Notes on the Diff tab, by task id (DIFF-6). */
  notes: Record<string, ReviewNote[]>;
  addNote: (taskId: string, note: Omit<ReviewNote, "id">) => void;
  dropNotes: (taskId: string, ids: number[]) => void;
  editNote: (taskId: string, id: number, body: string) => void;
  refreshSettings: (opts?: {
    githubReconnected?: boolean;
    githubDisconnected?: boolean;
    githubReviewTeam?: string | null;
  }) => Promise<void>;
  setSpecFirst: (on: boolean) => Promise<void>; // Spec first (SPEC-38), then the settings again
  /** `quiet` is a timer tick: no toast, and no spinner over a list already shown. */
  refreshIssues: (opts?: { quiet?: boolean }) => Promise<void>;
}

let toastSeq = 0;
let noteSeq = 0;

interface RefreshSlot {
  current: Promise<void> | null;
  next?: Promise<void> | null;
}

/**
 * Share overlapping polls; explicit refreshes share one successor instead.
 * They asked after something changed, which the in-flight read cannot see.
 */
async function coalesce(
  slot: RefreshSlot,
  poll: boolean,
  work: () => Promise<void>,
): Promise<void> {
  if (slot.current) {
    if (poll) return slot.current;
    if (!slot.next) {
      slot.next = slot.current
        .catch(() => {})
        .then(() => {
          slot.next = null;
          return coalesce(slot, false, work);
        });
    }
    return slot.next;
  }
  const run = work();
  slot.current = run;
  try {
    await run;
  } finally {
    if (slot.current === run) slot.current = null;
  }
}

const tasksSlot: RefreshSlot = { current: null };
const panesSlot: RefreshSlot = { current: null };
const messagesSlot: RefreshSlot = { current: null };
let reviewsSlot: RefreshSlot = { current: null };
// A slow sweep can outlast its timer. Allow one successor per connection,
// but don't make a reconnect wait for an obsolete sweep.
let sweepSlot = { busy: false, again: false };
const settingsReads = { started: 0, accepted: 0 };

function sameMessages(a: Message[], b: Message[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (m, i) =>
        m.id === b[i].id && m.at === b[i].at && m.read === b[i].read && m.count === b[i].count,
    )
  );
}

/** How long the splash waits for the first lists before the window opens anyway. */
const BOOT_WAIT = 1500;

// A sweep walks every task and can take a minute. A late result once replaced
// the newly opened PR with the earlier snapshot, so the panel offered to open
// it again. Use request order, not timestamps that two reads can share.
let prReadSequence = 0;
const latestTaskPrRequest = new Map<string, number>();

function resetReviewReads() {
  // An obsolete search (or its queued successor) cannot publish or block the next one.
  reviewsSlot = { current: null };
  return { reviewQueueLoading: false, reviewQueueError: null };
}

function resetGitHubReads(revision: number): Partial<State> {
  sweepSlot = { busy: false, again: false };
  latestTaskPrRequest.clear();
  return {
    ...resetReviewReads(),
    githubConnectionRevision: revision + 1,
    prs: {},
    prReadRevision: {},
    reviewQueue: null,
  };
}

/** "ACME:review": a project and stage already told to choose a status, this run. */
const askedFlow = new Set<string>();

/** Say what the PR sweep did about tickets (TKT-8). */
function reportTickets(moves: TicketMove[], toast: State["toast"], fail: State["fail"]) {
  for (const m of moves) {
    const target: Target = `ticket:${m.key}`;
    if (m.moved_to) toast("success", `${m.key} → ${m.moved_to}`, { target, record: "ticket" });
    else if (m.error) fail(m.error, { target, record: "ticket" });
    else if (m.unchosen) {
      const project = m.key.split("-")[0];
      if (askedFlow.has(`${project}:${m.stage}`)) continue;
      askedFlow.add(`${project}:${m.stage}`);
      toast(
        "info",
        m.stage === "review"
          ? `${m.key}'s pull request is ready for review. Choose which ${project} status means review in Settings → Tickets, and tickets will move there.`
          : `Every pull request for ${m.key} has merged. Choose where merged ${project} tickets go in Settings → Tickets.`,
      );
    }
  }
}

/**
 * Consecutive quiet failures of the task/pane polls. One blip is noise; two
 * in a row usually means the backend is unreachable or a token is gone.
 */
let watchStreak = 0;

/**
 * Drop a remembered selection whose task is gone.
 *
 * The id outlives the task in storage, and a dangling one would leave the app
 * opening on a task that no longer exists — an empty pane bar and no way back.
 */
function stillThere(tasks: TaskView[], selected: string | null): string | null {
  if (!selected || tasks.some((t) => t.id === selected)) return selected;
  write("selectedTask", null);
  return null;
}

export const useStore = create<State>((set, get) => {
  const watchOk = () => {
    watchStreak = 0;
    if (get().watchFailing) set({ watchFailing: false });
  };
  const watchFail = () => {
    watchStreak += 1;
    if (watchStreak >= 2) set({ watchFailing: true });
  };

  return {
    ...itemDetailsActions(set, get),
    projects: [],
    repoHealth: {},
    tasks: [],
    panes: [],
    waiting: [],
    prs: {},
    prReadRevision: {},
    reviewQueue: null,
    messages: [],
    reviewQueueLoading: false,
    reviewQueueError: null,
    agents: [],
    settings: null,
    githubConnectionRevision: 0,
    issues: [],
    issueTypes: [],
    priorities: [],
    issuesLoading: false,
    issuesLoaded: false,
    issuesTruncated: false,

    // Where the app was left. Restored so reopening lands on the work in
    // progress rather than on an empty shell, alongside the panes themselves.
    selectedTask: read<string | null>("selectedTask", null),
    opening: null,
    view: readOneOf("view", VIEWS, "work"),
    tab: readOneOf("tab", TABS, "terminals"),
    sidebarHidden: read("sidebarHidden", false),
    settingsOpen: false,
    settingsSection: null,
    focusIssueKey: null,
    focusPane: null,
    focusReview: null,
    toasts: [],
    failure: null,
    watchFailing: false,
    cursorIde: false,
    appActive: true,
    booted: false,

    // Selecting a task is always a request to look at it.
    select: (id) => set({ selectedTask: id, opening: null, tab: "terminals", view: "work" }),
    setView: (view) => set({ view }),
    setTab: (tab) => set({ tab }),
    toggleSidebar: () => set((s) => ({ sidebarHidden: !s.sidebarHidden })),
    toggleSettings: (open, section) =>
      set((s) => ({ settingsOpen: open ?? !s.settingsOpen, settingsSection: section ?? null })),
    showIssue: (key) => set({ view: "tickets", focusIssueKey: key }),
    clearFocusIssue: () => set({ focusIssueKey: null }),
    showPane: (focusPane) => set({ focusPane }),
    clearFocusPane: () => set({ focusPane: null }),
    showReview: (id) => set({ view: "reviews", focusReview: id }),
    clearFocusReview: () => set({ focusReview: null }),
    // Terminals watch this themselves: one that is no longer shown detaches,
    // and the backend stops sending its redraws to a webview nobody is
    // looking at.
    setAppActive: (appActive) => {
      if (get().appActive === appActive) return;
      set({ appActive });
    },

    toast: (kind, text, opts) => {
      const id = ++toastSeq;
      set((s) => ({ toasts: [...s.toasts, { id, kind, text, target: opts?.target }] }));
      const keep = opts?.record ?? false;
      // Swallowed: failing to keep a message must not raise another.
      if (keep) void api.addMessage(keep, kind, text, opts?.target).catch(() => {});
      setTimeout(() => get().dismissToast(id), 4000);
    },
    dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
    fail: (e, opts) => {
      const id = ++toastSeq;
      const text = failureText(e);
      set({ failure: { id, text } });
      const keep = opts?.record ?? "error";
      // Swallowed: failing to keep an error must not raise another.
      if (keep) void api.addMessage(keep, "error", text, opts?.target).catch(() => {});
      // Only its own: a newer failure keeps its full six seconds.
      setTimeout(() => {
        if (get().failure?.id === id) set({ failure: null });
      }, 6000);
    },

    refreshAll: async () => {
      // Settings (and with them the Jira list) must not wait on a full git-status
      // of every worktree: that is the slow part of boot, and Tickets used to
      // sit black until it finished. Kick both off together.
      const settingsP = get().refreshSettings();
      const focus = get().selectedTask;
      const taskVersion = ++detailsVersions.tasks;
      const paneVersion = ++detailsVersions.panes;
      const coreP = Promise.all([
        api.listProjects(),
        api.listTasks(focus),
        api.listPanes(),
        api.listAgents(),
        api.cursorIdeInstalled().catch(() => false),
      ]).then(([projects, tasks, panes, agents, cursorIde]) => {
        // Through `reuse`, as the polls do: it runs after every Sync, Clean up
        // and Add repos, and a wholesale swap redrew every task row, pane and
        // agent card even when the operation changed none of them.
        set((s) => {
          const kept =
            taskVersion === detailsVersions.tasks ? reuse(s.tasks, tasks, (t) => t.id) : s.tasks;
          return {
            projects: reuse(s.projects, projects, (p) => p.id),
            tasks: kept,
            panes:
              paneVersion === detailsVersions.panes
                ? reuse(s.panes, panes, (p) => p.id, samePane)
                : s.panes,
            agents: reuse(s.agents, agents, (a) => a.id),
            cursorIde,
            selectedTask: stillThere(kept, s.selectedTask),
          };
        });
        watchOk();
      });
      // The splash waits for both, but no longer than BOOT_WAIT: a slow git
      // status then fills the lists in an open window rather than holding it.
      const opened = setTimeout(() => {
        if (!get().booted) set({ booted: true });
      }, BOOT_WAIT);
      try {
        await Promise.all([settingsP, coreP]);
      } finally {
        clearTimeout(opened);
        // A failed first read still opens the window: the failure is in the
        // top bar's status, and a splash that never ends would hide it.
        if (!get().booted) set({ booted: true });
      }
    },

    refreshRepos: async () => {
      const projects = await api.listProjects();
      set((s) => ({ projects: reuse(s.projects, projects, (p) => p.id) }));
    },

    refreshRepoHealth: async () => {
      const rows = await api.repoHealth();
      set((s) => ({
        repoHealth: keep(s.repoHealth, Object.fromEntries(rows.map((h) => [h.project_id, h]))),
      }));
    },

    refreshTasks: (opts) =>
      coalesce(tasksSlot, opts?.poll ?? false, async () => {
        try {
          // Focus the selected task so its worktrees stay fresh; everything else
          // reuses the backend status cache unless it has an agent at work.
          const version = ++detailsVersions.tasks;
          const fetched = await api.listTasks(get().selectedTask);
          if (version !== detailsVersions.tasks) return;
          const live = new Set(fetched.map((t) => t.id));
          const before = get().tasks;
          set((s) => {
            // Whole, not field by field. A hand-picked list missed the base:
            // changed in the PR panel, Update from base went on saying
            // "← origin/main" while the backend merged the new one. A few dozen
            // tasks stringify in well under a millisecond, and nothing in them
            // changes on its own between polls.
            const tasks = reuse(s.tasks, fetched, (t) => t.id);
            if (tasks === s.tasks) return s;
            return {
              tasks,
              selectedTask: stillThere(tasks, s.selectedTask),
              // PR rows for a task that has been deleted have nothing to hang off any
              // more, and the watch would keep comparing against them for good.
              // Its Diff notes likewise, kept for as long as the app ran.
              prs: only(s.prs, live),
              prReadRevision: only(s.prReadRevision, live),
              notes: only(s.notes, live),
            };
          });
          for (const id of latestTaskPrRequest.keys()) {
            if (!live.has(id)) latestTaskPrRequest.delete(id);
          }
          // Its spec, edits and drafts too, which live in their own store.
          const byTask = useSpecs.getState().byTask;
          const specs = only(byTask, live);
          if (specs !== byTask) useSpecs.setState({ byTask: specs });
          // And the layout remembered for it in localStorage. The chats keep
          // theirs: they are no task, and never deleted.
          if (get().tasks !== before) {
            forgetAllBut(["activePane", "browserOpen"], new Set([...live, CHAT_TASK_ID]));
          }
          watchOk();
        } catch (e) {
          watchFail();
          throw e;
        }
      }),
    refreshPanes: (opts) =>
      coalesce(panesSlot, opts?.poll ?? false, async () => {
        try {
          const version = ++detailsVersions.panes;
          const fetched = await api.listPanes();
          if (version !== detailsVersions.panes) return;
          set((s) => {
            const panes = reuse(s.panes, fetched, (p) => p.id, samePane);
            return panes === s.panes ? s : { panes };
          });
          watchOk();
        } catch (e) {
          watchFail();
          throw e;
        }
      }),

    refreshWaiting: async () => {
      const version = ++detailsVersions.waiting;
      const waiting = await api.waitingPanes();
      if (version !== detailsVersions.waiting) return;
      set((s) => (sameJson(s.waiting, waiting) ? s : { waiting }));
    },

    /**
     * Ask GitHub what has happened to every task's pull requests.
     *
     * Quiet on failure: this runs on a timer whether or not anyone is looking,
     * and a token that has expired or a network that has gone away should not
     * put a toast on screen every couple of minutes.
     */
    refreshPrs: async () => {
      const { settings, prs } = get();
      if (!settings?.github_connected) return;
      const slot = sweepSlot;
      if (slot.busy) {
        slot.again = true;
        return;
      }
      slot.busy = true;
      const began = ++prReadSequence;
      try {
        const all = await api.githubAllPrs(Object.keys(prs));
        if (slot !== sweepSlot) return;
        // Merged, not replaced: a task the sweep could not read is left out of
        // its result, and dropping it here would blank a panel that had just
        // fetched those rows for itself. An explicit read can also land while
        // the sweep is still reading other tasks, even if that read began first.
        set((s) => {
          const fresh = all.filter(
            ({ task_id: id }) =>
              (latestTaskPrRequest.get(id) ?? 0) < began && (s.prReadRevision[id] ?? 0) < began,
          );
          return fresh.length === 0 ? s : acceptTaskPrReads(s, fresh, ++prReadSequence);
        });
        const moves = all.map((t) => t.ticket).filter((m): m is TicketMove => m !== null);
        reportTickets(moves, get().toast, get().fail);
        if (moves.some((m) => m.moved_to)) void get().refreshIssues({ quiet: true });
      } catch {
        // Left as it was: stale rows beat empty ones.
      } finally {
        slot.busy = false;
        if (slot === sweepSlot && slot.again) {
          slot.again = false;
          await get().refreshPrs();
        }
      }
    },

    notes: {},
    addNote: (taskId, note) =>
      set((s) => ({
        notes: { ...s.notes, [taskId]: [...(s.notes[taskId] ?? []), { ...note, id: ++noteSeq }] },
      })),
    dropNotes: (taskId, ids) =>
      set((s) => ({
        notes: { ...s.notes, [taskId]: (s.notes[taskId] ?? []).filter((n) => !ids.includes(n.id)) },
      })),
    editNote: (taskId, id, body) =>
      set((s) => ({
        notes: {
          ...s.notes,
          [taskId]: (s.notes[taskId] ?? []).map((n) => (n.id === id ? { ...n, body } : n)),
        },
      })),

    refreshTaskPrs: async (taskId) => {
      if (!get().settings?.github_connected) return;
      const began = ++prReadSequence;
      const connection = get().githubConnectionRevision;
      latestTaskPrRequest.set(taskId, began);
      const current = () =>
        connection === get().githubConnectionRevision &&
        latestTaskPrRequest.get(taskId) === began &&
        (get().prReadRevision[taskId] ?? 0) < began;
      try {
        const rows = await api.githubTaskPrs(taskId);
        if (!current()) return;
        set((s) => acceptTaskPrReads(s, [{ task_id: taskId, rows }], ++prReadSequence));
      } catch (error) {
        if (current()) throw error;
      }
    },

    setSpecFirst: (on) => api.setSpecFirst(on).then(() => get().refreshSettings()),
    refreshSettings: async (opts) => {
      const read = ++settingsReads.started;
      const connectionSaved = opts?.githubReconnected || opts?.githubDisconnected;
      const prior = get().settings;
      const team = opts?.githubReviewTeam;
      const teamSaved = team !== undefined && prior?.github && team !== prior.github.review_team;
      // A confirmed save supersedes earlier settings snapshots even before its reload.
      if (connectionSaved || teamSaved) settingsReads.accepted = read;
      if (connectionSaved) {
        set((s) => ({
          ...resetGitHubReads(s.githubConnectionRevision),
          ...(opts.githubDisconnected && s.settings
            ? { settings: { ...s.settings, github_connected: false, github: null } }
            : {}),
        }));
      } else if (teamSaved && prior?.github) {
        const github = { ...prior.github, review_team: team };
        set({ ...resetReviewReads(), settings: { ...prior, github } });
        void get().refreshReviewQueue({ quiet: true });
      }
      const settings = await api.getSettings();
      if (read < settingsReads.accepted) return;
      settingsReads.accepted = read;
      const { settings: before, githubConnectionRevision: revision } = get();
      const connectionChanged =
        Boolean(before?.github_connected) !== settings.github_connected ||
        before?.github?.api_url !== settings.github?.api_url ||
        before?.github?.web_url !== settings.github?.web_url;
      const teamChanged = before?.github?.review_team !== settings.github?.review_team;
      set({
        ...(!opts?.githubReconnected && connectionChanged ? resetGitHubReads(revision) : {}),
        ...(teamChanged && !connectionChanged ? resetReviewReads() : {}),
        settings: keep(before, settings),
      });
      // Only until the first list lands: someone with nothing assigned has an
      // empty list for good, and every settings change re-fetched 500 issues.
      if (settings.tracker != null && !get().issuesLoaded && !get().issuesLoading) {
        void get().refreshIssues();
      }
      // Disconnecting has to take the list with it, or the badge keeps counting
      // tickets from a site the app can no longer reach.
      if (settings.tracker == null && (get().issues.length > 0 || get().issuesLoaded)) {
        set({
          issues: [],
          issueTypes: [],
          priorities: [],
          issuesTruncated: false,
          issuesLoaded: false,
        });
      }
      // A team change needs a new search, but does not change the connection.
      if (!settings.github_connected) return;
      if (opts?.githubReconnected || teamChanged || (!teamSaved && get().reviewQueue === null))
        void get().refreshReviewQueue({ quiet: true });
    },

    refreshReviewQueue: (opts) => {
      const slot = reviewsSlot;
      const current = () => slot === reviewsSlot;
      return coalesce(slot, !!opts?.quiet, async () => {
        if (!current() || !get().settings?.github_connected) return;
        // A timer tick should not flash the spinner over a list already on
        // screen. The first load has nothing to keep, so it still shows one.
        const spin = !opts?.quiet || get().reviewQueue === null;
        if (spin) set({ reviewQueueLoading: true });
        try {
          const queue = await api.githubReviewQueue();
          if (!current()) return;
          set((s) => ({ reviewQueue: keep(s.reviewQueue, queue), reviewQueueError: null }));
        } catch (e) {
          if (!current()) return;
          // A timer tick stays quiet — the view shows the message — but a
          // Refresh you pressed should also say so up top.
          set({ reviewQueueError: errMessage(e) });
          if (!opts?.quiet) get().fail(e);
        } finally {
          if (spin && current()) set({ reviewQueueLoading: false });
        }
      });
    },

    refreshMessages: () =>
      coalesce(messagesSlot, false, async () => {
        const messages = await api.listMessages();
        if (!sameMessages(get().messages, messages)) set({ messages });
      }),

    refreshIssues: async (opts) => {
      if (get().settings?.tracker == null) return;
      // A timer tick should not flash the spinner over a list already on screen.
      const spin = !opts?.quiet || !get().issuesLoaded;
      if (spin) set({ issuesLoading: true });
      try {
        const [page, issueTypes, priorities] = await Promise.all([
          api.ticketsMine(),
          // Types rarely change and are cached in the backend; a failure here
          // must not stop the issues themselves from showing.
          api.ticketTypes().catch(() => get().issueTypes),
          // Asked once per connection, as rarely changed. Without them a sort
          // by priority keeps the order it had.
          get().priorities.length > 0 ? get().priorities : api.ticketPriorities().catch(() => []),
        ]);
        // As for reviews: an answer from a site disconnected meanwhile is not kept.
        if (get().settings?.tracker == null) return;
        set((s) => ({
          issues: reuse(s.issues, page.issues, (i) => i.key),
          issuesTruncated: page.more,
          issueTypes: keep(s.issueTypes, issueTypes),
          priorities: keep(s.priorities, priorities),
          issuesLoaded: true,
        }));
      } catch (e) {
        if (!opts?.quiet) get().fail(e);
      } finally {
        if (spin) set({ issuesLoading: false });
      }
    },
  };
});

/**
 * Panes stopped on purpose — Stop, or the old agent in a handoff — whose exit
 * is not news.
 *
 * `kill_pane` keeps the pane on screen, so its exit arrives like any other:
 * stopping an agent put "exited with 143" up as an error and posted "finished"
 * to Slack for work that had not finished.
 */
const stopping = new Set<string>();
export function markStopping(paneId: string) {
  stopping.add(paneId);
}
/**
 * Whether a pane's exit is news: a task's agent, gone without being asked
 * to. Asked of every exit, whatever the pane, since that is when its mark
 * is cleared. Asked only of a task's agents, a stopped chat or shell kept
 * its mark for as long as the app ran.
 */
export function exitIsNews(paneId: string, pane: PaneInfo | undefined): boolean {
  const onPurpose = stopping.delete(paneId);
  return !!pane && pane.kind === "agent" && pane.task_id !== CHAT_TASK_ID && !onPurpose;
}

export const selectedTask = (s: State) => s.tasks.find((t) => t.id === s.selectedTask) ?? null;

/**
 * The time, for labels that change with it: "2m ago".
 *
 * Those were worked out from `Date.now()` whenever something happened to
 * redraw, and a pane that has gone quiet is exactly one that stops changing —
 * so nothing redrew, and the label froze. Ticks only while the window is in
 * front.
 */
export function useNow(everyMs: number): number {
  const active = useStore((s) => s.appActive);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    // guard: allow poll — a clock for "3m ago" labels, stopped while the window is away.
    const t = setInterval(() => setNow(Date.now()), everyMs);
    return () => clearInterval(t);
  }, [active, everyMs]);
  return now;
}

// Remember where the app is, so reopening it lands back there rather than on
// an empty shell. Done by subscription rather than in each setter: the panes
// are restored whatever moved the selection, and this should be too.
// Compared field by field: this runs on every change to the store, each pane
// poll and toast included, and stringifying the four on each one was work
// done to find out that nothing moved.
let lastLayout: unknown[] = [];
useStore.subscribe((s) => {
  const now = [s.selectedTask, s.view, s.tab, s.sidebarHidden];
  if (now.every((v, i) => v === lastLayout[i])) return;
  lastLayout = now;
  write("selectedTask", s.selectedTask);
  write("view", s.view);
  write("tab", s.tab);
  write("sidebarHidden", s.sidebarHidden);
});
