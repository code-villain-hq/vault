/**
 * The real UI in a plain browser, with the Rust side answered from `world.ts`.
 *
 * The native window can be screenshotted but not driven, so this is how a
 * change to the UI gets looked at and clicked through — by a person or an
 * agent with a browser. Served by the same Vite that `bun run dev` and
 * `bun run dev:app` start; open /mock.html on it. Nothing here is bundled
 * into the app: `vite build` only builds index.html.
 *
 * URL parameters set the scene before the app boots:
 *
 *   ?scenario=busy|empty|unlinked|tickets|reviews   which world (default busy)
 *   &view=work|tickets|reviews|chat|runs|repos
 *   &task=t-login          the selected task (none: the All agents overview)
 *   &tab=spec|terminals|diff|pr
 *   &loop=checking|held|passed|gave_up   the task's agents on a loop (§21)
 *   &spec=split            ACME-123 with a spec per repository from before (SPEC-25)
 *   &role=member          a member of the workspace, who sees its settings (SET-7)
 *   &theme=night|day|tokyo-night|cursor-dark|cursor-light|ayu-dark|ayu-light|system   the saved theme (SET-5)
 *
 * From the console, or a browser tool's JavaScript:
 *
 *   __mock.calls                     every command invoked, with its arguments
 *   __mock.on("cmd", (args) => …)    answer a command differently from now on
 *   __mock.emit("pty:activity", id)  deliver a backend event
 *   __mock.world                     the data being served; edit it, then emit
 *
 * A command with no answer here logs "[mock] unanswered" and returns null.
 * That is the cue to add it to `answer` below.
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { BrowserView, Catchup, Cleaned, FlowStatus, LogSource, Message, PaneInfo, PhoneStatus, Project, RepoUpdate, StartTo, Synced, TaskView } from "../lib/types";
import { sortTickets, type TicketSort } from "../lib/ticket-sort";
import { ago, SCENARIOS } from "./world";
import { acpAnswers, seedAcp } from "./acp";
import { loopAnswers, seedLoop } from "./loop";
import { specAnswers } from "./spec";
import { epicAnswers, seedEpic } from "./epic";
import { runAnswers } from "./runs";
import { profileAnswers } from "./profiles";

type Args = Record<string, unknown>;
type Answer = (args: Args) => unknown;

const params = new URLSearchParams(location.search);
// A Map, so only the scenarios' own names are found: looked up on the plain
// object, `?scenario=toString` found Object's method and built no world.
const scenarios = new Map(Object.entries(SCENARIOS));
const world = (scenarios.get(params.get("scenario") ?? "busy") ?? SCENARIOS.busy)();
const calls: { cmd: string; args: Args }[] = [];
const theme = params.get("theme");
if (theme) world.settings.ui.theme = theme as typeof world.settings.ui.theme;
if (params.get("role") === "member" && world.settings.workspace) world.settings.workspace.admin = false;
seedEpic(world);
for (const p of world.panes) if (p.acp) seedAcp(p.id);
for (const p of world.panes) {
  const phase = params.get("loop");
  if (phase && p.task_id === params.get("task") && p.kind === "agent" && p.running) seedLoop(p.id, p.task_id, phase);
}
const overrides = new Map<string, Answer>();
/** A site's own names, highest first, as Jira lists them. */
const PRIORITIES = ["Highest", "High", "Medium", "Low", "Lowest"];

/** Base64 of the UTF-8 bytes, as the backend sends it, and how many bytes that was. */
/** A repository not on this Mac becomes one here, as the workspace has it. */
function placeElsewhere(id: string, path: string): Project {
  const at = world.notHere.findIndex((x) => x.id === id);
  if (at < 0) throw "that repository: it is on this Mac now, or gone from the workspace";
  const [e] = world.notHere.splice(at, 1);
  const p: Project = { id: `p-${e.name}`, name: e.name, path, default_branch: e.default_branch, group: e.group, store: null };
  world.projects.push(p);
  world.health.push({ project_id: p.id, clone: "ok", origin: `git@${e.remote_key.replace("/", ":")}.git`, store: null, synced_at: null, behind: 0, ahead: 0, found: null, update_guess: null, update_reason: null });
  return p;
}

function encode(text: string): { data: string; end: number } {
  const bytes = new TextEncoder().encode(text);
  return { data: btoa(String.fromCharCode(...bytes)), end: bytes.length };
}

function newPane(args: Args, kind: PaneInfo["kind"], task: string): PaneInfo {
  const agent = world.agents.find((a) => a.id === args.agentId);
  const p: PaneInfo = {
    id: `pane-${Date.now()}`,
    task_id: task,
    checkout_id: (args.checkoutId as string | null) ?? null,
    kind,
    title: agent?.name ?? "Shell",
    agent_id: agent?.id ?? null,
    cwd: world.tasks.find((t) => t.id === task)?.root ?? "/Users/you",
    running: true,
    exit_code: null,
    started_at: ago(0),
    last_output_at: ago(0),
    notice: null,
    activity: kind === "agent" ? "working" : "idle",
    activity_since: ago(0),
    topic: null,
    acp: !!args.acp,
  };
  world.panes.push(p);
  world.output[p.id] = kind === "agent" ? `${p.title} starting…\r\n` : "you@mac % ";
  return p;
}

/**
 * Each task's browser tab (§18). In the busy world, the login task's agent
 * has been using its tab; the others have none until the panel opens.
 */
const browsers = new Map<string, BrowserView>();
const noTab = (): BrowserView => ({
  chrome: true, tab: null, url: "", title: "", loading: false, agents_may: false, last_action: null, requests: [],
  dialog: null, held: false, tabs: [], driver: null,
});
if (world.panes.some((p) => p.task_id === "t-login")) {
  browsers.set("t-login", {
    chrome: true,
    tab: "tab-t-login-2",
    url: "http://localhost:5173/login",
    title: "Sign in · Acme",
    loading: false,
    agents_may: true,
    last_action: { text: "Clicked “Sign in”", x: 160, y: 236, at: Date.now() - 40_000 },
    requests: [{
      id: 1, task: "t-login", site: "github.com", at: Date.now() - 20_000,
      reason: "Read the OAuth app's callback URL settings, to see why the redirect loops",
    }],
    dialog: null,
    held: false,
    // Three tabs, the sign-in page active (BRW-16).
    tabs: [
      { id: "tab-t-login-1", name: "Acme", url: "http://localhost:5173/", active: false, loading: false },
      { id: "tab-t-login-2", name: "Sign in · Acme", url: "http://localhost:5173/login", active: true, loading: false },
      { id: "tab-t-login-3", name: "localhost", url: "http://localhost:5173/admin", active: false, loading: true },
    ],
    // The login task's Claude, still at work, between browser calls.
    driver: { pane: "pane-claude", agent: "Claude Code", calls: 0, last_at: Date.now() - 5_000 },
  });
}
function tabOf(task: string): BrowserView {
  let b = browsers.get(task);
  if (!b) {
    b = { ...noTab(), tab: `tab-${task}-1`, url: "about:blank" };
    b.tabs = [{ id: `tab-${task}-1`, name: "New tab", url: "about:blank", active: true, loading: false }];
    browsers.set(task, b);
    void emit("browser:changed", task);
  }
  return b;
}

/** Make the tab `id` the active one, as the backend's view would show it. */
function activate(task: string, id: string) {
  const b = tabOf(task);
  b.tabs = b.tabs.map((t) => ({ ...t, active: t.id === id }));
  const t = b.tabs.find((x) => x.active)!;
  b.tab = t.id;
  b.url = t.url;
  b.title = t.name;
  void emit("browser:changed", task);
}
let mockTabs = 10;

/** A page as Chrome would draw it into a frame: a sign-in form. */
async function frameOf(b: BrowserView, width: number, height: number, scale: number): Promise<ArrayBuffer> {
  const c = document.createElement("canvas");
  c.width = Math.round(width * scale);
  c.height = Math.round(height * scale);
  const g = c.getContext("2d")!;
  g.scale(scale, scale);
  g.fillStyle = "#f6f7f9";
  g.fillRect(0, 0, width, height);
  g.fillStyle = "#111";
  g.font = "600 22px -apple-system, sans-serif";
  g.fillText(b.title || "New tab", 40, 60);
  g.font = "13px -apple-system, sans-serif";
  g.fillStyle = "#666";
  g.fillText(b.url || "about:blank", 40, 84);
  for (const [i, label] of ["Email", "Password"].entries()) {
    g.fillStyle = "#333";
    g.fillText(label, 40, 118 + i * 52);
    g.strokeStyle = "#bbb";
    g.strokeRect(40, 124 + i * 52, 240, 30);
  }
  g.fillStyle = "#4f46e5";
  g.fillRect(40, 220, 240, 34);
  g.fillStyle = "#fff";
  g.font = "600 14px -apple-system, sans-serif";
  g.fillText("Sign in", 136, 242);
  const blob = await new Promise<Blob>((done) => c.toBlob((x) => done(x!), "image/jpeg", 0.8));
  return blob.arrayBuffer();
}

/** Settings → Remote access. Off, with one phone paired before. */
const phone: PhoneStatus = {
  tailscale: false,
  home: false,
  typing: false,
  port: 7421,
  listening: false,
  error: null,
  addresses: [],
  devices: [{ id: "ph-1", name: "iPhone", paired_at: ago(3 * 86_400), connected: false }],
  pairing: null,
};

function phoneAddresses(): PhoneStatus["addresses"] {
  return [
    ...(phone.tailscale ? [{ way: "tailscale" as const, ip: "100.101.102.103", interface: "utun4" }] : []),
    ...(phone.home ? [{ way: "home" as const, ip: "192.168.8.23", interface: "en0" }] : []),
  ];
}

const answer: Record<string, Answer> = {
  // What the app asks for on every launch.
  // A copy, as the real IPC's JSON always is: handing back the object a
  // setter just changed looked like no change to the store, and nothing redrew.
  get_settings: () => structuredClone(world.settings),
  set_ui_prefs: (a) => {
    world.settings.ui = a.ui as typeof world.settings.ui;
  },
  take_notices: () => [],
  // Connecting checks the token against the site; here any token will do.
  grafana_connect: (a) => {
    world.settings.grafana = { base_url: (a.baseUrl as string).replace(/\/+$/, ""), datasource: null };
    world.settings.grafana_connected = true;
    return 2;
  },
  // Grafana Cloud's own: the stack's logs, and the usage insights it adds.
  grafana_list_datasources: () => [
    { uid: "grafanacloud-logs", name: "grafanacloud-acme-logs", type: "loki", isDefault: false },
    { uid: "grafanacloud-usage-insights", name: "grafanacloud-acme-usage-insights", type: "loki", isDefault: false },
  ],
  grafana_set_datasource: (a) => {
    if (world.settings.grafana) world.settings.grafana.datasource = (a.uid as string | null) ?? null;
    return null;
  },
  // An AWS setup like a team's: SSO profiles per account, and a request
  // log bucket with an Athena table over it.
  aws_setup: () => ({
    cli: "/opt/homebrew/bin/aws",
    profiles: [
      { name: "acme-ci", region: "eu-central-1" },
      { name: "acme-prod", region: "eu-central-1" },
      { name: "default", region: null },
    ],
  }),
  s3_log_source_find: (a) => {
    if (!/^[a-z0-9.-]{3,63}$/.test(a.bucket as string)) throw new Error(`"${a.bucket}" is not an S3 bucket name`);
    if (a.profile === "default") throw new Error("the AWS sign-in for profile default has expired: run `aws sso login --profile default` in a terminal, then try again");
    return {
      folders: ["checkout-service/", "orders-service/", "search-service/"],
      files: 0,
      more: false,
      tables: [{
        database: "prod_request_logs", table: "all_request_logs", location: `s3://${a.bucket}/\${service}/\${source}/\${year}/\${month}/\${day}/`,
        columns: [{ name: "@timestamp", kind: "string" }, { name: "http_uri", kind: "string" }, { name: "message", kind: "string" }],
        partition_keys: [{ name: "service", kind: "string" }, { name: "day", kind: "string" }],
      }],
      // acme-prod may not list workgroups, but sees the one named after the
      // database; default sees none at all.
      workgroups: a.profile === "acme-prod" ? ["prod-request-logs"] : ["primary", "ci-request-logs"],
    };
  },
  s3_log_source_save: (a) => {
    const s = structuredClone(a.source) as LogSource;
    if (!s.name.trim()) throw new Error("give the source a name: agents ask for it by name");
    s.id ||= `src-${Date.now()}`;
    s.prefix = s.prefix && !s.prefix.endsWith("/") ? `${s.prefix}/` : s.prefix;
    const at = world.settings.log_sources.findIndex((x) => x.id === s.id);
    if (at >= 0) world.settings.log_sources[at] = s;
    else world.settings.log_sources.push(s);
    return s;
  },
  s3_log_source_remove: (a) => {
    world.settings.log_sources = world.settings.log_sources.filter((s) => s.id !== a.id);
    return null;
  },
  disconnect: (a) => {
    if (a.which === "slack") {
      world.settings.slack_bot = null;
      world.settings.slack = null;
      world.settings.slack_connected = false;
      world.slackBot.status = { state: "off", error: null };
      void emit("slackbot:changed");
      return null;
    }
    const which = a.which as "jira" | "github" | "grafana";
    world.settings[which] = null;
    world.settings[`${which}_connected`] = false;
    return null;
  },
  // The bot (§24): connecting checks both tokens with Slack; here any will do.
  slack_bot_view: () => structuredClone(world.slackBot),
  slack_bot_connect: (a) => {
    world.settings.slack_bot = {
      owner: a.owner as string, agent_id: a.agentId as string,
      bot_user: "UBOT0001", bot_name: "drdoom", enabled: true,
      auto: world.settings.slack_bot?.auto ?? true,
      personality: world.settings.slack_bot?.personality ?? "",
      model: world.settings.slack_bot?.agent_id === a.agentId ? world.settings.slack_bot?.model ?? "" : "",
    };
    world.slackBot.status = { state: "connected", error: null };
    void emit("slackbot:changed");
    return null;
  },
  set_slack_channel: (a) => {
    world.settings.slack = {
      enabled: true, notify_on_done: true, notify_on_prs: true, allow_agent_posts: true,
      ...world.settings.slack,
      channel: (a.channel as string).trim(),
    };
    world.settings.slack_connected = world.settings.slack_bot !== null;
    return null;
  },
  set_slack_prefs: (a) => {
    if (world.settings.slack) world.settings.slack = { ...(a.prefs as typeof world.settings.slack & object), channel: world.settings.slack.channel };
    return null;
  },
  set_slack_bot_agent: (a) => {
    if (world.settings.slack_bot) {
      world.settings.slack_bot.agent_id = a.agentId as string;
      world.settings.slack_bot.model = a.model as string;
    }
    return null;
  },
  set_slack_bot_on: (a) => {
    if (world.settings.slack_bot) world.settings.slack_bot.enabled = a.on as boolean;
    world.slackBot.status = { state: a.on ? "connected" : "off", error: null };
    void emit("slackbot:changed");
    return null;
  },
  set_slack_bot_personality: (a) => {
    if (world.settings.slack_bot) world.settings.slack_bot.personality = (a.text as string).trim();
    return null;
  },
  set_slack_bot_auto: (a) => {
    if (world.settings.slack_bot) world.settings.slack_bot.auto = a.on as boolean;
    return null;
  },
  list_messages: () => structuredClone(world.messages),
  add_message: (a) => {
    const id = Math.max(0, ...world.messages.map((m) => m.id)) + 1;
    world.messages.unshift({
      id, at: Date.now(), kind: a.kind as Message["kind"], level: a.level as Message["level"],
      title: a.title as string, body: "", target: (a.target as string | null) ?? null,
      read: false, count: 1,
    });
    void emit("messages:changed");
    return id;
  },
  mark_messages_read: (a) => {
    const ids = a.ids as number[] | null;
    for (const m of world.messages) if (!ids || ids.includes(m.id)) m.read = true;
    void emit("messages:changed");
    return null;
  },
  clear_messages: () => {
    world.messages = [];
    void emit("messages:changed");
    return null;
  },
  cursor_ide_installed: () => false,
  list_projects: () => structuredClone(world.projects),
  // A copy, as IPC would hand over: the same objects edited in place look
  // unchanged to the store (`lib/same.ts`), and the UI never sees the edit.
  list_tasks: () => structuredClone(world.tasks),
  // A copy too: a pane pushed onto the same array read as no change.
  list_panes: () => structuredClone(world.panes),
  list_agents: () => world.agents,

  // Terminals.
  pty_attach: (a): Catchup => {
    return { ...encode(world.output[a.paneId as string] ?? ""), reset: false };
  },
  // Keys, sizes and visibility: nothing to answer, and too frequent to log.
  pty_write: () => null,
  pty_resize: () => null,
  pty_detach: () => null,
  ...acpAnswers,
  ...loopAnswers(world.projects),
  resumable_agents: () => [],
  task_prompt: () => "Work on ACME-123: Fix login race.\n\nThe ticket says…",
  // Unanswered, a handoff opened on a null briefing and took the page down.
  handoff_prompt: () => "Continue ACME-123: Fix login race.\n\nWhat changed so far…\n\nThe last agent's terminal ended with…",
  // Specs (§19). A draft arrives in pieces, as the real one does.
  ...specAnswers(world),
  // Epics (§26).
  ...epicAnswers(world, (a, taskId) => newPane(a, "agent", taskId)),
  // The run log (§22).
  ...runAnswers(world.tasks.length > 0),
  ...profileAnswers(),
  spawn_agent: (a) => {
    const p = newPane(a, "agent", a.taskId as string);
    // An ACP agent is sent its opening prompt once its conversation opens.
    if (a.acp && a.prompt) acpAnswers.acp_prompt({ paneId: p.id, text: a.prompt });
    return p;
  },
  spawn_shell: (a) => newPane(a, "shell", a.taskId as string),
  spawn_chat: (a) => newPane(a, "agent", "chat"),
  waiting_panes: () => world.waiting,
  reopen_waiting_pane: (a) => {
    const w = world.waiting.find((x) => x.id === a.id);
    if (!w) throw new Error("That pane is no longer waiting to be reopened.");
    world.waiting = world.waiting.filter((x) => x !== w);
    void emit("panes:waiting");
    return newPane({ agentId: w.agent_id }, w.kind, w.task_id);
  },
  forget_waiting_pane: (a) => {
    world.waiting = world.waiting.filter((x) => x.id !== a.id);
    void emit("panes:waiting");
    return null;
  },
  close_pane: (a) => {
    world.panes = world.panes.filter((p) => p.id !== a.paneId);
    return null;
  },
  kill_pane: (a) => {
    const p = world.panes.find((x) => x.id === a.paneId);
    if (p) Object.assign(p, { running: false, exit_code: 143 });
    return null;
  },
  // A new pane where the old one stood, as the backend keeps its place.
  restart_pane: (a) => {
    const at = world.panes.findIndex((x) => x.id === a.paneId);
    if (at < 0) throw new Error("No such pane.");
    const old = world.panes[at];
    const p = newPane({ agentId: old.agent_id, checkoutId: old.checkout_id }, "agent", old.task_id);
    world.panes = world.panes.filter((x) => x !== p && x !== old);
    world.panes.splice(at, 0, { ...p, cwd: old.cwd, title: `${old.title.replace(/ \(resumed\)$/, "")} (resumed)`, started_at: old.started_at });
    world.output[p.id] = `${old.title} resuming the conversation…\r\n`;
    return world.panes[at];
  },

  // Diff.
  // The same files for every task, as that task's own: a task made in the
  // harness has checkouts the world's list never named.
  diff_files: (a) => {
    const task = world.tasks.find((t) => t.id === a.taskId);
    const known = new Set(task?.checkouts.map((c) => c.id));
    const own = task?.checkouts[0];
    return world.changed.map((f) =>
      known.has(f.checkout_id) || !own ? f : { ...f, checkout_id: own.id, repo: own.project_name });
  },
  diff_file: (a) =>
    `--- a/${a.path}\n+++ b/${a.path}\n@@ -1,3 +1,4 @@\n const retries = config.retries;\n-let attempt = 1;\n+let attempt = 0;\n+// Counted from zero: the first try is not a retry.\n while (attempt < retries) {\n`,
  task_commits: () => world.tasks.flatMap((t) =>
    t.checkouts.map((c) => ({ checkout_id: c.id, repo: c.project_name, commits: [] }))),
  task_branch_facts: () => [],
  task_outgoing: (a) => {
    const task = world.tasks.find((t) => t.id === a.taskId);
    return (task?.checkouts ?? []).map((c, i) => ({
      checkout_id: c.id, repo: c.project_name, error: null, leftovers_more: false,
      commits: i === 0
        ? [{ sha: "a".repeat(40), short: "aaaaaaa", subject: "Count retries from zero" }, { sha: "b".repeat(40), short: "bbbbbbb", subject: "Log the token while debugging" }]
        : [],
      leftovers: i === 0
        ? [{ path: "src/auth.ts", line: 14, kind: "debug", text: "console.log(\"token\", token);" }, { path: "src/auth.ts", line: 22, kind: "todo", text: "// TODO: cap the backoff" }]
        : [],
    }));
  },
  // GitHub.
  task_for_pr: (a) => {
    const had = world.tasks.find((t) => t.branch === a.head);
    if (had) return had;
    const repo = String(a.repo).split("/").pop() ?? "repo";
    const id = `t-pr-${world.tasks.length}`;
    const root = `/Users/you/.villain-worktrees/${a.head}`;
    const task: TaskView = {
      id, name: String(a.title), root, branch: String(a.head), issue_key: null, issue_url: null,
      created_at: ago(0), pane_count: 0,
      checkouts: [{
        id: `c-${id}`, task_id: id, project_id: `p-${repo}`, project_name: repo, path: `${root}/${repo}`,
        base: String(a.base), exists: true, broken: null, changed: 0,
        status: { ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, conflicted: 0, dirty_files: 0, branch: String(a.head) },
      }],
    };
    world.tasks = [...world.tasks, task];
    return task;
  },
  // Copies as well, for the same reason: a status moved in place was kept.
  github_all_prs: () => structuredClone(world.prs),
  github_task_prs: (a) => structuredClone(world.prs.find((p) => p.task_id === a.taskId)?.rows ?? []),
  github_review_queue: () => structuredClone(world.reviews),
  github_pr_feedback: () => world.feedback,
  checkout_branches: () => ["main", "develop"],
  set_checkout_base: (a) => {
    const c = world.tasks.flatMap((t) => t.checkouts).find((x) => x.id === a.checkoutId);
    if (c) c.base = String(a.base);
    for (const r of world.prs.flatMap((p) => p.rows)) if (r.checkout_id === a.checkoutId) r.base = String(a.base);
    return null;
  },

  // Jira.
  jira_issues: () => ({ issues: structuredClone(world.issues), more: false }),
  jira_issue_types: () => world.issueTypes,
  // Unanswered, New epic closed on a null epic and never asked for its repositories (EPIC-2).
  jira_create_issue: (a) => {
    const req = a.req as { summary: string; issue_type: string; project_key: string | null; parent_key: string | null };
    const key = `${req.project_key ?? "ACME"}-${300 + world.issues.length}`;
    const made = {
      ...world.issues[0], key, summary: req.summary, issue_type: req.issue_type, status: "To Do", status_id: "1",
      status_category: "new", epic_key: req.parent_key, epic_summary: null, url: `https://acme.atlassian.net/browse/${key}`,
    };
    world.issues.push(made);
    return structuredClone(made);
  },
  jira_transitions: () => world.transitions,
  jira_transition: (a) => {
    const t = world.transitions.find((x) => x.id === a.transitionId);
    const i = world.issues.find((x) => x.key === a.key);
    if (t && i) Object.assign(i, { status: t.to_status, status_id: t.to_id, status_category: t.to_category });
    return null;
  },
  jira_project_statuses: () => [
    { id: "1", name: "To Do", category: "new" },
    { id: "3", name: "In Progress", category: "indeterminate" },
    { id: "10322", name: "Review", category: "indeterminate" },
    { id: "10323", name: "Testing", category: "indeterminate" },
    { id: "10002", name: "Done", category: "done" },
    { id: "10016", name: "Cancelled", category: "done" },
  ],
  set_ticket_flow: (a) => {
    const jira = world.settings.jira;
    if (!jira) throw "Jira is not configured";
    const flow = (jira.flow[a.projectKey as string] ??= { review: null, merged: null });
    flow[a.stage as "review" | "merged"] = (a.status as FlowStatus | null) ?? null;
    return null;
  },
  set_ticket_start: (a) => {
    const jira = world.settings.jira;
    if (!jira) throw "Jira is not configured";
    (jira.flow[a.projectKey as string] ??= { review: null, merged: null }).started = a.to as StartTo;
    return null;
  },
  delete_task: (a) => {
    world.tasks = world.tasks.filter((t) => t.id !== a.id);
    return [];
  },
  jira_create_fields: () => world.requiredFields,
  // Jira orders a search (TKT-13); here the list is sorted as Jira would.
  jira_browse: (a) => ({ issues: sortTickets(world.issues, a.order as TicketSort, PRIORITIES), more: false }),
  jira_priorities: () => PRIORITIES,
  jira_issues_by_key: (a) =>
    [...world.issues, ...world.elsewhere].filter((i) => (a.keys as string[]).includes(i.key)),
  jira_start_work: (a) => {
    const key = a.key as string;
    const root = `/Users/you/.villain-worktrees/${key}`;
    const task: TaskView = {
      id: `t-${key.toLowerCase()}`, name: `${key} ${[...world.issues, ...world.elsewhere].find((i) => i.key === key)?.summary ?? ""}`.trim(),
      root, branch: key, issue_key: key, issue_url: `https://acme.atlassian.net/browse/${key}`,
      created_at: new Date().toISOString(), pane_count: 0, epic: !!a.epic,
      checkouts: (a.projectIds as string[]).map((pid) => {
        const name = world.projects.find((p) => p.id === pid)?.name ?? pid;
        return {
          id: `c-${key.toLowerCase()}-${name}`, task_id: `t-${key.toLowerCase()}`, project_id: pid, project_name: name,
          path: `${root}/${name}`, base: "main", exists: true, broken: null, changed: 0,
          status: { ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, conflicted: 0, dirty_files: 0, branch: key },
        };
      }),
    };
    world.tasks.push(task);
    if (a.agentId) newPane(a, "agent", task.id);
    return { ...structuredClone(task), moved: "In Progress" };
  },
  suggest_repos: () => ({ project_ids: [], reason: null }),
  project_branches: () => ["main", "develop"],

  // Repos: health, Sync, Locate, Clean up.
  repo_health: () => world.health,
  sync_repos: (a) =>
    (a.projectIds as string[]).map((id): Synced => {
      const p = world.projects.find((x) => x.id === id);
      const h = world.health.find((x) => x.project_id === id);
      if (!p || !h) return { project_id: id, repo: id, ok: false, detail: "No such repository" };
      if (h.clone !== "ok") {
        return { project_id: id, repo: p.name, ok: true, detail: `Fetched. The clone is not at ${p.path} any more: Locate it.` };
      }
      const moved = h.ahead ? 0 : h.behind ?? 0;
      h.synced_at = Math.round(Date.now() / 1000);
      if (moved) h.behind = 0;
      const detail = h.ahead
        ? `Fetched. main has ${h.ahead} commit of its own, so it was left alone.`
        : moved ? `Fetched. main moved forward ${moved} commits.` : "Fetched. main is up to date.";
      return { project_id: id, repo: p.name, ok: true, detail };
    }),
  // billing is the workspace's, added by Ada, with no clone here (REPO-9).
  list_elsewhere: () => world.notHere,
  locate_elsewhere: (a) => placeElsewhere(String(a.repositoryId), String(a.path)),
  clone_elsewhere: async (a) => {
    await new Promise((done) => setTimeout(done, 1200));
    const e = world.notHere.find((x) => x.id === a.repositoryId);
    return placeElsewhere(String(a.repositoryId), `${String(a.folder)}/${e?.name ?? "repo"}`);
  },
  locate_project: (a) => {
    const p = world.projects.find((x) => x.id === a.projectId);
    const h = world.health.find((x) => x.project_id === a.projectId);
    if (!p || !h) throw "no such repository";
    p.path = a.path as string;
    Object.assign(h, { clone: "ok", found: null, origin: `git@github.com:acme/${p.name}.git` });
    return p;
  },
  set_project_update_by: (a) => {
    const p = world.projects.find((x) => x.id === a.projectId);
    if (p) p.update_by = (a.by as Project["update_by"]) ?? null;
    return null;
  },
  update_from_base: (a) =>
    (a.picks as { checkout_id: string; by: string }[]).map((pick): RepoUpdate => {
      const c = world.tasks.flatMap((t) => t.checkouts).find((x) => x.id === pick.checkout_id);
      return {
        checkout_id: pick.checkout_id, repo: c?.project_name ?? "?", base: c?.base ?? "main",
        outcome: "updated", commits: 2, conflicts: [],
        detail: pick.by === "rebase" ? "rebased onto 2 new commits" : "merged 2 commits",
      };
    }),
  cleanup_plan: () => world.cleanup,
  list_repo_notes: () =>
    world.projects.map((p) => ({ project_id: p.id, notes: structuredClone(world.notes[p.id] ?? []) })),
  add_repo_note: (a) => {
    const list = (world.notes[a.projectId as string] ??= []);
    const at = Date.now();
    list.unshift({
      id: `n${at}`, repo: "", text: (a.text as string).trim(), paths: a.paths as string[], written_at: at,
      source: "you", written_commit: null, checked_at: at, checked_commit: null, changed: [],
    });
    return null;
  },
  edit_repo_note: (a) => {
    const n = Object.values(world.notes).flat().find((x) => x.id === a.id);
    if (n) Object.assign(n, { text: (a.text as string).trim(), paths: a.paths, checked_at: Date.now(), changed: [] });
    return null;
  },
  check_repo_note: (a) => {
    const n = Object.values(world.notes).flat().find((x) => x.id === a.id);
    if (n) Object.assign(n, { checked_at: Date.now(), changed: n.paths.length ? [] : null });
    return null;
  },
  delete_repo_note: (a) => {
    for (const k of Object.keys(world.notes)) world.notes[k] = world.notes[k].filter((x) => x.id !== a.id);
    return null;
  },
  cleanup_apply: (a) => {
    const ids = a.ids as string[];
    const done: Cleaned[] = ids.map((id) => {
      const item = world.cleanup.find((i) => i.id === id);
      if (!item) return { id, ok: false, detail: "Changed since the list was made. Look again." };
      return item.verdict === "blocked" ? { id, ok: false, detail: item.detail } : { id, ok: true, detail: "" };
    });
    world.cleanup = world.cleanup.filter((i) => !done.some((d) => d.ok && d.id === i.id));
    return done;
  },

  phone_status: () => structuredClone(phone),
  set_phone_access: (a) => {
    phone.tailscale = a.tailscale as boolean;
    phone.home = a.home as boolean;
    phone.typing = a.typing as boolean;
    phone.listening = phone.tailscale || phone.home;
    phone.addresses = phoneAddresses();
    if (!phone.listening) phone.pairing = null;
    world.settings.phone_open = phone.listening;
    return structuredClone(phone);
  },
  phone_pair: () => {
    if (!phone.listening) throw "Turn on Tailscale or the home network first, so the phone can reach the app.";
    phone.pairing = { code: "482913", seconds_left: 120 };
    return structuredClone(phone.pairing);
  },
  phone_forget: (a) => {
    phone.devices = phone.devices.filter((d) => d.id !== a.id);
    return null;
  },

  browser_view: (a) => structuredClone(browsers.get(a.taskId as string) ?? noTab()),
  browser_open: (a) => {
    const b = tabOf(a.taskId as string);
    const url = (a.url as string | null)?.trim();
    if (url) {
      b.url = url.includes("://") ? url : `http://${url}`;
      const host = new URL(b.url).hostname;
      b.title = host;
      b.agents_may = host === "localhost" || host === "127.0.0.1";
      void emit("browser:changed", a.taskId);
    }
    return null;
  },
  browser_go: () => null,
  browser_watch: async (a) => {
    const b = tabOf(a.taskId as string);
    const frames = a.frames as { id: number };
    const jpeg = await frameOf(b, a.width as number, a.height as number, a.scale as number);
    const internals = (window as unknown as { __TAURI_INTERNALS__: { runCallback: (id: number, data: unknown) => void } }).__TAURI_INTERNALS__;
    internals.runCallback(frames.id, { index: 0, message: jpeg });
    return null;
  },
  browser_unwatch: () => null,
  add_browser_sign_in: (a) => {
    const saved = { id: `si-${Date.now()}`, site: a.site as string, username: a.username as string };
    world.settings.browser_sign_ins.push(saved);
    return saved;
  },
  forget_browser_sign_in: (a) => {
    world.settings.browser_sign_ins = world.settings.browser_sign_ins.filter((s) => s.id !== a.id);
    return null;
  },
  browser_sign_in_form: () => ({ site: "localhost", username: null }),
  browser_save_sign_in: (a) => {
    const saved = { id: `si-${Date.now()}`, site: a.site as string, username: a.username as string };
    world.settings.browser_sign_ins.push(saved);
    return saved;
  },
  browser_hold: (a) => {
    tabOf(a.taskId as string).held = a.held as boolean;
    void emit("browser:changed", a.taskId);
    return null;
  },
  browser_new_tab: (a) => {
    const task = a.taskId as string;
    const b = tabOf(task);
    const id = `tab-${task}-${++mockTabs}`;
    const url = (a.url as string | null) ?? "about:blank";
    const at = b.tabs.findIndex((t) => t.active) + 1;
    b.tabs.splice(at, 0, { id, name: url === "about:blank" ? "New tab" : new URL(url.includes("://") ? url : `http://${url}`).hostname, url, active: false, loading: false });
    activate(task, id);
    return null;
  },
  browser_switch_tab: (a) => {
    activate(a.taskId as string, a.tab as string);
    return null;
  },
  browser_close_tab: (a) => {
    const task = a.taskId as string;
    const b = tabOf(task);
    if (b.tabs.length === 1) {
      b.tabs = [{ ...b.tabs[0], name: "New tab", url: "about:blank" }];
      activate(task, b.tabs[0].id);
      return null;
    }
    const at = b.tabs.findIndex((t) => t.id === a.tab);
    const wasActive = b.tabs[at]?.active;
    b.tabs.splice(at, 1);
    activate(task, wasActive ? b.tabs[Math.max(0, at - 1)].id : b.tabs.find((t) => t.active)!.id);
    return null;
  },
  browser_dialog: (a) => {
    const b = tabOf(a.taskId as string);
    if (!b.dialog) throw "no dialog is open";
    b.dialog = null;
    void emit("browser:changed", a.taskId);
    return null;
  },
  browser_answer_site: (a) => {
    for (const [task, b] of browsers) {
      const r = b.requests.find((x) => x.id === a.id);
      if (!r) continue;
      if (a.allow && !world.settings.browser_sites.includes(r.site)) world.settings.browser_sites.push(r.site);
      b.requests = b.requests.filter((x) => x !== r);
      void emit("browser:changed", task);
      return null;
    }
    throw "that request was already answered";
  },
  set_browser_sites: (a) => {
    const kept = [...new Set((a.sites as string[])
      .map((x) => x.trim().toLowerCase().replace(/^[a-z]+:\/\//, "").split(/[/:?#]/)[0].replace(/^\*\./, ""))
      .filter((x) => x.includes(".")))].sort();
    world.settings.browser_sites = kept;
    void emit("browser:changed", "");
    return kept;
  },
  browser_input: () => null,
  browser_copy: () => "copied from the page",
  browser_screenshot: (a) => (a.save ? "localhost-5173 2026-10-26 at 14.02.11.png" : null),

  // Plugins the UI reaches through @tauri-apps packages.
  "plugin:notification|is_permission_granted": () => true,
  // A folder picker that picks: Locate and Clone of a repository not on
  // this Mac (REPO-9) go through it.
  "plugin:dialog|open": () => "/Users/you/code",
  "plugin:app|version": () => "0.0.0-mock",
  "plugin:window|is_focused": () => true,
};

// Before anything imports @tauri-apps/api/window, which reads these.
mockWindows("main");
mockIPC(
  (cmd, args) => {
    const a = (args ?? {}) as Args;
    calls.push({ cmd, args: a });
    const fn = overrides.get(cmd) ?? answer[cmd];
    if (fn) return fn(a);
    if (!cmd.startsWith("plugin:")) console.warn("[mock] unanswered", cmd, a);
    return null;
  },
  { shouldMockEvents: true },
);

(window as unknown as { __mock: unknown }).__mock = {
  calls,
  world,
  on: (cmd: string, fn: Answer) => overrides.set(cmd, fn),
  /** Each task's browser tab; edit one, then emit "browser:changed" with its task. */
  browsers,
  emit: (event: string, payload?: unknown) => emit(event, payload),
};

// The store reads its layout from here on boot (lib/persist.ts).
const scene: Record<string, string | null> = {
  view: params.get("view") ?? "work",
  selectedTask: params.get("task"),
  tab: params.get("tab") ?? "terminals",
};
for (const [key, value] of Object.entries(scene)) {
  localStorage.setItem(`villain.${key}`, JSON.stringify(value));
}

await import("../main");
