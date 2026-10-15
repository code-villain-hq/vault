# Forge: notes for coding agents

Forge is an AI-first issue tracker in the spirit of Linear. Its specs and
decisions live in Forge itself, not in this repository: read them through the
Forge MCP tools before larger changes. `get_issue` lists the ones an issue
follows; `list_specs` also finds the product-wide ones ("Product: what Forge
is and why", "Product: concepts", "Architecture overview"). Decisions are
records there too (the early ones keep their numbers, "D1: …" to "D35: …");
record a new one with `save_decision` when you settle something.

Specs change only when the work is done (decision "Specs change only when the
work is done"). Don't edit a spec: before you start an issue, write its spec
change with `save_spec_change` (decision "A spec change is written first, and
only one issue changes a requirement at a time"); Forge won't let an agent
start an issue without one. It names the requirements the issue adds,
changes, removes or renames in each spec, in OpenSpec's delta format, and the
decisions it replaces; or it says "No spec change" and why. Keep requirements
small, one rule each. `get_issue` shows it, and it takes effect once the
issue is done.

## Commands

- `bun run db`: start Postgres (Docker, port 5433). `bun run db:reset` wipes it.
- `bun run dev`: Rust server (:7380) plus web app (:5173, proxies `/api`).
- `bun run dev:desktop`: Tauri app against the running dev server.
  `bun run build:desktop` packages it (it then talks to a server chosen at
  sign-in; see the "Desktop app" spec).
- `bun run app`: the whole app in Docker at http://localhost:7390 (one image:
  server plus built web app). `bun run app:logs` follows it.
- `bun run seed`: local test accounts (admin and member of "Forge"); the
  password is in `scripts/seed-dev.ts`. Needs `bun run dev` running.
  `bun run seed:dogfood` then describes this repository as services, for
  drift checks on Forge itself.
- `bun run dev:fake-ai`: `bun run dev` with the AI features on, against a
  stand-in model (no API key, no cost).
- `bun run runner:install`: on the PO's Mac, install the launch agent that
  starts Claude Code when Forge wakes it (`scripts/agent-runner/README.md`).
- `bun run reset-password <email>`: set a new password for someone.
- `bun run backup`: dump the database and attachments to `backups/`
  (restoring: see the script).
- `bun run gen:types`: regenerate `packages/types` from the Rust models.
- `bun run check`: fmt, clippy, Rust tests, types up to date, Biome,
  typecheck, bun tests, web build. It must pass before you finish.
  GitHub runs it on every pull request too (`.github/workflows/check.yml`),
  merged with the latest `main`, and shows the result on the pull request.
  (GitHub can't block merging a red pull request on this private repository's
  plan, so look before merging.) Locally, `FORGE_CHECK_SKIP_DESKTOP=1` leaves
  the desktop app out, as GitHub's main job does.

Rust tests need Postgres running (`bun run db`); `DATABASE_URL` comes from
`.cargo/config.toml`.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: the sync engine: applying transactions, change log, snapshots.
- `crates/server`: axum server: HTTP API, WebSocket feed, accounts, invites,
  MCP, the GitHub connection, drift checks.
- `crates/ai`: model provider (Claude), agent loop, read-only repository
  access.
- `packages/sync-client`: client side of sync (local store, outbox, feed).
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. `apps/desktop`: Tauri shell.
- `migrations`: SQL schema, applied by the server on start.
- `scripts`: dev, check, seeding, type generation, and Claude Code's runner
  (`agent-runner`).

## Rules

- Every write to a synced model goes through `forge_sync::apply_transaction`,
  whether it comes from the UI, an agent or the AI. Never write those tables
  any other way, or clients won't hear about it.
- Wire field names are snake_case, the same as the columns.
- To add a synced model, follow "Adding a synced model" below.
- Change the schema with a new numbered migration; don't edit one that has
  been committed. If it changes synced records directly, end it with
  `UPDATE workspaces SET sync_epoch = gen_random_uuid();` so clients resync.
- Changed a Rust type that's exported to TypeScript? Run `bun run gen:types`.
- Workspace endpoints go under `/api/w/{workspace_id}` and take the `Member`
  extractor, which checks membership. Admin-only ones call `require_admin()`.
- Test behaviour, not implementation: the sync engine against a real Postgres
  (`#[sqlx::test]`), the HTTP API through the router (`crates/server/tests`),
  the sync client against `FakeServer`.

## Adding a synced model

1. A migration for its table, with `id`, `workspace_id`, `created_at` and
   `updated_at` columns.
2. In `crates/domain`: the record, create and patch types, a `ModelKind`
   variant, and the `match` arms the compiler asks for.
3. In `crates/sync`: any fields only the server may set (`add_server_fields`).
4. `bun run gen:types`, then use it from the app.
