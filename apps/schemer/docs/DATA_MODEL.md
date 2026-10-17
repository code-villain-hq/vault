# Data model

The schema lives in `db/migrations/00001_init.sql`. It has been checked against PostgreSQL 16: it applies cleanly, rolls back cleanly and re-applies. This doc explains the ideas behind it.

## Three layers

1. **Events** (`events`): the source of truth for the projections below. Append-only, ordered by `seq`.
2. **Projections** (`work_items`, `changes`, `check_runs`, `releases`, `release_outcomes`, `edges`): read models shaped for fast queries. They can always be rebuilt by replaying events.
3. **The graph** (`edges`, itself a projection): typed links between any two nodes. This is what lets the product grow along the loop without a rewrite.

Everything else is **plain CRUD and not event-sourced**: `users`, `sessions`, `workspaces`, `workspace_members`, `workspace_invites`, `api_tokens`, `github_installations`, `repositories`, `metrics`, `metric_points`, `webhook_deliveries`, `jobs`. "Every state change is an event" applies to the six projection tables only.

### The write rule

> Every command appends one or more events **and** updates the affected projections **in the same database transaction**.

There's no separate projection process, no eventual consistency and no message broker in v1. It's simple, and it's still event-sourced: the log is complete, and projections are rebuildable.

```go
// Shape of every write (pseudo-code)
err := store.InTx(ctx, func(tx Tx) error {
    // 1. Serialise writes per workspace, so seq order = commit order within a workspace.
    //    SELECT pg_advisory_xact_lock(hashtextextended($workspace_id::text, 0))
    // 2. INSERT INTO events (...) VALUES (...)
    //      ON CONFLICT (workspace_id, idempotency_key) DO NOTHING RETURNING seq
    //    No row back = already applied: skip Apply and return the earlier result.
    //    (Don't catch a unique-violation error instead: in Postgres that aborts the transaction.)
    ev, applied, err := tx.AppendEvent(ctx, events.New(ws, "work_item", id, "work_item.status_changed", payload, actor, idemKey))
    if err != nil || !applied { return err }
    // 3. Update the projection row and set last_event_seq = ev.Seq
    if err := workitems.Apply(ctx, tx, ev); err != nil { return err }
    // 4. SELECT pg_notify('schemer_events', '<workspace_id>:<seq>'). NOTIFY is delivered on commit.
    return tx.Notify(ctx, ws, ev.Seq)
})
```

Why the advisory lock: `seq` is assigned when the row is inserted, not when it commits. Without the lock, event 101 could commit before event 100, and a sync client reading `seq > cursor` would skip 100 forever.

### Idempotency

- Anything that can be retried carries an `idempotency_key`: a GitHub delivery id, a client mutation id, or a job key. The unique index `(workspace_id, idempotency_key)` makes a retry a no-op.
- One command can append several events (for example a PATCH that changes status and priority). Each event gets its own key: `<request key>:<index>`, such as `m17:0` and `m17:1`.
- Jobs run at least once, so every job handler must be safe to run twice.

## Nodes: the whole loop is reserved, v1 builds the middle

| Node type | Stage | v1? | Table |
| --- | --- | --- | --- |
| `signal` | Customer feedback, support, errors | No (v4) | (future) |
| `idea` | Proposed product change | No (v3) | (future) |
| `decision` | Why, evidence, expected outcome | No (v3) | (future) |
| `work_item` | Unit of delivery | **Yes** | `work_items` |
| `change` | Pull request | **Yes** | `changes` |
| `check_run` | CI result | **Yes** | `check_runs` |
| `release` | Deploy to an environment | **Yes** | `releases` |
| `metric` | A number the team tracks | **Yes** (minimal) | `metrics`, `metric_points` |
| `outcome` | Metric before vs. after a release | **Yes** (as `release_outcomes`) | `release_outcomes` |

## Edges

| Kind | From → To | v1? | Created by |
| --- | --- | --- | --- |
| `implements` | change → work_item | **Yes** | Linker: item key found in PR title, body or branch |
| `verifies` | check_run → change | **Yes** | Check's `head_sha` = PR's `head_sha` |
| `ships` | release → change | **Yes** | Release inclusion job |
| `measures` | release → metric | **Yes** | Person tags the release with its expected metric |
| `supports` | signal → idea | v4 | |
| `decided_in` | idea → decision | v3 | |
| `planned_as` | decision → work_item | v3 | |
| `feeds` | outcome → signal | v4 | |

Edges have `source = 'derived'` (the system made it) or `'manual'` (a person did). Edges are soft-deleted with `removed_at`. When a person removes a derived edge, `removed_by_user = true` and an event records it; the linker must never re-create that edge.

## Event catalogue (v1)

Event types are `<node_type>.<verb in past tense>`. Payloads are JSON; list only what changed.

| Event | Payload (main fields) | Actor |
| --- | --- | --- |
| `work_item.created` | `number`, `title`, `description`, `status`, `priority`, `assignee_id`, `labels` | user |
| `work_item.edited` | changed fields among `title`, `description`, `priority`, `labels` | user |
| `work_item.assigned` | `assignee_id` (nullable) | user |
| `work_item.status_changed` | `from`, `to`, `source` (`manual`/`derived`), `reason` | user or system |
| `work_item.blocked` / `work_item.unblocked` | `reason` | system |
| `work_item.reverted` | `reverts_seq` (the undone event) + the restored fields | user |
| `change.opened` | PR fields | github |
| `change.updated` | changed PR fields (title, body, branch, head_sha, draft → open) | github |
| `change.merged` | `merge_commit_sha`, `merged_at` | github |
| `change.closed` | `closed_at` (closed without merging) | github |
| `change.linked` / `change.unlinked` | `work_item_id`, `source` | system or user |
| `check_run.reported` | `name`, `head_sha`, `status`, `conclusion`, timings | github |
| `release.reported` | `environment`, `version`, `commit_sha`, `status`, `source` | github or api |
| `release.contents_resolved` | `previous_commit_sha`, `change_ids[]` | system |
| `release.metric_tagged` | `metric_id`, `window_hours` | user |
| `release.outcome_computed` | `metric_id`, `before`, `after`, counts | system |

## Derived status: the rules

Lifecycle order: `backlog < todo < in_progress < in_review < done < released`. `cancelled` is outside the order and only set by a person.

Evaluate these rules after any event that touches an item's linked changes, checks or releases:

1. Draft PR linked → `in_progress`
2. Non-draft open PR linked → `in_review`
3. All linked PRs are merged or closed, with at least one merged → `done`
4. A successful release in the **production** environment ships every merged linked PR → `released`
5. For any open linked PR, take the latest run of each check name on its current `head_sha`. If any has conclusion `failure` or `timed_out` → set `blocked_reason = 'ci_failing'`. Clear it when the latest run of every check on the current head passes, whether that's a new commit or a re-run on the same commit. Blocking never changes the status.

Guardrails:

- **A person wins.** A manual status change sets `status_source = 'manual'`. The engine may move the item forward from there, but never back to an earlier status. This guardrail beats every rule below.
- **Never move backwards automatically**, with one exception: if every linked PR is closed without merging **and** `status_source = 'derived'`, return the item to `todo`. If a person set the current status, leave it alone.
- `cancelled` items are never touched by the engine.
- Each derived change is a `work_item.status_changed` event with `source: 'derived'` and a human-readable `reason` (for example "PR #42 merged"). That's what the timeline shows.

This is the heart of the product, so it gets the most tests: table-driven, one case per rule and per guardrail.

## Linking PRs to work items

- Pattern: `\b<ISSUE_KEY>-(\d+)\b`, case-insensitive, searched in the PR title, body and branch name (for example `sch-123-fix-login`).
- Re-run whenever the title, body or branch changes. Add new links; remove derived links whose key disappeared; never touch manual links.
- Unknown numbers are ignored.

## Release contents

GitHub deployment states map onto release status like this: `queued` and `pending` → `pending`; `in_progress` → `in_progress`; `success` → `success`; `failure` and `error` → `failure`; `inactive` → ignored (a newer deployment replaced it). The row is found by `(workspace_id, source, external_id)`, where `external_id` is the GitHub deployment id.

When a release reaches `success`:

1. Find the previous successful release in the same environment and repository. Store its commit as `previous_commit_sha`.
2. Ask GitHub for the commits between `previous_commit_sha` and `commit_sha` (the compare API) in a background job. Handle pagination.
3. Every merged change whose `merge_commit_sha` is in that list gets a `ships` edge.
4. **First release in an environment:** ship every merged change that was merged before `deployed_at` and isn't shipped yet.
5. Re-evaluate derived status for the affected work items.

## Release outcomes

- A release can carry one expected metric (`expected_metric_id`, `measures` edge).
- `window_hours` after `deployed_at`, a job computes the average of `metric_points` over `window_hours` before and after the release and writes `release_outcomes`.
- Show "not enough data" if either side has fewer than 3 points. Never show a number that isn't backed by data.

## Rebuilding projections

A `make rebuild-projections` command (built in milestone M1) truncates the six projection tables **in one statement** (`TRUNCATE work_items, changes, check_runs, releases, release_outcomes, edges`, since `release_outcomes` references `releases`), then replays `events` in `seq` order through the same `Apply` functions. Clients get a `resync` message afterwards. The test suite should run it too: replaying the log must produce exactly the projections that normal writes produced.
