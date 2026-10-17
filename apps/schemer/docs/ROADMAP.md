# Roadmap: v1 in 16 weeks

At 6–8 hours a week. Dates match the spare-time plan. Tick the boxes as you go; Claude Code should update this file when a task is done.

**Who writes it:**

- **[M]** Michael writes the first version (core backend learning); Claude explains, reviews and helps debug.
- **[C]** Claude can write it (boilerplate, config, frontend chores); Michael reviews.
- **[P]** Pair: agree the design together, then either side writes it.

---

## M0: Foundations (weeks 1–2, by 11 Oct 2026)

- [ ] [M] Tour of Go and Go by Example basics: errors, interfaces, goroutines, `context`
- [ ] [C] Repo skeleton as in `ARCHITECTURE.md`; `go.mod` at the repo root (module path `git.codevillain.eu/code-villain/vault/apps/schemer`); `db/embed.go` embedding the migrations
- [ ] [C] `Makefile`: `dev` (runs `migrate-up` first), `test`, `lint`, `migrate-up`, `migrate-down`, `sqlc`, `web-dev`
- [ ] [C] `docker-compose.yml` with Postgres 16; `.env.example`
- [ ] [P] `config` package (environment variables), pgx pool, `/healthz`, `/readyz`
- [ ] [C] goose wired up: `make migrate-up` applies `db/migrations/00001_init.sql`
- [ ] [C] sqlc configured, with a first query file (`db/queries/workspaces.sql`)
- [ ] [C] GitHub Actions CI: `go vet`, `golangci-lint`, `go test`, web build
- [ ] [C] `web/`: Vite, React, TypeScript (strict) and Tailwind skeleton, with a dev proxy to the Go API
- [ ] [M] Read `DATA_MODEL.md` end to end; turn every question into a doc fix or a new ADR

**Done when:** `make dev` starts Postgres, the server and the web app; `/readyz` returns 200; CI is green.

## M1: Core backend and first deploy (weeks 3–6, by 8 Nov 2026)

- [ ] [P] **Week 3: first public deploy.** Decide ADR 0006, write the Dockerfile and deploy pipeline, get a public URL. Deploy every week from here on.
- [ ] [M] `events`: append inside a transaction with the per-workspace advisory lock, idempotency via `ON CONFLICT DO NOTHING RETURNING`, `pg_notify('schemer_events', …)`; tests against real Postgres (testcontainers)
- [ ] [M] `workitems`: create, edit, assign and change-status commands, `Apply`, queries; numbering via a row lock on `workspaces.next_item_number`
- [ ] [P] Undo via a compensating event
- [ ] [P] Search: `websearch_to_tsquery`, trigram fallback for typos; benchmark with 10,000 seeded items (p95 under 50 ms)
- [ ] [C] Register a GitHub OAuth app for sign-in (dev and prod callback URLs); secrets in `.env`
- [ ] [C] `auth`: GitHub OAuth login, sessions, invites by GitHub login, middleware, workspace membership checks
- [ ] [C] `httpapi`: routes from `API.md`, the error format, request ids, `slog`
- [ ] [P] `make rebuild-projections`, plus a test that replaying the log reproduces the live projections
- [ ] [C] Web: sign-in, create workspace, list view, create and edit items (no realtime yet)

**Done when:** on the live URL, two users can sign in and manage items in one workspace, and the replay test is green.

## M2: The GitHub loop (weeks 7–10, by 6 Dec 2026)

- [ ] [C] Register the GitHub App (dev and prod), config, install flow and `/github/setup`
- [ ] [M] Webhook receiver: HMAC verification (constant time), store the delivery, enqueue, return 202
- [ ] [M] `jobs`: claim with `SKIP LOCKED`, backoff, `dead` state, reaper, worker pool with graceful shutdown; concurrency tests
- [ ] [P] Processors for `pull_request` and `check_run`, producing events and projections; payload fixtures in `testdata/`
- [ ] [M] `linker`: item key in title, body or branch; add and remove derived edges; respect `removed_by_user`
- [ ] [M] `status` engine as a pure function (item, linked changes, checks, releases) → next status and blocked flag; a table test for every rule and guardrail in `DATA_MODEL.md`
- [ ] [C] Web: item detail with timeline, board view, command palette, keyboard shortcuts

**Done when:** a real PR moves its item from `todo` through `in_review` to `done` with nobody touching it, and each step shows in the timeline within 5 seconds.

## M3: Realtime, releases, outcomes (weeks 11–13, by 27 Dec 2026)

- [ ] [M] Sync server: bootstrap endpoint, WebSocket stream with cursor catch-up, `LISTEN/NOTIFY` fan-out, `resync`
- [ ] [C] Sync client: Dexie store, WebSocket client with reconnect, optimistic mutations with rollback
- [ ] [P] Releases: `deployment_status` processor, `POST /api/v1/releases`, API tokens
- [ ] [M] `releases.resolve_contents` job: compare API with pagination, `ships` edges, the first-release rule, status engine re-run (→ `released`)
- [ ] [P] Metrics API and the `releases.compute_outcome` job (72-hour window, at least 3 points per side)
- [ ] [C] Web: releases page with contents and outcome
- [ ] [P] Observability: `slog` fields, Prometheus metrics, one alert, an uptime check

**Done when:** two browsers see each other's changes in under 1 second; a real deploy marks its PRs and items as released; a release page shows a before/after metric.

## M4: Hardening and launch (weeks 14–16, by 17 Jan 2027)

- [ ] [P] Rate limiting per token and per IP on public endpoints
- [ ] [C] Playwright end-to-end tests: sign-in, create item, PR-driven status (fixture webhooks), release page
- [ ] [P] Performance test against the budgets in `V1_SCOPE.md`; fix the slowest path
- [ ] [C] Security pass: cross-workspace access tests, secret handling, dependency audit
- [ ] [M] README with architecture diagram, updated ADRs, the launch post
- [ ] [P] Onboard 3 outside teams and track their feedback in the product itself

**Done when:** the v1 success metrics in `V1_SCOPE.md` are being tracked, and the project is on the CV.

---

## Later (v2+), not now

Docs staleness · slow and flaky test tracking · Jira importer · Slack · ideas and decisions with evidence · signals connectors · multiple workspaces, roles, billing.
