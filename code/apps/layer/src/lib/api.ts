import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AcpView,
  AddedRepo,
  AgentStatus,
  AwsSetup,
  Catchup,
  ChangedFile,
  CheckoutPr,
  Cleaned,
  CleanupItem,
  RequiredField,
  DiffScope,
  FeedbackItem,
  FlowStatus,
  Finished,
  FoundRepo,
  Ticket,
  TicketType,
  TicketPage,
  JiraTransition,
  LoopView,
  PaneInfo,
  AddedRepos,
  Project,
  ProjectNotes,
  ProjectStatus,
  RepoBranchFacts,
  RepoCommits,
  RepoFeedback,
  RepoHealth,
  RepoResult,
  RepoSuggestion,
  RepoUpdate,
  Resumable,
  WaitingPane,
  ReviewComment,
  ReviewQueue,
  RepoOutgoing,
  Settings,
  SlackBotView,
  SlackConfig,
  GrafanaDatasource,
  StartTo,
  AppliedAnswers,
  SpecApproved,
  SpecCheck,
  SpecKind,
  SpecPart,
  TaskSpec,
  Started,
  Synced,
  Task,
  TaskPrs,
  TaskView,
  UiPrefs,
  UpdateBy,
  AppNotice,
  Message,
  MessageKind,
  MessageLevel,
  Target,
  PhoneStatus,
  PhonePairing,
  BrowserInput,
  BrowserView,
  SavedSignIn,
  SignInForm,
  LogSource,
  S3Found,
} from "./types";
import type { RunLog } from "./types-runs";
import type { EpicView, Filed, StartedSubTask } from "./types-epic";
import type { Operation, TrackerKind } from "@layer/types";
import type {
  Elsewhere, ProfileView, ServerSignedIn, ServerWays, SetupApplied, SetupSnapshot, WorkspacePlace,
} from "./types-profiles";

export const api = {
  // projects
  listProjects: () => invoke<Project[]>("list_projects"),
  addProjects: (paths: string[], group?: string | null) =>
    invoke<AddedRepos>("add_projects", { paths, group: group ?? null }),
  setProjectGroup: (projectIds: string[], group: string | null) =>
    invoke<void>("set_project_group", { projectIds, group }),
  scanRepos: (root: string, maxDepth?: number) =>
    invoke<FoundRepo[]>("scan_repos", { root, maxDepth: maxDepth ?? null }),
  removeProject: (id: string) => invoke<void>("remove_project", { id }),
  projectBranches: (projectId: string) =>
    invoke<string[]>("project_branches", { projectId }),
  repoHealth: () => invoke<RepoHealth[]>("repo_health"),
  locateProject: (projectId: string, path: string) =>
    invoke<Project>("locate_project", { projectId, path }),
  listElsewhere: () => invoke<Elsewhere[]>("list_elsewhere"),
  locateElsewhere: (repositoryId: string, path: string) =>
    invoke<Project>("locate_elsewhere", { repositoryId, path }),
  cloneElsewhere: (repositoryId: string, folder: string) =>
    invoke<Project>("clone_elsewhere", { repositoryId, folder }),
  setProjectUpdateBy: (projectId: string, by: UpdateBy | null) =>
    invoke<void>("set_project_update_by", { projectId, by }),
  /** The command a loop runs in a repository (LOOP-1); null clears it. */
  setProjectCheck: (projectId: string, command: string | null) =>
    invoke<void>("set_project_check", { projectId, command }),
  syncRepos: (projectIds: string[]) => invoke<Synced[]>("sync_repos", { projectIds }),
  cleanupPlan: () => invoke<CleanupItem[]>("cleanup_plan"),
  cleanupApply: (ids: string[]) => invoke<Cleaned[]>("cleanup_apply", { ids }),
  listRepoNotes: () => invoke<ProjectNotes[]>("list_repo_notes"),
  addRepoNote: (projectId: string, text: string, paths: string[]) =>
    invoke<void>("add_repo_note", { projectId, text, paths }),
  editRepoNote: (id: string, text: string, paths: string[]) =>
    invoke<void>("edit_repo_note", { id, text, paths }),
  checkRepoNote: (id: string) => invoke<void>("check_repo_note", { id }),
  deleteRepoNote: (id: string) => invoke<void>("delete_repo_note", { id }),

  // tasks
  listTasks: (focus?: string | null) =>
    invoke<TaskView[]>("list_tasks", { focus: focus ?? null }),
  /** The task for one of your pull requests, made on its branch if there is none (REV-7). */
  taskForPr: (repo: string, head: string, base: string, title: string) =>
    invoke<Task>("task_for_pr", { repo, head, base, title }),
  /** Per repo, the commits a push would send and the leftovers they add (PR-11). */
  taskOutgoing: (taskId: string) => invoke<RepoOutgoing[]>("task_outgoing", { taskId }),
  deleteTask: (id: string, force = false) =>
    invoke<RepoResult[]>("delete_task", { id, force }),
  /**
   * Worktrees, then local branches, then the ticket. `landed` is each
   * checkout's merged PR head; a branch it does not contain is kept.
   */
  finishTask: (taskId: string, transitionId: string | null, landed: Record<string, string>) =>
    invoke<Finished>("finish_task", { taskId, transitionId, landed }),
  addCheckout: (taskId: string, projectId: string) =>
    invoke<AddedRepo>("add_checkout", { taskId, projectId }),
  removeCheckout: (checkoutId: string, force = false) =>
    invoke<void>("remove_checkout", { checkoutId, force }),
  suggestRepos: (args: { issueKey?: string | null; epicKey?: string | null }) =>
    invoke<RepoSuggestion>("suggest_repos", {
      issueKey: args.issueKey ?? null,
      epicKey: args.epicKey ?? null,
    }),

  // panes
  listAgents: () => invoke<AgentStatus[]>("list_agents"),
  listPanes: (taskId?: string) => invoke<PaneInfo[]>("list_panes", { taskId: taskId ?? null }),
  spawnShell: (taskId: string, checkoutId?: string | null) =>
    invoke<PaneInfo>("spawn_shell", { taskId, checkoutId: checkoutId ?? null }),
  /** `acp`: as a conversation over ACP rather than in a terminal (§20). */
  spawnAgent: (
    taskId: string, agentId: string,
    checkoutId?: string | null, prompt?: string | null, resume = false, acp = false,
  ) => invoke<PaneInfo>("spawn_agent", {
    taskId, agentId, checkoutId: checkoutId ?? null, prompt: prompt ?? null, resume, acp,
  }),
  /** Panes the last launch did not put back, waiting for the user (PANE-7). */
  waitingPanes: () => invoke<WaitingPane[]>("waiting_panes"),
  reopenWaitingPane: (id: string) => invoke<PaneInfo>("reopen_waiting_pane", { id }),
  forgetWaitingPane: (id: string) => invoke<void>("forget_waiting_pane", { id }),
  resumableAgents: (taskId: string, checkoutId?: string | null) =>
    invoke<Resumable[]>("resumable_agents", { taskId, checkoutId: checkoutId ?? null }),
  spawnChat: (agentId: string, prompt?: string | null, acp = false) =>
    invoke<PaneInfo>("spawn_chat", { agentId, prompt: prompt ?? null, acp }),
  ptyWrite: (paneId: string, data: string) => invoke<void>("pty_write", { paneId, data }),
  ptyResize: (paneId: string, rows: number, cols: number) =>
    invoke<void>("pty_resize", { paneId, rows, cols }),
  ptyAttach: (paneId: string, since: number | null) =>
    invoke<Catchup>("pty_attach", { paneId, since }),
  ptyDetach: (paneId: string) => invoke<void>("pty_detach", { paneId }),
  closePane: (paneId: string) => invoke<void>("close_pane", { paneId }),
  killPane: (paneId: string) => invoke<void>("kill_pane", { paneId }),
  /** Stop an agent and start it again where it was, on its conversation (PANE-14). */
  restartPane: (paneId: string) => invoke<PaneInfo>("restart_pane", { paneId }),

  // a loop (§21)
  /** Put a running agent on a loop: its checks run at each turn's end (LOOP-2). */
  startLoop: (paneId: string, rounds: number) => invoke<LoopView>("start_loop", { paneId, rounds }),
  stopLoop: (paneId: string) => invoke<void>("stop_loop", { paneId }),
  /** The agent's loop, running or ended; null if it was never on one. */
  loopView: (paneId: string) => invoke<LoopView | null>("loop_view", { paneId }),

  // an agent over ACP (§20); `ptyDetach` says it is off screen, as for a terminal
  /** What changed since `since`, and the pane is on screen from now (ACP-7). */
  acpView: (paneId: string, since: number | null) => invoke<AcpView>("acp_view", { paneId, since }),
  /** A prompt, with any images pasted into it (ACP-16). */
  acpPrompt: (paneId: string, text: string, images: { mimeType: string; data: string }[]) =>
    invoke<void>("acp_prompt", { paneId, text, images }),
  acpCancel: (paneId: string) => invoke<boolean>("acp_cancel", { paneId }),
  acpAnswer: (paneId: string, entry: number, option: string) =>
    invoke<void>("acp_answer", { paneId, entry, option }),
  acpSet: (paneId: string, setting: string, value: string) =>
    invoke<void>("acp_set", { paneId, setting, value }),

  // diff + git
  diffFiles: (
    taskId: string,
    scope: DiffScope,
    commit?: { checkoutId: string; sha: string } | null,
  ) =>
    invoke<ChangedFile[]>("diff_files", {
      taskId,
      scope,
      commit: commit?.sha ?? null,
      checkoutId: commit?.checkoutId ?? null,
    }),
  diffFile: (
    checkoutId: string,
    path: string,
    scope: DiffScope,
    commitSha?: string | null,
  ) =>
    invoke<string>("diff_file", {
      checkoutId,
      path,
      scope,
      commit: commitSha ?? null,
    }),
  taskCommits: (taskId: string) =>
    invoke<RepoCommits[]>("task_commits", { taskId }),
  sendReview: (paneId: string, comments: ReviewComment[]) =>
    invoke<string>("send_review", { paneId, comments }),
  commitTask: (taskId: string, message: string) =>
    invoke<RepoResult[]>("commit_task", { taskId, message }),
  /** Merge each repo's base into the branch; all of the task's repos when `checkoutIds` is null. */
  updateFromBase: (taskId: string, picks: { checkout_id: string; by: UpdateBy }[]) =>
    invoke<RepoUpdate[]>("update_from_base", { taskId, picks }),
  /** Abandon a conflicted merge or rebase, putting the branch back. */
  abortUpdate: (checkoutId: string) => invoke<void>("abort_update", { checkoutId }),
  /** Typed into `paneId` when given; otherwise only returned, to start an agent with. */
  sendMergeConflicts: (taskId: string, paneId: string | null, scope: string | null) =>
    invoke<string>("send_merge_conflicts", { taskId, paneId, scope }),
  pushTask: (taskId: string) => invoke<RepoResult[]>("push_task", { taskId }),

  // jira
  jiraConnect: (
    baseUrl: string, email: string, token: string,
    projectKey?: string | null, jql?: string | null,
  ) => invoke<string>("jira_connect", { baseUrl, email, token, projectKey, jql }),
  ticketsMine: () => invoke<TicketPage>("tickets_mine"),
  ticketTypes: (refresh = false) =>
    invoke<TicketType[]>("ticket_types", { refresh }),
  jiraTransitions: (key: string) => invoke<JiraTransition[]>("jira_transitions", { key }),
  jiraTransition: (key: string, transitionId: string) =>
    invoke<void>("jira_transition", { key, transitionId }),
  taskPrompt: (taskId: string, checkoutId?: string | null) =>
    invoke<string>("task_prompt", { taskId, checkoutId: checkoutId ?? null }),
  handoffPrompt: (paneId: string) => invoke<string>("handoff_prompt", { paneId }),
  draftPrDescription: (taskId: string) =>
    invoke<string>("draft_pr_description", { taskId }),
  optimizeIssueDescription: (args: {
    requestId: string;
    summary: string;
    description: string;
    kind: "epic" | "ticket";
  }) =>
    invoke<string>("optimize_issue_description", {
      requestId: args.requestId,
      summary: args.summary,
      description: args.description,
      kind: args.kind,
    }),
  readSpec: (taskId: string) => invoke<TaskSpec>("read_spec", { taskId }),
  /** Keep what the editor holds for one file (tasks: one checkout's); null lets it go (SPEC-6). */
  saveSpecDraft: (taskId: string, part: SpecPart, checkoutId: string | null, text: string | null) =>
    invoke<void>("save_spec_draft", { taskId, part, checkoutId, text }),
  /** Approve files of the spec: written into every repository (tasks into their own), and committed there. */
  approveSpec: (taskId: string, texts: { part: SpecPart; checkout_id: string | null; text: string }[], kind: SpecKind | null) =>
    invoke<SpecApproved>("approve_spec", { taskId, texts, kind }),
  /** Write what was approved back over one repository's copy changed outside the app (SPEC-22). */
  putBackSpec: (taskId: string, part: SpecPart, checkoutId: string) =>
    invoke<TaskSpec>("put_back_spec", { taskId, part, checkoutId }),
  /** Turn down a change an agent proposed, and tell it (SPEC-24). */
  turnDownSpecProposal: (taskId: string, part: SpecPart, checkoutId: string | null) =>
    invoke<TaskSpec>("turn_down_spec_proposal", { taskId, part, checkoutId }),
  /** Draft one file for the whole task, streamed as `spec:draft`: tasks are every repository's at once. */
  draftSpec: (taskId: string, part: SpecPart, kind: SpecKind | null, requestId: string) =>
    invoke<void>("draft_spec", { taskId, part, kind, requestId }),
  /** Merge the specs each repository had of its own into drafts of one (SPEC-25). */
  mergeSpecs: (taskId: string, requestId: string) => invoke<void>("merge_specs", { taskId, requestId }),
  /** Build the answered open questions into the requirements, streamed as `spec:draft` (SPEC-20). */
  applySpecAnswers: (taskId: string, requestId: string) =>
    invoke<AppliedAnswers>("apply_spec_answers", { taskId, requestId }),
  checkSpec: (taskId: string, checkoutId: string) => invoke<SpecCheck>("check_spec", { taskId, checkoutId }),
  /** What Start agent adds to the opening prompt: work through the tasks (SPEC-12). */
  specWorkPrompt: (taskId: string, checkoutId: string | null) =>
    invoke<string>("spec_work_prompt", { taskId, checkoutId }),
  /** Tell the task's running agents the spec changed (SPEC-14); how many were told. */
  tellSpecChange: (taskId: string, parts: SpecPart[]) =>
    invoke<number>("tell_spec_change", { taskId, parts }),
  setProjectSpecs: (projectId: string, inApp: boolean, folder: string) =>
    invoke<void>("set_project_specs", { projectId, inApp, folder }),
  requestPrDescription: (taskId: string, paneId: string) =>
    invoke<string>("request_pr_description", { taskId, paneId }),
  takePrDescription: (taskId: string) =>
    invoke<string | null>("take_pr_description", { taskId }),
  jiraSyncStatus: (key: string) => invoke<string | null>("jira_sync_status", { key }),
  jiraProjectStatuses: (projectKey: string) =>
    invoke<ProjectStatus[]>("jira_project_statuses", { projectKey }),
  setTicketFlow: (projectKey: string, stage: "review" | "merged", status: FlowStatus | null) =>
    invoke<void>("set_ticket_flow", { projectKey, stage, status }),
  setTicketStart: (projectKey: string, to: StartTo) => invoke<void>("set_ticket_start", { projectKey, to }),
  ticketsFind: (text: string, whose: string, includeDone: boolean, types: string[], order: string) =>
    invoke<TicketPage>("tickets_find", { text: text || null, whose, includeDone, types, order }),
  ticketPriorities: () => invoke<string[]>("ticket_priorities"),
  setTracker: (uses: TrackerKind) => invoke<void>("set_tracker", { uses }),
  ticketsByKey: (keys: string[]) => invoke<Ticket[]>("tickets_by_key", { keys }),
  ticketRequiredFields: (projectKey: string, issueTypeId: string) =>
    invoke<RequiredField[]>("ticket_required_fields", { projectKey, issueTypeId }),
  fileTicket: (req: {
    summary: string;
    description: string;
    issue_type: string;
    project_key: string | null;
    parent_key: string | null;
    fields: Record<string, unknown> | null;
  }) => invoke<Ticket>("file_ticket", { req }),
  /** `epic`: an epic-level ticket, made an epic whose spec makes sub-tasks (EPIC-2). */
  startWorkOnTicket: (
    key: string, projectIds: string[],
    agentId?: string | null, branchSuffix?: string | null, base?: string | null, epic = false,
  ) => invoke<Started>("start_work_on_ticket", {
    key, projectIds, agentId: agentId ?? null, branchSuffix: branchSuffix ?? null,
    base: base ?? null, epic,
  }),
  // epics (§26)
  epicView: (epicId: string) => invoke<EpicView>("epic_view", { epicId }),
  /** Make tasks whose tickets sit under the epic's in Jira its sub-tasks (EPIC-13). */
  adoptSubTasks: (epicId: string, taskIds: string[]) => invoke<EpicView>("adopt_sub_tasks", { epicId, taskIds }),
  /** File the sub-tasks not filed yet: tickets under the epic, tasks, their spec (EPIC-7). */
  fileSubTasks: (epicId: string, issueType: string | null, fields: Record<string, unknown> | null) =>
    invoke<Filed>("file_sub_tasks", { epicId, issueType, fields }),
  /** `anyway`: start one that waits, after the user agreed (EPIC-8). */
  startSubTask: (taskId: string, agentId: string, anyway = false) =>
    invoke<StartedSubTask>("start_sub_task", { taskId, agentId, anyway }),

  // github
  githubConnect: (apiUrl: string, webUrl: string, token: string, reviewTeam: string | null) =>
    invoke<string>("github_connect", { apiUrl, webUrl, token, reviewTeam }),
  githubReviewQueue: () => invoke<ReviewQueue>("github_review_queue"),
  githubTaskPrs: (taskId: string) => invoke<CheckoutPr[]>("github_task_prs", { taskId }),
  githubAllPrs: () => invoke<TaskPrs[]>("github_all_prs"),
  setCheckoutBase: (checkoutId: string, base: string) =>
    invoke<void>("set_checkout_base", { checkoutId, base }),
  taskBranchFacts: (taskId: string) =>
    invoke<RepoBranchFacts[]>("task_branch_facts", { taskId }),
  checkoutBranches: (checkoutId: string) =>
    invoke<string[]>("checkout_branches", { checkoutId }),
  githubRetargetPr: (checkoutId: string) =>
    invoke<string>("github_retarget_pr", { checkoutId }),
  /** `skip`: checkout ids left out of this call, neither pushed nor opened. */
  githubOpenPrs: (taskId: string, title: string, body: string, draft: boolean, skip: string[]) =>
    invoke<RepoResult[]>("github_open_prs", { taskId, title, body, draft, skip }),
  githubPrFeedback: (taskId: string) =>
    invoke<RepoFeedback[]>("github_pr_feedback", { taskId }),
  /** Typed into `paneId` when given; otherwise only returned, to start an agent with. */
  sendPrFeedback: (
    taskId: string, paneId: string | null, scope: string | null, items: FeedbackItem[],
  ) => invoke<string>("send_pr_feedback", { taskId, paneId, scope, items }),

  // slack
  setSlackChannel: (channel: string) => invoke<void>("set_slack_channel", { channel }),
  slackNotify: (text: string, context?: string, kind?: string) =>
    invoke<boolean>("slack_notify", { text, context, kind: kind ?? null }),
  setSlackPrefs: (prefs: SlackConfig) => invoke<void>("set_slack_prefs", { prefs }),

  // the Slack bot (§24)
  /** A token left blank keeps the one saved. */
  slackBotConnect: (botToken: string, appToken: string, owner: string, agentId: string) =>
    invoke<void>("slack_bot_connect", { botToken, appToken, owner, agentId }),
  setSlackBotOn: (on: boolean) => invoke<void>("set_slack_bot_on", { on }),
  setSlackBotAgent: (agentId: string, model: string) => invoke<void>("set_slack_bot_agent", { agentId, model }),
  setSlackBotAuto: (on: boolean) => invoke<void>("set_slack_bot_auto", { on }),
  setSlackBotPersonality: (text: string) => invoke<void>("set_slack_bot_personality", { text }),
  slackBotView: () => invoke<SlackBotView>("slack_bot_view"),

  // grafana (§23)
  /** How many Loki datasources the token can see. */
  grafanaConnect: (baseUrl: string, token: string) =>
    invoke<number>("grafana_connect", { baseUrl, token }),
  grafanaListDatasources: () => invoke<GrafanaDatasource[]>("grafana_list_datasources"),
  /** `null` goes back to agents naming one whenever there are several. */
  grafanaSetDatasource: (uid: string | null) => invoke<void>("grafana_set_datasource", { uid }),

  // S3 log sources (§23, OBS-8 to OBS-13)
  awsSetup: () => invoke<AwsSetup>("aws_setup"),
  s3LogSourceFind: (profile: string, region: string | null, bucket: string, prefix: string) =>
    invoke<S3Found>("s3_log_source_find", { profile, region, bucket, prefix }),
  /** Saved only once the profile has listed the bucket. */
  s3LogSourceSave: (source: LogSource) => invoke<LogSource>("s3_log_source_save", { source }),
  s3LogSourceRemove: (id: string) => invoke<void>("s3_log_source_remove", { id }),

  // settings
  getSettings: () => invoke<Settings>("get_settings"),
  takeNotices: () => invoke<AppNotice[]>("take_notices"),
  listMessages: () => invoke<Message[]>("list_messages"),
  addMessage: (kind: MessageKind, level: MessageLevel, title: string, target?: Target) =>
    invoke<number>("add_message", { kind, level, title, target: target ?? null }),
  /** Every message when `ids` is left out. */
  markMessagesRead: (ids?: number[]) => invoke<void>("mark_messages_read", { ids: ids ?? null }),
  clearMessages: () => invoke<void>("clear_messages"),

  // the run log (§22)
  listRuns: () => invoke<RunLog>("list_runs"),
  clearRuns: () => invoke<void>("clear_runs"),

  // profiles (§25)
  listProfiles: () => invoke<ProfileView[]>("list_profiles"),
  /** In a window of its own, or its window brought forward (PROF-3). */
  openProfile: (id: string) => invoke<void>("open_profile", { id }),
  createProfile: (name: string) => invoke<ProfileView>("create_profile", { name }),
  renameProfile: (id: string, name: string) => invoke<void>("rename_profile", { id, name }),
  /** One of the eight, `--profile-<n>` (PROF-10). */
  setProfileColour: (id: string, colour: number) => invoke<void>("set_profile_colour", { id, colour }),
  deleteProfile: (id: string) => invoke<void>("delete_profile", { id }),
  /** Once its server has started; null when it did not (PROF-11). */
  workspacePlace: () => invoke<WorkspacePlace | null>("workspace_place"),
  /** The changes queued for the workspace, handed over (PROF-13). */
  readSetupWrites: () => invoke<{ id: string; operation: Operation }[]>("read_setup_writes"),
  ackSetupWrites: (ids: string[]) => invoke<void>("ack_setup_writes", { ids }),
  /** What the workspace keeps, into this Mac's setup and notes (PROF-14). */
  applySetup: (snapshot: SetupSnapshot) => invoke<SetupApplied>("apply_setup", { snapshot }),
  // A workspace on a server (PROF-15 to PROF-18). `address` is a server's
  // address.
  serverWays: (address: string) => invoke<ServerWays>("server_ways", { address }),
  serverSignInBrowser: (address: string) => invoke<ServerSignedIn>("server_sign_in_browser", { address }),
  serverSignInCancel: () => invoke<void>("server_sign_in_cancel"),
  serverOpen: (id: string) => invoke<void>("server_open", { id }),
  serverResume: () => invoke<WorkspacePlace>("server_resume"),
  openMembers: () => invoke<void>("open_members"),
  setWorktreeRoot: (path: string | null) => invoke<void>("set_worktree_root", { path }),
  setUiPrefs: (ui: UiPrefs) => invoke<void>("set_ui_prefs", { ui }),
  disconnect: (which: "jira" | "github" | "slack" | "grafana") => invoke<void>("disconnect", { which }),
  cursorIdeInstalled: () => invoke<boolean>("cursor_ide_installed"),
  openInCursor: (path: string) => invoke<void>("open_in_cursor", { path }),

  // phone (PHONE-*)
  phoneStatus: () => invoke<PhoneStatus>("phone_status"),
  setPhoneAccess: (tailscale: boolean, home: boolean, typing: boolean) =>
    invoke<PhoneStatus>("set_phone_access", { tailscale, home, typing }),
  phonePair: () => invoke<PhonePairing>("phone_pair"),
  phoneForget: (id: string) => invoke<void>("phone_forget", { id }),

  // browser (BRW-*)
  browserView: (taskId: string) => invoke<BrowserView>("browser_view", { taskId }),
  /** Opens the task's tab, starting the browser; goes to `url` when given. */
  browserOpen: (taskId: string, url?: string) =>
    invoke<void>("browser_open", { taskId, url: url ?? null }),
  browserGo: (taskId: string, to: "back" | "forward" | "reload") =>
    invoke<void>("browser_go", { taskId, to }),
  /** Sizes the tab to the panel and sends its frames, JPEGs, to `frames`. */
  browserWatch: (taskId: string, width: number, height: number, scale: number, frames: Channel<ArrayBuffer>) =>
    invoke<void>("browser_watch", { taskId, width, height, scale, frames }),
  browserUnwatch: (taskId: string) => invoke<void>("browser_unwatch", { taskId }),
  browserInput: (taskId: string, input: BrowserInput) =>
    invoke<void>("browser_input", { taskId, input }),
  browserCopy: (taskId: string) => invoke<string>("browser_copy", { taskId }),
  /** The tab on the clipboard, or with `save` in Downloads: the saved file's name (BRW-22). */
  browserScreenshot: (taskId: string, save: boolean) =>
    invoke<string | null>("browser_screenshot", { taskId, save }),
  browserAnswerSite: (id: number, allow: boolean) => invoke<void>("browser_answer_site", { id, allow }),
  browserHold: (taskId: string, held: boolean) => invoke<void>("browser_hold", { taskId, held }),
  /** A new tab beside the active one, made active (BRW-16). */
  browserNewTab: (taskId: string, url?: string) => invoke<void>("browser_new_tab", { taskId, url: url ?? null }),
  browserSwitchTab: (taskId: string, tab: string) => invoke<void>("browser_switch_tab", { taskId, tab }),
  /** The last tab is left open, empty. */
  browserCloseTab: (taskId: string, tab: string) => invoke<void>("browser_close_tab", { taskId, tab }),
  addBrowserSignIn: (site: string, username: string, password: string) =>
    invoke<SavedSignIn>("add_browser_sign_in", { site, username, password }),
  forgetBrowserSignIn: (id: string) => invoke<void>("forget_browser_sign_in", { id }),
  /** The site and username the page's sign-in form holds; never its password. */
  browserSignInForm: (taskId: string) => invoke<SignInForm>("browser_sign_in_form", { taskId }),
  /** Saves what was typed into the page; the backend reads the password from it. */
  browserSaveSignIn: (taskId: string, site: string, username: string) =>
    invoke<SavedSignIn>("browser_save_sign_in", { taskId, site, username }),
  browserDialog: (taskId: string, accept: boolean, text?: string) =>
    invoke<void>("browser_dialog", { taskId, accept, text: text ?? null }),
  /** Returns the list as kept: each site as its host, what is not one dropped. */
  setBrowserSites: (sites: string[]) => invoke<string[]>("set_browser_sites", { sites }),
};

export function errMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}

/**
 * A failure as the user reads it: a sentence, capital first and a full stop
 * last (`studio/design/behaviour.md`). Errors from git, Jira or Slack arrive as
 * fragments ("invalid_auth", "push rejected").
 */
export function failureText(e: unknown): string {
  const text = errMessage(e).trim();
  if (!text) return "Something went wrong. Try again.";
  const capital = text[0].toUpperCase() + text.slice(1);
  return /[.!?…:)]$/.test(capital) ? capital : `${capital}.`;
}
