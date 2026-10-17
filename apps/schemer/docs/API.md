# API (v1 draft)

JSON over HTTPS. The React app uses session cookies; machine clients (CI, deploy scripts) use `Authorization: Bearer <api token>`. Work items are addressed by their number within a workspace (`/work-items/42` = `SCH-42`).

## Conventions

- **Errors** always look like `{"error": {"code": "not_found", "message": "Work item SCH-42 not found"}}` with a matching HTTP status. Codes: `bad_request`, `unauthorized`, `forbidden`, `not_found`, `conflict`, `validation_failed`, `rate_limited`, `internal`.
- **Idempotency:** writes accept an `Idempotency-Key` header (the client's mutation id). The same key twice returns the first result. If one request appends several events, the server keys them `<key>:0`, `<key>:1`, … (see `DATA_MODEL.md`).
- **Responses to writes** include the new `seq`, so the client can match them against the sync stream.
- **Timestamps** are RFC 3339 in UTC.
- **Pagination:** `?limit=50&after=<opaque cursor>`, and responses carry `next_cursor`.

## Auth

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/auth/github/login` | Redirects to GitHub OAuth (with `state` stored in a short-lived cookie) |
| GET | `/auth/github/callback` | Exchanges the code, upserts the user, creates a session, redirects to the app |
| POST | `/auth/logout` | Deletes the session |
| GET | `/api/me` | The current user and their workspaces |

## Workspaces

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/api/workspaces` | `{name, slug, issue_key}`; the creator becomes owner |
| GET | `/api/w/{ws}` | Workspace details and members |
| POST | `/api/w/{ws}/members` | `{github_login}`; owner only. Creates an invite, accepted automatically when that login signs in |
| POST | `/api/w/{ws}/tokens` | `{name}` returns `{token}` **once** |
| DELETE | `/api/w/{ws}/tokens/{id}` | Revoke |

## Work items

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/api/w/{ws}/work-items` | `{title, description?, status?, priority?, assignee_id?, labels?}` |
| GET | `/api/w/{ws}/work-items` | Filters: `q` (search), `status`, `assignee`, `label`, `blocked=true` |
| GET | `/api/w/{ws}/work-items/{n}` | Item plus linked changes, checks and releases |
| PATCH | `/api/w/{ws}/work-items/{n}` | Any subset of editable fields, including `status` |
| POST | `/api/w/{ws}/work-items/{n}/undo` | Reverts the caller's last change to this item |
| GET | `/api/w/{ws}/work-items/{n}/timeline` | Events for the item and its linked nodes, newest first |
| POST | `/api/w/{ws}/work-items/{n}/links` | `{change_id}`: manual link |
| DELETE | `/api/w/{ws}/work-items/{n}/links/{change_id}` | Remove a link (marks derived links `removed_by_user`) |

Example:

```http
PATCH /api/w/villain/work-items/42
Idempotency-Key: 5f0c...-m17
Content-Type: application/json

{"status": "todo", "priority": 2}
```

```json
{"item": {"number": 42, "key": "SCH-42", "status": "todo", "status_source": "manual", "priority": 2, "...": "..."}, "seq": 1893}
```

## Releases and metrics (machine API, bearer token)

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/api/v1/releases` | `{repository: "owner/name", environment, version?, commit_sha, status, deployed_at?, expected_metric?}` |
| POST | `/api/v1/metrics/{key}/points` | `{points: [{ts, value}]}`; up to 1,000 per call; upserts on `(metric, ts)` |
| POST | `/api/v1/metrics` | `{key, name, unit?, higher_is_better?}` |

A deploy step in any CI:

```bash
curl -fsS -X POST "$SCHEMER_URL/api/v1/releases" \
  -H "Authorization: Bearer $SCHEMER_TOKEN" -H "Content-Type: application/json" \
  -d "{\"repository\":\"$GITHUB_REPOSITORY\",\"environment\":\"production\",\"commit_sha\":\"$GITHUB_SHA\",\"status\":\"success\"}"
```

App-facing release views (session auth):

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/api/w/{ws}/releases` | Filter by `environment`; newest first |
| GET | `/api/w/{ws}/releases/{id}` | Contents (changes, work items) and outcome |
| PATCH | `/api/w/{ws}/releases/{id}` | `{expected_metric_id, window_hours}` |

## GitHub

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/api/w/{ws}/github/install` | Redirects to the GitHub App install page (with `state` = workspace) |
| GET | `/github/setup` | GitHub redirects here after install; stores the installation and repos |
| POST | `/webhooks/github` | Signature-verified; see ARCHITECTURE.md, flow 2 |

Webhook events the app subscribes to: `pull_request`, `check_run`, `deployment_status`. GitHub Apps receive `installation` and `installation_repositories` automatically, without subscribing.

## Sync

`GET /api/w/{ws}/sync/bootstrap` returns

```json
{"cursor": 1893, "work_items": [...], "changes": [...], "check_runs": [...], "releases": [...], "edges": [...], "members": [...]}
```

`GET /api/w/{ws}/sync?cursor=1893` upgrades to a WebSocket. Server-to-client messages:

```json
{"type": "upsert", "entity": "work_item", "seq": 1894, "mutation_id": "5f0c...-m18", "row": {"...": "..."}}
{"type": "remove", "entity": "edge", "seq": 1895, "key": {"...": "..."}}
{"type": "caught_up", "cursor": 1895}
{"type": "members_changed"}
{"type": "resync", "reason": "cursor_too_old"}
```

- Rows are sent in `seq` order. The client stores `cursor` = the last `seq` it applied.
- `resync` tells the client to drop its cache and call bootstrap again (for example after a projection rebuild).
- `members_changed` tells the client to refetch `GET /api/w/{ws}`; members aren't event-sourced.
- Client-to-server messages: only `{"type": "ping"}`. Writes go through the REST endpoints, which keeps sync one-way and simple.

## Ops

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/healthz` | 200 if the process is up |
| GET | `/readyz` | 200 if the database is reachable and migrations are current |
| GET | `/metrics` | Prometheus; protect it or bind it to an internal port in production |
