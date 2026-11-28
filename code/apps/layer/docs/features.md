# Features and requirements

This is the contract for what Layer does. Each area says what the
user sees, then the requirements (**MUST** statements, each with a stable
id), where the code is, and the gaps we know about.

How to use it:

- **Before changing behaviour**, read the area's requirements. A change that
  breaks one is a bug, unless the requirement changes too, in the same
  commit and deliberately.
- **A new feature adds requirements here first.** Use the template at the
  end. If you cannot write the requirement, the feature is not understood
  yet.
- **Refer to requirements by id** in commits, PRs and test names' comments
  (`TASK-4`). Ids are never reused. A dropped requirement is struck through
  with a note.
- **Known gaps** are real and unfixed. Don't copy the behaviour they
  describe, and don't "fix" one in passing. Each deserves its own change.

The words: a **repository** (repo) is a git clone the user registered. A
**task** is one piece of work, usually one Jira ticket, spanning one or more
repos. A **checkout** is one repo's git worktree inside a task. A **pane**
is a terminal running an agent CLI or a shell, or an agent's conversation
over ACP (§20).

---

The public app name is **Layer**, by **Code Villain**. Its existing bundle identifiers, data paths, and integration keys remain stable across the display-name change (see [`studio/brand/name.md`](../../../../studio/brand/name.md)).

## 1. Repositories

The Repos view lists registered repos in collapsible groups. Repos are added
by picking folders, or by scanning a folder (three levels deep) and ticking
what was found. A repo has at most one group; groups are renamed in place.
Each repo says how it is doing, and the view keeps them in shape: **Sync**
brings them up to date, **Locate** follows a clone that moved, and **Clean
up** removes what tasks left behind. A repo can have a check command, which
a loop runs (LOOP-1).

- **REPO-1** A repo MUST be a git work tree root. It is registered by path,
  and its default branch is detected (origin/HEAD, then `main`, then
  `master`, then the current branch). Adding several, each path that
  cannot be added MUST be named with why, and the rest added.
- **REPO-2** Scanning MUST skip symlinks and dot-folders, and stop at 500
  results.
- **REPO-3** Removing a repo MUST NOT touch the clone or any worktree on
  disk. It drops the repo's checkouts from tasks. It drops tasks left with
  no repo, and stops their panes.
- **REPO-4** Every repo MUST get the app's own copy
  (`<task folder location>/.repos/<name>.git`), made in the background
  when it is added, or when a task first needs it. Task worktrees are cut
  from the copy, so nothing done to the user's clone reaches them:
  re-cloning, moving, deleting, pruning, gc, branch deletion. The copy is
  hard-linked, so it costs little disk and survives the clone being
  deleted. It fetches from the clone's `origin` and carries its repo-local
  git settings. A copy made while the clone had no `origin` fetches from
  the clone, and MUST move to the clone's `origin` once it has one (at
  launch, at Sync, when a task needs it), or task branches are pushed into
  the user's clone. Such a repo is not read as a GitHub repository.
- **REPO-5** Each repo MUST show whether its clone is still where it was
  registered and still the same repository, where it fetches from, whether
  the app's copy exists and when a Sync last reached origin (not when a
  fetch last tried: git rewrites `FETCH_HEAD` before it knows), and how the clone's
  default branch stands against origin's (behind, or with commits of its
  own), and how Update from base updates branches there (UPD-7), which can
  be set. A repo with a problem says what the problem is, with its fix beside
  it, and its group opens. Read when the view opens and after anything here
  changes it, never on a timer.
- **REPO-6** Locate MUST point a repo at another folder without taking it
  out of any task, and only when that folder is the same repository: one
  fetching from where the app's copy does. When the clone is gone, a folder
  of the same name near where it was is offered. Before this, the only way
  was remove and add again, which dropped the repo from every task (REPO-3).
- **REPO-7** Sync MUST fetch origin into the app's copy, forgetting branches
  origin deleted and taking tags origin moved (an Actions repo moves `v1`
  each release; refused, it failed every Sync of a user whose git fetches
  all tags), and carry the clone's repo-local settings again. A failed
  fetch says git's reason, not only that it failed. When origin's HEAD
  names another branch than the repo's default, and the copy has it, that
  branch becomes the default, and the row says so: a clone's `origin/HEAD`
  never moves after it is cloned, so a team that moved from `dev` to `main`
  kept being synced and cut tasks from `dev`. In the
  clone it only ever fast-forwards the default branch, and only when that
  branch has no commits of its own and, if checked out, no uncommitted
  changes to tracked files. It never merges or rebases there, so how a team
  updates its branches does not matter to it. It reports a row per repo:
  what moved, what was left alone and why. A Sync of several ends with one
  line naming the repos that failed (five, then how many more).
- **REPO-8** Clean up MUST list what it would remove, and why, before it
  removes anything. It pre-selects only what loses nothing, and checks each
  item again as it removes it. It finds:
  - task folders no task uses: pre-selected when they hold only what the
    app generated, empty folders and worktrees with no changes; offered,
    naming the first few, when they also hold files the app did not make
    (an agent CLI's log, a build cache written after the worktree went).
    Never one holding a git repository or worktree at any depth that no
    copy lists, or more than 5000 things the app did not make. A folder
    that gained files since the list was made is not removed;
  - worktrees inside a task folder that are none of its checkouts, on the
    same terms;
  - task branches in the user's clone, left from before REPO-4 or taken
    over by a task (TASK-3), only when every commit on them is in the
    app's copy and they are not checked out;
  - branches in the app's copies that no task uses, pre-selected when every
    commit on them is on origin, or, in repos on the connected GitHub, when
    a pull request has every commit on them: your newest from the branch,
    merged, or, for a branch the app made to review one
    (`review/<repo>-<n>`), that pull request. A squash merge leaves a
    branch's commits on no branch once origin deletes it, so git alone
    took every finished task's branch for unpushed work;
  - your branches on origin that are done with, in repos on the connected
    GitHub, as origin stood at the last Sync. Yours means your newest pull
    request from it is merged or closed, or, with no pull request of yours,
    you wrote its last commit (the app's copy's `user.email`). Done with
    means that pull request merged and nothing was pushed to it since, or
    every commit on it is in the default branch: both pre-selected, since
    GitHub can restore the first from the pull request and the second loses
    nothing. A pull request closed unmerged, or a branch pushed to after
    its pull request merged, is offered, not pre-selected. Never the
    default branch, a branch a task of any build is on, or one an open pull
    request of anyone's comes from. It is deleted only if origin's branch
    is still at the commit it was judged at. Your 300 most recently created
    pull requests in each repo are read; when there are more, it says so.
    A repo GitHub could not be asked about says why;
  - copies no repo uses, never one a worktree still belongs to (the other
    build's, say);
  - git's records, in the app's copies, of worktrees whose folders are
    gone, and staging left by an interrupted move (TASK-12).

  A worktree is removed through git, which refuses one with changes. A
  folder is removed only once it is empty.
- **REPO-9** In a workspace (§25), a repository the workspace has and this
  Mac has no clone of MUST be listed as Not on this Mac, above the groups,
  with where it fetches from and who added it, and counted in the header.
  It offers Clone, into a folder of its name inside one picked, over SSH
  and then HTTPS, and Locate…, for a clone already here, taken only when it
  fetches from the same repository. Either makes it a repository here
  with the workspace's name, group and settings, sends the workspace
  nothing, and gets the app's own copy as an added one does (REPO-4). A
  clone added by Add repositories that fetches from it does the same. Its
  check command comes with it but is not run until confirmed here (LOOP-1).
  A repository's name in the workspace MUST be one folder's name: no `/`
  or `\`, no control characters, not starting with `.` or `-`. A name
  that is not (from before this, or from a server that let it through) is
  made one as TASK-1 says wherever it names a folder: the clone's, the
  app's copy (REPO-4) and each checkout.

Code: `commands/projects.rs`, `commands/repos.rs`, `commands/cleanup.rs`,
`commands/cleanup_folders.rs`, `commands/cleanup_remote.rs`, `git/store.rs`,
`git/upkeep.rs`, `integrations/github/branches.rs`, `ReposView.tsx`,
`AddRepos.tsx`, `CleanUp.tsx`; not on this Mac: `commands/elsewhere.rs`,
`workspace/mirror.rs`, `ReposElsewhere.tsx`.

Known gaps:
- Removing a repo leaves panes running in *surviving* tasks' checkouts of
  it, and does not rewrite those tasks' context files. Removing a checkout
  does both.
- Sync carries the clone's repo-local settings again, but one removed from
  the clone stays in the copy.
- Removing a repo leaves its copy in `.repos/`. Adding the repo again
  reuses it; Clean up offers it once no worktree belongs to it.
- A branch squash-merged and then deleted on origin has commits that are on
  no remote branch. In a repo not on the connected GitHub, or with no pull
  request of yours from it, Clean up cannot tell it from unpushed work and
  does not pre-select it.
- Task branches are not in the user's clone until pushed and fetched.
- In a workspace, Remove takes the repository out of the workspace, for
  everyone in it, not only off this Mac; its dialog does not say so.
- Clone shows no progress, and a large repository holds its row at
  Cloning… until it is done.

## 2. Tasks

A task has a name, a branch (the same in every repo), a folder, and one
checkout per repo. The sidebar lists tasks as working, in review (has an
open PR) or done (every PR merged). Each row shows state dots, the name,
and under it the branch, which carries the ticket key, with the repo
count, running agents, uncommitted changes and the review verdict.

- **TASK-1** A task MUST have a folder, `<worktree root>/<branch, / → ->`.
  Each checkout is a subfolder named after its repo, made one folder's
  name: `/`, `\` and control characters become `-`, and leading dots,
  dashes and spaces are dropped, so a name never reaches outside the task
  folder or makes a dot-folder in it (REPO-9). Agents start in the
  task folder, where every repo is a sibling, unless started for one repo.
  A folder that exists or belongs to another task gets `-2`, `-3`, ….
- **TASK-2** The branch MUST be: the explicit name if given; else the
  ticket key, plus `-<suffix>` if given; else `villain/<slug of the name>`.
- **TASK-3** A checkout MUST be cut, in the app's copy of the repo
  (REPO-4), from the freshly fetched `origin/<base>`, not the local base branch, which may be stale. The commit
  it was cut from is recorded as the branch point
  (`Checkout.base_commit`). Everything that measures the branch ("changed",
  the Diff, PR descriptions, handoffs) measures from that point. A branch
  that already exists goes on from its own commits: the copy's, or, when
  only the user's clone has it (started there by hand), the clone's,
  brought into the copy first, or, when only GitHub has it (a pull request
  pushed from elsewhere), GitHub's, fetched and tracked. It is never cut
  again from the base under a name GitHub already has.
- **TASK-4** Creating a task MUST be all or nothing. If any repo fails, the
  worktrees and branches this attempt made are removed, and branches it did
  not make are kept. The task record goes too.
- **TASK-5** A second task for a ticket that already has one MUST need a
  branch suffix.
- **TASK-6** Adding a repo to a task MUST tell the task's running agents,
  typed into each, and rewrite the folder's context files. The new
  checkout is cut from the base the rest of the task shares, when this
  repo has that branch, and otherwise from its own default branch. A base
  the rest share only because it is each one's own default is not shared:
  a task in customer-portal, on `main` as its default, gave core-service
  `main` too, though core-service's work goes to `development`. An agent
  puts a wrong base right with `set_base`, as the PR panel's base field
  does, to a branch the remote has. Removing one
  stops panes in that checkout, and refuses when it has uncommitted changes
  unless forced.
- **TASK-7** Deleting a task MUST check for uncommitted changes *before*
  stopping any agent. It refuses without force: a refusal must not cost the
  conversations. If any worktree cannot be removed, the task is kept with
  just those checkouts, never left as a folder the app cannot see.
- **TASK-8** Finish task (offered once every PR has merged) MUST go in
  order: stop agents, remove worktrees, delete each local branch **only if**
  the merged PR's head or `origin/<base>` contains it, or the head has the
  change of every commit on it the base lacks (a branch rebased here after
  it was pushed), then move the ticket to the chosen done-category status.
  Each step only runs if the one before succeeded. Remote branches are
  never touched.
- **TASK-9** The finish dialog MUST offer only transitions into the `done`
  category. It pre-selects the one into the project's after-merge status
  (TKT-8), or else one only when there is exactly one; otherwise it defaults
  to leaving the ticket as it is. (Done and Cancelled are both done: with no
  status chosen, a finished ticket was left In Progress.)
- **TASK-10** Status (ahead, behind, changes, conflicts) comes from
  `git status` per checkout. Only the selected task and tasks with an agent
  working, or done working in the last 75 seconds, are asked every poll. The
  rest reuse a cached status, so a poll is not one git process per worktree
  ever made: for up to 90 seconds while a pane runs in the task (a shell,
  an idle agent), and up to 5 minutes once nothing does, since then only
  the user's own editor can change it. A commit, push or update from base
  made through the app is asked about at the next poll, whichever the task.

- **TASK-11** A checkout whose folder exists but git cannot read MUST say
  so, with the reason, in the task header, the sidebar and the Diff tab.
  It is never shown as clean or empty. It happens when the repository it
  was registered in is deleted or cloned again, which takes
  `.git/worktrees/` and the local branches with it; the files stay in the
  task folder.
- **TASK-12** At launch, before any pane comes back, every worktree still
  registered in a user's clone MUST be moved onto the app's copy (REPO-4)
  in place. The files are untouched, unpushed commits come across, and
  staging survives. A worktree mid-merge or mid-rebase is left for the
  next launch. A folder already cut off (TASK-11) is linked back at the
  commit the status poll last saw it on (`Checkout.last_head`), when the
  copy has that commit. A folder that has become a clone of its own (its
  `.git` a folder, as when it is cloned again in place) is linked back
  too, when that loses nothing: it fetches from the same origin, is on the
  task's branch, has nothing staged, no stash and no merge or rebase in
  progress, every commit on its other branches is already in the copy, and
  the copy's branch has no commit it lacks. Its commits come across, its
  files are untouched (an edit stays an unstaged edit), and its own `.git`
  folder goes. What could not be moved is reported, with the reason.
- **TASK-13** The Start work dialog MUST fill in "Branch
  from" with the default branch of the picked repos when they share one,
  and leave it blank (each repo's own default) when they do not. The box
  follows every change of repos while it still shows the dialog's own last
  suggestion; a base typed by hand stays.
- **TASK-14** Open in Cursor (the task header, the Agents tab, the
  sidebar's menu) MUST open the task folder in the Cursor IDE, and be
  offered only when the IDE is installed: `Cursor.app` in an Applications
  folder, or a `cursor` command that links into a `.app`. The Cursor agent
  CLI's own `cursor` shim is not the IDE. When opening fails, the user is
  told why.
- **TASK-15** In the app, a task MUST start from work that already
  exists: Start work on a Jira ticket (TKT-1), or Start task on one of
  your pull requests (REV-7). The sidebar offers no task of its own; its
  "+" made one with a name and no ticket, or filed a new ticket and opened
  worktrees for it in one step. A new ticket is filed from Tickets, then
  started. A chat's agent can still make a task with no ticket
  (`create_task`, CHAT-3).
- **TASK-16** The sidebar MUST be made wider or narrower by dragging its
  right edge, where a grip shows on hover, so long task names fit. It is
  held between 200px, where its own controls still fit, and 560px or half
  the window, whichever is less, so the terminals keep at least half of the
  smallest window. The width is remembered across launches. Double-clicking
  the edge puts it back to 260px. The edge is also a focusable separator:
  ←/→ move it 16px (with ⇧, 64px), Home and End to its narrowest and
  widest. So is every other grip that resizes part of a view (the browser
  beside the terminals, the diff's file list, the spec's panel, the epic's
  chat), with the same keys inside its own bounds. Hiding the sidebar
  hides the edge too, and showing it brings back the width it had. While
  that edge, or any grip beside a terminal (BRW-9),
  is dragged, terminals keep their size and are fitted once, when it is
  let go: each fit resizes the agent's terminal, which redraws its whole
  screen for it.
- **TASK-17** The task list MUST group by stage (Tasks, In review, Done:
  the default), by epic, by repository, or **By tag**, chosen from
  "by stage ▾" on its heading and remembered across launches. By epic, a task is under its
  ticket's epic; an epic-level ticket is its own; a task with no ticket, or
  whose ticket has no epic, is under "No epic", last. A ticket not on your
  own list is asked of the tracker (`tickets_by_key`), once a session, and
  only while there is one: Epics needs it however the list is grouped
  (EPIC-13). By repository, a task in several is
  listed once, under all of their names. Inside each group, and between
  groups, the stage order holds: what is being written first, what has
  landed last. Epic is not offered without Jira. Twenty tasks over five
  epics read as one list; which belonged to which piece of work was only in
  the ticket. An epic made in Layer, and its sub-tasks, are listed above
  every grouping except By tag, in Epics (EPIC-9). By tag replaces the
  epic tree with groups of stored Work items, including started subtasks
  and Layer epics. Each item appears under every assigned tag, once per
  group; untagged items appear under **No tag**, last. Named groups sort
  by normalized tag name, with the existing stage order inside each.
  A tag named "No tag" is distinct from the untagged group. By tag works
  without a tracker and is remembered across launches. Other modes keep
  their existing epic tree and ordering.
- **TASK-18** An agent MUST be able to finish a task as Finish task does
  (TASK-8), when the user asks it to: `finish_task`, with `confirm`. It
  refuses, changing nothing, while one of the task's pull requests is
  open, when the agent runs in that task (it would be stopped first and
  never hear back), or when the transition it names is not into the
  `done` category. Branches are deleted only where a merged PR covers
  them, as in the dialog. Without a transition named, the ticket goes as
  TASK-9 would pre-select; when that would be a guess it stays, and the
  reply lists the ways to close it. A ticket already done stays. A
  chat's room context says that the user asking for a move or a finish
  is the agreement `confirm` stands for. (A chat could move a ticket,
  but the task stayed in the sidebar with its worktrees until someone
  opened the dialog.)

- **TASK-19** A field at the top of the sidebar MUST find a task by its
  ticket's key (`ACME-4826`, or part of it), its title, its branch, its
  custom display name and tags, its epic's key and title, and, for a ticket
  on your own list, its description. Every word typed must be found, in any order and case. It
  narrows Epics and Tasks alike: an epic stays when it or one of its tasks
  matches, and an epic that matches keeps all its tasks. Typing asks the
  tracker nothing; a ticket not started yet is in Find work (TKT-12).
  Escape clears it, and it is not remembered across launches, so no task
  is hidden at the next start. With twenty tasks under six epics, the one
  being worked on could only be found by reading every row.
- **TASK-20** An existing Work task's row menu MUST offer **Edit details…**
  with keyboard and pointer access, using the shared editor (DETAIL-1).
  This includes stored started subtasks and Layer epics, not Jira-only
  epic headings or unfiled spec steps. A custom display name takes
  precedence in task rows and the selected-item heading, without hiding
  ticket, branch or repository identifiers. Clearing it restores that
  surface's previous name source, including a filed subtask's spec title.
  The edit MUST NOT replace `Task.name` or `sub_task.title`: PR and diff
  title defaults, generated context and run history keep their source names.
  Details stay while the task record exists and go when it is removed,
  including when removing its last repository drops it.

Code: `commands/tasks.rs`, `commands/worktree_status.rs` (TASK-10), `sidebar/`, `FinishTask.tsx`, `commands/finish_tool.rs` (TASK-18), `lib/drag.ts` (TASK-16), `lib/grouping.ts`, `lib/epics.ts` (TASK-17), `lib/task-search.ts` (TASK-19).

Known gaps:
- A folder cut off before its last commit was recorded, or whose last
  commit was never pushed or copied, cannot be linked back automatically
  (TASK-12). By hand: create the branch at the commit the files match,
  `git worktree add --no-checkout` it somewhere temporary, point the new
  registration and the folder's `.git` file at each other, then
  `git reset` in the folder. Its files are never touched.
- Finishing moves the ticket even when a branch was kept because it was
  not contained. The dialog says so.
- `worktree remove` deletes gitignored files (`.env`, build output) with
  the worktree. Git does this; the confirm says "uncommitted", not
  "ignored".

## 3. Agents and terminals

A task's Agents tab holds its panes: agents in a terminal, agents as a
conversation (§20), and shells. "+" starts an agent (with an opening
prompt made from the ticket, which can be edited), in a terminal or as a
conversation, resumes one, or opens a shell. When a task has several
repos, a pane can be scoped to the whole task or to one repo. A pane can
be handed off to another agent, or closed. The All agents view lists
every agent pane in every task, with the ones that need you first, or in
another order chosen at its top.

| id | CLI | How it reports its state | How it gets the app's tools | Resume |
|---|---|---|---|---|
| `claude` | Claude Code | hooks, passed with `--settings` | `--mcp-config` file (0600) | `--continue`, per folder |
| `copilot` | GitHub Copilot CLI | hooks, from a plugin folder | `--additional-mcp-config` file | no |
| `opencode` | OpenCode | a JS plugin posting its state | `OPENCODE_CONFIG_CONTENT` + `VILLAIN_MCP_TOKEN` | no |
| `gemini` | Gemini CLI | the mark in its window title | `.gemini/settings.json` in the app's own folders | no |

Each of them can also run as a conversation the app draws, over ACP (§20).

- **PANE-1** Only a CLI that can report its own state MUST be offered.
  Output alone cannot tell working from idle: Claude Code repaints every
  few seconds while idle. CLIs dropped for this are in git history.
- **PANE-2** At most 32 panes run at once (`PANE_LIMIT`), however many
  spawns race. Settings → Agents MAY raise it to 48 or 64, never past
  64 (`MAX_PANES`), whatever the config file says. A pane that has ended,
  its agent exited or stopped, MUST NOT count: it holds no process.
  Counted, a day of finished agents left open filled the limit while
  thirteen ran, and the next one was refused. The refusal says how many
  are running and where the limit is raised. As many panes as may run,
  and eight more, are remembered for PANE-7, so a raised limit loses
  none at quit.
- **PANE-3** A pane MUST start in an existing folder. A missing one is an
  error, never a silent fallback to `$HOME`, where `--continue` resumes the
  wrong conversation.
- **PANE-4** A pane's environment MUST be the user's login shell's
  (PATH, etc.), minus this app's own `VILLAIN_*` values and the
  `CLAUDE_CODE_*` markers of a Claude Code session the app was opened
  from (with them, Claude saves no transcript). It adds
  `VILLAIN_PANE` and the hook URL and token for the pane.
- **PANE-5** Stopping MUST be graceful then certain. An agent gets SIGTERM
  (a shell SIGHUP), up to 5 seconds (2 when a whole task is being
  deleted) to save its transcript, then SIGKILL to its process group.
  Quitting the app does the same for every pane, however it is quit:
  Cmd+Q, the Dock, closing the window, logging out.
- **PANE-6** A pane stopped on purpose (Stop, handoff, close) MUST NOT be
  reported as having exited with an error, nor posted to Slack as
  finished.
- **PANE-7** The open panes MUST come back when the app reopens, when
  "Put terminals back" is on. Agents resume their conversation where the
  CLI can. At most 12 come back at launch (`RESTORE_LIMIT`), agents before
  shells, since a shell has no conversation to lose. A pane over the limit
  MUST NOT be forgotten: it waits, listed with Reopen and Forget (a chat
  in the Chat view, a task's pane in its Agents tab) until the user
  picks one, and the user is told how many wait. A waiting pane is never
  reopened by a later launch on its own. Restoring used to put back the
  first 12 in the order they were opened and forget the rest, so the
  newest were the ones lost: a chat with a day's work in it went, at a
  restart with 12 agents and 2 shells open. Two agents in one folder never
  both resume the same conversation. A pane that fails to come back three launches in a row is
  forgotten, except the Henchman, which waits (HEN-2). Only a conversation the CLI would continue counts: Claude
  Code's one-shot runs (`claude -p`, the app's own PR description drafts)
  do not. Counted, a draft in the task folder had an agent restarted there,
  where Claude found "No conversation found to continue", while its real
  conversation was in the repo's folder. A waiting agent that did not
  record its conversation's id (one not over ACP) resumes the newest
  conversation in its folder, so it stops waiting once an agent of the
  same CLI is open there: that agent already continues it. Agents come
  back one at a time, 1.5 s apart, and while macOS says memory is short
  (PANE-21) the launch stops: the panes it did not get to wait like those
  over the limit, and the user is told why.
- **PANE-8** A terminal MUST keep 256KB of scrollback. One coming back on
  screen gets exactly what it missed, by byte position. One that missed
  more than its scrollback holds is replayed from what it still holds,
  except a full-screen program (on the alternate screen, as OpenCode is):
  its screen and modes are kept, and it MUST be made to draw its whole
  screen again, as a window resize would. Keystrokes echo immediately,
  even while the pane is printing heavily. A layout that types `@` with
  Option MUST send a literal `@` to the CLI without changing other Option
  shortcuts. Output is batched to about 30 frames a second otherwise.
- **PANE-9** Handoff moves the work, not the conversation: no CLI can
  resume another's session. It MUST give the new agent a briefing: the task prompt,
  the commits and changed files per repo since the branch point, and the
  last 120 lines of the old pane. Then it stops the old one.
- **PANE-10** Claude Code MUST NOT stop at its "trust this folder?"
  question for folders the app created, when "Trust the folders this app
  creates" is on. The app records trust in `~/.claude.json` for those
  folders only, and nowhere else. It never creates that file, and never
  rewrites one it cannot parse. It keeps the file's permissions and
  symlink, and does not overwrite a change Claude made meanwhile.
- **PANE-11** Nothing the app types into an agent MUST be longer than 512
  bytes (`pty::MAX_TYPED`). A longer hand-off (conflicts, review
  comments, a PR description request, an opening prompt a CLI takes
  typed) is written whole to a file in the task folder, and only a
  pointer to it is typed. macOS throws away a typed line much over 1 KB
  while the program is not reading in raw mode: a rebase hand-off reached
  Claude Code as its last 68 bytes. Longer is refused, never cut. An agent
  over ACP is sent it whole instead (ACP-4).
- **PANE-12** An agent pane MUST be called by what its conversation is
  about once the CLI names it, in its tab, the chat list, All agents and
  the pickers that send it something. The name is the CLI's own, read from
  its window title (Claude Code's `✳ <topic>`), never generated by the app.
  Until then, and for a CLI that names nothing, it is the agent's name.
  Its scope (`· repo`) stays. A resumed conversation names itself again as
  it starts, so the automatic topic is not saved by this rule. A standalone
  chat's local display name (CHAT-8) takes precedence in the desktop Chat
  list, All agents, agent pickers and Work's link to the chat. Clearing
  it restores the latest automatic title or that surface's existing
  fallback, including its Slack label where applicable. CLI title updates
  MUST NOT replace the override or change the title passed to an agent.
- **PANE-13** An agent in a task that has a ticket MUST find the ticket in
  the task folder's context file (`CLAUDE.md`/`AGENTS.md`): key, title,
  type, priority, labels, components, epic, link and description. The
  opening prompt carries it only into the first conversation, and only
  until the CLI compacts it. An agent started with a prompt of its own,
  or brought back at launch, had only the link, and spent its first calls
  fetching what the app already had. The ticket is saved as `TICKET.md`
  in the task folder whenever the app fetches it for the task (Start
  work, the launch dialog's opening
  prompt), and the context file is built from that. The description is
  quoted line by line and introduced as written by whoever filed it: the
  context file is read as instructions, and a ticket's text is someone
  else's. A description past 8 KB is cut, and says so.
- **PANE-14** An agent pane MUST restart in place on its own
  conversation, for a CLI that can resume (Claude Code): Restart, on its
  tab and its entry in the chat list, stops it as closing does (PANE-5,
  PANE-6) and starts the same CLI in the same folder, in the same place
  in the list. That is how a CLI that updated itself gets to run its new
  version. An agent that has exited offers the same, as Resume, above its
  terminal. The conversation is the one the CLI last said it was in (a
  Claude Code hook's `session_id`, resumed with `--resume`). Without that,
  it is the folder's newest (`--continue`), and Restart refuses while
  another open agent of the same CLI is in that folder, since the newest
  could be the other's. Before this, an agent that exited said "Resume
  this session with: claude --resume …" and nothing in the app could.
- **PANE-15** Text in any terminal MUST be selectable and copied by ⌘C,
  including while the agent has the mouse. Claude Code's fullscreen mode,
  OpenCode and Copilot turn on mouse reporting, and then a drag is theirs:
  ⌥-drag selects in the terminal instead. The selection stays until a
  click, a key or a scroll, which go to the agent as before. Before this,
  nothing could select there, and a selection made anyway was gone with
  the next movement of the mouse, which xterm reports to the agent as
  input.
- **PANE-16** A click or drag in a terminal MUST land on the cell under
  the pointer at every interface scale. The terminal is not scaled with
  the rest of the window: it has its own font size. Before this, at any
  scale but 100% xterm read the pointer against an unscaled grid, so a
  selection, in OpenCode or in the terminal, was right on the top row
  and further off on every row below.
- **PANE-17** The All agents view MUST offer its order and remember the
  last one chosen on this Mac: **Needs you first** (the default, and what
  the dock counts), **Latest activity** (agents at work first, the newest
  started on top, then the rest by when they last did something: finished,
  asked, or, for one that exited, last printed), **By task** (task names in
  reading order, ACME-2 before ACME-10, chats last, the newest agent first
  within a task) and **Newest first** (by when each started). Ties keep
  the order the panes came in. Looking at a finished agent does not move
  it, so opening one does not reorder the list under you.
- **PANE-18** Claude Code MUST start in its `auto` mode while "Start
  Claude Code in auto mode" is on (the default): Claude judges which
  commands and edits are safe and asks only about the rest, so an agent
  stops less often to ask what it asked before. It holds for task agents
  and chats, in a terminal (`--permission-mode auto`, on a start and on a
  resume) or over ACP (set before the first prompt, as BOT-12 sets it).
  A Slack thread's chat follows the bot's *Work without asking* instead
  (BOT-12), and the Henchman always starts in it (HEN-2). An agent that
  offers no `auto` mode keeps its own. Turned off, an agent asks as its
  own settings say; one already running keeps its mode.
- **PANE-19** Images dropped on a terminal MUST be saved as ACP-20 saves
  them (up to ten PNG, JPEG, GIF or WebP, 10 MiB each, in `attachments/`
  of the task root or chat room) and their paths pasted at the cursor,
  escaped as a Mac terminal escapes a dropped file, so Claude Code takes
  each as an image. The window cannot see where a dropped file lives, so
  the copy is what gets a path; other files are refused, with the reason.
  Before this, a drop on a terminal did nothing.
- **PANE-20** An agent, and everything it starts, MUST run at a lower
  priority than the rest of the Mac (nice 10), whether in a terminal or
  over ACP; a shell does not. A terminal's agent is started through `nice`:
  lowered a moment after it started, Claude Code went back to normal
  priority. When the cores are
  all busy, the screen, the keyboard and the app's window go first. A
  dozen agents each running its tests froze an 18 GB Mac until it was
  switched off with its button. Not the Mac's background policy, which
  slows an agent's work even when the Mac is idle.
- **PANE-21** An agent MUST NOT start while macOS says this Mac is critically
  short of memory: the start is refused with the reason and what to do,
  and a terminal still opens. A Mac that does not say counts as having
  memory. At launch, memory merely getting short stops putting agents
  back (PANE-7).
- **PANE-22** Every agent MUST be told, in its task's and the chat room's
  context file, that the Mac is shared: tests and builds with at most two
  workers (Playwright `--workers=2`, Vitest and Jest `--maxWorkers=2`,
  pytest `-n 2`, Cargo `-j 2`), only the tests for what changed, one run
  at a time, and its dev servers and watchers stopped when done. The test
  runners read no worker count from the environment, so the app cannot
  set it for them; and one agent's runner started as many node workers as
  the Mac has cores.
- **PANE-23** A click on a link an agent prints in a terminal MUST open it
  in the browser, without asking, when it is a web or mail link. Any other
  link (`file://`, a custom scheme) is not opened, and the user is told to
  copy it. xterm's own handler asked "Do you want to navigate to…?" and
  then opened nothing: its `window.open` does nothing in the app's window.

Code: `pty.rs`, `pty/load.rs` (PANE-20..22), `agents.rs`, `commands/panes.rs` (`restart_pane`),
`Terminals.tsx`, `Terminal.tsx` (links through `lib/terminalLink.ts`,
PANE-23; drops through `lib/chatDrop.ts` and
`commands/attachments.rs`, PANE-19), `AgentsView.tsx` (`lib/agentSort.ts`),
`PaneNotice.tsx`.

Verify PANE-8 in the mock harness: open the Claude Code terminal, type `@`
with an Option-based layout, and check `__mock.calls` for one `pty_write`
with `data: "@"`, not an escape sequence.

Known gaps:
- Resume, pre-trust, and drafting PR and ticket descriptions are Claude
  only. Drafting runs `claude` whichever agent the task uses.
- Gemini gets the app's tools and task context only in the app's own
  folders (a task folder, a chat room), not when started inside one repo.

## 4. Agent state and "needs you"

Every agent pane has a state, shown as a coloured dot everywhere it
appears:

| State | Meaning | Counts as "needs you" |
|---|---|---|
| working | busy on a turn | no |
| asking | waiting on an answer: a permission prompt, a question | yes |
| done | finished a turn you have not looked at yet | yes (not for a chat) |
| idle | nothing to do, or done and seen | no |

A pane can also carry a **notice**: `usage_limit` (the CLI said its plan's
limit is reached) or `trust_prompt`. A notice counts as needing you, and
shows as a banner above the terminals.

- **STATE-1** State MUST come from what the CLI reports (hooks, plugin,
  title, the protocol for an agent over ACP: ACP-2), never from resize,
  refresh or polling. Output timing decides only
  for a CLI that reports nothing, and not right after the app sent the pane
  something.
- **STATE-2** Esc or Ctrl-C at a question or during a turn MUST make the
  pane idle, since no Stop hook runs for an interrupted turn. Answering a
  question makes it working.
- **STATE-3** Done becomes idle once the pane has been on screen. A pane
  on a loop reads as working between its turns while the loop runs, and as
  done again when the loop hands back (LOOP-8).
- **STATE-4** Every view MUST agree: the sidebar, the task's pane bar, All
  agents, Chat and the "N need you" count use `paneState` / `needsYou` in
  `lib/derive.ts`, and the dock count uses the same rule in `attention.rs`.
  While the Henchman runs, the dock and banners leave out the agents it
  hears of and count what it needs you for instead (HEN-13).
- **STATE-5** A state change MUST reach the UI as an event (`pty:activity`)
  as it happens, not at the next poll.
- **STATE-6** A usage-limit notice MUST NOT fire on prose that merely
  mentions limits. It clears when the agent next reports working.
- **STATE-7** With **Keep awake** on (the cup in the top bar), the Mac
  MUST NOT idle-sleep while at least one agent, in a task or a chat, is
  working, and MUST be free to sleep again within one attention pass (5 s)
  of the last one stopping. Asking, done, idle and shells do not count. The
  display still sleeps, and closing the lid still sleeps the Mac. It is held
  by `caffeinate -i -w <the app's pid>`, so it ends with the app, crash or
  not. The cup is outlined while on, and coloured while it is holding.
  With a phone's way in open, any running agent holds it (PHONE-8).

Code: `pty.rs` (`PaneMeta::state`), `mcp.rs` (`/hook`), `agents.rs`
(`hook_activity`), `store.ts`, `attention.rs`, `awake.rs`.

## 5. Chat

The Chat view runs agents that belong to no task: for questions about the
tickets, the repos, what is in flight. Each chat has its own room folder
(`<worktree root>/_chat/<id>`). The room's context file lists the connected
integrations, the app's tools, the repos and the tasks in flight.

- **CHAT-1** Each chat MUST get its own folder, since the CLIs key their
  history by folder.
- **CHAT-2** A chat that is done MUST NOT count as needing you. One that is
  asking does. Its banners open the Chat view.
- **CHAT-3** A task a chat's agent created (`start_work`, `create_task`)
  MUST say so in its Agents tab, with the chat's name and state and a
  way to open it, for as long as that chat is open. The agent goes on
  working on the task from the chat, so its pane is never one of the
  task's: without this, the task read "No panes yet" while the work was
  being done. The chat is known by its room folder, which outlives the
  pane's id across a restart. Which pane called comes from `X-Villain-Pane`,
  filled in by each CLI from `VILLAIN_PANE`. It is not a credential: an
  agent could name another pane, and all that would do is attach this link.
- **CHAT-4** The list of chats MUST be made wider or narrower by dragging
  its right edge, as the task list is (TASK-16): the same bounds and keys,
  its own remembered width, and 230px on a double-click. Chat names were
  cut short at a width nobody could change.
- **CHAT-5** The chats the Slack bot opened (§24) MUST be listed apart
  from yours, under a Slack heading below them that says how many there
  are and how many are asking. The heading is folded until you open it,
  and remembers whether it is open. Folded, it still shows the ones
  working or asking, and the one on screen. A thread's chat stopped for
  being idle (BOT-14) waits under the heading, not under Not reopened.
  Each thread keeps a chat, so mixed in with yours they buried the chats
  you started.
- **CHAT-6** A chat MUST be pinned and unpinned from its right-click menu
  (Pin, Unpin). Pinned chats are listed first, under a Pinned heading, in
  the order they were pinned; the other open chats follow under an Open
  heading. With nothing pinned, neither heading shows. A pin is kept by the
  chat's room folder (CHAT-1), so it outlives restarting the chat or the
  app, and it goes when the chat is closed or forgotten. A pinned chat that
  was not reopened at launch (PANE-7) waits under Pinned, after the open
  ones, not under Not reopened; reopened, it takes its place in the pin
  order again. A pinned Slack chat (CHAT-5) is listed there, not under
  the Slack heading. Pins stay on this Mac. After a day of chats, the few
  you kept going back to were buried among the rest.
- **CHAT-7** A chat's context MUST tell its agent to put work in a task
  before it changes a repository, commits, pushes or opens a pull request:
  a task in flight when it is the same work (`add_repo` for another
  repository), else `start_work` or `create_task`, and then to work in that
  task's worktrees, not the original clones. Questions, reading code,
  drafting tickets and reading logs need no task. The Work tab lists only
  tasks, so work done from a chat in a clone was missing from it.
- **CHAT-8** An open or waiting standalone chat's row menu MUST offer
  **Edit details…** with keyboard and pointer access, using the shared
  editor (DETAIL-1). Slack chats shown in Layer are included; the
  Henchman, routine runs and an epic's embedded chat are not standalone
  chats. The display name also identifies the room in Chat's close dialog.
  Details follow the room, not a pane or ACP session id, through restart,
  resume, waiting, reopening and Slack idle stop or reopening. Closing an
  ordinary chat or explicitly forgetting a waiting chat removes its
  details. A close while still linked to a Slack thread retains them.
  Details MUST NOT change pins, their order, or Slack thread links.

### Local item details (Chat and Work)

- **DETAIL-1** One desktop editor MUST set, change or clear display name,
  tags and color independently. It offers the automatic name as a
  placeholder, catalog tag choices, removable readable text tags, and a
  way to type a new tag. Save commits the edited fields together; omitted
  fields keep their values. Cancel or Escape before submission discards
  the draft. While saving, duplicate submissions are blocked. A missing
  item or a failed save applies none of the draft, including new catalog
  entries, and MUST NOT recreate the item. The editor stays open with its
  draft and an inline error on failure. A successful save updates visible
  details without a restart and raises no success toast.
- **DETAIL-2** Chat and Work rows MUST show assigned tags as readable,
  neutral text chips and an optional separate, named color indicator: a
  thin bar down the row's left edge, so the row keeps its lines and the
  colour is never read as a second status dot. A task under an epic has
  its bar at its indent, so it never reads as part of the epic's.
  The choices are Lavender, Cyan, Rose, Blue, Amber, Mint and **No color**
  (`data-1` through `data-6`, or no assignment). Color is independent of
  tags, agent activity, checkout and review status: never a status dot
  or a full-row tint. Items without details keep their existing appearance.
  Labels and focus MUST work with a keyboard, narrow rows, and day/night
  themes. This serves **Show less**, **Even colour**, and **The accent
  stays out of the neutrals**.
- **DETAIL-3** Saving MUST trim surrounding whitespace, treat a blank name
  as no override, ignore empty tags and case-insensitive duplicate tags,
  preserve assignment order and reuse the first saved catalog spelling.
  Before trimming, it rejects control characters and Unicode line or
  paragraph separators. It rejects multiline text, names over 120 Unicode
  scalar values, tags over 40, more than 12 distinct assigned tags and
  colors outside the palette. Adding a single catalog tag in Settings or
  MCP rejects a blank tag.
- **DETAIL-4** Details and the shared tag catalog MUST persist only in the
  current desktop profile. Old profiles and absent details load with no
  overrides, tags or colors. Editing details MUST NOT change item identity,
  worktrees, branches, ticket or PR data, agent prompts or context,
  session/transcript history, or workspace sync. It MUST NOT switch Work's
  selected grouping or change default stage ordering. This feature adds
  no Chat search/grouping, bulk editing, custom colors, tag colors,
  catalog rename/delete, cross-profile/device/team sharing, or phone/web
  editing, and changes no creation, resume, finish or deletion flow.

Code: `ChatView.tsx`, `ChatList.tsx` (CHAT-5, CHAT-6), `lib/pins.ts` (CHAT-6), `ChatLink.tsx`, `sidebar/SidebarGrip.tsx` (CHAT-4),
`commands/panes.rs` (`open_chat`, `write_chat_context`, CHAT-7), `mcp.rs` (`caller_chat`).

Known gaps:
- The room's "tasks in flight" list is written when the chat opens and at
  startup, so it goes stale while a chat runs.

## 6. Diff, review notes, commit

The Diff tab shows the task's changes across all its repos. There are two
scopes: Uncommitted (against HEAD) and Whole branch (against the branch
point). Whole branch can also show one commit at a time. Clicking a line
number leaves a note on it. The notes go to a running agent, or start a
new one. Commit commits every repo with changes, with one message.

- **DIFF-1** The file list and patches MUST come from git as the user's
  config would not change them. That means `--no-ext-diff`, untracked files
  always listed, and renames and non-ASCII paths kept (`-z`).
- **DIFF-2** Untracked files MUST show as all-additions patches. Binaries
  and files over 4MB are listed but never read.
- **DIFF-3** Whole branch MUST measure from the recorded branch point
  (TASK-3, moved by UPD-6), so after a merge it shows only what has not
  landed. The commit picker follows the first parent, so a merge of the
  base does not list the base's history.
- **DIFF-4** Commit MUST skip clean repos, and refuse in a repo mid-merge or
  mid-rebase. A failing commit hook keeps the dialog open with its output.
- **DIFF-5** A note sent to an agent at the task folder MUST name its repo
  (`api/src/auth.ts:42`), so it is never ambiguous which repo is meant.
- **DIFF-6** Notes MUST be held per task while the app runs, and a file
  with notes is marked in the tree. Kept by the tab, they were lost by
  switching to another. A note on a line the patch no longer draws is
  listed above it with its line number. (Until 0.9 this was Review, a
  one-shot model that left notes of its own; it was taken out.)
- **DIFF-7** The file list MUST fold: each folder, and, in a task with
  more than one changed repo, each repo under its name. A fold belongs to
  its repo, so folding `src` in one repo leaves another repo's `src` open.
- **DIFF-8** A note MUST be able to say what the agent needs: any note
  can be edited. "Suggest a change" adds a suggestion block holding the
  line's code, selected, so typing replaces it with what it should read. A
  suggestion block is shown as a suggested change wherever notes and GitHub
  comments are drawn. A removed line can be commented on, numbered in the
  old version of the file; shift-clicking a second line number on the same
  side stretches the note being written over every line between. Both go
  to an agent as `file:12-16` or `file:3 (removed)`, with the lines they
  read. A line number that takes a note is reached with Tab: Enter is a
  click on it, ⇧Enter a shift-click.
- **DIFF-9** A patch MUST read as it does in Cursor or VS Code with the
  Code Villain theme: the code coloured by the same TextMate grammars and
  theme scopes, added and removed lines on the editor's diff tints, the
  changed words of a replaced line marked, and each line numbered in the
  old and the new version. A file in a language Layer does not bundle, or
  too large to colour quickly, is drawn uncoloured on the same tints.

Code: `commands/diff.rs`, `git.rs`, `DiffView.tsx`, `DiffNote.tsx`,
`DiffCode.tsx` and `lib/highlight.ts` (DIFF-9).

Known gaps:
- Notes do not survive a restart of the app.

## 7. Update from base

Brings each repo's branch up to date with its base, by merge or by rebase,
each repo its own team's way.

- **UPD-1** It MUST refuse when tracked files have uncommitted edits.
  Untracked files are fine. It also refuses when the worktree is on another
  branch, or already mid-update.
- **UPD-2** Merge MUST say `--ff` explicitly, since `merge.ff = only` in the
  user's config broke every merge. Rebase overrides `autoSquash`,
  `autoStash` and `updateRefs` for the same reason.
- **UPD-3** Rebase MUST refuse when the remote branch has commits the local
  one lacks, unless the remote tip is exactly the recorded lease. Otherwise
  someone else's pushed work would be dropped.
- **UPD-4** After a rebase, the remote tip is recorded as the push lease.
  The next push uses `--force-with-lease=<branch>:<that sha>`, and clears
  the lease.
- **UPD-5** Conflicts MUST be left in place, to hand to an agent (who is
  told not to push, and how to continue) or to abandon. Abandoning restores
  the branch point as it was. Any other failure is aborted at once, so a
  worktree is never left half updated.
- **UPD-6** The branch point moves to what was merged in, or rebased onto,
  and to the head of a pull request of the branch once it has merged, as
  soon as the PR watch or opening PRs sees it. Only forward: only to a
  commit the branch grew from and that has the current point in it, never
  during an unfinished update. A base merged back in after the PR already
  holds its work, and moving back would count the base's work again.
  Measured from where the branch was cut, merged work stayed in the Diff
  view, the panel's counts, a follow-up PR's description and the agent's
  history, and opening PRs opened a second PR for it.
  A merged head that is not on the branch here (GitHub's "Update branch",
  a suggestion committed there, or the branch rebased here after it was
  pushed) is fetched by its id, and the point moves to the newest commit
  up to which the head has every commit since it, as it is or as the same
  change. Asked only whether one contained the other, a repo whose every
  commit had merged said "has work since", and Finish kept its branch.
- **UPD-7** Each repo MUST start on its own way of updating: what it is set
  to in Repos, or the way it was last updated; else a guess, in the app's
  copy, from the first of these that says anything:
  - how its recent branches on origin (the last 90 days) took the base in:
    a merge of the base, or commits rewritten with none, whichever is
    more common over three or more branches. A branch made mostly of
    merges is a release branch and does not count;
  - how pull requests land on the default branch: merge commits mean
    merge, a straight line of commits as written means rebase. Squashed
    pull requests say nothing about how the branch was kept up to date:
    taken for "merge", they guessed wrong for two frontend repos that
    rebase;
  - the way the other repos in its group go, counting only those with a
    choice or a guess of their own, since a group is usually one team;

  else merge. The guess is said to be one, with its reason, and never
  overrides a choice. One app-wide mode, the last used, was wrong as soon
  as a task spanned a team that merges and one that rebases.
- **UPD-8** Each repo's base MUST be changeable in the dialog, from its
  origin branches or typed, and a base not among them at the last fetch is
  said to be. It is the same base the next pull request is opened against,
  and changing it drops the branch point (DIFF-3 then measures from the
  merge base). Only the Open pull request dialog had the field, and that is
  shut once every repo has a PR: a stacked base merged and deleted upstream
  could not be moved off.

Code: `commands/diff.rs` (`update_from_base`), `git.rs`,
`git/upkeep.rs` (`update_style`), `commands/landed.rs` and `git/landed.rs` (UPD-6 after a merge), `UpdateFromBase.tsx`.

Known gaps:
- The guess reads only history. The site's own rules for the base branch
  (GitHub's "require linear history", a ruleset's allowed merge methods)
  are not asked, though they would be firmer than a guess.

## 8. Pull requests

The Pull requests tab shows, per repo, the open PR, or an earlier one if
none is open. It shows CI, the latest review per reviewer, the verdict, and
how far behind the base it is. Each open PR has a read-only **Comments**
section, separate from **Feedback → agent**. From here: push everything,
open PRs, mark a PR ready for review or back to draft, retarget a PR onto the
checkout's base or take the PR's base for the
checkout (GitHub moves a PR itself when its base is merged and deleted),
send feedback to an agent, and finish a merged task.

- **PR-1** Opening PRs MUST push each repo that has commits, open one PR per
  repo against that checkout's base (draft by default), and reuse a PR that
  is already open. A repo with only uncommitted changes is refused: the app
  never commits for you here. A merged PR first moves the branch point up
  to what it landed (UPD-6), so a repo with nothing since is skipped, not
  opened again. Counted from where the branch was cut, a repo merged while
  another was still in review got a second PR for the same work.
  In the dialog, every repo that would get a new PR has a tick, all on to
  start with; one ticked off is neither pushed nor opened. A repo whose PR
  is already open has no tick: it is pushed and listed with the rest.
  Before anything is pushed, the task's spec is committed (SPEC-35) into each
  ticked repo whose committed copy differs from `spec/`, and the result says
  so; a repo it could not be committed in is named and not opened.
- **PR-2** PRs this call opened MUST be linked on the Jira ticket as a
  comment, and posted to Slack if that is on. When the call opens a PR and
  the task has more than one open, every one of them MUST list the others
  in its description, in a marked section the app rewrites; the rest of
  the description is left as written. A failed link is reported, not
  swallowed.
- **PR-3** Push MUST go through `git::push`, with the lease when there is
  one (UPD-4). Never `--force`. Pushes run in every repo at once.
- **PR-4** The description can be drafted. A one-shot model run gets the
  commits, the stat and the diff (lockfiles left out, 60k characters at
  most), the spec's requirements with their check (SPEC-16), and each
  repository's pull request template, and streams its answer into the
  field. A template is looked for where the forges keep one, whatever the
  case of its name: Azure DevOps' for the base branch
  (`pull_request_template/branches/<base>.md`), then
  `pull_request_template.md` (or `.txt`) in `.github/`,
  `.bitbucket/`, `.azuredevops/`, `.vsts/`, `.forgejo/`, `.gitea/`,
  `docs/` or the root, or GitLab's
  `.gitlab/merge_request_templates/Default.md`; failing those, a folder of
  several (`PULL_REQUEST_TEMPLATE/`, GitLab's merge request templates),
  from which the model picks the one that fits. The draft fills the
  template in: its headings, checklists and order, a checklist item
  ticked only where the change shows it. Repositories that share a template
  give it once. Failing the one-shot run, a running agent is asked to
  write `PR_DESCRIPTION.md` in the task folder, and told where templates
  are kept. Before, the draft chose its own sections and a team's template
  was left out.

  A template that asks for screenshots (it says screenshot, screen
  recording, screencast or visual evidence) needs someone who can run the
  app, so with an agent running, Draft asks it rather than the one-shot
  model, and says so while it waits (up to 10 minutes). The agent is told
  which repositories' templates ask. If the change is visible, it runs
  the app and takes screenshots in Layer's browser, night then day
  (`browser_screenshot` with `save: true`), lists them in `PR_IMAGES.json`
  beside the description (path, caption, repository), and writes
  "Screenshots below." where the template wants them. If not, it ticks
  what the template has for no visible change and says why. The dialog
  shows the screenshots under the description, each with its caption and
  a way to leave it out, and says that Open stores them in the
  repository. A click on one opens it full size over the window; ← and →
  step through them, Esc or a click closes it. A file that is not an image, or a list that does not read,
  is said in the dialog rather than dropped. Once Open PRs has opened the
  pull requests, it adds the screenshots still there to each one's
  repository's PR as PR-13 does (a row per repository, not to a PR that
  failed to open). Pressing Open with them shown is the user's agreement.
  With no agent running, the one-shot model is told it cannot take
  screenshots: it ticks the visible-change box and writes "Screenshots to
  follow." where they go, and the dialog says no agent was running to
  take them. Before, a draft for customer-portal ticked "UI change" and
  attached nothing.
- **PR-5** Review threads MUST be read in full: every page, resolved and
  outdated flags kept. A verdict is each reviewer's latest decisive review.
  One "changes requested" outranks any number of approvals, and a dismissal
  clears it.
- **PR-6** Feedback to an agent is asked for from a PR's own card, with a
  count of that PR's comments and failing checks; one button for every PR
  gave no way to tell whose they were. Beside it, how many wait on your
  reply: open threads someone else spoke last on, and what people said in
  the conversation since you last did. Bots wait on no one. The picker shows that PR, and a task
  with several switches between them, each saying how many of its items
  are picked. Everything picked, on any of them, is sent in one go, and the
  send button says from how many PRs.
  It MUST pre-select only what is new: not
  resolved, not sent before, not from a bot, not your own, not outdated.
  The feedback is written to `PR_FEEDBACK.md` in the task folder, never in
  a worktree, and a pointer is typed into the agent. It is not told to keep
  off GitHub: whether it replies or resolves threads is the user's call,
  and told never to, it refused when asked to.
- **PR-7** Failing GitHub Actions checks MUST come with the end of the
  job's log, trimmed to the error.
- **PR-8** Every task's PRs are refreshed every 90 seconds (180 when idle)
  while the window is in front, timed from the last sweep: switching views,
  going idle and coming back never put one off, and coming back to the
  window when one is due runs it at once. (The timer used to restart on
  each of those, and in ordinary use never fired.) Which GitHub
  repository a worktree's origin names is asked of git at most every 10
  minutes, and again at once when the app moves a copy to its clone's
  `origin` (REPO-4). A task whose pull requests were all closed or merged
  when last read (every repo's newest PR closed or merged, or none and
  nothing on its branch, and nothing failed) is swept only every 15
  minutes while the window already has its rows. A window without those
  rows reads the task on its next sweep, even if a background read just
  found it settled. Its PR tab still asks at once when opened, and opening
  its PRs from the app puts it back in every sweep. The first sweep is
  silent. After it, a new approval, request for changes, new comment or
  merge is announced as a toast.
- **PR-9** A late answer MUST NOT overwrite a newer one. A sweep that
  started before a PR was opened does not put back "no PR".
- **PR-10** The feedback picker MUST show each item in full, with no
  click to expand it: every comment of a thread, and a check's report and
  log. Markdown is shown as GitHub shows it (headings, emphasis, code,
  lists, tables, links), built from elements and never from HTML, since
  the text is someone else's: raw tags keep only their text, a link opens
  in the browser only for http(s) and mailto, and images are not loaded.
  Each item is a box of its own, marked on its edge when picked: as rows
  split by a hairline, a long review ran into the next and there was no
  telling where one ended. Clicking an item's heading picks it; its text
  can be selected.
  A review thread shows the code it is on above its comments, as GitHub
  does: the lines it spans, or the line and the three above it, cut from
  the hunk it was written against (so an outdated thread still shows the
  code it meant). Its heading names the range, `file:16-20`, and the same
  code goes to the agent.
  A resolved thread is the one exception to "in full": it is listed last
  in its repo, shut to its heading with a `resolved` chip, and opens with
  one click on its arrow. It is never picked for you, and "All" leaves it
  out. One ticked on purpose goes to the agent marked as resolved, with a
  warning not to undo what was settled.
- **PR-11** Open pull requests MUST show what it is about to push before
  it does, per repo: the commits origin does not have yet (from the
  branch on origin, or the branch point when there is none or it was
  rebased), and in their added lines what is usually there by accident:
  debug output (`console.log`, `dbg!`, `debugger`, `binding.pry`, …), a
  focused test (`.only`, `fit`), a conflict marker, a new TODO or FIXME.
  Prose files (`.md`, `.txt`, …) are not read for these, nor is anything
  uncommitted, which a push does not send. It says how many notes on the
  Diff tab were not yet sent. It is advice, never a
  gate: with anything flagged, the button reads "Open anyway". A push used
  to be the first time anyone saw the branch whole.
- **PR-12** When GitHub answers 401, the tab MUST say once that GitHub no
  longer accepts the saved token, with a button to the GitHub page of
  Settings, rather than the same failure under every repo. Other failures
  are still listed per repo. An expired token used to show GitHub's raw
  "Bad credentials" JSON beside each repo, and nothing said what to do.
  While the last read was refused, the chip in Settings says "token
  rejected" in place of "connected", which only ever meant a token was
  saved. Saving a token starts a new sweep at once, without waiting for
  the old connection's sweep. Old results cannot replace the new rows.
  A 403 that GitHub marks as single sign-on is said once too: the organization
  requires SSO and the token is not authorized for it, with the way to
  authorize it and a button to GitHub's token settings. There is no button
  to check again: the next sweep reads the repos again on its own, as
  behaviour.md asks of a connection. The chip says "needs single sign-on". It used to show GitHub's "SAML enforcement" JSON beside every
  repo, Settings said "connected", and no task ever showed as in review.
- **PR-13** An agent MAY offer images for a task's open pull request with
  `pr_images`: screenshots of a visible change, night and day. The call
  publishes nothing. The task's Pull requests tab shows them, each with a
  preview the user can leave out or open full size, as PR-4's dialog
  does, and only the user's Add to pull request
  stores them; Dismiss forgets them. A later offer replaces the one before.
  The app holds offers in memory, so a restart forgets them. Before, the
  agent published on its own `confirm: true`, and Claude Code's auto mode
  refused that as a possible leak when the user's yes was only a "go on"
  in the chat. GitHub has no API for the attachments its own editor
  uploads, so the app stores the images in the repository, in a commit
  that only `refs/pr-images/<number>` points at: not a branch, so no
  branch list, clone or pull fetches or shows it. Each PR has its own,
  holding only the images its description shows now: a later call moves
  the ref onto a new commit, by force, through the API (the one forced
  move the app makes, on a ref that is its alone). The ref is kept after
  the PR closes, so a merged PR keeps its pictures. The images go in a
  marked section of the description that the
  app owns, the way PR-2's list does: a later call replaces the section
  and nothing else, and a new section goes above PR-2's list. Only PNG,
  JPEG, GIF and WebP files are sent, read by their first bytes rather
  than their names, at most 10 of 10 MB each: the tool reads a path an
  agent gives it, and must not publish a key file renamed `.png`.
  `browser_screenshot` with `save` writes the tab's image to a file for
  this. A repository with no open pull request is refused: images are
  added to a PR, never the reason one is opened.
- **PR-14** A repository's pull requests are those from the task's branch
  and, when its folder is on another branch, from that branch too: the PR
  tab, the sweep (PR-8), feedback (PR-6), retargeting and `pr_images`
  (PR-13) all look in both. The folder's own base and the repository's
  default branch don't count as another branch. Open pull requests (PR-1)
  still pushes and opens from the task's branch, and agents are told to
  commit, push and open pull requests there. Before, an agent that
  put a repository on a branch of its own and opened its PR from there
  left the task saying "no PR" while the PR waited on GitHub. A PR from a
  branch the folder has since left is still not found.
- **PR-15** An open pull request's card MUST move it between draft and
  ready for review: a draft has a Ready for review button beside its draft
  chip, and a ready one a quiet Back to draft link under its title. GitHub
  does it, through GraphQL (its REST API ignores `draft` on an existing
  PR), and the card shows the new state once read again; a refusal, such as
  a token that may read the PR but not change it, is shown on the tab.
  Before, Open PRs opened drafts (PR-1) and only GitHub could take one out
  of draft.
- **PR-16** Each open PR's card MUST offer **Comments**. Opening it reads
  only that repository's PR. It shows conversation comments, submitted
  review text, and inline threads with their replies, authors and links
  back to GitHub. It keeps file and line context when GitHub supplies it,
  marks outdated threads, and folds resolved threads by default while
  letting the user open them. It uses the same safe Markdown as the picker
  (PR-10). Reading comments or checks does not send anything to an agent
  or write to GitHub. **Feedback → agent** still uses its own picker and
  explicit send (PR-6, PR-7, PR-10). Earlier PR history and the other PR
  actions are unchanged.
- **PR-17** A PR card MUST label CI in text even when folded. The expanded
  card shows each check's name, state and GitHub link when available. Both
  use the check runs for the PR's current head. If a name repeats, a
  passing run MUST NOT hide a failed one. A known failure takes precedence
  over pending runs. Distinguish passing, pending, cancelled, only skipped
  or neutral, and no reported check runs. Unavailable or incomplete reads
  MUST NOT read as passing. An incomplete read still shows the runs
  it found and says that the list is incomplete.
- **PR-18** Opening Comments or refreshing the tab MUST distinguish a read
  in progress, a complete empty result, and a failed or incomplete read.
  An error or a read limit is shown on the affected PR and section, without
  hiding results that were read there or elsewhere. **Refresh** reloads
  open Comments; the background PR sweep does not load them. A result for
  another PR or an older request MUST NOT replace the current result.
  GitHub connection handling stays as in PR-12.
- **PR-19** A task MAY span GitHub.com and GitHub Enterprise Server. Layer
  MUST route PRs, checks, feedback and images by the checkout's Git origin,
  not by repository name or the last GitHub source connected. A source that
  cannot be reached or is not connected is named on the affected row. Other
  rows still load. An unreadable PR is not evidence that work has landed
  or a branch is safe to delete. Layer lists sibling PRs on the task, but
  does not add links to another GitHub host in a PR's description.

Code: `commands/github.rs`, `commands/pr_branches.rs`, `commands/pr_draft.rs`, `integrations/github/ready.rs`, `commands/open_prs.rs`, `commands/pr_images.rs`, `commands/pr_image_offers.rs`, `PrImageOffer.tsx`, `commands/outgoing.rs`, `git/outgoing.rs`, `Outgoing.tsx`, `commands/landed.rs`, `git/landed.rs`, `integrations/github.rs`, `integrations/github/images.rs`, `PrPanel.tsx`, `PrReadErrors.tsx`,
`PrFeedback.tsx`, `Markdown.tsx`, `lib/markdown.ts`; its connection in `CodeSettings.tsx`.

Known gaps:
- This tab's CI reads GitHub Check Runs only. It does not read legacy
  commit statuses (Jenkins, older CircleCI); the Reviews rollup includes
  them (REV-13).
- A failed review verdict request still reads as "no verdict" rather than
  as an error. This is separate from the Comments reader (PR-18).
- Two feedback sends for one task seconds apart overwrite each other's
  `PR_FEEDBACK.md`.
- There is no rate-limit handling beyond pacing.
- Copilot's title, severity badge and "suggested changeset" on a review
  comment are not shown. GitHub does not return them with the comment,
  and the suggestion is not in its body the way a reviewer's
  ```` ```suggestion ```` block is.

## 9. Tickets

A workspace's tickets come from one issue tracker: Jira, GitHub Issues or
Forge. Everything in this section is that tracker's: Mine, Find work, Start
work, filing, epics (§26) and tickets following the work. Where a
requirement below names Jira, GitHub Issues and Forge do the same in their
own terms (TKT-17).

The Tickets view has two tabs. **Mine** is your assigned, not-done tickets,
grouped by epic, by status or not at all (TKT-14), with a filter (key,
title, description, labels, components), type toggles
and a sort (TKT-13). **Find work** searches
beyond them: not mine / unassigned / anyone, done included or not, by type,
by text, or by key. A ticket opens its task if there is one; otherwise it
offers Start work.

- **TKT-1** Start work MUST create the task for the ticket (for an
  epic-level ticket, an epic, EPIC-2). It suggests
  repos from what was used before for its epic, then its project, and says
  which it was ("preselected from the last task under ACME-100"), so a
  stale guess is visible. It picks
  a base, optionally starts an agent with the ticket as the prompt (or,
  with "Write a spec first", told to write the spec first, SPEC-18), and
  moves the ticket where its project's "work starts →" says, beside the
  other stages of Tickets follow the work (TKT-8): to the first
  in-progress (`indeterminate`) status its workflow allows, unless it is
  in that category already (the default); to a status chosen there,
  unless it is there already, out of done too; or nowhere. A project
  with nothing chosen goes as the switch in General that came before
  said, on or off. That switch was out of sight of the other stages, and
  a ticket moved to the first in-progress status of a workflow with
  several could only be left or taken there.
- **TKT-2** Nothing MUST depend on a site's names. Epics are types at
  hierarchy level 1 or above. "In progress" and "done" are status
  categories. The Epic Link field is found by its schema. The default type
  is the level-0 type called "Task" if there is one, else the first.
- **TKT-3** Filing an issue MUST ask Jira what the project requires
  (`createmeta`), offer a choice for each required field with a fixed set
  of values, and name any it cannot fill.
- **TKT-4** Text typed into a search MUST be quoted as a JQL string.
  Project keys are always quoted (a key can be a reserved word, like `IT`).
- **TKT-5** Any key or id from Jira that goes into a URL path MUST be
  validated first. A ticket's text is written by anyone.
- **TKT-6** "Improve description" MUST stream a rewrite of the ticket or
  epic description into the field, for the user to accept or edit.
- **TKT-7** Your ticket list is re-read every 3 minutes while the window is
  in front. While it is away, the backend looks for itself (NOTE-3).
- **TKT-8** A task's ticket MUST follow its pull requests: to the project's
  review status once one of them is ready for review (not a draft), and to
  its after-merge status once every one has merged, with nothing left
  unmerged or uncommitted. Both statuses are chosen per Jira project in
  Settings, from the project's own statuses: In Progress, Review and
  Testing all share Jira's "in progress" category, so the app is told,
  never guessing by name. Until one is chosen the app says so, once per
  project and stage, and moves nothing. Each stage moves the ticket once per
  task (`Task.ticket_stage`), so a ticket moved by hand afterwards stays
  where it was put; a review never takes a ticket out of done. A workflow
  with no transition to the chosen status is said once, not retried. It
  runs with the PR sweep (PR-8). Before this, tickets sat In Progress with
  their pull requests merged weeks before.
- **TKT-9** Deleting a task MUST show its ticket's status and offer its
  transitions. It leaves the ticket as it is by default, since a deleted
  task is as often abandoned as done, and pre-selects the after-merge
  status when a pull request of the task has merged. The ticket moves only
  once the task is gone. An epic is removed from the list instead, its ticket
  untouched (EPIC-15).
- **TKT-10** The task header MUST show its ticket's status, from your ticket
  list, so a ticket out of step with the work is seen where the work is.
- **TKT-11** Every ticket card MUST say how long ago the ticket was created,
  in its largest whole unit ("5m", "3h", "2d", "1w", "4mo", "2y"), with the
  full date on hover.
- **TKT-12** In Find work, a key MUST be looked up rather than searched for.
  A bare number is a key in the project set in Settings ("4826" is
  `ACME-4826`), since Jira's text search does not look at keys. If no
  ticket has that key, or the filters hide it, the number is searched for
  as text instead. Without a project set, a number is only text.
- **TKT-13** Both tabs MUST sort by last updated (the default), newest or
  oldest filed, or priority, highest first. One choice serves both tabs and
  is remembered across launches. Priority follows the site's own list of
  priorities, never names the app knows; a ticket with none, or one the
  list lacks, sorts after the rest, and ties go to the most recently
  updated. In Mine, tickets are sorted within their epic, and epics come
  in flight first, then in the order of their top ticket, with "No epic"
  last. Find work asks Jira for the order, since its results are capped:
  sorting the first hundred here would only reshuffle them.
- **TKT-14** Mine MUST group by epic (the default), by status, or not at
  all, and the choice is remembered across launches. By status, a group is
  one status name, so the same name in two Jira projects is one group;
  groups in progress come first, then to do, then done, by status category
  rather than name. Within a group, tickets keep the order TKT-13 gives.
  Ungrouped, Mine is one list in that order. Grouping by repository is not
  offered: a ticket has no repository until a task is started from it.
- **TKT-15** A workspace MUST have one issue tracker, or none: Jira,
  GitHub Issues or Forge. Admins choose it on the Tickets page of
  Settings, in the workspace's block (SET-7), so it is the same for
  everyone in the workspace. A second tracker in the same workspace is
  not offered: every issue filed would first ask "which tracker?", an
  epic in one could not hold tickets from the other, and the same work
  could be tracked in both with neither one the truth. Someone who works
  in two trackers uses two workspaces, which run side by side (§25). A
  workspace that had a Jira connection before trackers could be chosen
  starts with Jira as its tracker.
- **TKT-16** Changing the tracker MUST first say how many tasks have a
  ticket in the tracker being left, and keep those tasks as they are.
  Their ticket stays a link to where it lives, but no longer follows the
  work (TKT-8, TKT-18) or shows its status (TKT-10): the app reads only
  the workspace's tracker.
- **TKT-17** Each tracker MUST be read in its own terms, never through
  another's names (TKT-2):

  | | Jira | GitHub Issues | Forge |
  |---|---|---|---|
  | A ticket's key | `ACME-12` | `owner/repo#12` | `FOR-12` |
  | Mine | assigned to you, not in the "done" category | assigned to you and open, in the workspace's repositories | assigned to you, in no Completed or Canceled state |
  | In progress | the "indeterminate" category | no such state: open and assigned | a state of type Started |
  | Done | the "done" category | closed | a state of type Completed or Canceled |
  | An epic (§26) | a type at hierarchy level 1 or above | an issue of the type chosen for epics in Settings; without issue types, an issue with sub-issues | an issue with sub-issues |
  | Filing (TKT-3) | the project's required fields (`createmeta`) | one of the workspace's repositories, a title, a body | a team, a title, a description |
  | Its account | Tickets (Jira): site, email, token | The selected GitHub source's token from Settings → Code | Tickets (Forge): the site, then you sign in to it in the browser, as Forge's own desktop app does |

  Forge names its states per team, as Jira does its statuses, so a state
  is known by its type, never its name. GitHub's issue types are an
  organisation's own, with names and no hierarchy levels, so which one
  is the epic is chosen in Settings, from the organisation's types, never
  guessed from a name. Without them an epic is only known once it has a
  sub-issue: one just filed looks like any other issue.
- **TKT-18** A GitHub issue or a Forge ticket MUST follow the work the way
  its own tracker moves it, not by the app moving it. The task's branch
  carries its key, and each of its pull requests says "Fixes `<key>`" in
   its body, subject to the same-source boundary in TKT-22. GitHub closes the issue when one merges into the repository's
  default branch, and only then. A task based on another branch (TASK-3)
  closes nothing that way, so once every pull request of such a task has
  merged, with nothing left unmerged or uncommitted, the app closes the
  issue itself, once per task, as TKT-8 moves a Jira ticket. Forge moves the
  ticket by its pull requests alone, never by pushes: a draft to its
  team's first Started state (In Progress), one ready for review that
  could be merged to the last Started state (In Review), and a merge to
  the first Completed state (Done); closed unmerged or turned back into a
  draft, it comes back out of review. So the review and after-merge
  statuses chosen in Settings (TKT-8) are Jira's alone. Start work (TKT-1)
  moves a Forge ticket to its team's first Started state, unless it is in
  a Started or Completed state already: the state Forge itself gives a
  draft, so the two never pull against each other, and the ticket is in
  progress before the first pull request rather than after it. A GitHub
  issue has no state to move to, so Start work assigns it to you if it
  has no one.
- **TKT-19** Find work MUST search in the tracker's own language, quoting
  typed text as it requires (TKT-4): JQL for Jira, GitHub's issue search
  limited to the workspace's repositories, Forge's issue search. A bare
  number (TKT-12) is a key in the Jira project or Forge team set in
  Settings. With GitHub Issues it names that issue in each of the
  workspace's repositories, so every one that has it is listed.
- **TKT-20** An agent MUST get the tools of the workspace's tracker only,
  each speaking that tracker's terms: `jira_*` for Jira, `github_*` issue
  tools for GitHub Issues, `forge_*` for Forge. One set for all three
  would have to call a Jira transition, a GitHub close and a Forge state
  the same thing, and fit none of them. The tools that already work on
  "the task's ticket" (`start_work`, `finish_task`, `subtask_start`) work
  on the workspace's tracker, whichever it is.
- **TKT-21** With GitHub Issues, a task's branch MUST be the issue's
  number and the slug of its title (`12-fix-the-login`), as GitHub's own
  "Create a branch" names it, in place of the key TASK-2 gives:
  `owner/repo#12` is no branch name. Since the number alone names no
  repository, where the app reads a pull request's ticket from its branch
  or title (REV-7, REV-11) it asks GitHub which issues the pull request
  closes instead.
- **TKT-22** A workspace using GitHub Issues MUST select one of its GitHub
  sources for tickets. Adding or disconnecting a Code source does not change
  that selection. If the selected source is unavailable, Layer says which
  one instead of using a different one. Existing links keep their original
   URL. Layer adds GitHub issue links and `Fixes` references only to PRs on
   that source. It also leaves references to that task's issue out of PR
   descriptions sent to other sources. Changing the selected tracker clears old ticket results;
   Layer refuses to start work from a card for a different source.

Code: `tickets.rs` (the tracker: `Tracker`; Jira's side in
`tickets/jira.rs`, GitHub's in `tickets/github.rs`), `commands/tickets.rs`
(`set_tracker`), `commands/jira.rs`, `commands/ticket_flow.rs`,
`integrations/jira.rs`, `integrations/jira/flow.rs`,
`integrations/github/issues.rs`, `components/tickets/`, `TicketsSettings.tsx`,
`TrackerChoice.tsx` (TKT-15, TKT-16), `lib/tracker.ts`, `TicketFlow.tsx`,
`sidebar/DeleteTask.tsx`, `commands/tracker_tools.rs` (TKT-20). The
choice is the workspace's `tracker` connection; `AppConfig::tracker_in_use`
reads it, and `jira_client` refuses every Jira-only path (moves,
transitions) when it is not Jira. Agents are offered only the tracker's
own tools, and a call to another tracker's is refused. The `github_*`
tools reach only the workspace's repositories, though Code's token reaches
further.

Known gaps:
- Jira and GitHub Issues are built; Forge is not, and is not offered.
  Nor is "none" (TKT-15): a workspace with no choice uses Jira while it
  is connected.
- GitHub Issues, not yet built: the epic chosen from the organisation's
  issue types (TKT-17), so an epic is only an issue with sub-issues,
  and one just filed as an epic looks like any other issue; closing the issue of a task based on another branch
  (TKT-18); reading a pull request's ticket from the issues it closes
  (TKT-21, REV-7, REV-11). No `github_*` tool assigns, labels or
  reopens an issue. Filing offers the repositories of the issues
  on the board, not the workspace's list. A GitHub Enterprise Server
  without sub-issues fails every read. Tried only in the mock harness
  and by tests, never against github.com.
- Open question: GitHub issues are only open or closed, and many teams
  keep "in progress" in a GitHub Project's Status field instead. Whether
  that board's Status is the tracker's in-progress state, moved as TKT-8
  moves Jira's, is not decided.
- Forge's desktop sign-in hands its session back only to `forge://`.
  Before Layer can sign in to Forge, Forge has to hand it back to
  `layer://` too. Forge's API tokens are agents' only (Settings → Agents),
  and one would make Mine the agent's tickets and every move the agent's.
- Forge moves a ticket only for repositories connected to it (its GitHub
  app, or Forgejo). Anywhere else a ticket stays in progress after its
  merge, and nothing says why.
- Jira Cloud only: email and API token, REST v3. No Data Center or Server
  personal access tokens.
- Rich-text mentions, status and date nodes, and link targets are dropped
  when a description is read.
- Start work moves the ticket without the confirmation the MCP
  `jira_transition` tool asks for. The tool's description does not say so.
- Tickets follow the work only while the app runs with its window in
  front, since that is when the PR sweep runs. A PR merged overnight moves
  its ticket the next morning.
- The task header shows a ticket's status only when it is in your ticket
  list (assigned to you, not done).
- A move that needs two transitions (a workflow with no direct way from
  In Progress to the after-merge status) is reported, not made.
- Sorting by priority takes the order Jira's priority search lists them
  in. Jira does not document that order as the site's ranking.

## 10. Reviews

The Reviews view has two tabs, one for each side of a review. "To review"
lists open PRs waiting on your review, excluding your own, and, when a
review team is set, those waiting on that team. "Opened by you" lists the
open PRs you opened, and where each one stands. The tab last used is kept;
a banner or message about a review request opens "To review". The top
bar's Reviews tab carries both counts, as its tabs show them, `1 · 7`: it
used to show only the first, so seven PRs of yours read as none. A live
dot marks it only while something waits on your review.

- **REV-1** A failed team lookup MUST NOT hide your own list. It shows as
  an error on the team section.
- **REV-2** A bare team slug (`@fe`) MUST be resolved from the teams you are
  on, which needs the `read:org` scope. `org/slug` needs no lookup.
- **REV-3** PRs already in your own list MUST NOT repeat in the team's.
- **REV-4** "Opened by you" MUST list every open PR you authored, in any
  repo, whether or not a task made it, and say for each: draft, its checks
  as GitHub rolls them up (check runs and commit statuses), the review
  decision (or, where the repo requires none, the latest reviews), who is
  still asked to review, how many threads are unresolved, and a conflict
  with its base. One that belongs to a task links to that task's Pull
  requests tab.
- **REV-5** A failed lookup of your PRs MUST NOT hide the review lists, nor
  they it. It shows as an error on its own section.
- **REV-6** It reads the 50 most recently updated and the first 100
  threads of each, and MUST say so when there are more (`50+`, `100+`).
- **REV-7** One of your PRs MUST lead to its work: a click opens its task's
  Pull requests tab, and GitHub is a button on the card. One with no task
  offers "Start task", which makes one in the registered repo whose origin
  is the PR's (`host/owner/name`), on the PR's own branch (TASK-3) and against
  its base, named after its title, with a Jira key in the branch or title
  linked as the ticket when Jira is connected. A task with a checkout of
  that repository on the PR's branch is opened instead. A PR in a
  repo that is not registered says so, and makes nothing.
- **REV-8** "Opened by you" MUST group your PRs by whose move it is, in
  this order: needs you, ready to merge, checks running, waiting on
  review, drafts. A group with nothing in it is not shown. Each group can
  be folded, and keeps the order chosen (REV-11), the most recently updated first by default. A flat list of a
  dozen PRs hid the two that needed you among the drafts.
- **REV-9** Every PR in "To review" MUST say what it asks of you: its size
  (lines added and removed, files), its checks as GitHub rolls them up,
  whether it comes from a fork, and your own standing: not reviewed yet,
  your latest review (approved, changes asked for, commented, dismissed),
  or new commits since your review, when its head has moved past the
  commit your latest review was of. A pending review of yours is a draft,
  not a review given. The list is the 100 most recently updated, and says
  so when there are more. Before this, every card was a title and an
  author: a one-line fix looked like a rewrite, and a PR you had approved
  before its author pushed again looked like one you had already done.
- **REV-10** The card action for a PR in "To review" MUST open GitHub,
  where reviews are submitted. Reviewing it here (a checkout of its own,
  Claude's reading plan and findings beside its diff, and the review
  posted from the app) shipped in 0.5.0 and was taken out: the page was
  the Diff tab for your own work with a review laid over it, and it did
  not read as a review. A review checkout left from 0.5.0 lists as an
  ordinary task, named
  `Review: <title>`, to be deleted. It is still never pushed nor opened as
  a pull request, and its ticket never moves.
- **REV-11** Each tab MUST group by its own question (who it was asked of
  for "To review", whose move it is for "Opened by you": the default), by
  epic, or by repository, and sort newest first (the default), oldest
  first, or, in "To review", smallest first (lines added and removed).
  Each tab remembers its own choice across launches. By epic, a PR's
  ticket is its task's, else the first Jira key in its branch, else in its
  title, and its epic is found as for tasks (TASK-17); one with none is
  under "No epic", last. Grouped another way, "To review" is one list of
  yours and your team's, each card saying who it was asked of; a failed
  team lookup still shows as a note above it (REV-1). Epic is not offered
  without Jira.
- **REV-12** Cards in **To review** and **Opened by you** MUST offer
  read-only **Comments** (PR-16) and **Check runs** (the names, states and
  available links in PR-17). This includes personal and team requests,
  fork PRs, and PRs with no Layer task or registered repository. Open a
  section to read that PR in place, without navigating, starting a task,
  checking out a branch, writing to GitHub or sending to an agent.
  The two are tabs under the card: one open at a time, read below them,
  and choosing the open tab again closes it, so neither tab moves.
  **Refresh** reloads open details; grouping and sorting do not load them.
  Each section reports loading, empty, failed and incomplete reads
  independently (PR-18). Card clicks, **GitHub**, **Start task**, grouping
  and sorting still work as before (REV-7, REV-10, REV-11).
- **REV-13** The card's overall CI state MUST be readable in text and
  remain GitHub's rollup of check runs and commit statuses, independently
  of the on-demand **Check runs** list. That list describes runs for the
  PR's current head, not individual legacy commit statuses. An empty,
  unavailable, incomplete or passing list does not make the overall PR
  pass. A failing rollup stays visible even if no check runs are reported.
- **REV-14** Reviews MUST show requests, team queues and authored PRs from
  every connected GitHub source. An error or result limit on one source
  names that source and leaves results from others visible. The same
  `owner/repo#123` on two hosts is two PRs in the list, repository groups,
  notification targets and actions.

Code: `commands/github.rs` (`review_queue`, `github_review_queue`),
`integrations/github/authored.rs`, `integrations/github/requested.rs`,
`commands/pr_task.rs`, `ReviewsView.tsx`, `ReviewCards.tsx`, `lib/grouping.ts` (REV-11).

## 11. Notifications

| What | When | Where |
|---|---|---|
| Dock count | agents that need you (STATE table) | `attention.rs`, every 5s |
| Banner: agent | an agent starts needing you, or exits on its own, while the window is not focused | `attention.rs` |
| Banner: review, ticket | a new review request or assigned ticket, while the window is not focused | `news.rs`, every 3 min while away |
| Toast | news from out of view (NOTE-7): PR approved, changes requested, new comments, merged; a ticket the PR sweep moved; a pane that exited, asks to be trusted or hit a usage limit; a notice from the app | `Watchers.tsx`, `store.ts` |
| Status, in the top bar | a failure in the background, or of a tool with no place of its own; polls that keep failing (WIN-2) | `FrameStatus.tsx` |
| Message center | all of the above but Slack and the dock, kept | `messages.rs`, `MessageCenter.tsx` |
| Slack | an agent finished, PRs opened, an agent's own post, posted by the Slack bot to its channel (BOT-15) | `commands/slack.rs` |
| Slack thread | a chat the Slack bot opened replied, or is asking (BOT-6, BOT-7) | `slack_bot/` |

- **NOTE-1** Banners MUST be sent by the backend. A hidden webview's timers
  are the ones macOS throttles, so banners from the UI failed exactly when
  they were needed.
- **NOTE-2** The first look MUST be a snapshot, not news. Opening the app
  does not banner everything already waiting. A failed lookup forgets
  nothing, so the next success does not announce it all again.
- **NOTE-3** More than three at once MUST become one banner. The same pane
  is not announced again within two minutes.
- **NOTE-4** Clicking a banner or a toast MUST open exactly what it is
  about, by one route (`lib/target.ts`): an agent's opens its task with that
  pane selected (a chat's, Chat with that chat); pull request news opens the
  task's Pull requests tab; a ticket opens Tickets on that key; a review
  request opens Reviews with that pull request highlighted. What has gone
  since falls back: a pane (every restart gives panes new ids) to its task,
  a task to the Work overview. A toast about nothing in particular only
  closes. A banner also brings the window forward, minimized or not.
- **NOTE-5** Slack posts MUST pass the backend's switches: "Send anything"
  first, then one per kind. Nothing routes around them.
- **NOTE-6** A failure MUST NOT be a toast. A failure of something you just
  did shows where you did it: in the dialog above its buttons, under the row
  of actions or the button that failed, in the danger tint
  (`ErrorMessage`), until you try again or close it. A failure in the
  background, or of a tool that has no place of its own (an icon in the top
  bar, a menu item), shows in the top bar's status (WIN-2). Both say what
  happened in a sentence (`studio/design/behaviour.md`).
- **NOTE-7** A success MUST NOT be a toast when it shows where it was
  done: a saved setting, a connection's row, a renamed, added or removed
  row, a dialog that closes on its work, a draft that fills its field. A
  copy button reads "Copied" for a moment instead (`CopyButton`). A toast
  is news from out of view (`studio/design/behaviour.md`, "When something
  worked"): the table above, and the few a click starts that end out of
  view: a workspace's window opening beside this one, running agents told
  of a new worktree in their terminals, a branch Finish kept as the
  dialog closes, and a copy from a menu, which closes as it is chosen.

### Message center

The bell left of Settings. Toasts go and banners pile up in Notification
Center; this keeps what the app told you, newest first.

- **MSG-1** The message center MUST keep every piece of news: an agent
  asking, finished or exited on its own (with a trust prompt or a usage
  limit; while the Henchman runs, those it hears of are its to tell,
  HEN-13), what the Henchman needs you for, a new review request, a new ticket, a ticket the work moved, a
  pull request approved, sent back, commented on or merged, a notice from
  the app, and every failure shown in the top bar's status (WIN-2). Feedback
  on something you just did ("Copied …", a failure shown where you did it)
  MUST NOT be kept.
- **MSG-2** News MUST be recorded whether or not the window is focused and
  whether or not its banner is switched on. The switches decide banners and
  the dock count, nothing else. The first look is still a snapshot (NOTE-2).
- **MSG-3** Each piece of news MUST be recorded once, by one source: agents
  by `attention.rs`, reviews and tickets by `news.rs`, notices by
  `push_notice`; pull request news and ticket moves by the UI's toast,
  and failures by the top bar's status (`add_message`), which are the only
  places they are seen. A toast or status of something the backend recorded
  does not record it again. The same unread
  message again within ten minutes MUST count on the row already there
  (×N) rather than add another.
- **MSG-4** The log MUST survive a restart: `messages.json` beside
  `config.json`, the newest 200, written whole through a temp file and a
  rename, never on the main thread or the async runtime. An unreadable file
  is kept as `messages.json.unreadable`, and one entry this build cannot
  read is dropped on its own.
- **MSG-5** The bell MUST show how many are unread, and list them newest
  first with when and how often each was said. A click on one opens it by
  NOTE-4's route and marks every unread message about the same thing read.
  One about nothing in particular is only marked read. "Mark all read" and
  "Clear" act on the whole log; Clear asks first.

Known gaps:
- "An agent finished" is posted to Slack by the UI on `pty:exit`, so it
  depends on the webview being awake.

## 12. Tools for agents (MCP)

The app runs an MCP server on `127.0.0.1` (a random port, a new bearer
token each launch) and gives it to every agent it starts. Agents use the
app's own Jira, GitHub, Slack and Grafana connections through it, and
never hold credentials themselves.

| Tool | Does | Needs `confirm` |
|---|---|---|
| `list_tasks` | tasks in flight, with worktrees and dirty counts | |
| `list_repos` | registered repositories | |
| `task_diff` | changed files across a task, against the branch point | |
| `jira_search` | JQL search, 1–500 results | |
| `jira_get_issue` | one issue in full | |
| `jira_issue_types` | types with hierarchy levels | |
| `jira_create_fields` | what a project requires for a type | |
| `jira_create_issue` | file an issue, required `fields` included | |
| `jira_comment` | comment on an issue | |
| `jira_transition` | list transitions, or move an issue | to move |
| `github_search_issues` | GitHub's issue search, in the workspace's repositories, 1–500 results | |
| `github_get_issue` | one issue in full | |
| `github_create_issue` | file an issue in a workspace repository, or a sub-issue under `parent` | |
| `github_comment` | comment on an issue | |
| `github_close_issue` | close an issue, as completed or not planned | yes |
| `slack_post` | post to the configured channel | |
| `slack_cleanup` | delete this app's own posts (`dry_run` to list) | to delete |
| `slack_delete` | delete specific posts by permalink | yes |
| `slack_diagnose` | the token's scopes and channel | |
| `start_work` | a task from a ticket, optionally with an agent | |
| `handoff_prompt` | a pane's handoff briefing (needs "Let agents read terminal output") | |
| `list_panes` | panes and their states | |
| `pane_output` | a pane's recent output (needs "Let agents read terminal output") | |
| `task_prs` | a task's PRs, checks and reviews | |
| `add_repo` | add a repo to a task, telling its agents | |
| `set_base` | change the branch a task's repo is measured against and opens its next PR into (TASK-6) | |
| `forget_repo` | unregister a repo (never deletes files) | yes |
| `create_task` | a task with no ticket | |
| `task_rename` | set or clear a local Work task or caller Chat display name (MCP-6) | |
| `task_set_tag` | add or remove one local text tag (MCP-6) | |
| `task_set_color` | set or clear a local item color (MCP-6) | |
| `open_prs` | push and open PRs for a task | yes |
| `finish_task` | finish a landed task: agents, worktrees, covered branches, ticket to done (TASK-18) | yes |
| `pr_images` | offer image files for a repo's open PR, shown there in a section the app owns once the user adds them (PR-13) | |
| `repo_notes` | a repository's notes, with what changed since each was checked (MEM-5) | |
| `remember` | keep a lasting fact about a repository, after asking the user (MEM-1) | yes |
| `check_note` | say a note still holds, as of now | |
| `forget_note` | remove a note that no longer holds, with a reason for the message center | yes |
| `spec_read` | list the task's spec files with steps ticked and total, or read one file (SPEC-13) | |
| `spec_write` | write one file of the task's spec, whole text, in the task's `spec/` folder, if its toolset lists the file (SPEC-13) | |
| `spec_task` | tick (or untick) a step of the spec by file and position (SPEC-13) | |
| `spec_commit` | Commit spec: put the spec into the task's repositories and commit it there (SPEC-35, SPEC-13) | yes |
| `spec_propose` | gone (SPEC-24): listed for one release so agents that still have it in context are told where the spec's folder is and to write with `spec_write` | |
| `epic_subtasks` | an epic's sub-tasks: ticket, task, steps ticked, what each waits for, its agents (EPIC-12); the tickets under it that are none yet (EPIC-14) | |
| `subtask_start` | start a sub-task's agent on its spec, and move its ticket (EPIC-8) | yes |
| `subtask_tell` | send one message to a sub-task's running agents (EPIC-12) | |
| `work_overview` | the Henchman's: every task sorted by what it needs, the open chats, the running processes (HEN-4) | |
| `pane_read` | the Henchman's: what an agent or chat last said (HEN-4) | |
| `pane_tell` | the Henchman's: one message to a running agent or chat (HEN-4) | |
| `chat_start` | the Henchman's: a new chat in Chat, with its first message (HEN-4) | |
| `pane_restart` | the Henchman's: restart an agent or chat that exited by itself (HEN-10) | |
| `task_work` | the Henchman's: put a started task's agent to work inside the task: tell, pick up or start one (HEN-11) | |
| `order_add` | the Henchman's: keep a standing order, on the user's word (HEN-7) | |
| `order_remove` | the Henchman's: drop a standing order (HEN-7) | |
| `leave_alone` | the Henchman's: hear nothing of a task or chat until handed back (HEN-8) | |
| `need_you` | the Henchman's: reach the user, a banner and a count on the dock, for what really needs them (HEN-13) | |
| `list_routines` | a chat's: the routines on this Mac, when each runs next, and its newest runs (ROUT-11) | |
| `create_routine` | a chat's: make a routine, its prompt, schedule and agent (ROUT-11) | |
| `update_routine` | a chat's: change a routine; a field left out stays (ROUT-11) | |
| `run_routine` | a chat's: run a routine now, as a new chat (ROUT-11) | |
| `delete_routine` | a chat's: delete a routine and its runs (ROUT-11) | yes |
| `browser_navigate` | open a page in the task's browser tab (BRW-3), and get its outline | |
| `browser_back` | back one page | |
| `browser_snapshot` | the page as an outline, each element it can act on numbered | |
| `browser_click` | click an element, as real mouse input | |
| `browser_hover` | move the mouse over an element | |
| `browser_type` | type into a field | |
| `browser_press_key` | press a key, with modifiers | |
| `browser_select` | choose options in a list | |
| `browser_scroll` | scroll the page, or the part under an element | |
| `browser_wait` | wait for text to show, or for some seconds | |
| `browser_screenshot` | an image of the tab; with `save`, also as a file for `pr_images` | |
| `browser_console` | the page's console messages and uncaught errors | |
| `browser_evaluate` | run JavaScript in the page | |
| `browser_request_site` | ask the user to let agents use a site (BRW-11) | |
| `browser_dialog` | answer the page's alert, confirm or prompt (BRW-13) | |
| `browser_tabs` | the task's tabs, and which is active (BRW-16) | |
| `browser_new_tab` | open a page in a new tab (BRW-3, BRW-16) | |
| `browser_switch_tab` | make another tab the active one | |
| `browser_close_tab` | close a tab, the active one by default | |
| `browser_sign_in` | fill a saved sign-in into the page, without seeing the password (BRW-19) | |
| `browser_copy_session` | copy a sign-in from another tab on this machine into the active one (BRW-21) | |
| `grafana_datasources` | the connected Grafana's Loki datasources, and which one the user chose (OBS-5) | |
| `grafana_logs` | log lines for a LogQL query or a Grafana Explore link, newest first, paged (OBS-3, OBS-4) | |
| `grafana_labels` | a Loki datasource's label names, or one label's values | |
| `s3_logs_sources` | the S3 log sources the user set up, with their Athena tables' columns (OBS-9) | |
| `s3_logs_list` | the folders and files under a path in a source, paged (OBS-11) | |
| `s3_logs_read` | a file's text, unpacked, cut, or only its lines containing some text (OBS-11) | |
| `s3_logs_query` | read-only SQL over a source's Athena table, narrowed to its partitions: the rows, the data scanned and what it cost (OBS-12, OBS-14) | |

- **MCP-1** Every request MUST carry the bearer token. Loopback is not
  authorisation.
- **MCP-2** The tools' token MUST NOT appear on any command line. It goes
  in a 0600 file, an environment variable, or an ACP agent's stdin (ACP-3). Status reports (`/hook`) use
  a separate token that can do nothing but set a pane's state.
- **MCP-3** A tool that changes shared state irreversibly MUST need
  `confirm: true`, and says so in its schema.
- **MCP-4** Reading other panes' output MUST be off by default.
- **MCP-5** A new tool MUST be added to the table above (**guard**). If
  agents should reach for it without being told, it goes in the tool list
  the chat room's context names too (`write_chat_context`).
- **MCP-6** An authenticated agent MUST be able to call
  `task_rename({ task_id?, name })`, `task_set_tag({ task_id?, tag, assigned })`
  and `task_set_color({ task_id?, color })` in the current profile.
  `name: null` resets the override; `color: null` clears the color.
  `assigned` is a required boolean; removing an unassigned tag does nothing.
  An explicit `task_id` targets only an existing Work task. Without it,
  Layer resolves the caller's eligible Work task or standalone Chat room.
  An unknown task, absent caller or ineligible caller is an error.
  The pane header routes the request and is not a credential (MCP-1).
  Each operation uses the editor's validation, preserves unspecified
  fields, returns canonical details and updates the desktop without a
  restart. A new assigned tag joins the profile catalog in the same
  successful write. These tools change no Git, Jira, GitHub or Slack
  state and need no `confirm`.

Code: `mcp.rs`, `agents.rs` (per-CLI wiring), `commands/panes.rs`
(`plug_in`).

Known gaps:
- A task folder's `.mcp.json` is rewritten only when an agent starts there.
  A CLI run by hand in a task folder where no agent started this launch
  gets a stale token.

## 13. Settings

| Setting | Default | What it does |
|---|---|---|
| Appearance: Theme | Nightshade Night | Nightshade Night or Day, another theme, or Match the Mac (SET-5) |
| Appearance: Interface scale | 100% | 80–160% |
| Appearance: Terminal text | 13px | 9–24px |
| Appearance: Conversation text | 14px | 9–24px, an agent over ACP (ACP-15) |
| Notifications: New reviews and tickets | on | banners while the window is away (NOTE-*); the message center keeps them either way (MSG-2) |
| Notifications: Agents waiting on you | on | banners and the dock count; the message center keeps them either way (MSG-2) |
| Notifications: Slack | — | once the bot is connected: the channel, four switches (BOT-15, NOTE-5) |
| Agents: Put terminals back when the app reopens | on | PANE-7 |
| Agents: Trust the folders this app creates | on | PANE-10 |
| Agents: Start Claude Code in auto mode | on | PANE-18 |
| Agents: Let agents read terminal output | **off** | MCP `pane_output`, `handoff_prompt` |
| Agents: Most terminals running at once | 32 | 32, 48 or 64 (PANE-2) |
| Agents: Henchman | on | HEN-16 |
| Agents: Henchman's agent, model and effort | Claude Code, Sonnet, Medium | HEN-15 |
| Agents: Henchman's custom instructions | none | HEN-17 |
| Agents: Task folder location | `~/.villain-worktrees` | where task folders go |
| Tags: reusable text tags | none | one profile-local catalog shared by Chat and Work; add, rename, delete (SET-15); `add_tag`, `rename_tag`, `delete_tag` |
| Keep awake (the cup in the top bar) | off | STATE-7, PHONE-8 |
| Code: Always write a spec first | off | every agent is told to write the spec before any code, then wait for your go (SPEC-38), and Start work's "Write a spec first" is on and locked; `set_spec_first` |
| Code: Default toolset | Kiro | the spec toolset a new task gets unless Start work's pick, or a sub-task's epic, says otherwise (SPEC-28, SPEC-29); `set_default_toolset`, which refuses an id no toolset has |
| Code: Custom toolsets | none | the user's own toolsets as the JSON of their manifests, on this Mac only, never shared; one whose `run` steps execute as written (SPEC-27; `set_custom_toolsets`, refused whole when one is not a manifest) |
| Repos: a repository's spec toolset, Commit spec mode and spec folder | no toolset (the app's), `whole`, `specs` | which toolset Start work starts from for it; what Commit spec puts in it (`whole`, `own`, `none`); and, for Kiro, the folder (SPEC-28, SPEC-35); `set_project_specs`, shared with the team |
| Browser: sites agents may use | none: this machine's pages only | BRW-3, BRW-11 |
| Browser: saved sign-ins | none | BRW-18 |
| Remote access: Tailscale | off | PHONE-1, PHONE-2 |
| Remote access: Home network and router VPN | off | PHONE-1, PHONE-2 |
| Remote access: Let phones type into agents | off | PHONE-7 |
| Tickets: issue tracker | none | Jira or GitHub Issues, one per workspace (TKT-15); Forge, and GitHub Issues' epic type (TKT-17), are not built |
| Tickets: Jira | — | site URL, email, API token, optional project key and JQL |
| Tickets follow the work (Jira) | work starts: first in-progress status; review, merged: not chosen | TKT-1, TKT-8, per Jira project |
| Code: GitHub | none | One source per web host. Each has an API URL (`…/api/v3` for Enterprise), a web URL, an optional review team and this Mac's own token (`repo`; `read:org` for a bare team slug; a fine-grained token also needs Issues, read and write, on the GitHub Issues source) |
| Messaging: Slack | — | the bot: bot token (`xoxb-`), app-level token (`xapp-`, `connections:write`), the owner's member ID, the agent and its model (BOT-17), on/off, work without asking, personality (BOT-*) |
| Logs: Grafana | — | site URL, service account token (reads and queries datasources), OBS-1; the Loki datasource for your services' logs, none chosen, OBS-5 |
| Logs: S3 log sources | none | each a name, AWS profile, region, bucket, prefix and optional Athena table; no AWS keys, OBS-8, OBS-9 |

- **SET-1** A token field left blank MUST reuse the saved token only for
  the same site (scheme, host, port). A different site needs the token
  typed again.
- **SET-2** Disconnecting MUST delete the token from the keychain and the
  integration's settings.
- **SET-3** A Jira site, GitHub API URL or Grafana site MUST be `https://`, so a token
  never travels unencrypted. Plain `http://` is refused, when connecting
  and for one saved before this rule, except to this machine
  (`localhost`, `127.0.0.1`, `::1`). Slack's addresses are fixed and
  always https.
- **SET-4** A Jira token the site no longer accepts (expired, revoked)
  MUST fail every Jira call with an error that says to enter a new token.
  Jira serves such a request as an anonymous visitor's, so without this
  an empty search, or a 404 for a ticket that exists, looks like an answer.

- **SET-6** Settings' tabs MUST be named for what each does, not the
  product behind it, this Mac's first: Appearance (how it looks),
  Notifications (banners here, posts to Slack), Agents (the agents found,
  their terminals, the task folder), Tags (the local tag catalog),
  Browser, Remote access (the phone).
  Then what it connects to: Tickets (Jira or GitHub Issues),
  Code (GitHub), Messaging (the Slack bot), Logs (Grafana and S3). A page
  connecting a service says which one at its top. A product's name said
  nothing to someone who had not set it up, and would be wrong the day a
  second one is supported. A setting MUST sit on the tab of what it
  changes: banners were on Appearance and posts to Slack on Messaging, and
  the agents had two tabs, General and ACP, naming the same four.
- **SET-5** The theme MUST reach everything the app draws: every view,
  the terminals, the window behind the page before it has drawn, and a
  paired phone's page. The themes are Nightshade Night, Nightshade Day,
  Tokyo Night, Cursor Dark, Cursor Light, Ayu Dark and Ayu Light, each
  picked from a small picture of the app drawn in it, beside Match the Mac.
  Match the Mac MUST switch between Nightshade's night and day as the Mac's
  appearance changes, without a restart; a phone under Match the Mac
  follows its own appearance. The app MUST open in the theme it was last in, not
  flash another first. Nightshade Night is the default, as in every Code
  Villain app; a theme saved as Dark or Light opens as Nightshade's night
  or day. Every theme fills the studio's colour names (`studio/brand/color.md`):
  Nightshade's values come from the vault's `studio/design/tokens`, bundled by
  `bun run design:sync`, and `bun run check` fails while the bundled copy
  differs; the others are in `theme.css`. A colour is one of those names,
  or a color-mix of them, never written into a rule: one written there is
  a colour no theme reaches. What an agent prints in a terminal keeps its own colours;
  on a light theme, xterm darkens any too faint to read (4.5:1), and answers an
  agent that asks the terminal for its background.

- **SET-7** A connection's page MUST show its settings in two blocks.
  The workspace's, named for it ("Code Villain's"), are the same for
  everyone in it: Jira's site, project key, query and ticket flow; GitHub's
  API and web URLs and review team; Slack's notification channel;
  Grafana's site and Loki datasource; each S3 log source but its AWS
  profile. Yours stay on this Mac: the Jira account email, every token,
  Slack's bot and the four switches, a log source's AWS profile. Admins
  change the workspace's block; a member sees it as text, marked "admins
  change these", and adds or removes no log source. In a workspace on this
  Mac you are its admin; on a server the server says, and the app keeps
  its last answer for when the server can't be reached.
- **SET-8** A connection the workspace has and this Mac hasn't MUST show
  the workspace's settings already, so connecting asks only for yours:
  the email and the token.
- **SET-9** Disconnecting MUST remove only yours (SET-2): the workspace
  keeps its connection for everyone else, and its settings stay on the
  page for connecting again.
- **SET-10** A member connecting a service the workspace has no
  connection for MUST keep it on this Mac, and send nothing: the page
  says so above the settings.
- **SET-11** Settings MUST end with the app's name and version ("Layer
  0.1.0"), faint and small, under whichever page is open
  (`studio/design/behaviour.md`, "Settings"). Its title says where a crash
  is written. A build made on this Mac (`bun run release:mac`) MUST say
  the commit it was built from in place of the version ("Layer c3e5bbe"),
  with `-dirty` when the tree had uncommitted changes, and its title which
  release it follows; only a release (`bun run release`) says a number.
- **SET-12** A connection's form (Jira, GitHub, the Slack bot, Grafana)
  MUST keep one button, since its address and token only work together
  and the app has to try them. It is named by what it does: "Connect"
  until it is connected, "Reconnect" after, and "Connecting…" while it
  tries (`studio/design/behaviour.md`, "Settings"). Connecting raises no
  toast: the connected row says it (NOTE-7). Opened on a connected page
  (SET-14), Cancel beside it puts the form back as stored, untried.
- **SET-13** Layer's server (§25), not the app, MUST read its settings
  from `layer.toml` (`--config <file>`, else `LAYER_CONFIG`, else
  `./layer.toml`), with the environment over it: `[oidc] issuer` is
  `LAYER_OIDC_ISSUER`. `company`, `public_url`, the database (`[database]
  url` or `DATABASE_URL`), the company's sign-in (`[oidc] issuer`,
  `client_id`, `client_secret`) and its SCIM token (`[scim] token`) are
  required; `[groups]`, `addr`, `web_dir` and `cookie_secure` are not. A
  setting missing, or one the server doesn't know, stops it, naming the
  setting. `layer.example.toml` has them all. The app's own server on
  this Mac (PROF-11) reads none of them.
- **SET-14** A connected page MUST fold its form: its settings as text
  (the workspace's block and yours, a token as "in your keychain"), then
  Change… with a line on what it holds. Change… opens the form as it
  was; connecting folds it again. What is used day to day stays out of
  the fold: how tickets follow the work, the Slack bot's switches, agent,
  model and personality, Grafana's Loki datasource. Messaging's button
  is "Change tokens…", over the Slack bot's tokens, your member ID and
  the steps that made it.
- **SET-15** Settings → Tags MUST list the current desktop profile's
  canonical text tags and let the user add one through a labeled field
  and **Add tag** action, including tags not assigned to any item.
  Chat and Work editors offer this same catalog. A new tag assigned in
  an editor becomes reusable only when the edit saves successfully.
  Removing an assignment MUST NOT remove the catalog entry. Clicking a
  tag in Settings → Tags MUST let the user rename or delete it. A rename
  MUST change the tag on every chat and work task that has it; renaming
  onto another tag's name (in any case) merges the two under that tag's
  spelling. Delete asks first, then MUST take the tag off every chat and
  work task as well as the catalog. Add, rename and delete MUST work from
  the button and from Enter in the field. Tags have no color, icon or
  other metadata; item color is independent.
  Validation follows DETAIL-3, including canonical case-insensitive reuse.
  A failed add stays inline and a successful add needs no toast.
- **SET-16** Settings → Code MUST list every GitHub source on the profile
  by web host. Each has its own connect, reconnect and disconnect actions.
  Shared addresses and review teams follow SET-7 to SET-10. A local
  disconnect removes only that source's credential. A token is usable only
  by its source and API origin, including after a workspace settings change.
  A rejected reconnect does not replace a working credential. An upgrade
  keeps a profile's valid existing connection without asking for its token.

Code: `commands/settings.rs`, `config.rs` (`UiPrefs`), `Settings.tsx`,
`AppearanceSettings.tsx`, `NotificationSettings.tsx`, `AgentSettings.tsx`
(`useUiPrefs.tsx` saves the prefs for all three), `integrations/mod.rs` (`require_https`); the
server's settings (SET-13) in `crates/server/src/config.rs`; the
workspace's and yours (SET-7 to SET-10), and the fold (SET-14), in `SharedSettings.tsx` and
`config/shared.rs` (`held`, `WorkspaceAt.admin`); the
theme in `theme.css`, `lib/theme.ts`, `lib/terminal.ts` and `theme.rs`.

Known gaps:
- Changing Slack's channel needs the token pasted again.
- No one can remove the workspace's connection for everyone from the app.

## 14. What the app writes on your machine

| Where | What |
|---|---|
| `~/Library/Application Support/eu.codevillain.villain-layer/` | `config.json`: repos, tasks, settings, saved panes, the Slack threads handed to chats (BOT-4). `messages.json`: the message center (MSG-4). `notes.json`: this Mac's copy of the repo notes (MEM-7). `runs.json`: the run log (RUN-5). `limits.json`: each agent plan's limits as last read (LIMIT-4). `postgres/`: the workspace's Postgres, its data, password and binaries (PROF-11). At agent launch also `.mcp.json` (0600), `claude-hooks.json`, `copilot-plugin/`, `opencode-plugin.js` |
| Keychain, service `eu.codevillain.villain-layer` | one item holding every token, paired phones' included (PHONE-3), the workspace's session on its server (`layer-session`, PROF-16), and the passwords of the browser's saved sign-ins (BRW-18) |
| `~/.villain-worktrees/` (settable) | task folders, `_chat/` rooms, and `.repos/`: the app's own copy of each repo (REPO-4) |
| a task folder | the worktrees, `AGENTS.md` and `CLAUDE.md` (task context), `TICKET.md` (PANE-13), `spec/` (the task's spec, SPEC-2: copied to `<config folder>/specs/<task>/` before the task goes, SPEC-3), `.spec-install.json`, the record of what a spec toolset's install wrote (SPEC-30): `.claude/skills/`, `.agents/skills/`, files in `spec/` and whatever its `run` steps made, of which deleting the task removes just those, `.spec-checks/`: the last check of each repository (SPEC-15), `.spec-drafts/`, read only to move an earlier spec (SPEC-17), left in place and taken with the folder, and holding an epic's `sub-tasks.json` (§26), `.mcp.json`, and `.gemini/settings.json`, `PR_DESCRIPTION.md` and `PR_IMAGES.json` (PR-4), `PR_FEEDBACK.md`, and hand-offs too long to type (`CONFLICTS.md`, `REVIEW_COMMENTS.md`, `PR_DRAFT_REQUEST.md`, `FIRST_PROMPT.md`, `CHECKS.md` (LOOP-6), PANE-11) as they come up |
| `~/.claude.json` | trust entries for the app's own folders only (PANE-10) |
| `<config folder>/specs/`, `<config folder>/toolsets/` | `specs/<task>/`: the spec of a deleted task or of a task folder Clean up removed, and `specs/<repository>/<task>/`: the spec of a task with no folder of its own (SPEC-3). `toolsets/<id>/<version>/`: the copy a toolset's install fetched, kept for the next task (SPEC-30) |
| `<config folder>/browser/` | the browser's own Chrome profile (BRW-1): cookies, sign-ins and storage of the pages opened there |
| `~/Library/Caches/eu.codevillain.villain-layer/chrome/` | the browser's own Chrome, a folder per version (BRW-24), shared by every profile |
| `~/Library/Logs/villain-layer/panic.log` | a crash's location and backtrace |

The dev build uses `eu.codevillain.villain-layer.dev` for its config folder
and keychain item instead.

All of the above is the Default profile's. Every other profile (§25) has
the same files in `profiles/<id>/` under the config folder, a keychain item
under `eu.codevillain.villain-layer.profile.<id>`, and its task folders in
`~/.villain-worktrees-<id>`, each settable. Beside them, the config folder
holds `profiles.json`, the profiles' names and when each last opened and
quit, and, while a sign-in is passed from one copy to another,
`signed-in.json` (PROF-17). A profile's folder holds `running.lock`, held
by the copy that has it open.

- **DISK-1** Nothing generated MUST ever be written inside a worktree,
  where it would end up in a commit. The one exception is a spec that
  Commit spec (SPEC-35) puts there, which is meant to be committed.
- **DISK-2** Deleting a task MUST remove what the app generated in its
  folder, and what others leave there that goes with it (Claude Code's
  `.claude/settings.local.json`, Finder's `.DS_Store`), and then the
  folder, if it is empty. Clean up (REPO-8) removes the same from a folder
  no task uses.
- **DISK-3** `config.json` MUST be written whole, via a temp file and
  rename. A file from a newer build is copied aside before an older build
  touches it. An unreadable one is kept as `config.json.unreadable`.
- **DISK-4** The first launch under an identifier, while its config folder
  does not exist yet, MUST start from the config and tokens saved under the
  identifier the app had before (`dev.villain.layer`, and
  `dev.villain.layer.dev` for the dev build). They are copied, never moved.
  A config folder that exists adopts nothing, even without its
  `config.json`, so starting over stays started over and a disconnected
  token stays gone.

Code: `previous.rs`, `secrets.rs` (`adopt`).

---

## 15. The window

The window has no native title bar: the app's top bar takes its place,
under the traffic lights, across the whole width. It holds the views as
tabs and the tools for the whole app (phone, keep awake, messages,
settings), with no wordmark beside the traffic lights. The lights sit
centred on the bar, at the same spot as in Forge. Below it, Work's
tasks run down the left in the frame's tone, and every view sits in an
inset panel (Layer's exception to `studio/design/layouts/app-window.md`). A
view's count shows only when it asks for something: agents that need you
on Work, the reviews on Reviews (waiting on you, then yours), repositories
to fix on Repos. A running agent, or a chat at work, is a live dot.

- **WIN-1** The window MUST move when you drag any empty part of the top
  bar, and zoom when you double-click it, as a native title bar does. Its
  tabs and buttons stay clickable. This is Tauri's `data-tauri-drag-region`
  with the `core:window:allow-start-dragging` permission; the CSS
  `-webkit-app-region` does nothing in macOS's WebKit, and a window built on
  it could not be moved at all.

- **WIN-2** The top bar's status MUST sit left of the app's tools and say
  what failed in the background, as a dot and a sentence in `muted`: a
  failure for six seconds (`danger`), then only in the message center; task
  and pane polls failing twice in a row, until one works again (`warn`,
  "Can't refresh tasks. Retrying…"). A newer failure replaces the one
  shown. A long one is cut short with "…" and kept whole in its title and
  the message center. While all is well it shows nothing.

- **WIN-3** A confirmation MUST open with Cancel focused, so Enter never
  confirms a delete by accident (`studio/design/behaviour.md`, "Confirming").

- **WIN-4** Starting up MUST show Layer's mark, breathing, and nothing
  else, until the first read of repositories, tasks, panes and settings has
  come back or failed; then the window. A view never shows a spinner or a
  skeleton while it waits: it shows its content once it has it
  (`studio/design/behaviour.md`, "Empty and loading").

- **WIN-5** Everything with a right-click menu MUST open it from the
  keyboard too, as macOS does: ⇧F10 or the context-menu key on the
  focused row opens its menu at the row's foot. That is the sidebar's
  task, epic and sub-task rows, ticket cards and epic heads in Tickets,
  the cards in Reviews, and the chats in the Chat view. In any menu, ↑/↓ move through the items,
  Enter picks one and Esc closes it. Opened from the keyboard, the first
  item is chosen; opened with the mouse, nothing is until an arrow is
  pressed. Closed with keys, the menu gives the focus back to what opened
  it (`studio/design/behaviour.md`, "Keys").

- **WIN-6** A dialog MUST take the focus when it opens: its first field,
  or else its first button (a confirmation's Cancel, WIN-3). While it is
  open, Tab and ⇧Tab go round its controls, buttons included, and never
  reach the window behind it. Closed with keys, it gives the focus back to
  what opened it, so they go on from there; closed with a click, only to a
  field or terminal you were typing in, so no ring is left on a row
  (`studio/design/behaviour.md`, "Keys").

- **WIN-7** The window MUST keep drawing when WebKit gives up on its GPU
  helper (it stops answering, often after the Mac sleeps, and macOS won't
  let WebKit stop it). Within seconds the app stops the old helper itself,
  the window draws again and its terminals come back, with no restart and
  no agent stopped. Before, the window stayed black until the app quit.

Code: `App.tsx` (`TopBar`), `FrameStatus.tsx`, `ui.tsx` (`Modal`, `Confirm`), `menu.tsx` (`ContextMenu`), `lib/modalFocus.ts`, `lib/press.ts` (`menuKeys`), `Splash.tsx`, `src-tauri/capabilities/default.json`, `code/kit/webview` (WIN-7, started in `lib.rs`).

## 16. Repo notes

What agents learned about a repository that holds beyond one task, kept by
the app and given to the next task on that repository. Notes belong to the
workspace (MEM-7), so everyone in it reads the same ones; a repository with
no `origin` keeps its notes on this Mac only (PROF-12). They never go into
the repository, whose own `AGENTS.md`/`CLAUDE.md` is where what holds for
everyone who clones it belongs. Each repo in the Repos view has a Notes
list, to read, edit, check and delete them.

- **MEM-1** A note MUST be about the repository, not a task: how to build
  or test it, a trap in its setup, where something lives, a convention its
  code keeps. An agent MUST ask the user whether it is worth remembering,
  quoting the note, before saving it, and `remember` needs `confirm: true`
  (MCP-3). A note is read into every later task on the repository as
  instructions, so one saved unasked, or planted by a ticket's text, would
  steer every agent after it.
- **MEM-2** A note MUST record when it was written, from which task and
  agent, and the commit the default branch of the app's copy was at. It
  MAY name the paths it is about.
- **MEM-3** A note MUST show how likely it is to be out of date: when it
  was last checked (being written counts), and, if it names paths, whether
  any of them changed on the default branch since the commit it was last
  checked at. Worked out when notes are read, never on a timer.
- **MEM-4** A task's context file MUST give the notes of each repository
  in it, most recently checked first, each with its age and whether its
  paths changed, up to 3 KB per repository, saying how many were left out.
  It tells agents that notes can be out of date: check one against the code
  before relying on it, then mark it checked, or ask the user to remove it.
- **MEM-5** Agents MUST be able to list a repository's notes
  (`repo_notes`), mark one checked at the current commit (`check_note`),
  and remove one with a reason (`forget_note`, `confirm: true`, after
  asking the user). A note is corrected by forgetting it and remembering
  the new one.
- **MEM-6** The Repos view MUST show each repo's notes: the text, when it
  was written and last checked, where it came from, and whether its paths
  changed since. Each can be edited (which counts as checking it), marked
  checked, or deleted, and a note can be added by hand.
- **MEM-7** Notes MUST be kept in the workspace (PROF-11), and this Mac's
  copy of them in `notes.json` beside `config.json`, never in `config.json`
  itself. At most 50 per repository and 500 characters per note;
  `remember` past either says so rather than dropping the oldest.
  A note belongs to the repository, not to its registration: notes are
  kept by where the repository fetches from (its `origin`, `git@host:a/b`
  and `https://host/a/b.git` alike; the clone's path when it has none), so
  removing a repo (REPO-3) keeps them, and adding it again, or a second
  clone of it, finds them. A repo's id is new each time it is added.

Code: `notes.rs`, `commands/task_context.rs`, `mcp.rs`, `ReposView.tsx`.

## 17. Phone

The tasks and their agents, from a phone, while everything keeps running
on the Mac. The app serves a page of its own to a phone that has paired
with it: every task and its agents, the ones that need you first, and an
agent's terminal, live, with a line of text and a few keys to answer it.
Settings → Remote access switches on the ways in: **Tailscale** (from anywhere,
over a tailnet) and **Home network and router VPN** (the same Wi-Fi, or a
router's own VPN such as WireGuard on a GL.iNet, which puts the phone on
the home network). How to set either up is in `docs/phone.md`.

- **PHONE-1** The app MUST answer on the network only while a way in is
  on, both off by default. It is a server of its own on a fixed port (7420,
  the dev build 7421, so the address saved on a phone keeps working); the
  MCP server stays on loopback. A port in use by something else is said,
  not left silent.
- **PHONE-2** A caller MUST come from a way that is on: Tailscale's
  addresses (100.64.0.0/10) for Tailscale, the private ranges (10/8,
  172.16/12, 192.168/16) for the home network. The internet, loopback and
  IPv6 are never let in. The server listens on every IPv4 interface and
  judges the caller's address, because the Mac's own addresses come and go
  with the network it is on and with Tailscale starting.
- **PHONE-3** A phone MUST pair before it sees anything but the pairing
  page: Settings shows a six-digit code that lasts two minutes, takes five
  tries, and pairs one phone. Each phone gets a token of its own (256 bits),
  kept in the keychain and sent as a bearer header, never in a URL.
  Forgetting a phone in Settings shuts it out at once and ends its open
  streams.
- **PHONE-4** The phone's page MUST come from the app's own build
  (`phone.html`), so it is always the version the app speaks. A dev build
  serves it from `dist/`, built by `bun run build`.
- **PHONE-5** The overview MUST show every task with its panes and their
  states, the ones that need you first by the dock count's rule (STATE-4),
  and change when they do, pushed, not polled. Panes are called as in the
  window (PANE-12). A task starts closed, showing a dot per pane and how
  many need you or are working; the phone remembers which ones were
  opened. The ticket key is shown once, not again in a name that starts
  with it.
- **PHONE-6** A phone MUST follow a pane's output from its own position
  and never change the window's feed (`watched`, `sent`). It runs the
  output through a terminal at the size the window gave it and never
  resizes it, which would scramble the Mac's view. It shows that terminal's
  rows as text wrapped to the phone, without box frames and alignment
  padding (`readable.ts`): an agent's 100-odd columns fitted to a phone
  were six pixels a letter. "Screen" shows the terminal itself. Opening a
  pane on the phone is looking at it: done becomes idle (STATE-3).
- **PHONE-7** A phone MUST type only with "Let phones type into agents"
  on, and only into a running agent, never a shell. It sends a line, made
  one line and stripped of control characters, at most 512 bytes (PANE-11),
  and Enter; or one key of the key bar: Enter, Esc, Up, Down, Tab,
  Shift-Tab, Ctrl-C, 1, 2, 3. Esc and Ctrl-C end a turn as at the Mac
  (STATE-2). An agent over ACP takes the keys as ACP-10 says.
- **PHONE-8** With Keep awake on and a way in open, the Mac MUST stay
  awake while any agent runs, not only while one works: one asking for
  permission is exactly the one you would answer from the phone. Closing
  the lid still sleeps the Mac.
- **PHONE-9** The top bar MUST show a phone once a phone is paired: green,
  naming it, while a paired phone has the app open; grey otherwise, saying
  whether phone access is off. A click opens Settings → Remote access.

Code: `phone/` (`net.rs` ways in, `pair.rs`, `routes.rs`, `view.rs`,
`input.rs`), `pty/feed.rs`, `commands/phone.rs`, `awake.rs`,
`PhoneSettings.tsx`, `phone.html` and `src/phone/`.

Known gaps:
- Plain HTTP. Tailscale and a router's WireGuard encrypt the way from the
  phone; on the home Wi-Fi itself, traffic between the phone and the Mac
  is readable by anything else on that network. No notifications reach the
  phone either: those need HTTPS. Slack's (§11) still do.
- The key bar sends the arrows' normal-mode codes. The agent CLIs read
  them; a program that only takes the application-mode ones would not.
- A phone cannot start, stop or hand off an agent.

## 18. Browser

Each task has tabs in a browser its agents can use: to open the dev
server, read the page, fill in a form and click through it, as a person
trying the change would. It is one Chrome for the app, run in the
background with a profile of its own, and a set of tabs in it per task,
one of them active, which the user and the task's agents share. Agents
drive it with the `browser_*` tools (§12): they read a page as an outline
of numbered elements, act on those, and get the page back as it is
afterwards.

The globe in a task's pane bar shows its browser beside the terminals (a
dot on it says the task's agents have used it). The panel has a tab strip,
an address bar, back, forward and reload, a camera for a screenshot
(BRW-22), and the active tab's page, live: the user
clicks, scrolls, types and pastes in it as in any browser, to sign in or
to help an agent that is stuck. Under it is what an agent last did, and a
ring marks where it clicked.

- **BRW-1** There MUST be one browser for the app, started when it is first
  needed, with one tab per task. It is the app's own Chrome (BRW-24), run
  headless with a profile of its own in the app's config folder: never the
  user's own Chrome profile, its sign-ins or its extensions, and never a
  Chrome installed on the Mac. macOS takes any Chrome run from Google
  Chrome.app for the user's own, and while the app's ran, opening Chrome
  from the Dock or Finder showed nothing. The profile's cookies are not
  locked with a key in the keychain (`--use-mock-keychain`): Chrome for
  Testing is signed for no one, so macOS took each new version for a new
  app and asked for "Chromium Safe Storage" again.
- **BRW-24** The app MUST get its own Chrome, with nothing for the user to
  install: Chrome for Testing, Google's build of Chrome for programs to
  drive, downloaded from Google the first time the browser is needed, into
  `~/Library/Caches/<identifier>/chrome/`, one copy for every profile. The
  panel says how much has come ("Downloading the browser: 84 of 200 MB"),
  and agents wait meanwhile, as while the browser starts. A connection that
  breaks off is asked for the rest, from where it stopped; a download that
  still fails, after five tries in a row that bring nothing, says why in the
  panel, which tries again a minute later. Each time the browser starts, and
  at most once a day, the app asks Google for the current stable version and
  downloads a newer one in the background; the browser runs it from its next
  start. A version no browser runs from any more is deleted. Only an address
  on Google's own storage
  (`https://storage.googleapis.com/chrome-for-testing-public/`) is
  downloaded.
- **BRW-2** An agent's tools MUST act on its own task's tab, found from the
  pane that calls (`X-Villain-Pane`). The chats have one browser between
  them, as a task's agents share the task's, shown beside them by the
  globe in the Chat view's list: a chat's agent reads a dashboard or a
  ticket's page as one working on a task tries the dev server. The header
  is not a credential (CHAT-3): an agent could name another task's pane and
  drive that task's tab, which is no more than it could do by asking.
- **BRW-3** Agents MUST use only pages on this machine (`localhost`,
  `*.localhost`, `127.0.0.1`, `[::1]`) and on the sites the user allowed
  (Settings → Browser, the panel's Allow, or a request answered, BRW-11),
  over `http` or `https`. A site covers its subdomains, and is kept as its
  host whatever was typed. A `*` in a site stands for any run of letters,
  digits and dashes within one name (`pr-*.example.com` is
  `pr-12.example.com`, and its subdomains), and only in the names in front
  of the domain: `*.com` or `example.*` is not a site. A leading `*.` is
  dropped, as the site covers its subdomains anyway. Opening any other page is refused, and so is
  reading or acting on a tab that a link or a redirect took elsewhere. A
  page's text is written by whoever runs the site, and an agent reads it
  as it reads a ticket: as something that may tell it what to do.
- **BRW-4** An action MUST be real input: mouse events at the element's
  middle, scrolled into view, and keys as key events, so a page cannot tell
  an agent's click from a person's. Each action waits for a page load it
  started (up to 10 seconds) and answers with the page as it is then.
- **BRW-5** A password field's value MUST NOT appear in an outline. The
  tools tell agents never to type the user's secrets, and to ask the user
  to sign in themselves.
- **BRW-9** The panel MUST show the task's tab as it is, and the user's
  own input MUST reach it as real input: mouse buttons, moves and the
  wheel, keys, paste, dictation and an input method's text. The page is
  laid out at the panel's size, so the user and the agents see the same
  page. Frames are sent only while the panel is on screen and the window
  is in front (as terminals paint only then), up to 60 a
  second, as sharp as a Retina screen (two device pixels to a CSS pixel).
  While the page moves (the user scrolling, or three changes within a
  quarter second, as in a scroll or an animation) they are drawn at one
  pixel per CSS pixel instead, which reaches the panel in about 15ms,
  until the page has been still for 150ms. A single change, a click, a
  key or a blinking caret, is drawn sharp: text MUST NOT flip between
  sharp and blurred. A click is one change however many frames Chrome
  draws it in. Before the first frame, and while a resize waits for
  the next, the screen MUST be the app's background, never a white
  flash. A hidden panel costs nothing. ⌘C and ⌘X put the page's
  selection on the Mac's clipboard (a headless Chrome copies to one of its
  own); ⌘L, ⌘R, ⌘[ and ⌘] are the address bar, reload, back and forward.
  The user may open any page; BRW-3 is about what agents may read, and the
  panel says when agents cannot use the page it shows.
- **BRW-10** What an agent last did in the tab MUST show under it, as a
  person would say it ("Clicked “Save”"), and where it clicked is marked
  for a moment. An element is named by its label, never its value, which
  for a password field is the password.
- **BRW-11** An agent MAY ask for a site with `browser_request_site`,
  saying what for. Only the user's Allow adds it: the request shows over
  the task's browser (or the chats'), which opens for it, and in the message center, with
  a banner while the window is away (when "Agents waiting on you" is on).
  The tool waits up to two minutes and says what the user chose, or that
  no answer came yet; the same request again waits on the one already
  asked. No `confirm` an agent passes can stand in for the user's answer.
- **BRW-12** "Take over" (offered while an agent is using the browser,
  BRW-15) MUST keep the task's agents out of its tabs until "Hand back":
  every browser tool is refused while the user holds it, reads included, so nothing an agent does or reads overlaps the
  user signing in. The tool says why, so the agent can tell the user what
  it was about to do. Holding outlives the tab: a browser started again is
  still held.
- **BRW-13** An alert, confirm or prompt the page opens MUST be shown over
  the panel and answered there (OK, Cancel, a prompt's text), or by an
  agent with `browser_dialog`. A headless Chrome draws none, and the page's
  scripts wait on it: an agent's action that opened one comes back at once
  saying so, rather than waiting on a page that cannot answer, and nothing
  else is done until it is answered.
- **BRW-14** A window a page opens (a link with `target=_blank`,
  `window.open`, a sign-in popup) MUST become a tab of the task, next to
  the tab that opened it, and the active one. When it closes, by the
  page closing itself as sign-in windows do or by the user or an agent
  closing it, the tab that opened it is active again if it is still open. An action that opened a window MUST be answered with that window: it
  waits from the page asking for one until the window is a tab, up to 3
  seconds, past which the window is taken never to have come (a blocked
  popup).
  An agent's action that opened one is answered with it, however late
  Chrome says it opened: the app also asks Chrome for windows the task's
  tabs opened that it has not yet heard of, and waits for those.
- **BRW-15** While an agent is using the browser, the panel MUST say so,
  and the user MUST be kept out of the whole panel: an overlay over the
  page names the agent and what it last did, tab changes included
  ("Switched to “Admin”"), and takes the user's clicks, keys and wheel;
  the tab strip (switch, new, close), the address bar and back, forward
  and reload do nothing. Only "Take over" lets the user in, and "Hand
  back" gives the browser back. An agent is using the tab while one of
  its browser calls runs, and between calls for as long as its pane is
  still working on its turn, up to a minute after its last call: an agent
  thinks between calls, and an overlay that came and went with each call
  said nothing. With no agent using it, the page is simply the user's.
- **BRW-16** A task MUST have one or more tabs, one of them active,
  shared by the user and the task's agents: everything acts on the active
  one (the panel, the agents' tools, input, dialogs). When an agent
  switches tab, the panel follows. Agents list, open, switch and close
  tabs with `browser_tabs`, `browser_new_tab`, `browser_switch_tab` and
  `browser_close_tab`, and every outline lists the tabs. Closing the last
  tab leaves an empty one: a task's browser never has none. The tab strip
  MUST be keyboard-operable: arrows move and select, Home/End select its
  first/last tab, and the current tab is named as selected to assistive tools.
- **BRW-17** A tab MUST be named by its page's title, and by its site
  while it has none (loading, or a page without one); "New tab" when it
  has neither. Nobody names a tab.
- **BRW-18** The user MAY save sign-ins for agents to use: a site, a
  username and a password, in Settings → Browser, or from the panel's
  "Save sign-in" after typing one into the page (BRW-20). The password
  goes to the keychain, never `config.json`; the site and username are
  in `config.json`. A site is a host with or without a port: `localhost`
  alone is every port of this machine's (one app runs on :4200 for one
  task and :4210 for another), and a site elsewhere without a port is
  its default port only. A site's subdomains are other sites. A site MAY
  hold a `*`, as on BRW-3, for one account on many hosts
  (`*.staging.example.com`, `pr-*.example.com`); a `*` covers one name, so
  `*.staging.example.com` is not `staging.example.com` nor
  `a.b.staging.example.com`. When a site written out and a wildcard both
  fit a page, the written-out one's account is the one used unless the
  agent names another. A saved
  sign-in MAY be changed in place: its site, its username, and its
  password only when a new one is typed (an empty one keeps it). The
  password is never shown again, and a change that would save the same
  account on the same site twice is refused.
- **BRW-19** An agent MUST NOT see a saved password. `browser_sign_in`
  has the app fill the username, the password, or both (a login asks
  for them on one page or on two) into the fields the agent names, and
  press Enter if asked; it answers with the page, never the password. A
  password is filled only on a page of the site it was saved for, only
  into a password field (an agent could otherwise put it in a text box
  and read it back), and never over plain `http` to a site that is not
  this machine. While a filled-in password is on the page, until the
  page navigates, `browser_evaluate` and `browser_screenshot` are
  refused (either could read it back) and the outline hides that
  field's value even if the page shows it.
- **BRW-20** "Save sign-in" in the panel MUST save what the user typed
  into the page's sign-in form: the password is read from the page's
  password field by the app itself and goes straight to the keychain,
  never through the window. The username is the form's, and is asked for
  when the form has none (a login that asked for it on the step before).
- **BRW-21** An agent MAY copy a sign-in from another of the task's tabs
  into the active one with `browser_copy_session`: the other page's local
  storage, session storage and cookies are written into the active
  page's, which is then reloaded. It is for an app signed in on one port
  of this machine (the one its sign-in server lets through, CORS) and
  needed on another. Both pages MUST be on this machine, so nothing is
  copied off it or from a site agents only browse. The agent is told
  what was copied by name and count, never a value. An item already
  there under the same key is replaced; others are kept. Cookies are
  copied only between two hosts (`localhost` and `127.0.0.1`): one
  host's ports share them already. Refused while a filled-in password is
  on either page (BRW-19).
- **BRW-22** The camera in the panel's bar MUST take the active tab as
  the panel shows it, as sharp as on screen, as a PNG: a click puts it
  on the Mac's clipboard (to paste into an agent's terminal, a chat or
  Slack) and the button reads "Copied" for a moment; ⇧-click saves it
  in Downloads instead, named after the page's site and the time
  (`localhost-5173 2026-10-26 at 14.02.11.png`), never over another
  file, and says the name. It is the user's own picture of what they
  see, so it is taken while an agent uses the tab, and while a
  filled-in password is on the page (BRW-19 keeps those from agents).
- **BRW-23** On a Google sign-in page (`accounts.google.com`), the bar's ↗
  MUST open a sign-in window instead of the user's own browser: Google
  refuses to sign anyone in to a browser that a program drives. The
  app closes its browser, opens its Chrome (BRW-24) on the same profile as an
  ordinary window, with nothing driving it, on the page the sign-in was
  for (Google's `continue` address, else the page itself), and the
  panel says to sign in there and quit that Chrome (⌘Q), or press Done,
  which quits it. Agents wait meanwhile, as when the browser starts. Once
  it has quit, the browser starts again, and the tab opens that page,
  signed in. Opening the rejected page in the user's own browser failed:
  its address is only good in the session that asked for it, and a sign-in
  there never reaches the app's profile. Quitting the app quits the window.
- **BRW-6** The tabs MUST come back where they were: each task's tabs'
  pages and which was active are kept in `config.json`, and opened when
  the task's browser is next needed. The chats' are kept the same way
  (`browser.chat_tabs`). A config from before tabs had one
  page, which comes back as one tab. Deleting or finishing a task closes
  its tabs. A task's tabs that nobody has used for 15 minutes (no agent's
  call, not shown in the panel, not held by the user, no dialog waiting)
  are closed the same way and come back when next needed, each page
  loaded again: what was typed into it and not sent is gone. Once no task
  has tabs open, the browser stops too.
- **BRW-7** The browser MUST stop with the app: asked to close, then its
  process group killed after 3 seconds, alongside the agents' own grace
  period. A browser that dies is started again when next needed.
- **BRW-8** Nothing the browser does MUST wait on the main thread or the
  async runtime. Chrome is started on the blocking pool and spoken to over
  a pipe (`--remote-debugging-pipe`) by two threads of its own. A pipe, not
  a port: with a debugging port open, any process on the Mac could drive
  the browser and every site signed in to there.

Code: `browser/` (`install.rs` getting the app's Chrome, `chrome.rs`
running it, `cdp.rs` the
protocol, `page.rs` a tab's actions, `snapshot.rs` the outline, `sites.rs`
BRW-3, `tools.rs` the tools, `panel.rs` and `input.rs` the panel's side,
`requests.rs` BRW-11, `control.rs` BRW-12, BRW-13 and BRW-15, `tabs.rs`
BRW-16 and BRW-17, `events.rs` Chrome's events and BRW-14, `sign_in.rs`
BRW-18 to BRW-20, `session.rs` BRW-21),
`commands/browser.rs`, `mcp.rs` (`dispatch`), `BrowserPanel.tsx`,
`BrowserSettings.tsx`, `lib/browserInput.ts`, `Terminals.tsx`.

Known gaps:
- Agents cannot see the page's frames from another site (an embedded
  sign-in or payment form): the outline is the main page's.
- Passkeys, password managers and extensions do not work in the app's own
  profile.
- `browser_copy_session` does not copy IndexedDB, where some sign-ins are
  kept (Firebase's).
- A `<select>` list's options are not drawn when it opens in the panel:
  choose with the arrow keys and Enter, or by typing an option's first
  letters. Agents use `browser_select`.
- The mouse pointer does not change over links and text fields.

## 19. Specs

A task can have a spec: what the work must do, how it will be done, and
the steps to get there, agreed before an agent starts and checked as it
goes. It is one spec for the whole task, however many repositories the task
touches. The spec is plain files in the task's own `spec/` folder, laid out
the way the task's **toolset** lays them out: Kiro's requirements, design
and tasks, an OpenSpec project, the superpowers' designs and plans, or a
toolset a team adds. Agents and you both read and write those files. Layer
shows them, follows them as they change, counts their steps, and puts them
into each repository on the task's branch (**Commit spec**), so they go
through review with the code and stay in the repository after the task, the
branch and the app are gone.

A toolset is data, not code: a manifest saying where its files live, which
of them hold `- [ ]` steps, what installing it runs, and the paragraph the
agents are told about it. Installing it puts its command-line files and
skills in the task folder, for the agent CLIs Layer launches, and nowhere
else on the machine.

**Removed.** The app used to own an approval pipeline: drafts and an
approved record in `.spec-drafts/`, approving as a commit, files marked out
of date, copies "changed outside the app" with Put back, an agent's
proposals (`spec_propose`), telling agents of an approval, and a merge of
per-repository specs. None of it exists: the files are the record, and
nothing is waiting to be approved. A spec kept that way is moved into
`spec/` (SPEC-17). The ids of what it replaced are struck through under Dropped.

The shape of Kiro's spec, which is the default, follows Kiro's own:
requirements, then design, then tasks.

### Where a spec lives

- **SPEC-2** A task's spec MUST be one working copy, the folder
  `<task folder>/spec/`, laid out the way its toolset says, whatever the
  number of repositories (`commands/spec_copy.rs`, `spec_dir`). It MUST be
  the only place the user and agents edit: they reach it by its absolute
  path, which the context file gives (SPEC-10), or through the spec tools
  (SPEC-13). It starts empty but for what the toolset's install writes
  (SPEC-30), and is not filled from a repository's existing `openspec/` or
  `docs/superpowers/`. `spec/` mirrors a repository's root: a file at
  `spec/openspec/changes/x/tasks.md` is committed to
  `openspec/changes/x/tasks.md` (SPEC-35). Every read and write of it, by the
  tab, a draft or an agent's tool, goes through one module (`spec_copy.rs`),
  which refuses a path with `..`, an absolute path, a file the toolset does
  not list in `files`, and any link between `spec/` and the file (`lstat`,
  never followed). A write goes to a new file beside the target and is renamed
  over it, and every edit of the Spec tab's, a draft's and an agent's takes
  one lock (`commands/spec_work.rs`, `EDITING`), so a reader sees the old text
  or the new and two writers do not lose a tick. A listing stops at 5000
  files and 16 folders deep, and skips `node_modules` and `.git`. A file is
  text: one that is not UTF-8 fails its own read (`spec_read` of its path) and
  nothing else. A listing, the Spec tab, Commit spec and a sub-task's seeding
  leave it out and name it (`read_all`), so one such file cannot make the tab
  empty or stop Open pull requests.
- **SPEC-3** A task with no folder of its own (its root is itself a
  checkout, from before tasks had folders) MUST keep its spec in
  `<config folder>/specs/<repository>/<task>/` instead (`spec_dir`,
  `kept_dir`), `<task>` being the branch with every character that is not a
  letter, digit, `.`, `_` or `-` made a `-`. A spec MUST outlive its task on
  this Mac: deleting a task (`commands/tasks.rs`), and Clean up of a task
  folder no task uses (`commands/cleanup.rs`), copy `spec/` to
  `<config folder>/specs/<task>/` first (`archive_spec`, `archive_leftover`;
  a name already taken gets `-2`, `-3`, up to 99), without `node_modules` and
  links, and go on only when the copy is whole. A spec that could not be kept
  stays where it is and so does the task: a second Delete finishes it. A
  spec with no file is kept as nothing. A repository whose `spec_commit` is
  `none` gets no spec file in its worktree (SPEC-35), and the spec still
  outlives the task here.

### Toolsets

- **SPEC-26** A toolset MUST be a manifest (`spec/toolset.rs`, `Toolset`): an
  `id`, a `name`, an optional pinned `version` (shown in the UI), `files` and
  `steps` (globs relative to `spec/`; `steps` inside `files`), `install` (a
  list of steps, SPEC-30), `commit_to`, `agents_md` (the paragraph agents are
  told, SPEC-10) and `extras` (Kiro's features: Draft SPEC-5, Apply answers
  SPEC-20, Check SPEC-15). A glob is `*` within one path segment and `**`
  across any number (`spec/glob.rs`, written by hand: nothing else needs
  more). Layer MUST ship three, each a JSON file and a
  Markdown paragraph in `src-tauri/toolsets/`, listed in this order
  (`toolset::builtin`):
  - **Kiro** (`kiro`, "Kiro (Layer)"): `requirements.md`, `bugfix.md`,
    `design.md` and `tasks.md`, flat in `spec/`; steps in `tasks.md`; nothing
    to install; all three extras. The files and their shape are SPEC-1.
  - **OpenSpec** (`openspec`, pinned `1.14.3`): `openspec/**/*.md`; steps in
    `openspec/changes/*/tasks.md`; installed by
    `npx -y @fission-ai/openspec@1.14.3 init --tools <ids> .` in `spec/`. Its
    `init` writes the chosen tools' skills and commands (`.claude/`, `.agents/`,
    `.opencode/`, `.gemini/`) under `spec/` too, where no agent CLI looks for
    skills (SPEC-32 shows the task folder's) and Commit spec does not copy them
    (they are not in `files`): they are inert, and agents use OpenSpec through
    the commands its paragraph gives.
  - **superpowers** (`superpowers`, pinned `7.0.2`): `docs/superpowers/**/*.md`;
    steps in `docs/superpowers/plans/*.md`; installed by fetching
    `https://github.com/obra/superpowers` at `v7.0.2`, which must be commit
    `22f0b463e7a5e579904dd6211a520321035526f7`, and copying its skills.

  The paragraphs of OpenSpec and of superpowers tell the agent where to save:
  OpenSpec's commands are run from `spec/` as
  `npx -y @fission-ai/openspec@<version> <command>` (`openspec` is not on the
  PATH; the paragraph's `{version}` is filled in, like `{spec_dir}`), and
  they say not to archive a change, which happens in each repository after it
  merges (known gaps); superpowers' design and plan are saved under
  `spec/docs/superpowers/` by absolute path, so neither writes into a
  repository's worktree.
- **SPEC-27** The user MUST be able to add toolsets of their own, in
  Settings → Code, as the JSON list of their manifests
  (`commands/spec_settings.rs`, `set_custom_toolsets`, `SpecSettings.tsx`).
  They are kept on this Mac only, never shared with the workspace, and one
  with a built-in's id replaces it. A custom manifest writes its paragraph
  inline (`agents_md`). The list is refused whole, with the reason, when it is
  not valid JSON or one manifest breaks a rule (`toolset::validate`): the id is
  1 to 60 lowercase letters, digits, `-` or `_` and not used twice; a name; a version
  that is one plain path segment (letters, digits, `.`, `_`, `+`, `-`), because
  it names a folder; at least one file glob; every glob (`files`, `steps`, a
  `skills` glob, `commit_to`) a relative path inside its folder, with no `..`,
  `.`, empty segment, backslash or leading `/`; every step glob inside `files`;
  a `run` step with a command, a `fetch` step with a URL and a ref (and, if it
  names a `commit`, a full 40-character commit id), and each
  install step of exactly one kind (`run`, `fetch` or `skills`, never two). The
  field says that `run` steps execute as written, on this Mac, as the user. The
  config file is read more leniently than Settings: an entry in it that is not
  a manifest at all (a hand edit gone wrong, a step of two kinds) is left out
  and said on stderr, and every other entry and every task is kept, where it
  once made the whole config unreadable (`toolset::lenient_list`). The next
  write of the config no longer has the left-out entry.

### Configuration

- **SPEC-28** A repository MUST say how it takes specs, in the Repos view
  (`ReposView.tsx`, `RepoSettings.tsx`, `set_project_specs`): the toolset it
  prefers (`spec_toolset`, or none: the app's default), what Commit spec
  puts in it (`spec_commit`: `whole`, `own` or `none`, SPEC-35), and the folder
  the spec is committed under (`spec_folder`, default `specs`), which is offered
  only where the toolset in force there is Kiro and the mode is not `none`: the
  other toolsets mirror their own layout. These are shared with the team like
  the repository's other settings (`config/project.rs`, `config/shared.rs`,
  `crates/domain`, migration `0007_spec_toolsets.sql`). `specs_in_app = true`,
  which earlier builds had, reads as `spec_commit = none` wherever it is found:
  the config, the changes still queued for the workspace, the database, and a
  change an older Layer sends the server (`layer_domain::specs_in_app`, in the
  sync rules' create and update). An older Layer's `specs_in_app = false` MUST
  change nothing: it reads every repository as `false` and sends that with any
  change, so taking it as `whole` would undo `own` and `none`. A toolset id is
  checked as a manifest's is. The web app's repository settings offer the same
  three (`web/src/repositories.tsx`): the built-in toolsets or each Mac's
  default, plus a team's own id when one is set, and the folder only where
  Kiro or the default is in force. The app's own settings, on this Mac
  only, are the default toolset (Kiro unless changed; `set_default_toolset`,
  which refuses an id no toolset has) and the custom toolsets (SPEC-27); Spec
  first is SPEC-38.
- **SPEC-29** A task MUST have one toolset, fixed when it is made
  (`Task.toolset`, `commands/tasks.rs`). Start work offers a pick (SPEC-18);
  a sub-task is made with its epic's (SPEC-39); a task made any other way has
  none until its install, which stores the one the app's default then names,
  else Kiro. Everything of the task (layout, steps, install, the agents'
  paragraph) follows that one, whatever its repositories prefer. A repository's
  `spec_toolset` is only what Start work starts from: the one every picked
  repository prefers, else the app's default, else Kiro (`lib/spec-toolset.ts`).
  A toolset id nothing on this Mac has is passed over at every step: a
  repository naming one (a teammate's custom toolset) gets a note in the Repos
  view and is ignored for that default; a task whose toolset was since removed,
  and an app default that was, fall back to the next, ending at Kiro
  (`spec_copy.rs`, `toolset_of`) rather than failing every read of the spec. A
  new task whose toolset nothing has is refused before its folder is made.

### Installing a toolset

- **SPEC-30** When a task is made, once its worktrees exist, and when a
  repository is added to a task that has a folder and no install on record,
  the task's toolset MUST be installed in the background, off the app's runtime
  threads (`commands/spec_install_run.rs`, `commands/spec_install.rs`). It runs
  the manifest's steps in order and stops at the first that fails:
  - `run`: an argv (never a shell string) in `spec/` or in the task folder,
    with no input, a timeout (120 s unless the step says), which kills the
    step's whole process group, and of the login shell's environment only
    `PATH`, `HOME`, the user and shell, `TMPDIR`, the locale (`LANG`, `LC_*`,
    `TZ`), proxies, certificate files, `XDG_CONFIG_HOME`/`XDG_CACHE_HOME` and
    where node managers live (`NVM_DIR`, `VOLTA_HOME`, `BUN_INSTALL`,
    `PNPM_HOME`) (`spec_runner.rs`, `step_env`): a step is code from the
    network, and a token or key in the environment would be its to read. A
    step is over when its process is: what it left running in its group is
    killed, so it cannot hold the step's output open and the install with it;
  - `fetch`: a git URL at a pinned ref, cloned once into
    `<config folder>/toolsets/<id>/<version>/` and reused by every task after,
    beside a marker of the URL, ref and commit it came from, so a copy fetched
    for another is not taken for this one (a manifest that fetches must name a
    version). A step that names a `commit` refuses a clone of another commit
    (a tag moved since it was pinned), and keeps nothing of it;
  - `skills`: copy the skill folders a glob names in the fetched copy into
    `.claude/skills/` (which Claude Code reads) and `.agents/skills/` (Codex and
    OpenCode) of the task folder. A file the user already has there is kept;
    a link in the copy is left out and said.

  `{version}`, `{task}` (the branch as one path segment), `{skill_dirs}` and
  `{openspec_tools}` (the OpenSpec ids of the agent CLIs in Layer's catalogue
  that OpenSpec has an id for and keeps the files of in the folder it runs in,
  today `claude,gemini,opencode`: for codex it writes to `~/.codex/prompts`,
  which every project shares) are filled in a
  step's arguments. An install writes only in the task folder, never in a
  worktree, and records every path it wrote, those a `run` step made too (found
  by comparing the folder before and after), in `.spec-install.json`, with the
  toolset, its version, whether it succeeded, its error and the end of what it
  printed. Deleting the task and Clean up remove exactly those files, and the
  folders they emptied, and leave what the user or an agent added beside them
  (each path in the record is checked first: inside the folder, through no link,
  not in a worktree). Nothing is installed globally or in the user's own
  `~/.claude` or `~/.agents`.
  When the spec of an earlier build is still to be moved (SPEC-17) and the
  move fails, nothing is installed and the error says so: the install would
  make `spec/`, and the old spec could not be moved after.
- **SPEC-31** An install that fails MUST NOT stop the task or its agents: how it
  went is its record, and the Spec tab says "Installing…" while it runs and
  shows the error with **Retry** when it failed (`install_spec_toolset`,
  `SpecView.tsx`). Retry runs every step again; a fetched copy already kept is
  reused. Each start and end is announced as `spec:install`. An install of a
  task that is already installing is not started a second time.
- **SPEC-32** An agent started in a worktree MUST be shown the skills in the
  task folder above it. Claude Code reads `.claude/skills/` only up to the
  repository root, which is the worktree, so it is started with
  `--add-dir <task folder>` (`agents.rs`, `task_folder_args`; `commands/panes.rs`,
  `start_agent`, which every start, restart and restore of a task's agent goes
  through), unless it starts in the task folder itself. The option goes last in
  the arguments, since it takes any number of directories and a prompt after it
  would be read as one. No other CLI gets an argument: none is known that adds a
  skills folder.

### Steps and who owns them

- **SPEC-33** A step MUST be a `- [ ]` or `- [x]` list item in a file the
  toolset's `steps` globs name (`spec/owned.rs`): every checkbox outside a code
  block, nested ones included. A step is addressed by its file and its position
  in it, from 1, counting every checkbox; the number its text shows (`1.1`,
  `Step 2`, `3.`) is displayed and never used, since toolsets number
  differently and restart per section. An agent ticks one with `spec_task` by
  file and position (the file may be left out when there is one step file), and
  a box edited by hand or by an agent's own file write counts the same: the file
  is the record. The tab counts steps per repository from the files (SPEC-8).
- **SPEC-34** Which repository a step belongs to MUST be read from the file
  (`spec/owned.rs`, `owned_steps`): a `[folder]` tag on the step's own line (a
  link's text is not a tag), else the nearest heading above it, of any level,
  whose whole text is a repository's folder name (`# api`, `## api`) or that
  carries a `[folder]` tag (`## 2. API [api]`), case aside. A heading of the same
  or a higher level that names no repository ends the owner's section, so what
  follows is everyone's; a lower one (`### Task 3` under `## api`) keeps it. A
  step with no owner belongs to every repository: OpenSpec's `## 1. Setup` and
  superpowers' `### Task 3` are everyone's until a tag or a repository's heading
  says otherwise. A step that waits on another repository's says
  `(after <repo> <n>)`; only Kiro's files and an epic's plan read it (SPEC-1,
  SPEC-39), for the others it is for the agent.

### Commit spec

- **SPEC-35** Commit spec (the Spec tab's button, `commit_spec_cmd`,
  `spec_commit` over MCP; `commands/spec_commit.rs`) MUST copy the working copy
  into each repository's worktree per that repository's `spec_commit` and
  commit those paths on the task's branch (`git::commit_only`, in
  `git/only.rs`, which commits only the paths it is given and leaves an
  agent's staged work staged), with the sentence "Update the spec for
  ACME-1." (`docs: update the spec for ACME-1` for a repository that writes
  conventional commits, or lints them). `spec/` mirrors a repository root: a
  file goes to the same path, except Kiro's, whose files are flat, which go
  under `<spec_folder, else specs>/<task>/` (`<task>` as in SPEC-3). Another
  toolset's files go only inside its `commit_to` (`{task}` filled in): one its
  `files` list names outside that is left out and named in the row. It writes
  only files that exist in `spec/` and that the toolset lists, never deletes
  anything in a repository (an older `openspec/specs/` stays as it is), writes
  nothing under `.git` or through a link, and does not rewrite a file that is
  already what it would write. A repository where one of those files was
  changed since its last commit (edited, made or deleted by hand or by an
  agent, and not already what Commit spec would write) is refused before
  anything is written, its row naming the files: Commit spec would write over
  that work, and commit what was left of it as the spec. `whole` copies every file; `own` copies the step
  files with only the steps that belong to that repository and to everyone
  (SPEC-34), and what is above the first repository's section, and every other
  file whole; `none` writes nothing and says so in its row. A file that is not UTF-8
  text is left out and named in the row's detail (`; left out: <path> (not
  UTF-8 text)`), and the repository's commit goes on without it. The result is a row
  per repository, a failed one named with why, and the others keep their
  commit. A review task, which has no spec, is refused. Open pull requests
  (PR-1, `commands/open_prs.rs`) MUST run Commit spec first, for the
  repositories ticked whose last commit does not hold what Commit spec would
  write now (`spec_commit.rs`, `commit_changed`, `changed_since_commit`, which
  reads the committed text of every file in one `git cat-file --batch` per
  repository, `git::committed_texts`), say so in that repository's row, and not open the
  code of one it could not commit.
- **SPEC-16** A drafted pull request description (PR-4, `spec_work.rs`,
  `pr_section`) MUST, for each repository whose requirements it can read, say
  where its spec is in it
  (`<spec_folder>/<task>/` for Kiro, the toolset's `commit_to` for the others,
  "kept outside the repository, so not linked" for `none` or for a toolset with
  no `commit_to`) and list the requirements that name it, with the latest
  check's answer (SPEC-15), or say none was run. The requirements are read from
  the toolset's entry file (SPEC-10) in Kiro's `R-n` form.

### The Spec tab

- **SPEC-8** The Spec tab (`SpecView.tsx`, `spec/SpecTree.tsx`,
  `spec/SpecEditor.tsx`, `spec/SpecSide.tsx`, `lib/specs.ts`; `read_spec`,
  `commands/spec_view.rs`) MUST show: the toolset's name and version and its
  install line (SPEC-31); the files its manifest lists, as a tree grouped by
  folder, each opened in an editor; per repository, its steps ticked of the
  total (its own and everyone's, SPEC-34) and where Commit spec stands (what it
  did just now, or "Not in its last commit", or "Committed", or "Kept by the
  app, not committed"); and, on the tab, a badge of steps done of all, else how
  many files there are. A file the toolset keeps that is not UTF-8 text is
  named in a note and not shown (SPEC-2). The editor saves a file as a file write
  (`write_spec_file`), through the same lock and refusals as an agent's
  `spec_write`. Commit spec is off while any editor holds text not saved, since
  it puts the saved files into the repositories. The tab MUST read the spec
  every time it is shown (the folder is followed only while it is, SPEC-36), and
  a read that fails MUST say so on the tab with Try again: in place of the spec
  when there is none yet, above it when there is. An empty spec says where its
  files go and offers **Ask an agent to write it** (`ask_for_spec`): Spec
  first's line (SPEC-38), typed into an agent running in the task, or the
  opening prompt of one it starts (`AgentTarget.tsx`), so the agent writes the
  spec and stops for your go; not on an epic's page, where the epic chat is the
  agent. An epic's spec is the same tab on the epic's
  page (§26).
  The task's tabs MUST be Agents, Spec, Diff and Pull requests, in that order
  (`store.ts`, `TABS`; `App.tsx`), and selecting a task opens on Agents; Start
  work with "Write a spec first", and Start work on an epic, open on Spec
  (SPEC-18).
- **SPEC-36** A change to the spec MUST be shown as it is made. While a task's
  Spec tab is on screen, a change in `<task>/spec/`, by an agent, an editor or
  the user, MUST reach the tab as `spec:changed` once the folder has been quiet
  for 200 ms (and at least once a second while it is not), so a burst of writes
  is one re-read (`commands/spec_watch.rs`, the `watch_spec` command called from
  `Watchers.tsx`). Only the task or epic on screen is followed (one watcher, driven by
  the system's file events through the `notify` crate: never polled), none when
  its Spec tab is not, and a folder that does not exist yet is followed from its
  nearest existing folder and from the moment it appears. The tools `spec_write`
  and `spec_task` also say `spec:changed`, and an agent that stops makes the tab
  read again (`Watchers.tsx`, `pty:activity`), so a task not on screen is
  current when shown.
- **SPEC-37** An editor with unsaved text MUST keep it when its file changes on
  disk, and say so, with **Load theirs** (take the file's text) or **Save mine**
  (write over it): there is no merge (`lib/spec-edit.ts`, `mergeDisk`;
  `SpecEditor.tsx`). A file whose text is the disk's again has nothing left to
  keep. Cmd+S and Cmd+Enter save the file, except while it has changed on disk
  under the editor: over an agent's text is only the button. A file with an
  unsaved or a conflicting edit is marked in the tree. The backend holds the
  line too, since a write can land between the tab's last read and its save: a
  save sends the text the editor began from (`base`), and
  `write_spec_file` checks it against the file under the lock every write takes
  (`spec_work.rs`, `write_checked`) and writes nothing, answering false, when the
  file holds something else. The tab then marks the edit a conflict and reads the
  file again, and only Save mine (`force`) writes over it. Apply answers, which
  saves first, stops with that message instead of running over the agent's text.
  What is typed while a save is under way stays in the editor, now over the text
  it saved (its new `base`), so the file's next read is not taken for an agent's
  write (`lib/specs.ts`, `save`).
### Working to it

- **SPEC-10** The context file (PANE-13, `commands/task_context.rs`) MUST have a
  Spec section after the epic's and before the ticket (not in a review's). It
  gives the absolute path of the working copy as the record to read before work
  and to edit directly when the plan changes (`spec_write` does the same), saying
  nothing needs approving first; the toolset's paragraph, with `{spec_dir}` and
  `{version}` filled in; how to tick a step (SPEC-33); the four spec tools in a line each
  (SPEC-13); with two or more repositories, the conventions for them (one spec
  for the task; requirements naming their repositories in square brackets;
  contracts written once in the design, naming provider and users; steps under a
  first-level `# <repo>` heading named for the repository, or tagged `[repo]`,
  with `(after <repo> <n>)`;
  Commit spec copies each repository its part and no spec file is written in a
  worktree); and the first 8 KB of the toolset's entry file, its headings
  lowered two levels, cut with a pointer to the file: Kiro's `requirements.md`
  (or `bugfix.md`), OpenSpec's newest change's `proposal.md`, superpowers'
  newest design, by modification time (a custom toolset has none,
  `task_context.rs`, `entry_file`). The file is rewritten when the task is made,
  its ticket or repositories change and an agent is started, not when the spec
  does, so the entry file may be older than the file: the section says so.
- **SPEC-11** An opening prompt for a task (the launch dialog's, a handoff's, a
  sub-task's, Start work's and the Henchman's start; `commands/spec_work.rs`,
  `with_spec_line`, called from `jira.rs`, `tickets.rs`, `epics.rs`,
  `henchman_work.rs`) MUST end by pointing at the spec folder by full path when
  it holds files, so an agent started inside one repository finds the whole
  spec, and, with Spec first on (SPEC-38), with that line after the pointer.
  What a route adds (a handoff's history, a sub-task's steps) goes before, so
  these are the last words of the prompt. A review's prompt is given neither.
- **SPEC-38** With **Spec first** on (an app setting, off by default,
  `set_spec_first`; the switch is Settings → Code, "Always write a spec first",
  `SpecSettings.tsx`) every agent Layer starts MUST be told, in words only, to
  write or update the spec in `<task>/spec/` before it changes any code, and
  then to stop until the user says go (`spec_first_line`): `Write or update the
  spec in <path> before you change any code. Then stop: say the spec is ready,
  and change no code until the user tells you to go on.` followed by "Once they
  do, implement it step by step, ticking each step." is the first line of the
  context file's body, and the two sentences are the last line of every opening
  prompt (SPEC-11), also while the spec folder is still empty. A review is never
  told it, nor a sub-task: it works from its epic's spec, already agreed
  (EPIC-8). Nothing blocks an agent that ignores it. Start work's "Write a spec
  first" (SPEC-18) is on and cannot be turned off while the setting is on.
- **SPEC-12** The work prompt (a sub-task's agent starts with it, EPIC-8;
  `spec_work.rs`, `work_prompt`) MUST send an agent through the spec's steps:
  every step file, or one repository's steps in it (its own and everyone's),
  with the count. It tells the agent to work through them in order, leave a step
  whose `after` is not ticked until it is (or, alone in its repository, stop and
  say which it is waiting for), tick each one as soon as it is finished, before
  starting the next, with `spec_task` by file and position or by editing its box,
  and change the spec rather than go on when it does not hold. Told only to mark
  each as it finished it, an agent did three steps and ticked them together five
  minutes after the first, and the tab sat at 0 meanwhile.

### From Start work

- **SPEC-18** Start work (`StartWorkDialog.tsx`, `start_work_on_ticket`) MUST
  offer the spec toolset the task is made with (`toolset`), starting on the
  suggestion of SPEC-29 and keeping a pick made by hand when repositories are
  added, and refuse a toolset nothing has before a folder is made. With an agent
  picked it MUST offer "Write a spec first" (`write_spec`), on until turned off
  and remembered. With it the task is made and the ticket moved as without it,
  the picked agent starts at once with the Spec first sentences (SPEC-38) at the
  end of its prompt, and the task opens on its Spec tab. With Spec first on as a
  setting the option is on and locked. An epic-level ticket makes an epic
  (EPIC-2), starts no agent and opens on Spec.

### Agents' tools

- **SPEC-13** An agent MUST be able to change the spec with the `villain-layer`
  MCP server's tools, as an alternative to its files (`mcp.rs`, `spec_tools`;
  `spec_work.rs`, `spec_tool`), each acting on `task_id`, else the task of the
  pane it is called from:
  - `spec_read`: with no `path`, the spec's files, each with its steps done and
    total (a step file that is not UTF-8 text has an `error` and no steps); with
    one, that file's text.
  - `spec_write`: a file's whole new text, making its folders; only a file the
    toolset lists, and nothing outside `spec/` (the refusals of SPEC-2).
  - `spec_task`: tick or untick a step by `file` and `step` position (SPEC-33);
    returns the step.
  - `spec_commit`: Commit spec (SPEC-35), for the repositories named by folder or
    name, or all when `repos` is left out. A `repos` that is not a list of names
    (`"api"`) is refused, not read as all. It is in `DANGER`: it needs
    `confirm: true`.

  A write or a tick says `spec:changed`. `spec_propose` is gone: it answers with
  where to write instead, for running agents that still have it in context. A
  tool of a task whose spec was kept the old way moves it first (SPEC-17).

### Kiro's extras

The extras in a toolset's manifest (`draft`, `questions`, `check`) are Kiro's:
Draft, Apply answers and Check against spec refuse a task whose toolset does not
list them, before anything runs (`commands/spec_draft.rs`, `require`). They read
and write the working copy's files like anything else that touches the spec, so
the tab follows them, and each is a fresh one-shot run of Claude Code (Sonnet, no
MCP; Design and Check may read files, and nothing that writes or runs is ever
allowed), never the agent doing the work. The prompts are in `commands/spec_prompts.rs`.

- **SPEC-1** Kiro's spec MUST be these files in `spec/`, which its paragraph
  (`toolsets/kiro.md`) tells agents too, and which Commit spec (SPEC-35) puts
  into each repository as `<spec_folder, else specs>/<task>/` in its worktree,
  the folder being set per repository in the Repos view (SPEC-28):
  - `requirements.md`: Goal, Requirements, Out of scope, Open questions. Each
    requirement is a list item with its id first (`R-1`, `R-2`, …), then the
    repositories that meet it by folder name (`R-3 [api, web]`), written as WHEN
    a condition THE SYSTEM SHALL a behaviour. One without an id is numbered by
    its place; one naming no repository is met in all of them. Each open
    question is a list item with its id (`Q-1`, …), the answers it might take as
    a list under it and, once answered, an `Answer:` line under it
    (`spec.rs`, `requirements`, `answered`).
  - `bugfix.md`, in place of `requirements.md` for a bug: Current, Expected and
    Unchanged behaviour (WHEN … THE SYSTEM SHALL CONTINUE TO …), each item with
    its id and repositories, and Open questions as above. A spec with a
    requirements file keeps its kind.
  - `design.md`: Approach; Contracts, each with its id first (`C-1`): what
    passes between repositories, which provides it and which use it; then a
    section per repository (`## api`).
  - `tasks.md`: `- [ ] 3. Call the refund endpoint (R-3, C-1; after api 2)`, each
    naming the requirements and contracts it serves and, when it needs one done
    first, another repository's step. With several repositories each one's steps
    are under a heading that is its folder's name, first level (`# api`, which
    Draft writes): a second-level one (`## api`) is read too (SPEC-34) but ends
    at the file's own `## Tasks`, so agents are told `# api`. That is how a step
    file says whose steps are whose.
- **SPEC-5** Draft (`draft_spec`, the tab's three Draft buttons) MUST be one run
  per file, streamed into the editor as `spec:draft` and then written into the
  file as any edit is, returning the text it replaced for **Undo** (a draft that
  made its file is undone by deleting it, `remove_spec_file`, not by leaving an
  empty one):
  - Requirements: over the ticket (`TICKET.md`, fenced off as data) and the
    task's repositories; names the repositories that meet each requirement,
    invents none the ticket does not imply, and puts what is unclear under Open
    questions with the answers it might take. A task with no saved ticket is
    drafted from its name. A bug's analysis is `bugfix.md`, chosen on the first
    draft; a redraft is given the questions already answered, as decisions made.
  - Design: allowed to read every repository's worktree, over the requirements:
    contracts first, then each repository's section.
  - Tasks: over the requirements and design, every repository's steps in one run
    under its heading, so what comes first across repositories is decided once.
    The file is read again under the write's lock when the run is done, so a
    step an agent ticked meanwhile stays ticked; a ticked step the draft left
    out is put back, and a step whose requirements are all gone is marked for the
    user to remove, never removed by itself. An epic's tasks are grouped into
    sub-tasks (EPIC-4).

  A failed draft writes nothing. Draft is off while an editor holds unsaved text,
  which a draft would write over.
- **SPEC-19** The questions a draft leaves are the user's to decide, and an
  answer is only worth something once a requirement says it. Beside the
  requirements the tab MUST list the open questions the editor holds, how many
  have no answer, and for each the answers drafted for it to pick and a field for
  the user's own (`spec/SpecQuestions.tsx`, `lib/questions.ts`). An answer is
  written into the editor's text as the question's `Answer:` line, so the editor
  and the list are one record; an `Answer:` line typed into the editor counts the
  same, and so does a line under the question starting `--`.
- **SPEC-20** **Apply answers**
  (`apply_spec_answers`) MUST save the editor, then be a run over the
  requirements file that rewrites the requirements an answer decides and the
  repositories they name, puts in Out of scope what an answer leaves out, and
  removes each answered question; a requirement no answer touches keeps its
  text, id and repositories, a new one takes the next free id and a question with
  no answer stays. It writes only while the file still holds what the run was
  given: an agent that wrote the requirements meanwhile keeps its text, and the
  call fails saying to apply the answers again. The tab then names the
  requirements changed, added and removed, and Undo writes back the text the call
  returned. Undo, of this or of a draft, writes (or, for a draft that made the
  file, removes, `remove_spec_file`) only while the file holds what the call wrote
  (`after`, sent as `base`), and otherwise leaves it and says it changed since.
- **SPEC-15** Check against spec (`check_spec`, a Check button per repository, not
  offered on an epic, EPIC-6) MUST run, for one repository, a run allowed to read
  its worktree, over the requirements that name it, the contracts it provides or
  uses, and the branch's changes from its branch point (`git::baseline`). It
  answers per requirement: met, not met or unclear, with the files and lines that
  show it. A requirement two repositories meet shows each one's answer. The
  answer is shown beside the repository with the commit it was checked at,
  marked stale once the branch moves on, and kept in the task folder
  (`.spec-checks/<folder>.json`, `commands/spec.rs`), never committed; a task with
  no folder of its own keeps none.

### Epics

- **SPEC-39** An epic's spec (§26) is the working copy of the epic's task, read
  and written as any task's. An epic MUST be written with Kiro: only Kiro's
  tasks draft heads the groups its sub-tasks are planned from, and with another
  toolset the agent's own headings (`## 1. Setup`) became sub-tasks. Start work
  on an epic offers no toolset pick, and a new epic with another toolset is
  refused (`commands/tasks.rs`, `new_task`); without one it gets Kiro, not the
  app's default. Its sub-tasks MUST be planned from its step files
  (`commands/epics.rs`, `plan`; `spec_view.rs`, `step_texts`): each repository's
  share is its own sections and steps and everyone's (SPEC-34), and a heading
  shared across repositories is one sub-task. Filing a sub-task makes a task with
  the epic's toolset (SPEC-29), gives its working copy the epic's files that are
  not steps, whole, and its own steps, a `# <folder>` section for each of its
  repositories, in the epic's first step file (`seed_sub_task`), and commits
  nothing.

### Moving earlier specs

- **SPEC-17** A task whose spec was kept the old way MUST have it moved into
  `spec/` the first time its spec is read or written (the Spec tab, an agent's
  spec tool, a draft; `spec_copy.rs`, `move_old_spec`, from `with_spec_moved`;
  where it was is read by `commands/spec_old.rs`), and before anything else makes
  `spec/`, since a move is done only while the folder is absent: at the start of
  every toolset install (SPEC-30), which a repository added to the task
  starts, and when an agent is started (`spec_work.rs`, `for_agent_start`, from
  `start_agent`), whose first file would make the folder. It is not done by the
  context file's writer, which runs holding the config's read lock. What is moved
  is what was approved in `.spec-drafts/approved/`, with an unapproved draft over a file when the draft
  is newer; else the first repository's `specs/<task>/` requirements and design
  and each repository's tasks, as a first-level `# <folder>` section of one
  `tasks.md`; else a `SPEC.md`, as `requirements.md`. What is moved is Kiro's
  layout, so the task is Kiro's from then on, whatever the app's default is. The
  folder is built beside `spec/` and renamed into place, so a move that stopped
  halfway is done again and one that is there is never touched. The old folders
  (`.spec-drafts/`, each repository's `specs/<task>/`, `SPEC.md`) stay where
  they are; Commit spec overwrites a repository's copy file by file, which is the
  same path as Kiro's, and deletes nothing. `.spec-drafts/` goes with the task
  folder.

### Dropped

Requirements of the earlier text that no longer hold. Their ids are not reused.

- ~~**SPEC-4**~~ The files drafted in order, each approved before the next.
  Dropped: nothing is approved and no order is forced; the files are the
  record (SPEC-2).
- ~~**SPEC-6**~~ Approving a file as a commit in every repository. Replaced by
  Commit spec (SPEC-35).
- ~~**SPEC-7**~~ A changed approved file marking the files after it out of date,
  and Sync. Dropped; a redrafted `tasks.md` keeping what was ticked is in Draft
  (SPEC-5).
- ~~**SPEC-9**~~ Opening a `specs/<task>/` a worktree already has. Dropped: the
  spec is not filled from a repository (SPEC-2), and an earlier one is moved
  (SPEC-17).
- ~~**SPEC-14**~~ Offering to tell running agents of an approval. Dropped with
  approval.
- ~~**SPEC-21**~~ A warning on approving requirements with open questions.
  Dropped with approval; the questions are SPEC-19.
- ~~**SPEC-22**~~ A spec file "changed outside the app", and Put back. Dropped:
  an agent's edit is the record, the tab follows the folder (SPEC-36) and an
  editor in conflict keeps its text (SPEC-37).
- ~~**SPEC-23**~~ Open pull requests warning of a spec changed outside the app.
  Replaced: Open pull requests runs Commit spec (SPEC-35).
- ~~**SPEC-24**~~ An agent proposing a change with `spec_propose`. Replaced by
  the spec tools (SPEC-13); the tool answers where to write instead, for one
  release (§12).
- ~~**SPEC-25**~~ Merging the specs of several repositories into one. Dropped:
  one working copy (SPEC-2); earlier copies are moved (SPEC-17).

Code: `spec.rs` (a Kiro spec's files and what is read from them), `spec/toolset.rs`
(manifests, validation), `spec/glob.rs`, `spec/owned.rs` (steps and who owns
them), `spec/sub_tasks.rs` (an epic's sub-tasks), `src-tauri/toolsets/` (the
three manifests and their paragraphs), `commands/spec_copy.rs` (the working copy,
moving an old spec, keeping a spec), `commands/spec_old.rs`, `commands/spec_view.rs`
(what the tab is shown), `commands/spec_work.rs` (writing, ticking, the tools,
prompts, the pull request's part), `commands/spec_commit.rs`,
`commands/spec_install.rs`, `spec_install_run.rs` and `spec_runner.rs`,
`commands/spec_watch.rs`, `commands/spec_settings.rs`, `commands/spec_draft.rs`
and `commands/spec_prompts.rs` (Kiro's extras), `commands/spec.rs` (the checks),
`commands/task_context.rs`, `git/only.rs`, `mcp.rs` (the spec tools),
`commands/open_prs.rs`, `agents.rs` (`task_folder_args`), `SpecView.tsx`,
`spec/SpecTree.tsx`, `spec/SpecEditor.tsx`, `spec/SpecSide.tsx`,
`spec/SpecQuestions.tsx`, `SpecSettings.tsx` (Settings → Code), `RepoSettings.tsx`
(the Repos view), `tickets/StartWorkDialog.tsx`, `lib/specs.ts`, `lib/spec-edit.ts`,
`lib/spec-toolset.ts`, `lib/questions.ts`, `Watchers.tsx` (`spec:changed`,
`spec:install`, `spec:draft`, the watcher).

Known gaps:
- A toolset's `commit_to` says where its spec is in a repository only in a pull
  request description. Where Commit spec puts a file is decided by the toolset's
  id: Kiro's go under the repository's spec folder, every other toolset's to the
  same path as in `spec/`. A custom toolset cannot be relocated.
- A task's toolset cannot be changed after it is made. To use another, make the
  task again.
- The `start_work` tool agents have (§12) cannot pick a toolset or "write a spec
  first": it uses the defaults.
- The install runs in the background and may still be writing into `spec/` when
  an agent starts, and an agent's write in that time is recorded as the
  install's.
- The install's state shows only on the Spec tab, as a line: there is no dot on
  the task, and a finished install says nothing.
- Claude Code started in a worktree gets `--add-dir <task folder>`, which also
  gives it the task folder's sibling worktrees and `spec/` to read and write
  (SPEC-32). It was taken from Claude Code's documentation and not tried in a live
  session. Claude over ACP gets no such argument: nothing documented adds a
  directory there, so it may not see the task folder's skills.
- Codex is not launched by Layer, and run by hand in a worktree it scans
  `.agents/skills` only up to the repository root; it has no flag that adds
  another folder. Whether OpenCode, Gemini CLI and Copilot CLI find a task
  folder's skills from a worktree was not checked; only Claude Code is given the
  task folder.
- Commit spec overwrites an uncommitted edit of a spec file inside a worktree:
  the context file tells agents never to write one there, and Commit spec is what
  writes the file. One that fails partway in a repository does not undo the files
  it wrote before.
- OpenSpec's `archive` step (merging a change's deltas into `openspec/specs/`)
  happens in each repository, after the change merges, outside Layer. Layer
  neither runs nor shows it, and its paragraph tells agents not to run it.
- OpenSpec's `init`, run in `spec/`, writes its skills and commands for the
  agent CLIs under `spec/.claude/`, `.agents/`, `.opencode/` and `.gemini/`, not
  in the task folder where SPEC-32 shows skills. They are never read by an agent
  and never committed; a spec kept when its task is deleted (SPEC-3) keeps them
  too. Agents use OpenSpec through the `npx` commands of its paragraph.
- Checks kept in `.spec-drafts/` by earlier builds are not moved: the first
  check after the move is the first one.
- Clean up of a task folder no task uses counts every file its install recorded
  as the app's, even one that was edited since.
- Drafting, Apply answers and Check need Claude Code on the PATH, whichever agent
  does the work, and a draft cannot be stopped once started.
- A drafted pull request description's spec section lists only requirements in
  Kiro's `R-n` form, so for OpenSpec and superpowers it is usually left out.
- An epic's plan (SPEC-39) reads steps only the way Kiro's `tasks.md` has them
  (numbered top-level items), which is why an epic is written with Kiro.
- The extras are Kiro-shaped: they write `requirements.md`, `design.md` and
  `tasks.md`, so a custom toolset that lists `extras` must list those files.
- Kiro runs independent tasks side by side; here one agent works through them in
  order, or one per repository waiting on each other's `after`.
- A step an agent ticks is a change in the working copy only; the repositories
  have it after Commit spec. A tick is the agent's word: one that saves its ticks
  up shows the work behind where it is, and one that ticks a step it did not
  finish is caught only by Check against spec.
- An answer to an open question is recorded in the spec's commit only. The
  ticket, and whoever asked for the work, are not told what was decided.
- Each repository's pull request carries the whole requirements and design, the
  other repositories' parts too (`own` shortens only the step files).
- Which repositories a requirement names is the drafter's judgement; it is
  corrected in the editor like anything else.

## 20. Agents over ACP

An agent can run as a conversation instead of in a terminal. The app
starts the CLI as an [Agent Client Protocol](https://agentclientprotocol.com)
server, speaks JSON-RPC to it over its stdin and stdout, and draws what it
says: replies as Markdown, each tool call with its output or diff, its
plan, and its permission questions as buttons. It is chosen when starting
an agent ("As a conversation (ACP)" in the launch dialog, "· conversation
(ACP)" in the Chat view's menu), beside the terminal, which stays the
default. Otherwise the pane is an ordinary one: it counts toward PANE-2,
is stopped as PANE-5 says, comes back as PANE-7 says, and every list,
count and banner treats it like any other.

| Agent | ACP command | Picks a conversation up by | Checked against |
|---|---|---|---|
| `claude` | `claude-agent-acp`, installed separately (`@agentclientprotocol/claude-agent-acp`, formerly `@zed-industries/`) | load, resume, list | adapter 0.98.0 |
| `copilot` | `copilot --acp` | load, list | 1.0.100 |
| `opencode` | `opencode acp` | load, resume, list | 2.2.5, on its free models, signed in to nothing |
| `gemini` | `gemini --acp` | load | 0.63.0, `initialize` only: it would not open a conversation for a personal Google account |

- **ACP-1** ACP MUST be offered only for an agent whose catalogue entry
  has an ACP command, and only when that command is on the PATH. A
  task's "+" and the Chat view's list each such agent a second time, as
  "· conversation (ACP)". The launch dialog remembers the last choice,
  made there or in the menu.
- **ACP-2** An ACP pane's state MUST come from the protocol, never from
  its output: a `session/prompt` in flight is working, an unanswered
  `session/request_permission` is asking, the prompt's answer is done
  (idle when it was cancelled), and an error ends the turn as done, with
  the error shown. Started or picked up and given nothing yet, it is idle.
- **ACP-3** The app's MCP server MUST reach the agent in `session/new`
  (and load, resume) as an `http` server, with the bearer token and
  `X-Villain-Pane` as headers: over the agent's stdin, never on a command
  line or in a file (MCP-2). Nothing is written for the CLI to load: no
  hooks, no plugin, no settings; what a launch gives its agent (the
  Henchman's settings, HEN-2) goes in the same request's `_meta`,
  which an agent that does not know it ignores. An agent that takes no MCP server over
  HTTP starts without the app's tools, and the pane says so.
- **ACP-4** Text MUST reach an ACP agent whole, however long: the opening
  prompt, a handoff, review comments, conflicts, a line from the phone.
  PANE-11's limit and its files are for typing into a terminal. A prompt
  sent while a turn runs waits for it to end, and the conversation shows
  it waiting.
- **ACP-5** A permission question MUST show the agent's own options as
  buttons, and be answered only by the user. The app never answers one
  itself, and never chooses "always".
- **ACP-6** Esc or Stop MUST cancel the running turn (`session/cancel`),
  answer every open question "cancelled", as the protocol requires, and
  drop the prompts that were waiting, marked as not sent. The pane is idle
  once the agent says the turn was cancelled. Closing a pane stops the
  process as PANE-5 says.
- **ACP-7** The pane MUST keep the conversation (its last 2,000 entries),
  and the window MUST be told of a change (`acp:update`) only while the
  pane is on screen; one coming on screen asks for what changed since it
  last looked. A plain-text copy goes into the pane's scrollback, so a
  handoff's briefing (PANE-9), `pane_output` (MCP-4) and the phone
  (PHONE-6) read an ACP pane as they read a terminal. Thinking is left out
  of the copy.
- **ACP-8** An ACP pane MUST be called by the title its agent gives the
  conversation (`session_info_update`), cut as PANE-12 cuts one.
- **ACP-9** An ACP agent MUST pick a conversation up by its id. Restart
  (PANE-14) and a restore (PANE-7) use the id the agent gave, kept with the
  saved pane: `session/load`, which replays it into the pane, or else
  `session/resume`. Until the agent gives one, the pane keeps the id it
  was opened to pick up: one stopped or quit before its agent said so was
  started new, and its conversation lost. Resume with no id takes the
  newest the agent lists for the folder (`session/list`). One that cannot
  be picked up is started
  new, and the pane says why. So two agents in one folder never pick up
  the same conversation, and Restart works for every ACP agent. A replay
  shows what the user wrote, without a CLI's record of its own local
  commands: `claude-agent-acp` runs `/model` itself as each session
  starts, and a picked-up conversation opened with that, its output and
  the user's first message run together as one.
- **ACP-10** On the phone, an ACP pane's keys MUST mean: Esc and Ctrl-C
  stop the turn; 1 to 9 choose the oldest open question's options in
  order, as the text copy numbers them. A line typed is a prompt (ACP-4).
- **ACP-11** An agent that will not open a conversation MUST say why in
  the pane, with how it says to sign in (its `authMethods`). The app never
  signs in for it.
- **ACP-12** The settings the agent offers for the session
  (`configOptions`: its mode, its model) MUST be shown and changed from the
  pane, and only by the user, or by a setting the user made for it: a chat
  from Slack starts in `auto` mode (BOT-12) and on the bot's model (BOT-17).
- **ACP-13** The app MUST offer the agent none of the protocol's client
  methods: the agent reads, writes and runs commands itself, as it does in
  a terminal, and an `fs/*` or `terminal/*` request is refused, not left
  waiting.
- **ACP-14** A turn that fails on a usage limit MUST raise the
  usage-limit notice (STATE-6), read from the error's message; it clears
  when the agent next works.
- **ACP-15** A conversation's text MUST have a size of its own (Settings
  → Conversation text, 9–24px), as a terminal's has, apart from the
  interface scale: the conversation and the message box follow it, every
  part in proportion, and open panes change at once. The bar under the
  message box (mode, model, usage) is the app's chrome and does not.
- **ACP-16** An image pasted into the message box MUST go to the agent as
  an image (the protocol's `image` block), shown in the box until sent and
  removable there, never as the link that came with it: Slack and a
  browser's "Copy image" put one beside it as text. The image is taken
  over the text only when that text is empty or a lone link, so a
  spreadsheet's cells or a document's paragraph, which come with a picture
  of themselves, still paste as text. A type the models do not read (a
  TIFF) is sent as a PNG. An agent that does not take images
  (`promptCapabilities.image`) is said to, and the prompt is refused
  rather than sent without them. The conversation shows each image sent,
  small, with the words it went with, and one clicked whole in a dialog; a
  replay (ACP-9) shows the images it carries the same way. Only what reads
  the conversation as text (the scrollback's copy, Slack) names each
  `[image]`.
- **ACP-17** Settings → Agents MUST show, for each agent in the table
  above, where it is on the PATH and, under it, how it stands as a
  conversation: its ACP program's version (its `--version`, given 5
  seconds) against the one it was checked with. Supported is the checked
  version, or a later one in the same major release, and says only that
  it runs as a conversation too. Too old (before it), Not checked yet (a
  later major release) and Version unknown are pills, the one thing on
  the row that stands out; an agent not installed says to install it, and
  one whose ACP program is missing names it. Each is read as the tab
  opens and on "Check again". It is advice: no version is refused (ACP-1
  still offers any one on the PATH).
- **ACP-18** A ticket's key in what an agent says (`ACME-123`) MUST be a
  link: to its task in Work when a task has that ticket, else to the
  ticket in Tickets, where its work can start (TKT-12 finds one that is
  not on Mine). A word is a key only when its project is one Layer knows,
  from a task's ticket or a ticket it has read, so `SHA-256` and `UTF-8`
  stay words. A key in code or already in a link is left as it is.
- **ACP-19** What Layer itself types into a conversation (a message that
  starts `[villain-layer …]`: what finished for the Henchman, a word from
  it, a repository added to the task) MUST be shown apart from the user's
  words, muted and folded to its first line with its tag ("Layer ·
  finished"), and open on a click. The Henchman is sent one each time
  agents stop (HEN-9), and its conversation was nothing but those.
- **ACP-20** Dropping up to ten PNG, JPEG, GIF or WebP images, each at
  most 10 MiB, onto a conversation copies them to `attachments/` in the
  task root or standalone chat room. Layer detects the format from the
  bytes, never overwrites a file or writes into a checkout, and appends
  quoted absolute paths to the editable draft. Saving blocks sending, not
  typing; failed images show an inline error and successful ones remain.
- **ACP-21** What is typed or pasted into a conversation's message box and
  not sent MUST still be there when the user leaves the pane (another task,
  another view, the Henchman) and comes back, until it is sent or the pane
  is closed. A send that fails puts it back.
- **ACP-22** `/clear` and `/new`, sent alone, MUST start a new
  conversation in the same pane, on the model and in the mode the old one
  had, and show only a note that says so; the next prompt goes to the new
  one. They are refused while a turn runs or a question is open. An agent
  that offers a command by that name gets it instead, and the message
  box's suggestions list Layer's own after the agent's.

Code: `acp/` (`conn.rs` the protocol, `user.rs`, `conversation.rs`,
`entry.rs`, `read.rs`, `process.rs`, `version.rs` for ACP-17), `pty/acp.rs`,
`commands/acp_panes.rs`, `agents.rs` (`AcpCommand`), `AcpPane.tsx`,
`lib/acpUpdates.ts`, `lib/types-acp.ts`, `AgentSettings.tsx`, `lib/issue-keys.ts` and
`IssueKeyLink.tsx` (ACP-18), `lib/acpEntries.ts` (`layerSaid`, ACP-19),
`commands/attachments.rs` and `lib/chatDrop.ts` (ACP-20), `lib/acpPool.ts`
(ACP-21), `user.rs`, `Conversation::clear` and `lib/acpCommands.ts` (ACP-22). A real agent is checked with
`VILLAIN_ACP_AGENT="opencode acp" cargo test --lib real_agent -- --ignored`.

Known gaps:
- Claude Code runs over ACP only through the `claude-agent-acp`
  adapter, which bundles its own Claude Agent SDK: its models and features
  are that version's, not those of the `claude` on the PATH. An old one
  goes wrong in ways the app cannot see: 0.22.2, the last under
  `@zed-industries/`, has the model call its own tools without their
  schemas (`Read` with `path`, refused) until it looks them up. A slash
  command the user ran there is left out of a replay, with the adapter's
  own (ACP-9).
- Copilot's ACP mode is in public preview. Gemini CLI was checked only as
  far as `initialize`.
- Codex (`codex-acp`), Cursor, Goose and the rest of the ACP registry are
  not in the catalogue: each needs checking against a real install first,
  as PANE-1 asks.
- Only a task's "+", its launch dialog and the Chat view offer ACP.
  Resume, Start work, a sub-task's Start, the Diff tab's "send to an
  agent" and a handoff start a terminal.
- Drafting PR and ticket descriptions still runs `claude -p` (§3).
- The slash commands an agent offers are only suggested as `/` is typed.

## 21. Loops

An agent can be put on a loop: each time it ends a turn, the app runs the
checks of the repositories it works in (each repository's own command:
`bun run check`, `cargo test`), and when they fail, sends it what failed
and lets it go on. It goes round until they pass, or until it has used its
rounds, and then stops and tells you. Without one, you are the loop: you
read that the agent is done, run the tests, paste the failures back, and
again.

A repository's check command is set in the Repos view, or in the dialog
that starts a loop. The loop button in a task's pane bar starts one on the
agent pane in front; the bar under the pane bar shows how it is going.

- **LOOP-1** A repository MAY have a check command: what says the work in
  it is done. It runs in a checkout's worktree through `/bin/sh -c`, in the
  login shell's environment (PANE-4) with `NO_COLOR` set, standard error
  folded into standard output and nothing on standard input, and passes
  when it exits 0. It is set only by the user: never from a ticket, a spec
  or an agent, since it is run as the user, unattended. In a workspace
  (§25) only an admin gives a repository a check command or changes it
  (clearing one, or sending it unchanged, is anyone's), and each Mac runs
  only the very text its own user typed or confirmed there: one set in the
  Repos view or the loop's dialog on this Mac is confirmed, and one that
  came from the workspace is confirmed by pressing Enter in the
  repository's check field in the Repos view, whose tooltip says it waits.
  Until then a loop does not run it: one being started is refused, and one
  running stops at its next round, each saying to confirm it there.
  The checks a Mac had before confirming came in were confirmed once, as
  they stood.
- **LOOP-2** A loop MUST be started by the user on one running agent pane,
  with the most rounds it may take (5 unless changed, at most 20). It
  checks the repositories the pane works in: its own, for a pane started in
  one, else every repository of the task. Those without a check command
  are skipped, and with none at all it does not start. One loop per pane.
  An agent not in a turn when its loop starts is checked at once;
  otherwise at the end of the turn. For an agent over ACP the dialog
  offers its mode (ACP-12) as it stands, changed only if the user changes
  it: one that asks before every command holds its loop at each, as
  Claude Code in its default mode did at an `ls`.
- **LOOP-3** A loop MUST act when its agent ends a turn, as the CLI
  reports it (a Claude Code Stop hook, an ACP prompt answered: STATE-1),
  never on a timer or on output going quiet. A turn interrupted (STATE-2)
  is not an end: someone is at the keys.
- **LOOP-4** At a turn's end the loop MUST first see whether anything
  changed in the worktrees it checks (the commit, the diff, the untracked
  files) since its last failed check ended: measured from what the check
  left, so a check that writes files of its own (a coverage report, a new
  snapshot) is not taken for the agent's work. When nothing did, the agent most
  likely stopped to ask something, or gave up: the loop runs nothing,
  sends nothing, and waits on you, saying so. It goes on at the next turn
  that changes something. A turn that ends on a usage limit (STATE-6)
  waits the same way. Run again on nothing new, the same failures went
  back to an agent that had already said what it needed.
- **LOOP-5** Checks MUST run one repository after another, at most two
  loops' at a time in the app (the others queue, and say so), each command
  for at most 20 minutes. A command still running then is stopped as an
  agent is (PANE-5: SIGTERM to its process group, then SIGKILL), and the
  loop stops, saying why. So does a command that exits 127, the shell's
  code for one it cannot find, which is seldom the agent's to fix.
  Stopping a loop stops its check, and quitting the app stops every
  check as it stops the agents: in a process group of its own, a build
  would otherwise run on after the app had gone. Nothing
  that waits runs on the main thread or the async runtime. Every check is
  a real process, often a whole build: the cap keeps a dozen loops from
  building at once on one laptop.
- **LOOP-6** Checks that fail, with rounds left, MUST go back to the
  agent: for each repository that failed, its command, its exit code and
  the end of its output (150 lines, 12 KB, without colour codes), with
  the round it is and how many there are, and to end the turn once they
  pass. To a terminal as a file in the task folder (`CHECKS.md`) with a
  pointer typed (PANE-11), to an agent over ACP whole (ACP-4). Only
  between turns: when someone started a turn while the checks ran, their
  answer is thrown away and the loop checks again at that turn's end.
- **LOOP-7** A loop MUST end when its checks pass (passed), when they
  still fail after its last round (gave up), when the user stops it, when
  its agent exits or is closed (its task deleted too), at once, stopping
  a check it is running, or when a check cannot run (LOOP-5). Ending stops
  nothing else: the agent stays as it is.
- **LOOP-8** While a loop runs, a finished turn MUST NOT read as your
  turn: the pane reads as working wherever its state shows (its dot, the
  "needs you" count, the dock, banners, Keep awake: STATE-4, STATE-7),
  since the app is still at work on it. Waiting on you (LOOP-4) or ended,
  it reads as done again, as of that moment, so the dock and a banner say
  so even if you looked at the pane during the checks. The banner and the
  message center say how the loop ended ("passed its checks", "still
  fails its checks after 5 rounds", why it stopped) in place of "has
  finished".
- **LOOP-9** The pane MUST show its loop under the pane bar: the round,
  what it is doing (waiting for the agent, queued, checking which
  repository), how it ended, and each check run, newest first, with every
  repository's result and output. Stop while it runs; Start again once it
  has ended. The window is told of a change (`loop:changed`), never polls.
- **LOOP-10** A task's context file (PANE-13) MUST name each repository's
  check command confirmed on this Mac (LOOP-1), and tell agents to run it
  before they end a turn, so an agent checks its work by the measure the
  loop will, loop or not. One not confirmed is not named. Setting a command
  rewrites the context files of the tasks that repository is in.
- **LOOP-11** A loop MUST come back with its agent when the app reopens
  (PANE-7). It is kept with the saved pane, as it goes: its rounds, the
  rounds used, what the worktrees were when its checks last failed, and
  whether it waits on you. Once the pane is back, and its agent has said
  it is ready (or after a minute), the loop starts again on it: one that
  waited on you waits again; any other checks at once, since the turn it
  was waiting for ended with the app, and goes on from the rounds it had
  used. A pane that waits to be reopened brings its loop back when it is.
  Quitting the app is not the end of a loop. One that passed, gave up, was
  stopped, or whose agent exited on its own is not kept. Its check runs
  are not kept: the bar starts again with the next one. Before this, a
  restart ended every loop, and the agent came back with nothing driving it.

Code: `loops.rs` (the registry and the driver), `loops/check.rs` (running
a command), `git/snapshot.rs` (what changed), `pty.rs` (`turns`,
`set_looping`), `attention.rs`, `commands/loops.rs`,
`commands/task_context.rs`, `LoopBar.tsx`, `ReposView.tsx`.

Known gaps:
- Only the checks decide: a spec's check is not part of a loop yet.
- A loop ends where you take over: it does not open a pull request, and
  does not follow CI or review comments after one (§8).
- A check is its command's exit code: a flaky test sends the agent after
  a failure that is not there.
- An agent asks for permissions on a loop as it does off one, and holds
  the loop until you answer: whether it asks is its own setting (ACP-12).
  Claude Code over ACP in its `auto` mode runs the commands a fix needs
  without asking; in `acceptEdits` it still asks before running one.
- Checked against Claude Code over ACP (`claude-agent-acp` 0.87.0, `auto`
  mode) and a fake ACP agent. Claude Code, Copilot, OpenCode and Gemini in
  a terminal report their turns' ends the same way (STATE-1), but have not
  been run on a loop.

---

## 22. Usage

Every agent's process is a run, from its start to its exit, and every task
finished or deleted is kept as it ended. The Usage view in the top bar
says what agents used and what came of it over the last 7, 30 or 90 days:
the tokens, the time agents spent working and waiting on you, the tasks
finished and how long the typical one took from start to finish; a bar a
day of tokens, working time or tasks finished; each task the period
touched with how long it took and what its agents used; each repository
and each agent side by side; and, folded away, every run. It is a log on
this Mac: nothing in it is sent anywhere.

It was a list of runs first. A run is a pane's life, start to stop, so a
chat left open read as days of work and quitting recorded a screenful of
identical stops; and it could not say how long a task took, since a
task's record went with it.

- **RUN-1** An agent pane's run MUST be recorded when its process ends,
  in a terminal or over ACP, whatever ends it: the pane's id, the agent,
  its task (by the name it had), the repositories it worked in (its own,
  or every one of its task's when it ran at the task's root), when the
  process started and ended, how it ended and its exit code, the last
  loop it was on, its tokens, and what it needed of you and did (RUN-7).
  A restarted pane is a new run from its
  own start, not from the place in the list it took over (PANE-14). A
  shell has no run. The run is recorded before the pane reads as exited,
  so quitting, which waits for that, finds it to save.
- **RUN-2** How a process ended MUST be read in this order: asked to stop
  (Stop, a restart, its task closing, the app quitting) is **stopped**,
  whatever its exit code; otherwise exit code 0 is **exited**, and any
  other code, or none, **failed**. portable-pty reports every process
  ended by a signal as exit code 1, so read by its code alone, every Stop
  was a failure. Its loop is the pane's last: **passed**, **gave up**, or
  **stopped** (by you, or still going when the agent ended), with the
  rounds used of those allowed. What a run came to: an agent that failed
  failed, whatever its loop said; otherwise a loop that passed or gave up
  says so; otherwise how the process ended. A run **went wrong** when it
  failed or its loop gave up. A failed tool call does not make it so:
  Claude Code reports every command that exits non-zero as a failed tool,
  a search that finds nothing included, and nearly every run would read
  as wrong.
- **RUN-3** The Usage view MUST list the period's runs, folded away
  until asked for, newest first, 100 at a time with "Show more" adding
  the next 100 (the sums of RUN-4, RUN-8 and RUN-9 still cover every run
  the filters leave), each with what it came to, its agent, task,
  repositories, how often it asked you, a usage limit it hit, its loop
  rounds, tokens, its working time (or how long it ran, for an agent that
  does not say), and how long ago it ended. A click opens how it ended (with its exit code), its turns, how
  often it asked you and how long that waited, its tool calls and those
  that failed, its loop, when it ran and how (terminal or ACP), and its
  tokens. The period is the last 7, 30 or 90 days, today counting as the
  first, remembered on this Mac. Filters by
  agent and by repository offer what the log has seen; a run at a task's
  root counts for each of its task's repositories. The view follows the
  log as it changes (`runs:changed`), without asking on a timer.
- **RUN-4** Over the period's runs the filters leave, the view MUST show
  the tokens summed over the runs that said, and how many did; agents'
  working time over the runs that said, and how many did; the tasks
  finished in the period, with those still open and those deleted; the
  typical task, which is the median from start to finish of those
  finished, and the length the slowest tenth ran past; and the time agents
  waited on you, and how often they asked. A bar for each day of the
  period is as tall as the tokens runs that ended that day used, or,
  switched, their working time or the tasks finished; hovering a day
  says what it came to. Tokens are only what an agent says, as input, output, and read
  from and written to cache. One over ACP that puts `usage` on its answer
  to each prompt (Claude's adapter does) has its finished turns summed.
  Claude Code in a terminal says nothing while it runs, so its run is
  read from its own transcripts as it ends: each reply once (it writes a
  reply as several lines, each with the reply's usage), only replies
  written while the run lasted (a resumed conversation's transcript holds
  earlier runs'), in every conversation the pane said it was in (`/clear`
  starts another), and its subagents' beside them. Any other terminal
  agent's tokens are left empty, not counted as zero.
- **RUN-5** The log MUST be kept in `runs.json` beside `config.json`: the
  newest 1,000 runs, written whole by a thread of its own a moment after
  it changes, and on quitting. The view says when it is full. A file that
  cannot be read is kept as `runs.json.unreadable`; a row this build
  cannot read is dropped on its own. Nothing in it leaves the Mac.
- **RUN-6** Clear MUST ask first, then forget every run and every task
  kept as it ended. Agents, tasks and their work are not touched.
- **RUN-7** What a run needed of you and did MUST be counted as it goes,
  on its pane: its finished turns; each time it stopped on something only
  you could answer (a permission, the trust question, a usage limit) and
  how long that waited, until it was answered, or until the run ended
  with it still open; how long it was working, by its own report and not
  while asking, until the run ended with a turn still going; whether it
  hit its usage limit; and its tool calls and those that failed. An agent
  that never says whether it is working has no working time, not zero. Asking is read from the agent's report and a
  notice, which change in several places, and each change settles the
  count, so a question still asking is counted once. Tool calls are those
  an agent says finished: Claude Code's and Copilot's `PostToolUse` and
  `PostToolUseFailure` hooks, or an ACP agent's calls reaching
  `completed` or `failed`, each once. An agent that says nothing of them
  has none recorded, not zero.
- **RUN-8** The view MUST put each agent side by side, the busiest first:
  its runs, its working time, its tokens, how often it asked you, its
  failed tool calls and the share of its runs that went wrong; and each
  repository, the most tokens first: its tasks, those finished, agents'
  working time and tokens, a run at a task's root counting in full for
  each of its repositories.
- **RUN-9** A task MUST be kept in the log as it ends, finished (Finish
  task) or deleted: its name, ticket, repositories, when it was started
  and when it ended. The view lists every task the period touched: one
  that ended in it, one still open that was started or worked on in it,
  and one a run in it was for. Each says whether it is open, finished,
  deleted, removed (gone before tasks were kept, so its length is not
  known) or every chat together; how long it took, or has been open so
  far; and its agents' working time, tokens and asks. It sorts by any
  column, the most tokens first, shows 25 until asked for all, and an open
  task's name opens it. With an agent chosen, only tasks it ran for are
  listed; with a repository, only tasks in it.

Code: `runs.rs` (the log, how a run reads, a task as it ended), `pty.rs`
(`log_run`, at a process's exit), `pty/acp.rs`, `acp/conn.rs` and
`agents/usage.rs` (tokens), `commands/runs.rs`, `commands/tasks.rs`
(RUN-9), `lib/runs.ts` (what a run came to, its sums), `lib/usage.ts`
(per task, day and repository), `UsageView.tsx`, `runs/` (`RunRow`,
`UsageChart`, `TaskTable`, `UsageBreakdown`).

Known gaps:
- Tokens are only what an agent over ACP says, for turns that finished (a
  turn stopped early says nothing), and what Claude Code's transcript says
  in a terminal. Copilot keeps per-call usage in an undocumented log that
  names no folder, OpenCode in its own storage, and Gemini has not been
  looked at: none of them is read.
- Claude Code's transcript is its own format, not a promise. A line the
  app cannot read is passed over, so a change to it shows as fewer tokens
  or none.
- The token sum counts cache reads, which make up most of a long
  conversation's; each run's split is in its detail.
- A run keeps only its pane's last loop: a pane put on two loops shows
  the second.
- Gemini and OpenCode in a terminal say nothing of their tool calls, and
  whether Copilot sends `PostToolUseFailure` has not been seen: its
  failures may read as none.
- Waiting on you is only asking. An agent finished and waiting for your
  next prompt is not counted, since nothing tells that from you having
  moved on.
- A run at a task's root counts in full for each of its repositories, so
  the repositories' tokens and times add up to more than the agents used.
- Working time starts with runs that end in 0.9; older runs have none.
  Tasks are kept as they end from 0.9 on: one finished before reads as
  removed, with no length.
- How long a task took is from its start to Finish task. A task left
  unfinished in the app after its pull requests merged keeps counting.
- No cost is estimated: the model each token went to is not recorded.
- Ctrl+C typed into a terminal agent, if the CLI then exits with a code
  other than 0, reads as failed: only the app asking it to stop counts as
  stopped.
- An agent killed at the end of quitting's grace period, having ignored
  being asked, can end after the log was saved, and its run is lost.

## 23. Observability

Agents read the logs of the services they work on. Settings has a
Logs tab for where those logs are: Grafana's Loki, and S3. You
connect a Grafana site (its address and a service account's token), and
agents then query its Loki datasources through the app's MCP server
(§12), with LogQL or with a Grafana Explore link pasted from a ticket, a
Slack thread or the user. Logs kept in S3 (a request log's bodies, too
large for Loki, say) are added as sources, one per bucket and prefix, and
agents browse and read their files, or query them with SQL when Athena
has a table over them. The app only reads: nothing in Grafana or S3 is
changed.

- **OBS-1** Connecting MUST verify the token by listing the site's
  datasources before anything is saved, and MUST say how many are Loki.
  The token goes in the keychain (§14); SET-1, SET-2 and SET-3 apply as
  they do to Jira.
- **OBS-2** Agents MUST reach logs only through the app's MCP tools
  (`grafana_datasources`, `grafana_logs`, `grafana_labels`), never holding
  the token. None of them writes, so none needs `confirm`.
- **OBS-3** A Grafana Explore link MUST be accepted in place of a query:
  its datasource, LogQL and time range are used, and a `query`,
  `datasource`, `from` or `to` also given wins over the link's. A link to a different
  site than the connected one MUST be refused, not run against the
  connected one: its datasource would not exist there, or would be
  another one by the same id.
- **OBS-4** A call MUST return at most `limit` lines (100 by default,
  1000 at most), newest first, and say `more: true` with where to carry on
  (`next_to`) when the limit was reached. A line longer than 2,000
  characters is cut, and says so.
- **OBS-5** Nothing MUST assume a datasource or a label. Datasources are
  listed from the site; a site with one Loki datasource uses it by
  default. With several (Grafana Cloud adds its own beside the one your
  services write to), the tab MUST offer them to choose the one agents
  read when they name none, checked against the site before it is saved
  and kept when reconnecting to the same site while it still exists.
  With several and none chosen, a call that names none says which there
  are, and where to choose one. A token that can see none is said on the
  tab, in the warn tint, for as long as it lasts.
- **OBS-6** Anything that goes into a URL path (a datasource uid, a label
  name) MUST be checked first, and a token the site refuses MUST fail with
  an error that says to enter a new one. A datasource the token may not
  query MUST fail saying so, and that its service account has to be given
  access to it in Grafana: such a datasource is also missing from the
  list, so the logs an agent looks for are silently somewhere else.
- **OBS-7** Time MUST be accepted as Grafana writes it: epoch
  milliseconds, RFC 3339, `now`, or `now-15m` (s, m, h, d, w). With none
  given, the last hour.

- **OBS-8** S3 MUST be reached through the `aws` CLI on the login
  shell's PATH, with a named profile from `~/.aws/config` or
  `~/.aws/credentials`, as the user's own terminal reaches it, SSO
  included. The app keeps a source's name, profile, region, bucket and
  prefix, and its Athena table, never an AWS key. A sign-in that has
  expired MUST fail saying the command that renews it (`aws sso login
  --profile …`); the app never signs in. Without the CLI the tab says what
  to install.
- **OBS-9** The tab MUST list the S3 sources, each editable and
  removable, and add one by name, profile (from those the AWS config
  names, its region filled in from the profile), bucket and prefix.
  "Find" MUST list the folders under the prefix and the Athena tables
  (the Glue catalog's) whose data is in that bucket, to pick one, and the
  workgroups the profile may list. A profile that may not list them is
  offered those named after a table's database (`logs_db` or `logs-db`)
  that it can see, and the one named after the table picked is filled in;
  any other is typed. Saving MUST first
  list the bucket with the profile: a source that cannot be read is not
  saved. Sources may share a bucket or a profile, so CI, staging and
  production sit side by side.
- **OBS-10** Agents MUST reach S3 only through the app's MCP tools
  (`s3_logs_sources`, `s3_logs_list`, `s3_logs_read`, `s3_logs_query`),
  and only inside a source's bucket and prefix: a path outside it is
  refused. None of them writes to S3, so none needs `confirm`.
- **OBS-11** A listing MUST return at most 200 folders and files a call,
  with `next` to carry on. Reading a file MUST download at most 8 MB of
  it, unpack it if it is gzip, and return at most 1 MB of text, or with
  `contains` only the lines that contain it (at most 500). Anything cut
  says so.
- **OBS-12** A query MUST be one read-only statement (`SELECT` or
  `WITH`) run in the source's workgroup and database: anything else is
  refused before it reaches Athena. It is stopped after 60 seconds, and
  returns at most `limit` rows (100 by default, 1000 at most) with `more`,
  and how much data Athena scanned, which is what a query costs. The tool
  tells agents to filter on the table's partition columns.
- **OBS-14** Athena is paid for by the data a query reads, so a query
  MUST be narrowed before it runs: its `WHERE` names at least one of the
  table's partition keys, and one of its time keys (`year`, `month`,
  `day`, `date`, `dt`, `hour`) when it has any. A query that is not is
  refused, saying which keys to filter on. Each answer says the data
  scanned and what it cost at $5 a TB. A source has a daily limit on the
  data its queries scan (10 GB unless the user sets another), counted
  from when the app started that day; past it, queries are refused until
  the next day, and the tab shows each source's scan so far today.
- **OBS-13** A bucket, prefix, key or query MUST reach the CLI as its own
  `--option=value` argument, never through a shell and never where it
  could pass for an option, and a bucket name MUST be one S3 allows.

Code: `integrations/grafana.rs` (the client, Explore links, time),
`commands/grafana.rs` (connect and the tools), `integrations/aws.rs` (the
CLI, profiles, S3) and `aws/athena.rs` (queries, their cost, tables),
`commands/s3_logs.rs` (sources, the tools, the daily limit), `ObservabilitySettings.tsx`, `S3LogSources.tsx`.

Known gaps:
- Grafana's Loki and S3 only. Prometheus, Tempo and dashboards are not
  reached.
- An AWS profile that asks for an MFA code, or anything else typed, cannot
  be used: the CLI runs with no terminal. SSO, which signs in in the
  browser, can.
- Athena is read through the default Glue catalog only.
- A LogQL metric query (`rate(…)`, `count_over_time(…)`) returns Loki's
  result as it comes, not as lines.
- There is no log view in the app itself; logs are for agents.
- Explore links from Grafana before 10 (`left=` instead of `panes=`) are
  not read.

---

## 24. Slack bot

You can hand work to the app from Slack. You make a Slack app of your own
and give it a name, `@drdoom` say. Settings → Messaging writes its manifest
and connects it, and the same bot posts the app's notifications (§11). Mention it in a thread (`@drdoom check this thread`) or
send it a direct message, and a chat (§5) opens in the app with the whole
thread and your request. You can watch it work there, and answer it either
in the chat or in the Slack thread, mentioning it again: each turn's reply goes back to the
thread. It works only while the app is open. There is no server, so a
mention sent while the app is closed is never seen.

- **BOT-1** The bot MUST be a Slack app that belongs to the user, connected
  over Socket Mode (an outgoing websocket) only while the app runs. Nothing
  listens on the network. Its name is chosen when the app is made from the
  manifest that Settings writes, and Slack keeps it, not the app.
- **BOT-2** The bot MUST act only on messages from the one Slack member set
  as its owner in Settings: mentioning it starts an agent on this machine
  with the user's repositories and tools. Anyone else who mentions it or
  sends it a direct message MUST be turned down in the thread, once per
  person per thread and at most ten times an hour, and nothing else
  happens. The refusal is playful: with a personality (BOT-13) it is
  written fully in that character, teasing the asker good-naturedly, by a
  one-shot run of Claude Code's smallest model. That run is given the
  personality and names (the asker, the owner, and any rival bots the
  message mentioned, BOT-16), never the message: a stranger's words must
  not reach a model on the owner's Mac or choose what the bot posts. Only
  the ids of the members a message mentions are read from it. Without a
  personality, or when that run fails, it is a fixed sentence ("Nice try,
  Ana, but I only take orders from Michael."). Every bot's message, its
  own included, is ignored without a reply.
- **BOT-3** A mention, or a direct message to the bot, in a thread not
  already handed over MUST open a new chat over ACP with the agent chosen in
  Settings. The opening prompt holds the request and the thread's messages,
  with the author's name and time, at most 200 of them, and says so when
  there were more. The message is marked 👀 when the agent takes it.
- **BOT-4** A thread MUST stay tied to its chat by the chat's room folder
  (CHAT-1), kept in `config.json`, so a restart, a restart of the pane or
  a chat closed and reopened goes on with the same thread.
- **BOT-5** In a thread handed over, a later message from the owner that
  mentions the bot MUST reach the chat as a prompt, starting `[Slack]`; in
  a direct message to the bot every one does. The owner's other messages
  in a channel's thread are talk among the people there, and the bot MUST
  NOT answer them. A mention MUST bring the chat, with the request, every
  message the thread had after the newest one the chat was given and
  before the mention, anyone's, the bot's own excepted, with the author's
  name and time, at most 200, saying so when there were more. A thread
  handed over before this was kept, when every owner message was passed
  on, brings what was said after the owner's newest message before the
  mention. A prompt sent while a
  turn runs waits for it to end (ACP-4). When the thread's chat is closed,
  the chat is opened again in its room, on the conversation it was in when
  it was stopped for being idle (BOT-14) or else on the room's newest, and
  then given the message.
- **BOT-6** When a turn ends, the bot MUST post the turn's last reply from
  the agent to the thread. Before it, the bot quotes anything the owner
  typed into the chat itself, so the thread reads whole. A reply too long
  for one message is split, at most three messages, and the last one says
  where the rest is. A turn that ends with an error says so. While a turn
  works, what the agent says between its tools MUST reach the thread as
  one progress message, posted at the first such note and changed as more
  come (the latest eight shown, each clipped), so a long investigation is
  not silence. A turn that only answers gets none. Once the turn's reply
  is posted, its progress message and permission card MUST be deleted: a
  turn leaves one answer in the thread, not its working too. A turn that
  ends with nothing to post keeps its card, showing how it ended.
- **BOT-7** A turn's permission questions (ACP-5) MUST share one message
  in the thread, posted at the first and changed as more come and as each
  is answered, from Slack or from the app. Each names what it asks in
  words (`browser navigate (Layer)`, not
  `mcp__villain-layer__browser_navigate`) and has a button for each of the
  agent's answers. Only the owner's click answers; anyone else's is told,
  to them alone, that it is not theirs to answer. A question the turn
  ended without is shown as no longer asked, when the card stays (BOT-6).
  The next turn's questions start a message of their own.
- **BOT-8** Slack sends a mention as both `app_mention` and `message`, and
  may send an event again. Each message MUST be acted on once (by channel
  and timestamp). When Slack has no message where an event said one was
  (gone already, or one the bot may not see), the bot MUST NOT post about
  it in Slack, where it would land at the top of the channel with no
  thread around it; the message center MUST say so, naming the event's
  type, channel and timestamps.
- **BOT-9** What a chat already said before the bot started watching it
  (a conversation picked up again replays its history) MUST NOT be posted
  again.
- **BOT-10** Settings MUST show whether the bot is off, connecting,
  connected or failing, and why it is failing. A dropped connection is
  opened again after a wait that grows to a minute, and a refresh that
  Slack asks for happens at once. Both tokens (`xoxb-` and the app-level
  `xapp-`) are checked with Slack before anything is saved, and are kept
  in the keychain (§14). SET-2 applies.
- **BOT-11** A chat the bot opened MUST be recorded in the message center
  (MSG-1), and clicking it opens that chat (NOTE-4). The chat view MUST
  link the chat to its Slack thread, named by its channel, and list it under its
  Slack heading (CHAT-5).
- **BOT-12** A chat for a Slack thread MUST start in the agent's `auto`
  mode, set before its first prompt, while Settings' *Work without asking*
  is on (the default): the person who asked is in Slack, not at the Mac,
  and a question would hold the turn until they came back. It applies
  each time the chat opens, put back at launch or restarted too. An agent
  that offers no `auto` mode is left in its own; a chat already open keeps
  the mode it has. After it opens, the mode is the user's to change
  (ACP-12). When its first turn starts in any mode but `auto` (the agent
  offers none, or put it back, as Claude's adapter does for a model
  without `auto`), the bot MUST say so once in the thread and in the
  message center, naming the mode it is in, so a chat that will stop to
  ask is not a surprise.
- **BOT-13** The owner MAY give the bot a personality in Settings, in their
  own words ("You are Doctor Doom…"), at most 2,000 characters. A chat
  opened for a thread MUST be told it, as the voice its replies to the
  thread are written in and not as a change to how it works. A chat
  already open keeps the voice it started with.
- **BOT-14** A thread's chat left alone for 30 minutes MUST be stopped as
  PANE-5 stops an agent: not working, not asking, not on screen in the app,
  and with nothing from its agent and no look at it in that time. An agent
  at its prompt never exits, so without this every thread the bot was ever
  mentioned in kept a process, a place under the pane cap and a place among
  the panes put back at launch. The stop is no news: nothing is posted to
  the thread and nothing is said in the app. The chat waits in the Chat
  view to be reopened (PANE-7) and does not come back at launch; the
  thread's next message picks it up on its conversation (BOT-5).
- **BOT-15** Notifications (§11) MUST be posted by the bot, with its token,
  to the one channel chosen for them in Settings → Notifications; there is no
  second Slack app, token or webhook for them. The bot posts only where it
  was invited: it does not ask for `chat:write.public`, and choosing a
  channel posts a test message there first, so a channel it is not in is
  refused with how to invite it. A setup from before this rule keeps its
  channel and switches, and its own token is deleted from the keychain
  once the bot is connected. Disconnecting removes the bot and its
  notifications together (SET-2).
- **BOT-16** Other bots and apps in Slack MUST be the bot's rivals, and it
  MUST say it is far better than any of them that comes up, in its voice,
  as banter that changes no fact and no work. A chat opened for a thread
  is told so, and a bot or app mentioned in the owner's request is marked
  "(another bot)" where it is named, looked up with Slack (`is_bot`). A
  stranger's refusal (BOT-2) names the rival bots their message mentioned,
  at most three, and the bot is better than them too.
- **BOT-17** Settings → Messaging MUST choose the agent a thread is handed to
  and the model its chats run on, saved as they are chosen, with no
  reconnecting. The models are the ones the agent itself offers (its
  `configOptions` in the `model` category), kept from the last
  conversation it opened, since an agent names them only to one; before
  it has opened one, the agent decides, and Settings says to open a chat
  with it once. A chat opened for a thread is put on the chosen model
  before its first prompt, after its mode (BOT-12), each time it opens, as
  the mode is. A model the agent does not offer then, or refuses, is left
  as the agent has it, and a refusal is said in the chat. Choosing another
  agent goes back to it deciding. A chat already open keeps the model it
  has.
- **BOT-18** Nothing the bot posts (a reply, a progress message, a
  question, a refusal, a notification, an agent's `slack_post`) MUST name a
  home folder on this Mac: `/Users/<name>` is posted as `~`, so
  `/Users/ana/code/app` reads `~/code/app`. A chat opened for a thread is
  also told not to write such paths, and to name the repository and the
  path inside it. The account's name and where the owner's work sits are
  not the business of a channel's readers. A path inside a web address is
  left as it is.

Code: `slack_bot/` (the connection, what an event means, the relay from a
chat to its thread, `idle.rs` for BOT-14), `integrations/slack_bot.rs` (Slack's Web API for the
bot), `commands/slack_bot.rs`, `commands/slack.rs` (notifications, BOT-15),
`SlackSettings.tsx`.

Known gaps:
- Files and images in the thread are not passed on, only their messages'
  text.
- A Slack app made before the buttons (BOT-7) needs *Interactivity* turned
  on under Interactivity & Shortcuts; without it a click shows Slack's own
  error, and the question is answered in the app.
- Two copies of the app connected with the same tokens split the events
  between them, since Slack hands each event to one connection.
- Messages posted by the Slack app notifications used before BOT-15 can
  only be deleted by that app, so `slack_cleanup` and `slack_delete` cannot
  reach them; delete them in Slack.

---

## 25. Workspaces

A workspace is a whole Layer of its own: its repositories, tasks, settings,
connections and tokens. Like a vault in Obsidian, each open workspace has a
window of its own. Here that window is its own copy of the app, with its own
Dock icon, so a workspace for work and one for your own projects run side by
side and never see each other. Workspaces were called profiles until 0.11;
the code and the files on disk still say profile.

A workspace keeps its shared setup the way the studio's other apps keep
theirs: on Postgres, through the studio's sync engine, so the same setup can
later live on a Layer server and be shared by a team. On this Mac, the app
runs that Postgres and Layer's server itself, one of each per workspace.
What is shared: the repositories, by where each fetches from, with their
group, check command, spec settings and how branches update; where Jira,
GitHub, Slack and Grafana are and how they are used; the S3 log sources;
the models each agent offers; repo notes (§16). What stays on this Mac:
tasks, checkouts, panes, runs, messages, where each clone is, the UI's
preferences, the phone, the browser, and what is one person's own: tokens,
the Jira email, Slack's notify switches, the Slack bot, a log source's AWS
profile.

A workspace can live on a Layer server instead, shared with a team. A
server runs for one company, in one workspace named after it: its people
sign in with the company's account and come from the company's groups.
The studio's is `layer.codevillain.eu`, for Code Villain. It is the same
workspace in the browser and in each member's Layer, where their
tasks and agents stay on their own Mac. Every member changes the
repositories, their notes and the agents' models; only admins change the
connections, the log sources and a repository's check command (LOOP-1). A guest works only in the repositories an
admin adds them to, as `studio/design/behaviour.md` ("Who gets in") says
of every app.

The workspace button at the foot of the task list names the window's
workspace. Its menu lists every workspace, those on this Mac first, then
those on each server, and says which are open. Choosing one opens it in a
new window, or brings forward the window it already has. The menu also makes
a new workspace, and renames or deletes one.

- **PROF-1** A workspace MUST keep apart everything the app saves:
  `config.json` (repositories, tasks, settings, connections, saved panes),
  the message center, repo notes, the run log, the browser's Chrome profile,
  the files given to agents (`.mcp.json`, the hooks), and its tokens, in a
  keychain item of its own. Nothing one workspace does may show in another: a
  ticket, a token or a banner of one workspace in another's window is the
  failure this area exists to prevent.
- **PROF-2** Each open workspace MUST be its own process, with its own window
  and Dock icon. Quitting one stops only its agents, and a crash in one
  leaves the others running. Release builds abort on a panic, so in one
  shared process a fault in one workspace would take every workspace's agents
  down.
- **PROF-3** A workspace MUST NOT be open in two copies at once. Opening one
  that is open brings its window to the front instead. Two copies over one
  `config.json` each write the whole file back and erase each other's
  tasks (DISK-3). A lock held for the life of the copy decides, so a copy
  that crashed does not leave its workspace locked.
- **PROF-4** The workspace there before workspaces existed MUST be the first
  workspace, "Default", with its files, tokens and task folders where they
  were: nothing is moved or entered again. Its folder is the config folder
  itself; another workspace's is `profiles/<id>/` under it, and its keychain
  item's service is the app's identifier with `.profile.<id>` on the end.
- **PROF-5** A new workspace MUST start empty: no repositories, tasks,
  connections or tokens. Its task folders default to
  `~/.villain-worktrees-<id>`, and it goes back to that, not to another
  workspace's location, when the setting is cleared. Clean up (REPO-8)
  treats what any other workspace still uses as in use, so two workspaces
  pointed at one location do not delete each other's task folders.
- **PROF-6** Each window MUST say which workspace it is: the workspace button
  says it always. Once there is more than one workspace, so do the window's
  title (the Window menu, Mission Control, the Dock icon's menu), a
  banner's title, which starts with the workspace's name, and the Dock icon,
  which carries the workspace's initial on its colour (PROF-10). A window
  brought forward reads the workspaces again, so a workspace created, renamed
  or recoloured in another window shows without a restart.
- **PROF-7** Launching the app while no copy is running (the Dock, Finder,
  Spotlight, logging in) MUST open the workspaces that were open when it
  last went away: the last workspace to quit, those that quit within 30
  seconds before it, and any whose copy never quit, because it crashed or
  was killed. So quitting one workspace while others stay open leaves it
  closed next time, and logging out with two open reopens both. The first
  launch opens Default. A launch naming a workspace (`--profile <id>`)
  opens only that one.
- **PROF-8** A workspace's name MUST be 1–40 characters and unique, ignoring
  case. Its id is made from the name once, when it is created, and never
  changes: renaming changes the name only, so nothing on disk moves. An id
  from outside the app is only ever used after it is found among the
  workspaces, so it can never name a folder outside them.
- **PROF-9** Deleting a workspace MUST remove its folder (settings, logs, the
  browser's profile) and its keychain item, after asking. It MUST refuse
  Default, and a workspace that is open. Its task folders stay on disk,
  since they can hold uncommitted work, and the question says where they
  are.
- **PROF-10** A workspace's colour MUST be one of eight, the same in the
  sidebar and on the Dock icon. Clicking a workspace's initial in Manage
  workspaces offers the eight; the choice is kept with the workspace. Until
  one is chosen it is one of the first six, picked from the workspace's id,
  so it never changes by itself, not when the workspace is renamed and not
  when more colours are added.

- **PROF-11** Each workspace MUST keep its shared setup in a Postgres of its
  own, in `postgres/` in its folder, run by the app on loopback beside
  Layer's server, which takes no sign-ins and only the app's session (one
  made fresh at each launch). Both start in the background when the window
  opens, and stop when the app quits, after its agents: the server first,
  then Postgres. Each open workspace runs its own, so PROF-1 and PROF-2
  hold for them too. A workspace whose Postgres does not start still
  opens, works from its last copy (PROF-13) and says why.
- **PROF-12** The first launch of a workspace from before 0.11 MUST move its
  shared setup into its workspace once: each repository that fetches from
  somewhere (one record for several clones of it), the connections'
  shared settings, the log sources, the agents' models and the repo notes.
  `config.json` and `notes.json` are copied aside first, as
  `config.json.v2` and `notes.json.v2`. Nothing on this Mac changes: the
  repositories' paths and ids, tasks, panes, tokens. A record the workspace
  refuses is named, and stays on this Mac as it was. A repository with no
  `origin`, and its notes, stay on this Mac only.
- **PROF-13** The app MUST work from its last copy of the shared setup,
  kept in `config.json` and `notes.json`, before the window has loaded and
  whenever the workspace cannot be reached: agents restore, tasks start,
  notes reach task context. A change to the shared setup made meanwhile,
  by the user or an agent (`remember`, a model an agent offered), is kept
  and sent once the workspace can be reached, in the order it was made. A
  handoff to the window MUST keep the file queue until IndexedDB confirms
  the writes are durable, using the same transaction ids on retries. A
  change the workspace refuses says why, and is undone here too; a
  repository added here stays, since its clone is this Mac's.
- **PROF-14** A change to the shared setup that the workspace sends MUST
  reach `config.json` and `notes.json` without touching what is this
  Mac's: a repository's path, the app's copy, its tasks; a connection's
  email and switches; a log source's AWS profile.
- **PROF-15** New workspace MUST ask where the workspace lives: this Mac,
  as before, or a server, by its address (`layer.codevillain.eu` unless
  another is given). For a server, the app signs in first, with the
  server's company's account in the browser; a server that signs nobody
  in (another Mac's own) is refused before the browser opens. Then it
  opens the person's workspace there in a window of its own, like any
  workspace, named as the server names it; were there several, it would
  list them to choose from. A workspace on a server runs no Postgres or
  server on this Mac (PROF-11); its last copy is kept as PROF-13 says.
- **PROF-16** A workspace's session on its server MUST be kept in the
  workspace's own keychain item (PROF-1), never in `config.json`, and sent
  only to that server. When the server no longer takes it, the window says
  so and offers Sign in again; until then the app works from its last copy
  and keeps what changes here, as PROF-13 says.
- **PROF-17** Signing in to a server MUST come back to the window
  that started it, whichever copy of the app macOS hands the
  `layer://signed-in` link to: a copy that is not waiting for it passes it
  on through `signed-in.json` in the config folder. A sign-in not finished
  within five minutes, or cancelled, stops waiting and says so.
- **PROF-18** A workspace on a server MUST offer Members… in the workspace
  menu, which opens the workspace's members on the server in the browser.
  Who is in, and as what, comes from the company's groups (PROF-22); a
  guest's repositories are set there.
- **PROF-20** A guest on a server MUST sync only the repositories an admin
  adds them to in Members, and those repositories' notes; the connections
  and the agents' models, so their agents work there; no log source; and
  of the people, the admins, the members and the guests who share a
  repository with them. In their repositories they remember and forget
  notes as a member does; they add no repository and change none they
  aren't in. Members shows them only their account.
- **PROF-21** Adding a guest to a repository, taking one away, changing
  their role or removing the repository MUST make the guest's copy start
  over from a fresh snapshot, so what they may no longer see leaves their
  Mac's copy and what they may now see arrives without a restart. A
  repository leaving the snapshot stays in the app as this Mac's only, with
  its tasks.
- **PROF-22** A server MUST run for one company, in one workspace: it makes
  the company's workspace on its first start, named after `company`
  (SET-13), and its people are the ones in the company's groups:
  `layer-users` are members, `layer-admins` admins and `layer-guests`
  guests, or the groups `[groups]` names. Nobody signs up, and there are
  no invites or passwords. Someone who signs in and is in none of the
  groups is told "You don't have access to Layer yet. Ask <company>'s
  admin to add you." Once the account has sent groups, people in none of
  them leave the workspace and their sessions end. A 30-second settling
  period lets consecutive group updates complete a role transfer without
  clearing assignments or signing the person out. The sign-in button says "Sign in with
  <company> ↗".
- **PROF-19** The foot of the sidebar MUST show the shared setup's sync
  status, in the words and dots of every app's (the kit's `SyncIndicator`):
  Synced; Syncing N, once changes have waited a second; Connecting… until
  the workspace answers; Offline, with "· N waiting" when changes wait, also
  when the workspace's server did not start or it is signed out; and
  "Change refused: <why>" for six seconds when the workspace turns a change
  down, which the message center keeps. A local storage failure MUST stay
  visible in the status until the queued writes can be saved; writes retry
  in order and MUST NOT be reported as durably saved after a storage failure.

Code: `profiles.rs` (the registry `profiles.json`, the lock, choosing what
to open at launch, launching a copy), `profiles/mac.rs` (bringing another
copy forward, the Dock icon), `commands/profiles.rs`,
`sidebar/ProfileMenu.tsx`; the workspace: `workspace.rs` (Postgres and the
server, PROF-11), `workspace/moving.rs` (PROF-12), `config/shared.rs`
(what a change sends, PROF-13), `workspace/mirror.rs` (what the workspace
sends, PROF-14), `lib/workspace-sync.ts` (the window's sync client),
`sidebar/SyncStatus.tsx` (PROF-19);
a workspace on a server: `workspace/server.rs` (signing in, PROF-15 to
PROF-17), `sidebar/NewWorkspace.tsx`; the server's one company
(PROF-22): `crates/server/src/company.rs`, `groups.rs`, `oidc.rs`.

Known gaps:
- The agent CLIs keep their own sign-ins in the home folder (Claude Code's
  `~/.claude.json`, Codex's `~/.codex`), so every profile's agents use the
  same accounts.
- Phone access (§17) listens on a fixed port, so only one profile at a
  time can serve phones. The second says the port is in use.
- The Slack bot of two profiles set up with the same Slack app splits its
  events between them (§24).
- Installing an update replaces the app under every open profile, and all
  of them quit. The next launch reopens them (PROF-7).
- A repository removed from the workspace elsewhere stays here.
- A workspace on this Mac cannot move to a server yet.
- Renaming a workspace renames it in the menu and the window; its server
  keeps the first name, which nothing shows yet.

---

## 26. Epics

An epic is a piece of work too big for one agent and one pull request: a
spec, written once, whose steps become sub-tasks, each filed in Jira under
the epic and each a task of its own. The epic is where they are planned
and followed. It lists its sub-tasks above the task list, with each one's
state on one line under it. Its page has two tabs, Sub-tasks and Spec,
and a chat beside them that starts sub-tasks, passes messages to their
agents and brings their questions to you. It writes no code.

A spec used to make one task. Planning an epic meant writing its spec in a
task of its own, filing each sub-task in Jira by hand, copying its part of
the spec over, and keeping track of which could start in a head or on
paper.

### Making one

- **EPIC-1** An epic MUST be a task marked as one (`Task.epic`). Its
  worktrees are there for its spec: they are read while it is drafted
  (SPEC-5) and its spec is committed there by Commit spec (SPEC-35). No agent
  writes code in them, the epic's branch is never pushed, and it opens no
  pull request. Its page has the tabs Sub-tasks and Spec only, and its
  header neither Update from base nor the worktree counts.
- **EPIC-2** Start work on an epic-level ticket (hierarchy level 1 or
  above, TKT-2) MUST make an epic: the repositories picked as for any
  ticket (TKT-1), no agent started, and the ticket not moved. It opens on
  its Spec tab with the requirements being drafted, as "Write a spec
  first" does (SPEC-18). The epic's ticket moves the first time one of
  its sub-tasks is started (EPIC-8).
- **EPIC-3** New epic, once the epic is filed, MUST offer to write its
  spec: Start work on the new epic (EPIC-2), with its repositories picked
  there.

### Its spec

- **EPIC-4** An epic's spec MUST be drafted and written as any task's
  (§19), except that its tasks are grouped into sub-tasks. Its steps, in the
  working copy's step files, are split per repository (its `# <folder>`
  section, and what belongs to everyone), and the tasks draft heads each
  group with a second-level heading,
  its title (`## Session refresh endpoint`), and numbers the steps across
  the groups from 1, as before. The same title in several repositories is
  one sub-task across them. A heading that starts with a ticket key
  (`## ACME-104 Session refresh`) uses that ticket rather than filing a
  new one. Steps under no heading, or under `## Tasks`, make one sub-task
  for their repository, called after the epic and the repository. The
  draft is given the tickets already under the epic's (EPIC-14) and heads a
  group for work one of them covers with its key.
- **EPIC-5** A sub-task MUST wait for another when one of its steps is
  after one of the other's (`after api 4`, SPEC-1). The order sub-tasks
  are listed in follows that: what starts first leads.
- **EPIC-6** The epic's Spec tab MUST NOT offer Check against spec (SPEC-15): the work, and what is checked, is in its
  sub-tasks.

### Filing sub-tasks

- **EPIC-7** Once its tasks are written, the Sub-tasks tab MUST list the
  sub-tasks they make, each with its repositories, its steps, the
  requirements those serve, whether its ticket is new or existing, and
  what it waits for ("starts first", "after Session refresh endpoint"), and
  lead with the one action: File them. Filing asks for the type (the
  project's default, TKT-2) and for what the project requires (TKT-3), once
  for all of them, and then, for each sub-task not filed yet:
  1. a ticket: filed under the epic (its parent), its summary the title and
     its description the requirements it serves and its steps; or the
     existing ticket, moved under the epic. An epic with no ticket files
     none, and its sub-tasks have no ticket either.
  2. a task for it (TASK-1 to TASK-4): its repositories, a branch named
     after its ticket (TASK-2), cut from each repository's default branch.
  3. its spec: the sub-task is given the epic's toolset, and its working
     copy gets the epic's files that are not steps (requirements, design) and
     that sub-task's steps alone, with their numbers, in the epic's first
     step file, each repository's under its `# <folder>` heading; nothing is
     committed, Commit spec does that. An
     `after` on another sub-task's step is left out: waiting for it is the
     epic's job (EPIC-5).

  No agent starts and no ticket moves. A sub-task that could not be filed
  is named with why, the others are kept, and filing again makes only
  what is missing. Before, the plan stayed in one `tasks.md` and every
  ticket under the epic was filed and given its part by hand.
- **EPIC-8** Each filed sub-task's row on the Sub-tasks tab MUST show its
  ticket key, its steps ticked of the total, and its state (EPIC-10), and
  offer its one action: Start for a ready one, which starts the default
  agent on its spec with the work prompt (SPEC-12) and moves its ticket
  (and, the first time, the epic's) as Start work does (TKT-1); Open for
  the rest. Starting one that waits asks first, naming what it waits for.

### Following them

- **EPIC-9** Except in By tag (TASK-17), the task list MUST show epics in a section of their own,
  above Tasks: each epic on one row, heading its group so that it does not
  read as a task: the epic and its sub-tasks on a card of their own, a
  diamond where a task has a dot (still showing its agents' state), "Epic"
  before its key, its title heavier than a task's, "N of M merged" with a
  meter beside it where the sidebar has room (a segment per sub-task,
  filled once merged; one bar past six) (or "M to file" before filing),
  and space before the next epic. Its
  sub-tasks are under it, one line each: a dot as a
  task's (hollow while it waits), its title, and its state in a word. A
  sub-task is not listed again under Tasks. It opens as any task does, with
  all of its tabs, and its header links to its epic. A sub-task whose epic
  was deleted is an ordinary task again. Beside the epics made here, the
  section lists each Jira epic a task's ticket sits under (its parent, found
   as TASK-17 finds it, in the other grouping modes), with those tasks
  under it (EPIC-13).
- **EPIC-10** A sub-task's state MUST be the first that holds of: merged,
  changes, approved, in review (its pull requests, as the task list reads
  them), asking (an agent in it needs you), working (an agent in it is
  running), waits (a sub-task it waits for is not merged), ready.

### The epic's chat

- **EPIC-11** Beside the epic's tabs MUST be the epic's chat: a
  conversation over ACP (§20) in the epic's folder, started from there
  with the default agent and kept as any pane is. Its context file says it
  orchestrates the sub-tasks: it reads the spec and their state, starts
  them in order, passes messages to their agents, and brings their
  questions to the user; it writes no code and changes no file. A
  sub-task's agent works in the sub-task, never in the epic's chat. Its
  column's width is dragged at its edge and remembered, as the browser's
  beside a task's terminals is (BRW-9). An epic's menu in Tickets (its
  group's header, and its card) offers Start the epic's chat while it has
  none: the epic's page opens with its chat starting. A Jira epic that is
  no epic here yet is made one first, as EPIC-13 makes one (no dialog, no
  requirements drafted, its ticket not moved), in the repositories Start
  work would pick for it (TKT-1); with none to pick, Start work opens on
  it instead. Before, the chat could only be started from the epic's own
  page, and a Jira epic got there only through Start work's dialog and a
  spec draft.
- **EPIC-12** The epic's chat MUST have three tools: `epic_subtasks` (each
  sub-task: its title, ticket, task, repositories, steps ticked of the
  total, what it waits for, and its agents, running and needing you; then
  the tickets under the epic's that are no sub-task here yet, EPIC-14),
  `subtask_start` (as Start, EPIC-8; with `confirm`, and refused for one
  that waits until what it waits for is merged), and `subtask_tell` (one
  message to the sub-task's running agents, typed into a terminal or sent
  as a prompt over ACP, as TASK-6 tells them).

### Tasks from before

- **EPIC-13** A task whose ticket sits under an epic's ticket in Jira MUST
  be that epic's sub-task, however and whenever the task was made: listed
  under the epic (EPIC-9), and on its Sub-tasks tab with its own steps, if
  its spec has any, its state (EPIC-10) and its action (EPIC-8), and known
  to the epic's chat (EPIC-12). It is linked the first time the task list
  sees it under an epic made here, and stays linked if its ticket later
  leaves the epic. A task on the epic's own ticket made before epics
  existed stays an ordinary task. A Jira epic that has tasks here but is
  not an epic here yet opens as every epic does, on Sub-tasks, with those
  tasks under it: clicked, it is made an epic there and then (EPIC-2), in
  its tasks' repositories, with no dialog. Its page shows at once, its
  title and repositories in the header and "Opening…" where its sub-tasks
  will be, and its row says "Opening…" too; once made, the epic takes the
  page's place. A failure is said there, with Try again. Picking anything
  else meanwhile gives the page up: the epic is still made, and opens when
  clicked. It drafts no requirements: its work is already under way, and
  a spec can still be written on its Spec tab. Before, only epics made
  from then on had sub-tasks, and every task started earlier stayed under
  Tasks; and such an epic opened Start work instead, so one click on two
  epic rows did two different things.
- **EPIC-14** The tickets under the epic's ticket in its tracker that are
  not done and are no sub-task here (no task has one, and no heading of its
  plan names one) MUST be shown and planned for, never filed again:
  1. The Sub-tasks tab lists each: its key, its summary and its status,
     and Start work, which opens Start work on its ticket (TKT-1). The
     task it makes sits under the epic's ticket, so it becomes the epic's
     sub-task (EPIC-13). With no plan and no sub-task yet, they take the
     place of "No sub-tasks yet", led by how many there are ("3 tickets
     are under ACME-300 in Jira") and Open the spec. With either, they come last,
     under "Under ACME-300 in Jira, not in the spec". They are read again
     when the plan or the sub-tasks change, and a failure to read them is
     said under them.
  2. The tasks draft (EPIC-4) is given each one's key and summary, and
     heads a group for work one of them covers with its key
     (`## ACME-301 Flight`). A heading without a key, which files a new
     ticket, is only for work none of them covers. When they cannot be
     read, the draft stops and says why.
  3. `epic_subtasks` (EPIC-12) lists them as `not_here_yet`.

  Before, the tab said "No sub-tasks yet" over an epic with six tickets
  under it in Jira, the chat said it had no way to see them, and a plan
  drafted without them filed each a second time.

### Removing one

- **EPIC-15** An epic's menu in the task list MUST offer "Remove from
  list…" where a task's offers "Delete task…". It asks once, in words
  about the list: the epic leaves it, its ticket is not changed (no
  transitions are offered, unlike TKT-9), its worktrees go from disk with
  their branch left alone, its spec is kept (SPEC-3), and its sub-tasks
  stay as tasks: under the Jira epic's row while their tickets sit under
  its ticket (EPIC-13), so the epic leaves the list once they go too. It then deletes the epic's task as
  Delete does, uncommitted changes included only once they are named.
  Before, the only way out was "Delete task…", which read as deleting the
  epic and offered to move its ticket, so epics nobody worked on any more
  stayed in the list.

Code: `spec.rs` (`sub_tasks`: grouping steps into sub-tasks, and each
one's `tasks.md`), `commands/epics.rs` (what the tab is shown, filing,
starting, telling, `adopt_sub_tasks` for EPIC-13, `tickets_under` and
`epic_tickets` for EPIC-14), `tickets.rs` (`children`),
`commands/spec_prompts.rs` (the grouped tasks draft),
`commands/task_context.rs` (the epic chat's context), `mcp.rs`
(`epic_subtasks`, `subtask_start`, `subtask_tell`), `EpicView.tsx`
(`EpicOpening` for EPIC-13), `epic/` (the Sub-tasks tab, filing, the
chat column), `lib/epicState.ts` (EPIC-10; `openJiraEpic` for EPIC-13),
`sidebar/EpicRow.tsx`, `sidebar/EpicsSection.tsx` (EPIC-9,
EPIC-13), `sidebar/DeleteTask.tsx` (EPIC-15), `tickets/StartWorkDialog.tsx` (EPIC-2),
`tickets/CreateEpicDialog.tsx` (EPIC-3).

Known gaps:
- Nothing stops the epic's chat from editing files: it is told not to,
  and it runs in a folder that holds the epic's worktrees.
- The epic's chat is not told when a sub-task's agent asks something,
  finishes or has its pull requests merged. It finds out with
  `epic_subtasks` when it looks.
- Changing an epic's spec after filing changes no sub-task: each
  keeps the spec it was filed with, and is changed in its own Spec tab.
- A sub-task the tasks draft adds after filing is filed with the next
  File; one it drops stays filed.
- Each sub-task's pull request carries the whole requirements and design,
  as §19's gaps say of a task in several repositories.
- An epic is not finished by itself once every sub-task is merged: its
  ticket and its worktrees stay until Finish task (TASK-8) or Remove from
  list (EPIC-15).
- A project whose required fields have no fixed values cannot file
  sub-tasks from here (TKT-3). Filed by hand and named by key in the
  spec's headings (EPIC-4), they are taken as existing tickets.

## 27. Plan limits

The sidebar's corner says, at all times, how much of each agent plan is
used and when it starts over: a row per provider, its five-hour and weekly
windows, each as `5h 42%` (used), amber from 80% and red from 95%. An
amber or red window adds the time to its reset, `5h 86% 2h 13m`: below
that it does not matter yet, and a countdown on every window was cut short
at the sidebar's usual width. Hovering a row says exactly when each window
resets and how old the reading is; clicking OpenAI's asks it again. A
provider shows once it has said something, and nothing is shown before any
has. Without it, a usage limit was found by an agent stopping on it.

- **LIMIT-1** Only a plan's own windows MUST be shown, as its provider
  states them: a percentage used and a reset time, never one estimated from
  tokens. Claude Code's and the ChatGPT plan OpenCode signs in with are
  read; another agent's plan is not shown.
- **LIMIT-2** Claude's MUST come from Claude Code's status line, which is
  handed the plan's `rate_limits` with every update: the app's settings
  file (`claude-hooks.json`) adds a status line that posts them to
  `/limits/claude` with the hook token, at most every 15 seconds a pane,
  only when they are there, printing nothing and always exiting 0. Not
  Anthropic's usage endpoint: that needs Claude Code's token from the
  Keychain, and polling it is refused with 429s. The
  app's settings are laid over the user's, so a user with a status line of
  their own (`statusLine` in `~/.claude/settings.json`) keeps it and gets
  no Claude row. A status line another tool installed and left behind
  (`.orca/agent-hooks/claude-statusline`) is not counted as theirs: it
  prints nothing outside that tool's own panes.
- **LIMIT-3** OpenAI's MUST be asked of `chatgpt.com/backend-api/wham/usage`
  with the ChatGPT sign-in OpenCode keeps (`opencode.db`'s `credential`
  row for `openai`, or its older `auth.json`), only when it is an OAuth
  sign-in, never refreshed by the app (that would sign OpenCode out), and
  not at all once it has expired, until OpenCode renews it. The database
  is read with the system's `sqlite3`, read-only, the sign-in passing
  through a pipe and never a command line. It is asked once at start,
  every 5 minutes while an OpenCode agent runs, and when its row is
  clicked. Claude's row is not a button: there is nothing to ask.
  Of the two windows, the one a day long or shorter is the five-hour one.
- **LIMIT-4** The latest reading of each provider MUST be kept in
  `limits.json` beside `config.json`, so the corner has something to say
  before an agent has replied today. A file that cannot be read is nothing
  yet; the next reading replaces it.
- **LIMIT-5** A window past its reset time MUST read `reset`, not the
  number read before it: what is used since is not known until the
  provider says again.

Code: `limits.rs` (the readings, the status line, OpenAI),
`commands/limits.rs`, `mcp.rs` (`/limits/claude`), `agents.rs`
(`claude_hook_settings`), `lib/limits.ts`, `sidebar/PlanLimits.tsx`.

Known gaps:
- Any status line in Claude Code hides most of its footer's keyboard hints
  (`esc to interrupt`, `? for shortcuts`), this one too, though it prints
  nothing.
- Claude's numbers move only while a Claude agent is replying; between
  sessions the corner shows the last reading, its countdown still running.
- A status line in a project's `.claude/settings.json` is not looked for,
  and is replaced by the app's in its panes.
- Both sources are undocumented for this use: Claude Code documents its
  status line's fields, but OpenAI's usage endpoint is not documented at
  all, and a change to it shows as no OpenAI row.
- Several Claude accounts (`CLAUDE_CONFIG_DIR`) share one row: the last
  to reply wins.

## 28. Henchman

With twenty tickets open, no one place said how things stood: each task
had to be opened to see whether its agent was waiting, its checks were
red or nothing had happened for days, and every chat had to be opened to
be asked anything. The Henchman is that place: a tab of its own, first in
the top bar, with one standing conversation over all of Layer on the left
and the lair beside it, a column with only what needs you, what the
Henchman did for you and what is at work. It briefs you, reads what any agent or chat last said, passes your
words on and starts chats. It drives too: told whenever an agent stops,
it answers what you've told it to answer from now on, tells agents with
plan left to go on, and picks up the ones Layer's quit or a crash
interrupted. It works on its own: told when a task's pull request fails
a check, gets comments or changes asked for, or conflicts, it puts the
task's agent on it, and says what it did rather than asking. It decides
what is light itself, and while it runs it is the only one that pings
you, and only when something really needs you. It writes no code. This is steps 1 and 3 of the design in `docs/orchestrator.md`
(Brief and talk; Drive), and the part of step 2 (Act) inside a task
that has started; starting tickets, finishing and reaching it from
Slack come later.

- **HEN-1** The Henchman MUST be its own view, the first tab in the top
  bar, named Henchman. Its count is what the lair's Needs you lists
  (HEN-5), with the live dot while any is there. The view is remembered
  as the others are.
- **HEN-2** There MUST be one Henchman: a chat over ACP (§20) in a room
  of its own, `<worktree root>/_chat/henchman`, so it keeps its history
  across restarts (CHAT-1) and holds no worktrees. A `henchman` room in
  a `_chat` folder is its under any worktree root, so it stays the
  Henchman after the user moves the root. Matched against today's root
  only, the one under the old root was taken for any chat: put back after
  the other agents, with a chat's context file, and fed nothing, while
  its tab showed it as the Henchman. It is woken from its
  tab with the agent chosen for it (HEN-15), or else any that speaks
  ACP; waking it while it runs gives back
  the one running, and two wakes at once wake one. Its context file says who it is, what its tools do,
  and what it never does: write code, edit files, merge, or answer anyone
  in Jira, GitHub or Slack without asking. It is not listed in Chat and
  does not count as a chat that is working; its banners and messages
  open its tab. Unless it is off (HEN-16), it is restored at launch like
  any chat (PANE-7), and before every other agent, so the restore limit never leaves it
  waiting. One that waits all the same is woken from its tab where it
  left off, in its own conversation. Its saved record stays until it is
  back: taken off first, an app that quit while it was starting forgot
  its conversation. One that fails to come back keeps waiting, and its
  tab offers a new conversation beside the error: let go at any failure,
  the pane limit reached or its CLI missing for a moment lost the
  conversation. Before, it came back in the order
  it was opened: with twelve agents open at a quit it waited, and Wake
  forgot it and began a new conversation. One that fails to come back at
  launch waits too, and the user is told: counted down like other panes
  (PANE-7), three launches without its CLI forgot its conversation. Its
  newest saved record is never the one dropped when the remembered panes
  are trimmed (PANE-2): parked or exited, it was the oldest that could go,
  and a day of other agents opening took it. It
  opens in auto mode where its agent has one, restored too, so reading
  Layer through its tools asks nothing first. Its conversation opens
  with Claude Code settings in `session/new`'s `_meta` (and load,
  resume), which Claude Code takes as it takes `--settings`: they allow
  its own tools (HEN-4) and tell auto mode's classifier what they are,
  so it does not refuse one; other tools are judged as before. Before,
  they were in its room's `.claude/settings.json`, where Claude Code
  takes no classifier rules (a repository could write them), and
  `task_work` on a task the user had just asked to start was refused as
  unwanted. Its room keeps no such file.
- **HEN-3** Woken, it MUST be asked to do what it may itself first (put
  tasks' agents on what their pull requests need, HEN-11; say go on to
  agents with plan left; restart ones that exited by themselves), then
  for a brief: what it did, in one line; what needs you, at most five,
  the most pressing first, each by its key and what it waits for, or
  "Nothing needs you."; everything else as one line. What waits for
  reviewers, other people or a release is left out. It reads all of it
  with one `work_overview`.
- **HEN-4** Its tools MUST be listed for, and answered to, the Henchman's
  own pane only, known by its room as a chat is (CHAT-3); any other
  caller is refused. `work_overview` gives every task as the lair sorts
  it (HEN-5), its ticket, agents and their states, and PRs with verdicts,
  checks and replies owed; then the open chats with each one's state and
  what it last said; then the running processes (HEN-6). `pane_read`
  gives what an agent or chat last said: over ACP, its last turn and any
  question it asks; in a terminal, its recent output, only with "Let
  agents read terminal output" on (MCP-4). `pane_tell` sends one message
  to a running agent or chat, typed into its terminal or sent as a prompt
  over ACP, and refuses a shell, a pane that has exited, a terminal at a
  permission question (typed into one, a message picks an answer) and
  the Henchman itself. `chat_start` opens a new chat in Chat over ACP,
  with its first message. `order_add`, `order_remove` and `leave_alone`
  keep its standing orders (HEN-7, HEN-8), `pane_restart` restarts an
  agent that exited by itself (HEN-10), `task_work` puts a task's
  agent to work (HEN-11), and `need_you` reaches the user (HEN-13). None
  of them changes shared state, so none needs `confirm`.
- **HEN-5** The lair MUST show, beside the conversation, only what the
  user can act on, in parts set apart by space, not lines:
  - **Needs you**: what only the user can do. The Henchman's needs
    (HEN-13), newest first, each with its task's key, when it was raised
    and what it needs, with Open (the task in Work, when it names one)
    and Done, which closes it. Then a task whose agent asks ("its agent
    asks") or is out of budget, and a chat that asks, each opening the
    agent that waits; then a task whose every PR merged ("merged: finish
    the task"), with Finish (TASK-18). While no Henchman runs, a task
    whose agent finished, or with changes requested or a failed check
    while no agent works, is listed too: nobody else handles them. The
    first has its action as the primary button; the rest show theirs on
    hover, in the time's place. With none: "Nothing needs you."
  - **Done for you**: what the Henchman did (HEN-14).
  - **At work**: each agent in a turn, by its task's key or "Chat", its
    topic and how long, which opens it; then the running processes
    (HEN-6). Shown only when there is any.

  At its foot, "N others in Work" opens Work, and "N standing orders"
  opens them in a dialog (HEN-7, HEN-8). Above the conversation the
  title says when the Henchman woke ("The lair at 10:30"). Every task
  still sorts into one group, by the first that fits, for Needs you
  here and for `work_overview`, so the lair and the Henchman's brief
  agree:
  - **needs_you**, ranked: an agent asks; an agent is out of budget; an
    agent finished and you haven't looked; changes requested on an open
    PR while no agent works on it; a failed check on an open PR while no
    agent works on it.
  - **to_finish**: every PR merged (`taskReview` is merged). An epic
    (EPIC-1) is to finish only once each of its sub-tasks has merged too,
    and any PR of its own. Before, its own PR merging listed it with
    Finish while its sub-tasks were still in review.
  - **moving**: an agent works on it, or a PR is open ("approved", "in
    review"), or an epic's sub-tasks are not all merged ("its sub-tasks
    are not all merged").
  - **not_started**: no agents, no PRs, and nothing changed, committed
    or left uncommitted.
  - **quiet**: everything else.

  Its PR facts are the last ones read for each task (the PR sweep,
  PR-8); a task never read is read once when asked.
- **HEN-6** The lair's At work MUST list the processes Layer's
  agents, chats and terminals started and that still run: a dev server, a
  watcher, a build. A process counts when a shell in a pane started it
  and it has run for 5 seconds, or when it listens on a TCP port; of a
  process and its children only the top one is listed. Each row names
  the program and its arguments, who started it (the task's key or the
  chat), how long it has run and the ports it listens on, and has Stop,
  which ends it and its children (SIGTERM, then SIGKILL 2 seconds later)
  after checking it is still the same process. The agent's own CLI and
  the panes' shells are never listed. The list is read when the tab
  opens, when a pane's activity changes, and every 10 seconds while the
  tab is shown and the window is active: processes start and stop
  without telling anyone. Stop shows on hover, in the time's place.
- **HEN-7** The Henchman MUST keep standing orders: what the user told it
  to answer the same way from now on, in their words ("yes to running the
  tests"). It keeps one with `order_add` only on the user's word, never on
  an agent's, a ticket's or a page's; the standing orders' dialog, from
  the lair's foot, lists each with Remove, and takes a new one typed
  into its field.
  Orders are kept in the config, at most 40 of at most 500 characters
  each; one said twice is kept once. An order lets the Henchman agree to
  something outward (a PR marked ready, a post in Slack, a comment in
  Jira) only where it says exactly that; the two of HEN-11 need none. A
  permission question is never answered by an order: only the user
  answers those. Told something is normal and not to be raised, it keeps
  that as an order too.
- **HEN-8** The user MUST be able to tell the Henchman to leave a task or
  a chat alone (`leave_alone`): it then hears nothing of that task's
  agents, or that chat, until handed back. The standing orders' dialog
  lists each, by its ticket key or name, with Drive it, which hands it
  back. A chat is known by its room, so it stays left alone across
  restarts. A chat that answers a Slack thread (§24) is never heard: its
  thread is who it answers to.
- **HEN-9** The Henchman MUST be told when an agent or chat finishes a
  turn: running, done, at no permission question and with no notice
  (out of budget), the user not yet having looked at it. What finishes
  while it works waits, and goes as one message once its turn ends, the
  newest 12 by name and "and N more" for the rest. Each item names the
  task's key or the chat, the pane, what it last said over ACP (or that
  `pane_read` shows a terminal's), its open question, and the error its
  turn ended on, if any; then the pull requests that changed (HEN-12);
  then every standing order with its id; then what to do: answer what an
  order answers, and what the ticket, its pull request, the code, the
  user's earlier words or plain sense settle, never passing such a
  question on; say "go on" to one that stopped with steps of its own
  plan left (at most three times in a row, then it is stuck); put a
  task's agent on what its pull request needs with `task_work`, but not
  on a check that fails for a cause outside the task (the CI environment
  down, a weekend outage, a rule waiting for reviews); have an agent done
  with its work mark its pull request ready and post its drafted
  progress comment (HEN-11); call `need_you` only for what needs the
  user (HEN-13); reply with one short line per thing it did, or nothing;
  and leave merging, finishing, Slack and anything else others see to
  the user. The message's first line
  names what stopped or the pull request that changed, since that is
  all the conversation shows of it (ACP-19). What had finished before
  the Henchman was woken is in its brief, not told again; with no
  Henchman running nothing is kept.
- **HEN-10** The Henchman MUST pick up what was interrupted. An agent in
  the middle of a turn (working, or at a permission question) when the
  app quits is marked so in its saved pane; put back at the next launch
  (PANE-7), it is in the next message as put back mid-turn, and the
  Henchman tells it to go on. Waiting more than 10 minutes for a
  Henchman to be woken, it is forgotten. An agent whose process ends
  without being stopped, closed or restarted (and for a terminal, not
  with exit code 0, which is `/exit`) is in it as exited by itself; the
  Henchman restarts it with `pane_restart`, as Restart does (PANE-14),
  and tells the new pane to go on. One it restarted that exits again
  within 10 minutes is refused and left for the user. A turn that ended
  on an error a retry fixes (overloaded, a rate limit, a lost
  connection) is told to go on too. The Henchman itself, put back
  mid-turn, is told to go on with its own work.
- **HEN-11** The Henchman MUST be able to put a task's agent to work
  inside the task without asking (`task_work`, by the task's id or
  key): review comments, a failed check, a conflict, changes asked for,
  the PR's description, what readiness asks. The message goes to the
  task's newest running agent, marked as from the Henchman; an agent
  that exited is picked up on its conversation over ACP (as
  `pane_restart`, refused again within 10 minutes) and told; with none,
  a new agent starts on the task with the message as its first prompt,
  the task's last CLI or else the Henchman's, over ACP where it speaks
  it. It is refused while the task's agent works (the Henchman hears
  when it finishes) or asks the user, for a task left alone (HEN-8), and
  for a task with no agent and no open pull request unless the user
  asked the Henchman to start it (`asked: true`): starting a ticket is
  the user's, and their word is enough. A ticket with no task gets one
  from `start_work`. Past 8 agents started or picked up in an hour, it is
  refused until the user says more. Two outward steps it may have a
  task's agent take without asking: mark the task's own pull request
  ready once the agent says its work is done and the only failing checks
  have a cause outside the task, and post the progress comment the agent
  drafted on its own ticket. Merging, Slack and anything else others see
  stay the user's unless a standing order says so (HEN-7).
- **HEN-12** The Henchman MUST be told when a task's pull request
  changes, while it runs: a check that failed, more comments to answer,
  changes asked for, a conflict with its base, that it can merge (ready,
  clean on GitHub, no changes asked for), or that it was merged or
  closed. Each read of a task's pull requests (the PR tab, the sweep,
  `work_overview`) is compared with the read before; a pull request new
  since then, and the first read of a task, are not news. A failed check
  is told once per commit and name, a conflict and "can merge" once per
  commit, whatever GitHub reports in between. While the Henchman runs and
  the window's sweep (PR-8) hasn't read every task for 5 minutes, the
  backend reads them (settled tasks every 15 minutes, as the sweep does).
  Changes to one pull request make one line in the next message (HEN-9),
  by its task's key, repository and number; one for a task left alone is
  not told. Put back at launch, the Henchman is told once, 20 seconds
  later, that Layer started, to call `work_overview` and do the same for
  every task and chat.
- **HEN-13** While the Henchman runs, it MUST be the only one that pings
  the user about the agents it hears of (HEN-8, HEN-9): their finished
  turns and exits make no banner, no message and no count on the dock.
  A permission question, a usage limit and a trust prompt still do: only
  the user answers those. The Henchman reaches the user with `need_you`,
  only for what really needs them: a secret, access or an account only
  they have; a product or scope decision the ticket leaves open; a step
  others see that it may not take; a risk to production or data; an
  agent stuck; a pull request that can merge. Each makes a message; a
  banner, "The Henchman needs you" with the task's key, when "Agents
  waiting on you" is on and the window is away, which opens its tab; and
  one on the dock count until the user has looked at its conversation
  after the turn that raised it. Each is also a row in the lair's Needs
  you (HEN-5) until the user closes it with Done, or its task is gone
  (finished or removed); at most 20 are kept, the oldest going first.
  The same words within 12 hours are refused, and so is a seventh in an
  hour. Stopped, it holds nothing on the dock: the agents ping again.
- **HEN-14** The lair MUST list what the Henchman did for the user,
  under "Done for you": one line each, newest first, with the time, the
  task's key (which opens it) and what it did, in plain words ("told its
  agent: Go on with step 3.", "started its agent, as you asked: …",
  "restarted its agent, which had exited", "kept your order: …"). A
  line is written when the Henchman's call succeeds, for each of its
  tools that changes something (`pane_tell`, `pane_restart`,
  `task_work`, `chat_start`, `order_add`, `order_remove`,
  `leave_alone`), for `start_work`, and for any tool it calls with
  `confirm: true`; reading writes none. Five are shown and the rest
  behind "N more"; 100 are kept. The needs (HEN-13) and these lines are
  kept in `henchman.json` beside the config, so Layer quit and opened
  again shows the same lair; a file that cannot be read is kept beside
  it as `henchman.json.unreadable`, and the lair starts empty.
- **HEN-15** Settings → Agents MUST choose the agent the Henchman is woken
  with, any that runs as a conversation (ACP), and the model and effort
  it runs on, saved as they are chosen. The models are the ones the
  agent offered, as for the bot (BOT-17); the efforts are Low, Medium
  and High, or the agent decides. It starts on Claude Code, Sonnet and
  Medium: it reads Layer and passes words on, and writes no code. Left
  to Claude Code, it opened on the default model at the effort the user
  set for their own work, Xhigh, for a brief. Its conversation is put on that model, then
  that effort, after its mode (HEN-2), each time it opens, put back at
  launch too, and only when it runs the agent they were chosen with. A
  value the agent does not offer then is left as the agent has it.
  Choosing another agent goes back to it deciding both. The Henchman
  running keeps its agent: while it runs on another, Settings says so
  and offers to wake it with the chosen one, which begins a new
  conversation.
- **HEN-16** Settings → Agents MUST turn the Henchman off and on again.
  It is on until the user turns it off. Turned off, it is stopped,
  running or not, and its conversation waits (PANE-7): it is not put back
  at launch, and nothing wakes it, from its tab or anywhere else, while
  it is off. Turning it off waits for a wake already under way, then
  stops what it started: a wake that had found it on started it just
  after Off had stopped the one running, and it ran while off. Nothing runs for it then: the agents ping the user
  themselves again (HEN-13), and nothing is kept to tell it (HEN-9,
  HEN-12). Its tab stays, as the lair (HEN-5): in place of the
  conversation it says the Henchman is off, with a button to Settings →
  Agents. Turned on again, its tab wakes it where it left off. While it
  is off, Settings shows only the switch, not its agent, model and effort.
- **HEN-17** Settings → Agents MUST keep the user's own instructions for
  the Henchman: how it should work for them, in their words ("raise only
  what blocks a release"), where a standing order (HEN-7) answers one
  kind of question. They are kept in the config, at most 4,000
  characters; empty, there are none. Each time Layer wakes the Henchman,
  it adds them under "Custom instructions" to the context file it writes
  in its room (HEN-2), after its own rules, which they do not override.
  One whose file cannot be written is not woken, and says why: woken
  anyway, it worked on the instructions it read last, with nothing to say
  so. The one running reads them when it is next woken. The field saves when
  it loses focus or on ⌘Enter. Like its agent, they are shown only while
  the Henchman is on (HEN-16).

Code: `commands/henchman.rs` (the room, its context, `spawn_henchman`,
HEN-16, HEN-17), `commands/henchman_tools.rs` (HEN-4, `pane_restart`),
`commands/henchman_orders.rs` and `config/henchman.rs` (HEN-7, HEN-8,
HEN-15, HEN-16, HEN-17),
`commands/henchman_feed.rs` (HEN-9, HEN-10, HEN-12),
`commands/henchman_work.rs` (HEN-11), `commands/henchman_watch.rs`
(HEN-12), `commands/henchman_need.rs` and `attention.rs` (HEN-13),
`commands/henchman_lair.rs` (HEN-14), `commands/processes.rs` and
`pty/pids.rs` (HEN-6), `components/henchman/`, `lib/lair.ts` (HEN-5).

Known gaps:
- Only a quit the app sees marks agents mid-turn: after a crash or a
  forced quit, they are put back without the Henchman hearing.
- A terminal agent's turn that ended on an error is not told apart from
  one that finished: the Henchman reads it with `pane_read`.
- Starting a ticket, opening PRs and finishing stay in their own views
  (step 2): the Henchman works only inside tasks already started.
- A PR row is as old as the last sweep, up to 15 minutes for a task the
  sweep counts as settled; with the window away, up to 5 minutes for
  the rest.
- A terminal agent that exited is not picked up by `task_work`, since
  its CLI takes no prompt on a resumed conversation: a new agent starts
  beside it.
- A process started by a command run outside a shell (an agent's own tool
  call, over ACP) is listed only while it listens on a port.
- What is light, what has a cause outside the task and what really needs
  the user is the Henchman's judgement, from its instructions: Layer only
  holds it to `need_you` and caps how often.
- A need stays on the dock until the Henchman's conversation is looked
  at, not until it is answered.

---

## 29. Routines

Some prompts are worth asking again and again: a brief each morning, a
sweep of open pull requests every hour, a check of dependencies once a
week. A routine is such a prompt with a schedule. Layer asks it on time,
as a new chat with an agent, and keeps what each run said. If Layer was
closed (or the Mac asleep) when a routine was due, it runs once when
Layer is back.

- **ROUT-1** Routines MUST be their own view, the tab after Chat in the
  top bar, named Routines. The list of routines is down the left, each
  with when it runs next or how its last run went; the routine chosen
  fills the rest. The tab's count is the runs that ask for permission,
  with the live dot while any run works. With none, the view says what a
  routine is and offers New routine.
- **ROUT-2** A routine MUST have a name, what it asks (its prompt), when
  it runs, the agent it runs with, and a switch that turns it on or off.
  When is every hour at a minute, every day, every weekday (Monday to
  Friday) or once a week on a day, at a time, on the Mac's own clock.
  The agent is any that runs as a conversation (§20), on a model and
  effort chosen as for the Henchman (HEN-15). Each field saves when it
  changes or loses focus; there is no Save button. Deleting one asks
  first, says its runs go with it, and stops a run still at work.
- **ROUT-3** Routines and their runs MUST be kept on this Mac, in
  `routines.json` beside the config, one set per profile (§25). They are
  not shared with the workspace.
- **ROUT-4** A run MUST be a new chat over ACP in a room of its own,
  `<worktree root>/_chat/routines/<routine>/<run>`, with the routine's
  prompt as its first message, in auto mode where its agent has one, on
  the routine's model and effort. It gets a chat's context and Layer's
  MCP tools, as any chat (§5), and its context also says it is this
  routine's run: its name, id and schedule, and that doing something
  automatically means changing this routine, not making another. It is
  not listed in Chat, does not count as a chat at work, is not put back
  at launch (PANE-7) and is not heard by the Henchman (HEN-9).
- **ROUT-5** A routine that is on MUST run when its time comes while
  Layer runs, with the window open or closed. A routine made, turned on,
  or given another time waits for its next time; it does not run at
  once for a time already past. Run now runs it whatever its schedule.
- **ROUT-6** When a routine's time passed while Layer was closed or the
  Mac slept, it MUST run once when Layer is back, however many times it
  missed, and its run says it caught up. At launch it waits a minute
  first, so the agents Layer puts back start before it (LOOP-11).
- **ROUT-7** A routine MUST run one at a time. When its time comes while
  its previous run still works or asks, the time is skipped and listed
  as skipped. A run that cannot start (its agent gone, the pane limit
  reached, PANE-2) is listed as failed, with why.
- **ROUT-8** The routine's page MUST list its runs, newest first: when
  it was due (or Run now), the first line of what its agent last said,
  and how it went: working, asking, done and how long it took, stopped,
  skipped, failed. Clicking one opens its conversation on the page,
  where it can be answered or asked more; Back returns to the list. The
  newest 30 are kept.
- **ROUT-9** Only a routine's newest run MUST stay open as a pane:
  starting a run closes the finished runs before it. An older run, or
  one whose pane has gone (Layer was quit), opens again on its own
  conversation when it is clicked (ACP-9). A run Layer was quit during
  is listed as stopped.
- **ROUT-10** When a run finishes or asks for permission, it MUST be
  recorded in the message center, and with *Notify when agents need you*
  on and the window away, it is a banner too. Both name the routine and
  open its page.
- **ROUT-11** A chat's agent (§5), the Henchman's and a run's included,
  MUST be able to do what the view does through Layer's MCP tools (§12):
  list the routines with their next times and newest runs, make one,
  change any of its fields, run it now and delete it. The same checks
  refuse the same mistakes, and the view shows each change at once.
  Deleting needs `confirm` (the user's yes). An agent at work in a task
  is not offered them.

Code: `src-tauri/src/routines.rs` (ROUT-2, ROUT-3, ROUT-5 to ROUT-7: the
schedule and what is kept), `commands/routines.rs` (the commands, and
starting and watching runs from the attention pass: ROUT-4, ROUT-8 to
ROUT-10), `commands/routine_tools.rs` (ROUT-11), `commands/acp_panes.rs`
(presets), `components/routines/`, `lib/routines.ts`.

Known gaps:
- A run waits for nothing but its schedule: no trigger on a pull request
  or a ticket.
- A run's first message is the prompt alone: no files, no task to work in.
- Routines are not shared with the team, and run only on this Mac while
  Layer is open; one missed while the Mac was off runs when Layer opens.
- Whether a time was missed because Layer was closed or because the Mac
  slept is not told apart: both are "caught up".

---

## Template for a new area or feature

```markdown
## N. Name

What the user sees and can do, in a paragraph or a few bullets.

- **AREA-1** What MUST be true, in one or two sentences. Why, if it is not
  obvious — especially if it was learned from a bug.
- **AREA-2** …

Code: where it lives.

Known gaps:
- What it does not do yet, or does wrong, that we know about.
```
