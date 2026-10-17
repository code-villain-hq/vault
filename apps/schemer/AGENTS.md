# CLAUDE.md

Instructions for Claude Code working in this repo. Read this file first, then the docs in the order below.

## What this is

**Schemer** (code name `schemer`) is a Code Villain product: a delivery tool that feeds itself. It keeps work items current from GitHub, CI and deploys, and connects each release to the metric it was meant to move. v1 is the delivery slice: Work item → Change (PR) → Check (CI) → Release, plus a release-metric hook.

It's also Michael's portfolio project for moving from senior frontend to fullstack. **Learning the backend is a goal in itself**, so how we work matters as much as what we build.

## Read in this order

1. `docs/VISION.md`: the problem, the loop, the customer
2. `docs/V1_SCOPE.md`: what v1 does, acceptance criteria, performance budgets
3. `docs/ARCHITECTURE.md`: components, stack, repo layout, key flows
4. `docs/DATA_MODEL.md`: events, projections, graph, derived-status rules
5. `docs/API.md`: endpoints and the sync protocol
6. `docs/adr/`: decisions already made (don't re-litigate them without a new ADR)
7. `docs/ROADMAP.md`: milestones and tasks; work through them in order

## Working agreement

Michael is a senior frontend engineer (14 years; React, TypeScript, Angular, Vue; Node.js; micro-frontends; Playwright). He's **new to Go** and building his backend depth.

- **Follow the task tags in `ROADMAP.md`:**
  - **[M]** Michael writes the first version. Explain the approach and the Go concepts involved, suggest the function signatures and the tests, then review his code. Don't write the implementation unless he asks ("just write it"); if he does, explain the key lines afterwards.
  - **[C]** You write it; Michael reviews. Keep it small and explain anything non-obvious.
  - **[P]** Agree the design together first (a short plan in chat), then either of you writes it.
- **Teach Go through TypeScript.** Where it helps, compare to the TS/Node equivalent (for example `context.Context` vs `AbortSignal`, interfaces vs structural types, errors as values vs exceptions). Skip frontend basics; he knows them.
- **One task at a time.** Plan briefly, implement, test, then tick the box in `ROADMAP.md`. Don't start the next task unasked.
- **Small commits** in Conventional Commits style (`feat(workitems): add status command`).
- **When a decision changes,** write or update an ADR in `docs/adr/` and fix any doc it contradicts in the same change.
- **Visual design** (brand, colours, type, layout direction): propose 2–3 directions and ask Michael before building. He prefers bold and experimental over safe and corporate. Plain functional UI for internal screens is fine without asking.
- **Brand:** Schemer is part of Code Villain, alongside Layer and Caster. The Code Villain direction is "Doom-core": armoured and regal, gunmetal, green and gold, dark theme first with a light theme alongside. Stay inside that direction, and still propose options and ask before building any visual design.
- **Names in code:** folder `apps/schemer`, Go module `git.codevillain.eu/code-villain/vault/apps/schemer`, binary `schemer`, env var prefix `SCHEMER_`. Use "Schemer" in UI copy.

## Commands (created in M0; keep this list current)

```bash
make dev            # Postgres (docker compose) + migrate-up + Go server + Vite dev server
make web-dev        # Vite dev server only
make test           # go test ./... (uses testcontainers) + web unit tests
make lint           # golangci-lint + eslint + tsc --noEmit
make migrate-up     # goose up
make migrate-down   # goose down (one step)
make sqlc           # regenerate typed queries after editing db/queries/*.sql
make rebuild-projections  # replay events into projections (M1)
```

## Invariants: never break these

1. **Events are append-only.** Never `UPDATE` or `DELETE` rows in `events`.
2. **Every write = event + projection in the same transaction.** No direct writes to projection tables outside an `Apply` function.
3. **Everything retryable is idempotent.** Webhook deliveries, client mutations and jobs carry idempotency keys. Job handlers must be safe to run twice.
4. **Every query is scoped by `workspace_id`.** The HTTP layer checks membership. No endpoint may read across workspaces.
5. **Webhook signatures are verified before anything else**, in constant time.
6. **Derived status follows `DATA_MODEL.md` exactly.** The status engine stays a pure function with a table test per rule.
7. **No secrets in the repo.** Configuration comes from environment variables; `.env.example` lists them without values.

## Go conventions

- Standard library first: `net/http` routing with method and path patterns, `log/slog`, `context` everywhere (first parameter).
- `pgx/v5` plus `sqlc`. SQL lives in `db/queries/*.sql`. No ORM, no query builder.
- Errors: return them, wrap with `fmt.Errorf("create work item: %w", err)`, check with `errors.Is`/`errors.As`. No `panic` for normal errors.
- Keep interfaces small, defined where they're used, not where they're implemented.
- No package-level mutable state. Dependencies are passed in structs from `main`.
- `internal/<domain>` owns its commands, `Apply` and queries. `httpapi` only translates HTTP ↔ domain.
- Goroutines always have an owner and a way to stop (context cancellation); shut down gracefully on SIGTERM.

## Frontend conventions

- React, TypeScript `strict`, Vite, Tailwind CSS, TanStack Router, Dexie.
- The UI reads from the Dexie store only; the network updates the store. Mutations are optimistic with a mutation id.
- Keyboard first: every action is reachable from the command palette, and common ones have single-key shortcuts.

## Testing

- Go: table-driven tests. Integration tests run against a **real Postgres** via testcontainers-go. Never mock the database.
- GitHub payloads: recorded JSON fixtures in `testdata/`, never live calls in tests.
- The status engine and the job queue need the most tests (edge cases, concurrency, retries).
- Web: Vitest for logic (sync store, reducers); Playwright in `e2e/` for critical flows.
- A replay test checks that rebuilding projections from `events` equals the live projections.

## Definition of done (per task)

- Tests written and passing; `make lint` clean
- Acceptance criteria from `V1_SCOPE.md` met, where the task touches one
- Docs updated if behaviour or a decision changed
- Checkbox ticked in `docs/ROADMAP.md`

## Don't

- Add services, brokers, Redis, Kubernetes or an ORM (see ADR 0001)
- Build v2+ features (staleness, Jira importer, decisions, signals) before v1 ships
- Add AI features unless Michael asks
- Use data from Michael's employer (for example their Jira or repos). Test with his own repos and fixtures only.
