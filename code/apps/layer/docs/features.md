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
  fetch says git's reason, not only that it failed. In the
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
  clone added by Add repositories that fetches from it does the same.

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
  Each checkout is a subfolder named after its repo. Agents start in the
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
  repo has that branch, and otherwise from its own default branch. Removing one
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
  widest. Hiding the sidebar hides the edge too, and showing it brings
  back the width it had. While that edge, or any grip beside a terminal (BRW-9),
  is dragged, terminals keep their size and are fitted once, when it is
  let go: each fit resizes the agent's terminal, which redraws its whole
  screen for it.
- **TASK-17** The task list MUST group by stage (Tasks, In review, Done:
  the default), by epic, or by repository, chosen from "by stage ▾" on its
  heading and remembered across launches. By epic, a task is under its
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
  every grouping, in Epics (EPIC-9).
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

Code: `commands/tasks.rs`, `commands/worktree_status.rs` (TASK-10), `sidebar/`, `FinishTask.tsx`, `commands/finish_tool.rs` (TASK-18), `lib/drag.ts` (TASK-16), `lib/grouping.ts`, `lib/epics.ts` (TASK-17).

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
every agent pane in every task, with the ones that need you first.

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
- **PANE-2** At most 32 panes run at once (`MAX_PANES`), however many
  spawns race. It is a ceiling, not a setting.
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
  forgotten. Only a conversation the CLI would continue counts: Claude
  Code's one-shot runs (`claude -p`, the app's own PR description drafts)
  do not. Counted, a draft in the task folder had an agent restarted there,
  where Claude found "No conversation found to continue", while its real
  conversation was in the repo's folder. A waiting agent that did not
  record its conversation's id (one not over ACP) resumes the newest
  conversation in its folder, so it stops waiting once an agent of the
  same CLI is open there: that agent already continues it.
- **PANE-8** A terminal MUST keep 256KB of scrollback. One coming back on
  screen gets exactly what it missed, by byte position. One that missed
  more than its scrollback holds is replayed from what it still holds,
  except a full-screen program (on the alternate screen, as OpenCode is):
  its screen and modes are kept, and it MUST be made to draw its whole
  screen again, as a window resize would. Keystrokes echo immediately,
  even while the pane is printing heavily. Output is batched to about 30
  frames a second otherwise.
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
  it starts, so nothing is saved.
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

Code: `pty.rs`, `agents.rs`, `commands/panes.rs` (`restart_pane`),
`Terminals.tsx`, `Terminal.tsx`, `AgentsView.tsx`, `PaneNotice.tsx`.

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

Code: `ChatView.tsx`, `ChatList.tsx` (CHAT-5), `ChatLink.tsx`, `sidebar/SidebarGrip.tsx` (CHAT-4),
`commands/panes.rs` (`open_chat`, `write_chat_context`), `mcp.rs` (`caller_chat`).

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
  read.
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
none is open. It shows checks, the latest review per reviewer, the verdict,
and how far behind the base it is. From here: push everything, open PRs,
retarget a PR onto the checkout's base or take the PR's base for the
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
  most), and the spec's requirements with their check (SPEC-16), and
  streams its answer into the field. Failing that, a running
  agent is asked to write `PR_DESCRIPTION.md` in the task folder.
- **PR-5** Review threads MUST be read in full: every page, resolved and
  outdated flags kept. A verdict is each reviewer's latest decisive review.
  One "changes requested" outranks any number of approvals, and a dismissal
  clears it.
- **PR-6** Feedback to an agent is asked for from a PR's own card, with a
  count of that PR's comments and failing checks; one button for every PR
  gave no way to tell whose they were. The picker shows that PR, and a task
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
  minutes, and its repositories' git is left alone in between; its PR tab
  still asks at once when opened, and opening its PRs from the app puts it
  back in every sweep. The first sweep is silent. After it, a new
  approval, request for changes, new comment or merge is announced as a
  toast.
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
  saved. Saving a token sweeps every task's PRs at once, or straight after
  a sweep already under way, so the refusal does not linger.
- **PR-13** An agent MAY show images in a task's open pull request with
  `pr_images` (after the user agrees): screenshots of a visible change,
  night and day. GitHub has no API for the attachments its own editor
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

Code: `commands/github.rs`, `commands/open_prs.rs`, `commands/pr_images.rs`, `commands/outgoing.rs`, `git/outgoing.rs`, `Outgoing.tsx`, `commands/landed.rs`, `git/landed.rs`, `integrations/github.rs`, `integrations/github/images.rs`, `PrPanel.tsx`, `PrReadErrors.tsx`,
`PrFeedback.tsx`, `Markdown.tsx`, `lib/markdown.ts`.

Known gaps:
- Only the first 100 check runs of a commit are read.
- Legacy commit statuses (Jenkins, older CircleCI) are not read at all.
- A failed checks or reviews request reads as "no checks" / "no verdict"
  rather than as an error.
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
grouped by epic, by status or not at all (TKT-14), with a filter, type toggles
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
  with "Write a spec first", leaves that to the Spec tab, SPEC-18), and
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
  once the task is gone.
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
  | Its account | Tickets (Jira): site, email, token | Code (GitHub)'s token: nothing more to connect | Tickets (Forge): the site, then you sign in to it in the browser, as Forge's own desktop app does |

  Forge names its states per team, as Jira does its statuses, so a state
  is known by its type, never its name. GitHub's issue types are an
  organisation's own, with names and no hierarchy levels, so which one
  is the epic is chosen in Settings, from the organisation's types, never
  guessed from a name. Without them an epic is only known once it has a
  sub-issue: one just filed looks like any other issue.
- **TKT-18** A GitHub issue or a Forge ticket MUST follow the work the way
  its own tracker moves it, not by the app moving it. The task's branch
  carries its key, and each of its pull requests says "Fixes `<key>`" in
  its body. GitHub closes the issue when one merges into the repository's
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

Code: `tickets.rs` (the tracker: `Tracker`; Jira's side in
`tickets/jira.rs`, GitHub's in `tickets/github.rs`), `commands/tickets.rs`
(`set_tracker`), `commands/jira.rs`, `commands/ticket_flow.rs`,
`integrations/jira.rs`, `integrations/jira/flow.rs`,
`integrations/github/issues.rs`, `components/tickets/`, `TrackerChoice.tsx`
(TKT-15, TKT-16), `lib/tracker.ts`, `TicketFlow.tsx`,
`sidebar/DeleteTask.tsx`. The choice is the workspace's `tracker`
connection; `AppConfig::tracker_in_use` reads it, and `jira_client` refuses
every Jira-only path (moves, transitions, the `jira_*` tools) when it is
not Jira.

Known gaps:
- Jira and GitHub Issues are built; Forge is not, and is not offered.
  Nor is "none" (TKT-15): a workspace with no choice uses Jira while it
  is connected.
- GitHub Issues, not yet built: the `github_*` tools for agents (TKT-20;
  the `jira_*` tools refuse, except `jira_issue_types` and
  `jira_create_fields`, which answer for the workspace's tracker); the
  epic chosen from the organisation's issue types (TKT-17), so an epic is
  only an issue with sub-issues, and one just filed as an epic looks like
  any other issue; closing the issue of a task based on another branch
  (TKT-18); reading a pull request's ticket from the issues it closes
  (TKT-21, REV-7, REV-11). Filing offers the repositories of the issues
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
  is the PR's (`owner/name`), on the PR's own branch (TASK-3) and against
  its base, named after its title, with a Jira key in the branch or title
  linked as the ticket when Jira is connected. A task already on that
  branch is opened instead, with the repo added if it lacked it. A PR in a
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
- **REV-10** A PR in "To review" MUST open on GitHub, where it is
  reviewed. Reviewing it here (a checkout of its own, Claude's reading
  plan and findings beside its diff, and the review posted from the app)
  shipped in 0.5.0 and was taken out: the page was the Diff tab for your
  own work with a review laid over it, and it did not read as a review. A
  review checkout left from 0.5.0 lists as an ordinary task, named
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
  limit), a new review request, a new ticket, a ticket the work moved, a
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
| `forget_repo` | unregister a repo (never deletes files) | yes |
| `create_task` | a task with no ticket | |
| `open_prs` | push and open PRs for a task | yes |
| `finish_task` | finish a landed task: agents, worktrees, covered branches, ticket to done (TASK-18) | yes |
| `pr_images` | show image files in a repo's open PR, in a section the app owns (PR-13) | yes |
| `repo_notes` | a repository's notes, with what changed since each was checked (MEM-5) | |
| `remember` | keep a lasting fact about a repository, after asking the user (MEM-1) | yes |
| `check_note` | say a note still holds, as of now | |
| `forget_note` | remove a note that no longer holds, with a reason for the message center | yes |
| `spec_task` | tick (or untick) a numbered step of a repository's spec `tasks.md` (SPEC-13) | |
| `spec_propose` | propose a change to the task's spec, with why, as a draft for the user to approve (SPEC-24) | |
| `epic_subtasks` | an epic's sub-tasks: ticket, task, steps ticked, what each waits for, its agents (EPIC-12) | |
| `subtask_start` | start a sub-task's agent on its spec, and move its ticket (EPIC-8) | yes |
| `subtask_tell` | send one message to a sub-task's running agents (EPIC-12) | |
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

Code: `mcp.rs`, `agents.rs` (per-CLI wiring), `commands/panes.rs`
(`plug_in`).

Known gaps:
- A task folder's `.mcp.json` is rewritten only when an agent starts there.
  A CLI run by hand in a task folder where no agent started this launch
  gets a stale token.

## 13. Settings

| Setting | Default | What it does |
|---|---|---|
| Theme | Nightshade Night | Nightshade Night or Day, another theme, or Match the Mac (SET-5) |
| Interface scale | 100% | 80–160% |
| New reviews and tickets | on | banners while the window is away (NOTE-*); the message center keeps them either way (MSG-2) |
| Agents waiting on you | on | banners and the dock count; the message center keeps them either way (MSG-2) |
| Put terminals back when the app reopens | on | PANE-7 |
| Let agents read terminal output | **off** | MCP `pane_output`, `handoff_prompt` |
| Keep awake (the cup in the top bar) | off | STATE-7, PHONE-8 |
| Remote access: Tailscale | off | PHONE-1, PHONE-2 |
| Remote access: Home network and router VPN | off | PHONE-1, PHONE-2 |
| Remote access: Let phones type into agents | off | PHONE-7 |
| Browser: sites agents may use | none: this machine's pages only | BRW-3, BRW-11 |
| Browser: saved sign-ins | none | BRW-18 |
| Tickets follow the work (Jira) | work starts: first in-progress status; review, merged: not chosen | TKT-1, TKT-8, per Jira project |
| Trust the folders this app creates | on | PANE-10 |
| Terminal text | 13px | 9–24px |
| Conversation text | 14px | 9–24px, an agent over ACP (ACP-15) |
| Task folder location | `~/.villain-worktrees` | where task folders go |
| Tickets: issue tracker | none | Jira, GitHub Issues or Forge, one per workspace (TKT-15) |
| Tickets: Jira | — | site URL, email, API token, optional project key and JQL |
| Tickets: Forge | — | site URL, then a sign-in in the browser; optional team key |
| Code: GitHub | — | API URL (`…/api/v3` for Enterprise), token (`repo`; `read:org` for a bare team slug; a fine-grained token also needs Issues, read and write, when GitHub Issues is the tracker), review team |
| Tickets: GitHub Issues | — | the issue type epics are, when the organisation has issue types (TKT-17) |
| Observability: Grafana | — | site URL, service account token (reads and queries datasources), OBS-1; the Loki datasource for your services' logs, none chosen, OBS-5 |
| Observability: S3 log sources | none | each a name, AWS profile, region, bucket, prefix and optional Athena table; no AWS keys, OBS-8, OBS-9 |
| Messaging: Slack | — | the bot: bot token (`xoxb-`), app-level token (`xapp-`, `connections:write`), the owner's member ID, the agent and its model (BOT-17), on/off, work without asking, personality (BOT-*); its notifications: the channel, four switches (BOT-15, NOTE-5) |

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
  product behind it: Appearance, Tickets (Jira), Code (GitHub), Messaging
  (Slack), Observability, Remote access (the phone), Browser, General. A page
  connecting a service says which one at its top. A product's name said
  nothing to someone who had not set it up, and would be wrong the day a
  second one is supported.
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
  is written.
- **SET-12** A connection's form (Jira, GitHub, the Slack bot, Grafana)
  MUST keep one button, since its address and token only work together
  and the app has to try them. It is named by what it does: "Connect"
  until it is connected, "Reconnect" after, and "Connecting…" while it
  tries (`studio/design/behaviour.md`, "Settings"). Connecting raises no
  toast: the connected row says it (NOTE-7).
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

Code: `commands/settings.rs`, `config.rs` (`UiPrefs`), `Settings.tsx`,
`AppearanceSettings.tsx`, `integrations/mod.rs` (`require_https`); the
server's settings (SET-13) in `crates/server/src/config.rs`; the
workspace's and yours (SET-7 to SET-10) in `SharedSettings.tsx` and
`config/shared.rs` (`held`, `WorkspaceAt.admin`); the
theme in `theme.css`, `lib/theme.ts`, `lib/terminal.ts` and `theme.rs`.

Known gaps:
- Changing Slack's channel needs the token pasted again.
- No one can remove the workspace's connection for everyone from the app.

## 14. What the app writes on your machine

| Where | What |
|---|---|
| `~/Library/Application Support/eu.codevillain.villain-layer/` | `config.json`: repos, tasks, settings, saved panes, the Slack threads handed to chats (BOT-4). `messages.json`: the message center (MSG-4). `notes.json`: this Mac's copy of the repo notes (MEM-7). `runs.json`: the run log (RUN-5). `postgres/`: the workspace's Postgres, its data, password and binaries (PROF-11). At agent launch also `.mcp.json` (0600), `claude-hooks.json`, `copilot-plugin/`, `opencode-plugin.js` |
| Keychain, service `eu.codevillain.villain-layer` | one item holding every token, paired phones' included (PHONE-3), the workspace's session on its server (`layer-session`, PROF-16), and the passwords of the browser's saved sign-ins (BRW-18) |
| `~/.villain-worktrees/` (settable) | task folders, `_chat/` rooms, and `.repos/`: the app's own copy of each repo (REPO-4) |
| a task folder | the worktrees, `AGENTS.md` and `CLAUDE.md` (task context), `TICKET.md` (PANE-13), `.spec-drafts/`: spec drafts, what was approved, and checks (SPEC-2, SPEC-6, SPEC-15), `.mcp.json`, and `.gemini/settings.json`, `PR_DESCRIPTION.md`, `PR_FEEDBACK.md`, and hand-offs too long to type (`CONFLICTS.md`, `REVIEW_COMMENTS.md`, `PR_DRAFT_REQUEST.md`, `FIRST_PROMPT.md`, `CHECKS.md` (LOOP-6), PANE-11) as they come up |
| `~/.claude.json` | trust entries for the app's own folders only (PANE-10) |
| `<config folder>/browser/` | the browser's own Chrome profile (BRW-1): cookies, sign-ins and storage of the pages opened there |
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
  where it would end up in a commit. The one exception is a spec the user
  approved (SPEC-1), which is meant to be committed.
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
to fix on Repos. A running agent or chat is a live dot.

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

Code: `App.tsx` (`TopBar`), `FrameStatus.tsx`, `ui.tsx` (`Confirm`), `Splash.tsx`, `src-tauri/capabilities/default.json`.

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
  needed, with one tab per task. It is the Google Chrome (or Chromium)
  installed on the Mac, run headless with a profile of its own in the
  app's config folder: never the user's own Chrome profile, its sign-ins or
  its extensions. Without Chrome, the tools say what to install.
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
  host whatever was typed. Opening any other page is refused, and so is
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
  its default port only. A site's subdomains are other sites.
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
- **BRW-6** The tabs MUST come back where they were: each task's tabs'
  pages and which was active are kept in `config.json`, and opened when
  the task's browser is next needed. The chats' are kept the same way
  (`browser.chat_tabs`). A config from before tabs had one
  page, which comes back as one tab. Deleting or finishing a task closes
  its tabs.
- **BRW-7** The browser MUST stop with the app: asked to close, then its
  process group killed after 3 seconds, alongside the agents' own grace
  period. A browser that dies is started again when next needed.
- **BRW-8** Nothing the browser does MUST wait on the main thread or the
  async runtime. Chrome is started on the blocking pool and spoken to over
  a pipe (`--remote-debugging-pipe`) by two threads of its own. A pipe, not
  a port: with a debugging port open, any process on the Mac could drive
  the browser and every site signed in to there.

Code: `browser/` (`chrome.rs` finding and running it, `cdp.rs` the
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
the steps to get there, agreed before an agent starts and checked when it
is done. It is one spec for the whole task, however many repositories the
task touches: one set of requirements, one design, and a list of steps for
each repository. Every repository gets a copy, committed on the task's
branch, so it goes through review with the code and stays in the
repository after the task, the branch and the app are gone. The Spec tab,
first of a task's tabs, drafts each file, holds it while you edit it, and
commits it when you approve it.

A spec kept beside the task (`SPEC.md` in the task folder) was deleted
with the task, and nothing ever checked the work against it: an agent was
told what done meant, and nobody asked afterwards whether it got there.

A spec of its own in each repository drifted. A requirement two
repositories met together was written in both and edited in one. Each
repository's design described the endpoint between them in its own words,
in its own run, and nothing compared the two. Changing one repository's
requirements left the other's design and steps as they were. And an agent
that edited a spec file was believed: the tab showed its text as approved,
nothing after it was marked out of date, and it was committed with the
code.

The shape follows Kiro's specs: requirements, then design, then tasks,
each reviewed before the next is drafted.

### What a spec is

- **SPEC-1** A task's spec MUST be kept in each of its repositories as the
  folder `specs/<task>/` in its worktree, `<task>` being the task's branch
  with `/` as `-` (`specs/ACME-123-fix-login-race/`). The `specs/` folder
  is set per repository in the Repos view. It holds:
  - `requirements.md`: Goal, Requirements, Out of scope, Open questions.
    Each requirement is a list item with its id first (`R-1`, `R-2`, …),
    then the repositories that meet it by folder name (`R-3 [api, web]`),
    written as WHEN a condition THE SYSTEM SHALL a behaviour, so that
    someone can check it. One without an id is numbered by its place; one
    naming no repository is met in all of them. Each open question is a
    list item with its id first (`Q-1`, …), the answers it might take as a
    list under it, and, once answered, an `Answer:` line under it.
  - `bugfix.md`, in place of `requirements.md` for a bug: Current
    behaviour, Expected behaviour, Unchanged behaviour (WHEN … THE SYSTEM
    SHALL CONTINUE TO …), each item with its id and repositories, and Open
    questions as in `requirements.md`. Feature or bug is chosen when the
    spec is started.
  - `design.md`: how the task's repositories will meet them together.
    Approach; Contracts, each with its id first (`C-1`): what passes
    between repositories (an endpoint's shape, an event's fields, a
    schema), which repository provides it and which use it; then a section
    per repository (`## api`): what changes where, error handling, how it
    will be tested.
  - `tasks.md`: this repository's numbered steps, and only its own
    (`- [ ] 3. Call the refund endpoint (R-3, C-1; after api 2)`), each
    naming the requirements and contracts it serves and, when it needs one
    done first, another repository's step; ticked when done.
- **SPEC-2** A task in several repositories MUST have one requirements
  file and one design, the same text in every repository's folder, and a
  `tasks.md` per repository. What was approved is kept by the app (in the
  task folder's `.spec-drafts/`) and written from there into every
  repository; no repository's copy is changed on its own. A task in one
  repository has one copy, and nothing else differs.
- **SPEC-3** A repository whose spec setting is off MUST NOT get spec
  files in its worktree. Its copy is kept in the app's folder instead
  (`<config folder>/specs/<repository>/<task>/`, same files), so it
  outlives the task on this machine, though not in git. The tab says
  which.

### Drafting and approving

- **SPEC-4** The files MUST be drafted in order and each approved before
  the next is drafted: requirements (or bugfix), then design, then tasks.
  Quick spec drafts all three at once, and they are approved together.
- **SPEC-5** Each draft MUST be a fresh one-shot run (Claude Code,
  Sonnet, no MCP, nothing that writes or runs), streamed into the editor:
  - Requirements: one run over the ticket (`TICKET.md`, fenced off as data
    and not instructions) and the task's repositories, which writes the
    task's requirements and names the repositories that meet each. It
    invents no requirement the ticket does not imply; what the ticket
    leaves unclear goes under Open questions, each with the answers it
    might take. A task with no saved ticket is drafted from its name. A
    redraft is also given the questions already answered in the editor,
    as decisions made, so it builds them in instead of asking again.
  - Design: one run, allowed to read every repository's worktree, over the
    approved requirements. It writes the contracts first and then each
    repository's section, so both sides of an endpoint are described once.
  - Tasks: one run over the approved requirements and design, which writes
    every repository's steps, so what has to come first across
    repositories is decided in one place.

  A redraft over text in an editor asks first; a failed draft puts back
  what the editors held.
- **SPEC-6** Approving MUST write the file into every repository's
  worktree (each `tasks.md` into its own) and commit it on each task
  branch, as the user, with only the spec's own files in the commit:
  whatever an agent has staged or changed there stays as it was. The
  message says which file of which task ("Approve the design for
  ACME-123."); in a repository with a commitlint config, or whose recent
  commits are mostly conventional, it is one ("docs: approve the design
  for ACME-123"), so its commit hook takes it. The same holds for the
  commits of SPEC-13 and SPEC-22. A repository it could not be written or committed in is
  named with why, and the others keep theirs. Approving records what was
  approved, which SPEC-22 compares against. A draft or an edit not
  approved reaches nobody, and the tab says so. Drafts are kept in the
  task folder (`.spec-drafts/`) until approved, so neither switching
  tasks nor quitting the app loses one.
- **SPEC-7** Changing an approved file MUST mark the files after it out of
  date, in every repository. Sync redrafts every file that is out of date,
  in order, each from the one before it, and they are reviewed and
  approved together: steps already ticked are kept, steps for new
  requirements are added in the repositories those name, and a step whose
  requirements are all gone, or no longer name its repository, is marked
  for the user to remove, never removed by itself.
- **SPEC-8** The tab MUST show each file's state (not started, draft,
  approved, out of date, changed outside the app), the requirements with
  the repositories each names, and per repository its steps ticked of the
  total. A requirements file with no requirements is warned about: they
  are what done means. The panel beside the editor that shows them is
  dragged wider or narrower by its left edge (always leaving the editor
  room), double-clicked back to its usual width, and keeps its width.
- **SPEC-9** A task whose worktree already has `specs/<task>/` MUST open
  it: a task cut from a branch that has one, or a second task for the
  same ticket on the same branch. Copies that differ between repositories
  are shown as changed outside the app (SPEC-22), or as specs from before
  (SPEC-25).

### Changed outside the app

The spec folder is in the worktree, where agents, editors and merges all
write. An approved file that changed there is not approved: nobody agreed
to it, and what was drafted after it was drafted from something else.

- **SPEC-22** The app MUST notice a spec file, in any repository, whose
  text is not what was approved: when the tab is shown, when an agent in
  the task stops working, and before Open pull requests. A tick is not a
  change (SPEC-13): `tasks.md` is compared without its boxes. The tab
  marks the file changed outside the app, shows what changed and in which
  repository, and offers:
  - Approve: the text is approved as in SPEC-6, written into every
    repository's copy, and the files after it are out of date (SPEC-7).
  - Put back: the approved text is written back over it, in that
    repository.

  Until one is chosen, agents and checks go on reading what was approved
  (SPEC-10, SPEC-15).
- **SPEC-23** Open pull requests (PR-1) for a task with a spec file
  changed outside the app MUST say which, and ask first: a reviewer would
  read a spec nobody approved.
- **SPEC-24** An agent MUST propose a change to the spec with the
  `spec_propose` tool, not by editing the files: which file (requirements,
  design, or one repository's tasks), its whole new text, and why. It
  lands in that file's editor as a draft, with the reason and the pane
  that proposed it, and the tab is marked until the user approves it or
  turns it down; either way the agent is sent one line saying which
  (SPEC-14). An editor holding a draft of the user's refuses it, and the
  agent is told to ask instead.

### Answering open questions

The questions a draft leaves are the user's to decide, and an answer is
only worth something once a requirement says it: an agent works to the
requirements, and Check against spec reads nothing else. Answering used to
mean typing beside a question at the bottom of the editor, rewriting the
requirements it decided by hand, and finding that Redraft threw both away.

- **SPEC-19** Beside the requirements, the tab MUST list the open
  questions the editor holds, how many have no answer, and for each the
  answers drafted for it to pick, each shown whole however long, and a
  field for one of the user's own.
  An answer is written into the editor's text as the question's `Answer:`
  line, so the editor and the list are one record, kept as a draft is
  (SPEC-6). An `Answer:` line typed into the editor counts the same, and
  so does a line under the question starting `--`.
- **SPEC-20** Apply answers MUST be a one-shot run, as a draft is
  (SPEC-5), over the requirements as the editor holds them, which builds
  each answer into them: it rewrites the requirements an answer decides,
  and the repositories they name, puts in Out of scope what an answer
  leaves out, and removes each answered question. A requirement no answer
  touches keeps its text, id and repositories, a new one takes the next
  free id, and a question with no answer stays. The tab then names the
  requirements changed, added and removed, and Undo puts back what the
  editor held. A failed run puts it back by itself.
- **SPEC-21** Approving requirements whose editor still holds open
  questions MUST first say how many have no answer (an agent will stop to
  ask them, SPEC-10) and how many answers are not built in yet (Check
  against spec reads only the requirements), and approve only if the user
  goes on.

### Working to it

- **SPEC-10** The context file (PANE-13) MUST name each repository's spec
  folder and carry the approved requirements once, above the ticket, as
  the user's own: they are what done means, they win where they and the
  ticket disagree, what they put out of scope is left alone, and open
  questions are asked rather than guessed. It says to change the spec
  only with `spec_propose` (SPEC-24). Past 8 KB they are cut, and point at
  the files.
- **SPEC-11** An opening prompt for a task with a spec (`task_prompt`: the
  launch dialog, Start agent, handoffs) MUST end by pointing at the spec
  folders by full path, so an agent started inside one repository finds
  the others too.
- **SPEC-12** Start agent on the tab MUST start an agent on the approved
  tasks: at the task folder for every repository, or in one repository
  for its own. Its prompt tells it to work through the steps in order,
  leave a step whose `after` is not ticked until it is (or, alone in its
  repository, stop and say which it is waiting for), and mark each step
  done as soon as it is finished, before starting the next. Told only to
  mark each as it finished it, an agent did three steps and ticked them
  together five minutes after the first, and the tab sat at 0 meanwhile.
  The steps are listed in the order SPEC-1's `after` gives, not in the
  order of the task's repositories.

  Start agent is on the tab once, above the files, not in a repository's
  panel, and its button says what it starts and how much is left: every
  repository's steps by default (`Start Claude Code on both repositories
  · 13 steps left`), or one repository's when that is picked. While an
  agent is running in the task, the tab names it, with a way to open its
  pane, and starting another asks first. In a
  repository's panel, beside its tasks, the button read as starting that
  repository only, though it started both; not sure, the user clicked it
  again in the other panel, and two agents set off on the same steps.
- **SPEC-13** An agent MUST mark a step done with the `spec_task` tool,
  which ticks it in `tasks.md` and updates the tab. Ticks are committed
  with the next approval or before Open pull requests (PR-1), with only
  the spec's files in the commit. A tick an agent makes by editing the
  file itself counts the same: the file is the record.
- **SPEC-14** Approving a change while agents in the task are running
  MUST offer to tell them: each is sent one line naming the files that
  changed, typed into a terminal or sent as a prompt over ACP. Never on
  its own, since a line typed into a busy agent interrupts it.

### Checking against it

- **SPEC-15** Check against spec MUST run, per repository, a one-shot run
  allowed to read the worktree, over the approved requirements that name
  it, the contracts it provides or uses, and the branch's changes from its
  branch point (`git::baseline`). It answers per requirement: met, not met
  or unclear, with the files and lines that show it. A requirement two
  repositories meet shows each one's answer, and is met only when both
  say so. The answer is shown beside each requirement, with the commit it
  was checked at, and marked stale once that branch moves on. It is kept
  in the task folder, never committed.
- **SPEC-16** A drafted pull request description (PR-4) MUST, for a
  repository with a spec, link its spec folder and list the requirements
  that name it with the latest check's answer, or say none was run.

### From Start work

- **SPEC-18** Start work, with an agent picked, MUST offer "Write a spec
  first", on until turned off, and remembered. With it the task is made
  and the ticket moved as without it, but no agent starts: the task opens
  on its Spec tab with the requirements being drafted (unless the task
  already has a spec, SPEC-9), and Start agent there starts the agent
  Start work had picked (SPEC-12).

### Moving from earlier specs

- **SPEC-17** A task with a `SPEC.md` from before MUST offer it as the
  requirements draft, and remove it once those are approved.
- **SPEC-25** A task whose repositories each have a spec of their own
  from before (their requirements or designs differ) MUST offer to merge
  them: a one-shot run, as a draft is, over every repository's files,
  which writes one requirements file and one design, each requirement
  naming its repositories, and each repository's steps with their
  references renumbered and their ticks kept. An id that changed is noted
  beside its new one (`R-4 (was web R-2)`). It is shown as drafts and
  approved as usual; no worktree changes until then.

Code: `spec.rs` (the files, and what is read from them),
`commands/spec.rs` (where the spec and its copies live, what was
approved, approving), `commands/spec_view.rs` (what the tab is shown),
`commands/spec_outside.rs` (Put back, `spec_propose` and turning a
proposal down), `commands/spec_work.rs` (ticking, prompts, the pull
request's part, telling agents), `commands/spec_draft.rs` and
`commands/spec_prompts.rs` (drafting, applying answers, checking,
merging), `commands/task_context.rs`, `git/only.rs` (committing only the
spec), `mcp.rs` (`spec_task`), `commands/open_prs.rs`, `SpecView.tsx`,
`spec/SpecStartAgent.tsx`, `spec/SpecBanners.tsx`, `spec/SpecSide.tsx`,
`spec/SpecQuestions.tsx`, `Outgoing.tsx` (SPEC-23), `lib/specs.ts`,
`lib/questions.ts`, `Watchers.tsx` (reading the spec again when an agent
stops), `tickets/StartWorkDialog.tsx`, `ReposView.tsx` (the
per-repository setting).

Known gaps:
- Drafting runs Claude Code, whichever agent will do the work.
- A spec is approved by whoever runs the app. There is no second
  approver, and the commit is the only record of who approved it.
- Kiro runs independent tasks side by side; here one agent works through
  them in order, or one per repository waiting on each other's `after`.
- A draft cannot be stopped once started.
- A step an agent ticks is an uncommitted change in the worktree until the
  next approval or Open pull requests, and shows in the Diff tab until then.
- Drafting and checking need Claude Code on the PATH.
- An answer is recorded in the spec's commit only. The ticket, and
  whoever asked for the work, are not told what was decided.
- Each repository's pull request carries the whole requirements and
  design, the other repositories' parts too.
- A spec changed outside the app is noticed when an agent stops or the
  tab is shown, not as it is written.
- A tick is the agent's word. One that saves its ticks up shows the
  work behind where it is, and one that ticks a step it did not finish
  is caught only by Check against spec.
- Which repositories a requirement names is the drafter's judgement;
  it is corrected in the editor like anything else.
- A task with no folder of its own (an older one, whose one repository
  is its root) keeps no record of what was approved, so nothing there is
  seen as changed outside the app.
- A spec approved before the app kept a record of it is read from the
  repositories' copies, and nothing in it is seen as changed outside the
  app until it is approved again (or merged, SPEC-25).
- Drafts left unapproved from when each repository had a spec of its own
  are not carried over; what was approved is, by the merge (SPEC-25).
- Start agent counts every agent running in the task as working through
  the spec, whatever it was started for.

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
| `claude` | `claude-agent-acp`, installed separately (`@agentclientprotocol/claude-agent-acp`, formerly `@zed-industries/`) | load, resume, list | adapter 0.87.0 (and 0.22.2) |
| `copilot` | `copilot --acp` | load, list | 1.0.93 |
| `opencode` | `opencode acp` | load, resume, list | 1.18.35 |
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
  hooks, no plugin, no settings. An agent that takes no MCP server over
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
  `session/resume`. Resume with no id takes the newest the agent lists for
  the folder (`session/list`). One that cannot be picked up is started
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
  rather than sent without them. The conversation names each image
  `[image]`, as a replay does.

Code: `acp/` (`conn.rs` the protocol, `user.rs`, `conversation.rs`,
`entry.rs`, `read.rs`, `process.rs`), `pty/acp.rs`,
`commands/acp_panes.rs`, `agents.rs` (`AcpCommand`), `AcpPane.tsx`,
`lib/acpUpdates.ts`, `lib/types-acp.ts`. A real agent is checked with
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
  Resume, Start work, Start agent on a spec, the Diff tab's "send to an
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
  or an agent, since it is run as the user, unattended.
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
  check command, and tell agents to run it before they end a turn, so an
  agent checks its work by the measure the loop will, loop or not. Setting
  a command rewrites the context files of the tasks that repository is in.
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

Agents read the logs of the services they work on. Settings has an
Observability tab for where those logs are: Grafana's Loki, and S3. You
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
  not silence. A turn that only answers gets none.
- **BOT-7** A turn's permission questions (ACP-5) MUST share one message
  in the thread, posted at the first and changed as more come and as each
  is answered, from Slack or from the app. Each names what it asks in
  words (`browser navigate (Layer)`, not
  `mcp__villain-layer__browser_navigate`) and has a button for each of the
  agent's answers. Only the owner's click answers; anyone else's is told,
  to them alone, that it is not theirs to answer. A question the turn
  ended without is shown as no longer asked. The next turn's questions
  start a message of their own.
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
  (ACP-12).
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
  to the one channel chosen for them in Settings → Messaging; there is no
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
connections and the log sources. A guest works only in the repositories an
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
spec, approved once, whose steps become sub-tasks, each filed in Jira under
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
  (SPEC-5) and the approved files are committed there (SPEC-6). No agent
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

- **EPIC-4** An epic's spec MUST be drafted and approved as any task's
  (§19), except that its tasks are grouped into sub-tasks. Under each
  repository the tasks draft heads each group with a second-level heading,
  its title (`## Session refresh endpoint`), and numbers the steps across
  the groups from 1, as before. The same title in several repositories is
  one sub-task across them. A heading that starts with a ticket key
  (`## ACME-104 Session refresh`) uses that ticket rather than filing a
  new one. Steps under no heading, or under `## Tasks`, make one sub-task
  for their repository, called after the epic and the repository.
- **EPIC-5** A sub-task MUST wait for another when one of its steps is
  after one of the other's (`after api 4`, SPEC-1). The order sub-tasks
  are listed in follows that: what starts first leads.
- **EPIC-6** The epic's Spec tab MUST NOT offer Start agent (SPEC-12) or
  Check against spec (SPEC-15): the work, and what is checked, is in its
  sub-tasks.

### Filing sub-tasks

- **EPIC-7** Once its tasks are approved, the Sub-tasks tab MUST list the
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
  3. its spec, approved (SPEC-6): the epic's requirements and design, and
     in each repository that sub-task's steps alone, with their numbers. An
     `after` on another sub-task's step is left out: waiting for it is the
     epic's job (EPIC-5).

  No agent starts and no ticket moves. A sub-task that could not be filed
  is named with why, the others are kept, and filing again makes only
  what is missing. Before, the plan stayed in one `tasks.md` and every
  ticket under the epic was filed and given its part by hand.
- **EPIC-8** Each filed sub-task's row on the Sub-tasks tab MUST show its
  ticket key, its steps ticked of the total, and its state (EPIC-10), and
  offer its one action: Start for a ready one, which starts the default
  agent on its spec as Start agent does (SPEC-12) and moves its ticket
  (and, the first time, the epic's) as Start work does (TKT-1); Open for
  the rest. Starting one that waits asks first, naming what it waits for.

### Following them

- **EPIC-9** The task list MUST show epics in a section of their own,
  above Tasks: each epic on one row, with "N of M merged" (or "M to file"
  before filing), and its sub-tasks under it, one line each: a dot as a
  task's (hollow while it waits), its title, and its state in a word. A
  sub-task is not listed again under Tasks. It opens as any task does, with
  all of its tabs, and its header links to its epic. A sub-task whose epic
  was deleted is an ordinary task again. Beside the epics made here, the
  section lists each Jira epic a task's ticket sits under (its parent, found
  as TASK-17 finds it, whatever the list is grouped by), with those tasks
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
  beside a task's terminals is (BRW-9).
- **EPIC-12** The epic's chat MUST have three tools: `epic_subtasks` (each
  sub-task: its title, ticket, task, repositories, steps ticked of the
  total, what it waits for, and its agents, running and needing you),
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
  not an epic here yet opens Start work on its ticket (EPIC-2), with its
  tasks' repositories picked. The epic it makes opens on Sub-tasks, with
  those tasks under it, and drafts no requirements: its work is already
  under way, and a spec can still be written on its Spec tab. Before, only
  epics made from then on had sub-tasks, and every task started earlier
  stayed under Tasks.

Code: `spec.rs` (`sub_tasks`: grouping steps into sub-tasks, and each
one's `tasks.md`), `commands/epics.rs` (what the tab is shown, filing,
starting, telling, `adopt_sub_tasks` for EPIC-13),
`commands/spec_prompts.rs` (the grouped tasks draft),
`commands/task_context.rs` (the epic chat's context), `mcp.rs`
(`epic_subtasks`, `subtask_start`, `subtask_tell`), `EpicView.tsx`,
`epic/` (the Sub-tasks tab, filing, the chat column), `lib/epicState.ts`
(EPIC-10), `sidebar/EpicRow.tsx`, `sidebar/EpicsSection.tsx` (EPIC-9,
EPIC-13), `tickets/StartWorkDialog.tsx` (EPIC-2, EPIC-13),
`tickets/CreateEpicDialog.tsx` (EPIC-3).

Known gaps:
- Nothing stops the epic's chat from editing files: it is told not to,
  and it runs in a folder that holds the epic's worktrees.
- The epic's chat is not told when a sub-task's agent asks something,
  finishes or has its pull requests merged. It finds out with
  `epic_subtasks` when it looks.
- Changing an epic's approved spec after filing changes no sub-task: each
  keeps the spec it was filed with, and is changed in its own Spec tab.
- A sub-task the tasks draft adds after filing is filed with the next
  File; one it drops stays filed.
- Each sub-task's pull request carries the whole requirements and design,
  as §19's gaps say of a task in several repositories.
- An epic is not finished by itself once every sub-task is merged: its
  ticket and its worktrees stay until Finish task (TASK-8).
- A project whose required fields have no fixed values cannot file
  sub-tasks from here (TKT-3). Filed by hand and named by key in the
  spec's headings (EPIC-4), they are taken as existing tickets.

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
