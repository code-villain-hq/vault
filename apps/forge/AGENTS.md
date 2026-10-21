# Forge: notes for coding agents

Forge is an AI-first issue tracker in the spirit of Linear. Its specs and
decisions live in Forge itself, not in this repository: read them through the
Forge MCP tools before larger changes. `get_issue` lists the ones an issue
follows; `list_specs` also finds the product-wide ones ("Product: what Forge
is and why", "Product: concepts", "Architecture overview"). To find what the
specs say about something, `search_specs` returns only the requirements that
mention it; `get_spec` with `outline` lists a spec's requirements. Decisions are
records there too (the early ones keep their numbers, "D1: …" to "D35: …");
record a new one with `save_decision` when you settle something.

## Shared design and brand

The vault's [`design/`](../../design/README.md) holds the shared design
system. Read [`design/principles.md`](../../design/principles.md) before
any UI change and name the principle the change serves. Color and type
tokens live in [`design/tokens/`](../../design/tokens/README.md); the JSON
is their source, and `build.py` generates CSS and Swift. Reference layouts
and component rules live alongside the tokens.

Nightshade is generated from `theme.base`, `theme.accent` and
`theme.contrast` in `design/tokens/color.json`; the shared `theme.py`
calculates night/day roles. Change the inputs or shared recipes rather
than adding per-view colours. See [`theme.md`](../../design/tokens/theme.md).
Regenerate shared outputs first, then run `bun run design:sync` here.
Forge's numerical snapshot comes from generated `color.resolved.json`.

[`brand/`](../../brand/README.md) holds the studio's identity and voice,
including [`color.md`](../../brand/color.md) and
[`type.md`](../../brand/type.md). Forge uses Nightshade in night and day,
Atkinson Hyperlegible Next for interface text, Atkinson Hyperlegible Mono
for code, and Grenze Gotisch only for display headings at 28px and above.
Poison is the brand: it fills the primary action (with dark ink), tints
selection and lights live dots (`bg-signal`). Cloak, lifted, marks focus. Links use ink, bold and
underlined.

The sidebar wordmark uses Grenze Gotisch 700 at 28px, following
[`design/layouts/app-window.md`](../../design/layouts/app-window.md).
It always reads "Forge"; the workspace picker is a separate control.
The desktop app hides the native title bar and shows the traffic lights in
the wordmark's place (`isDesktop` in `layout.tsx`).
The frame uses `bg` for the sidebar and toolbar and `panel` for content;
below 560px the sidebar becomes a horizontal navigation row. The shared
control rules are in [`design/components.md`](../../design/components.md):
28px buttons (secondary ones filled), underlined links, tabs with a line under the current one, segmented controls, tinted chips and status pills, fields with focus rings, and
elevation through tone. They're implemented once in
[`design/components.css`](../../design/components.css), synced into
`src/design/`: use its `ui-*` classes (`buttonClass()` composes
`ui-button`), never a local copy of a recipe. `design/lint` fails CI on
literal colours, sizes or radii in CSS or classes. Shape tokens are bundled with color and type.
Tint recipes stay translucent: `status-tint`, `chip-tint` and
`highlight-tint` (`bg-highlight`, the active menu option) blend with their
host. Status text uses separate shared ink tokens; custom workflow
colours use `statusStyle` to check ink against the composite without
changing the stored hue. Dialogs use `dialog`, with `dialog-field` and
`dialog-field-hover` for form controls inside them. The toolbar names the
view; do not add a duplicate visible content heading. Preserve an accessible
`h1`, and keep a record's actual title visible.
Forge is the proving ground for these shared rules. Implement the system
and test it in real views; bring broader changes to the overall design
back for discussion rather than introducing product-specific exceptions.
The Forge mark is the outlined gothic F with the strike from
[`brand/mark/forge.svg`](../../brand/mark/forge.svg); `design:sync` copies it
to the web icon. Don't substitute a typed initial or the old blue F.

The web app bundles snapshots in `apps/web/src/design/` so standalone and
Docker builds include the same tokens and licensed fonts. Run
`bun run design:sync` from this folder after changing shared tokens or
the fonts in `apps/site/assets/fonts/`; don't edit the snapshots by hand.
`bun run check` fails when they or the shared outputs have drifted.
A studio-wide visual change starts in `brand/` or `design/`, then Forge
implements it. Product behavior stays here.

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
  password is in `scripts/seed-dev.ts`. Needs `bun run dev` running, with
  passwords on (`FORGE_OIDC_ISSUER` empty; README, "Signing in with Code Villain").
  `bun run seed:dogfood` then describes Forge's code in the vault as
  services, for drift checks on Forge itself.
- `bun run dev:fake-ai`: `bun run dev` with the AI features on, against a
  stand-in model (no API key, no cost).
- `bun run runner:install`: on the PO's Mac, install the launch agent that
  starts Claude Code when Forge wakes it (`scripts/agent-runner/README.md`).
- `bun run reset-password <email>`: set a new password for someone.
- `bun run backup`: dump the database and attachments to the backup folder,
  outside the repository: `FORGE_BACKUP_DIR`, or else `backup/forge` next to
  the vault's checkout (restoring: see the script).
- `bun run gen:types`: regenerate `packages/types` from the Rust models.
- `bun run check`: fmt, clippy, Rust tests, types up to date, Biome,
  typecheck, bun tests, web build. It must pass before you finish.
  The vault's CI runs it on every push that touches Forge
  (`.forgejo/workflows/forge.yml`, on the MacBook's runner). It runs on push
  only, not on pull requests, so a pull request shows no result: run the
  check before you open one. Locally, `FORGE_CHECK_SKIP_DESKTOP=1` leaves the
  desktop app out.

Rust tests need Postgres running (`bun run db`); `DATABASE_URL` comes from
`.cargo/config.toml`.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: the sync engine: applying transactions, change log, snapshots.
- `crates/server`: axum server: HTTP API, WebSocket feed, accounts, invites,
  MCP, the GitHub and Forgejo connections, Code Villain sign-in (`oidc.rs`),
  drift checks.
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
