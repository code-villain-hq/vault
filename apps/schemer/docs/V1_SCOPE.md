# v1 scope: the delivery slice

v1 goes deep on the middle of the loop: **Work item → Change → Check → Release**, plus one small hook at the end (a release records one metric before and after). It must be good enough that Michael moves his own projects onto it, and that 3 outside teams try it.

The data model is designed for the whole loop from day one (see `DATA_MODEL.md`), but v1 only builds the node types below.

## Users

- **Maker:** an engineer on a small team. Wants to write a ticket in 5 seconds and never touch its status again.
- **Lead:** wants to see what's in progress, what's blocked, what shipped this week, and whether the last release helped.

## Features and acceptance criteria

### 1. Sign-in and workspace

- Sign in with GitHub (OAuth). Sessions are stored server-side in Postgres.
- A user can create a workspace with a name and an issue key (for example `SCH`), and invite people by GitHub username. An invite is accepted automatically when that person signs in. The v1 UI shows one workspace at a time.
- **Done when:** a new user can sign in, create a workspace, and invite a second user who can see the same items.

### 2. Work items

- Fields: number (`SCH-123`), title, description (Markdown), status, priority (0–4), assignee, labels.
- Statuses: `backlog`, `todo`, `in_progress`, `in_review`, `done`, `released`, `cancelled`. Plus a `blocked` flag with a reason.
- Every change to a work item is stored as an event. The item page shows a timeline of everything that happened to it, from people and from GitHub.
- Undo for your last change to an item (by appending a compensating event, never deleting).
- **Done when:** create, edit, change status, assign, label and undo all work, and the timeline shows each change with who or what made it.

### 3. Keyboard-first, instant UI

- Command palette (`Cmd/Ctrl+K`) for every action. Single-key shortcuts for the common ones (`c` create, `s` status, `a` assign, `l` label, `/` search).
- List view and board view, both filterable by status, assignee and label.
- Optimistic updates: the UI changes immediately, and the server confirms or the change rolls back with a message.
- Realtime: changes by other users and by GitHub appear without a reload.
- A local cache in the browser (IndexedDB), so the app opens instantly with the last known data.
- **Done when:** two browsers on the same workspace see each other's changes in under 1 second, and the app shows cached data before the network responds.

### 4. Search

- Full-text search over title and description, plus fuzzy matching on titles, plus filters.
- **Done when:** search returns in under 50 ms at p95 with 10,000 work items (measured on the server).

### 5. GitHub connection (the core differentiator)

- Install a GitHub App on one or more repos. The app receives webhooks for pull requests, check runs and deployments.
- **Linking:** a PR is linked to a work item when its title, body or branch name contains the item key (`SCH-123`, case-insensitive; for example branch `sch-123-fix-login`).
- **Derived status** (full rules in `DATA_MODEL.md`):
  - Draft PR opened → `in_progress`
  - PR ready for review → `in_review`
  - A check fails on the PR's latest commit → `blocked` flag with reason `ci_failing`; cleared when the latest run of every check on that commit passes (a re-run counts)
  - All linked PRs merged or closed, with at least one merged → `done`
  - A successful production release contains the merged PRs → `released`
- A manual status change always wins until a later code event moves the item further forward.
- **Done when:** a real PR on a real repo moves its linked item from `todo` through `in_review`, `done` and `released` with nobody touching it, and each step appears in the timeline within 5 seconds of the GitHub event.

### 6. Releases

- Releases come from GitHub `deployment_status` events, or from a simple API call in any CI pipeline (`POST /api/v1/releases` with a token).
- The system works out which merged PRs (and therefore which work items) each release contains, by comparing commits with the previous release in the same environment.
- A releases page lists what shipped, when and where.
- **Done when:** a release page shows the correct list of PRs and work items for a real deploy.

### 7. Release metric hook (the thin end of the loop)

- A workspace can define metrics (for example `signup_conversion`) and push data points through an API (`POST /api/v1/metrics/{key}/points`).
- A release can be tagged with one metric it's expected to move. After a window (default 72 hours) the system records the average before and after.
- **Done when:** a release page shows "signup_conversion: 3.1% → 3.6% (72 h before vs. after)" computed from pushed data.

## Performance budgets

| Measure | Budget |
| --- | --- |
| API p95 latency (reads and writes) | < 100 ms |
| Search p95 at 10k items | < 50 ms |
| GitHub event → visible in the UI | < 5 s |
| Change by another user → visible | < 1 s |
| App open with warm cache → first render | < 300 ms |

## Out of scope for v1

- Signals, ideas and decisions as node types (the schema reserves them)
- Docs staleness, and slow/flaky test tracking (v2)
- Jira importer, Slack, public API beyond releases and metrics
- Multiple workspaces per user, roles beyond owner/member, billing
- Offline *writes* (offline reads from the cache are in scope)
- AI features

## Success metrics for v1

- Michael runs all his own projects on it for 4 weeks straight.
- 3 outside teams connect a real repo and keep using it for 2+ weeks.
- For those teams, at least 80% of status changes are derived, not manual.
