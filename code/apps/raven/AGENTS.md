# Raven: notes for coding agents

Raven watches the studio's running apps on three levels: Health for
anyone, Technical for developers, Product for the PO (not built yet). The
README says what each level shows and what's decided. It is built the
way Glyph is: a Rust server on Postgres, a React app (`apps/web`) and a
Tauri desktop shell (`apps/desktop`), without sync.
When something here works like Glyph or Forge, their code is the
reference; keep them alike.

## Shared design and brand

Read [`studio/design/principles.md`](../../../studio/design/principles.md) before any UI
change and [`studio/design/behaviour.md`](../../../studio/design/behaviour.md) before
changing what Raven does. Use the `ui-*` classes of
[`studio/design/components.css`](../../../studio/design/components.css) and token-backed
Tailwind classes; `studio/design/lint` fails on literal colours, sizes or radii.
Status colours go only where something needs a look: a calm app, bar or
row stays quiet. Charts follow the dataviz rules: one axis, thin marks,
a legend, a hover tooltip.

The web app bundles snapshots of the tokens, `components.css`, the fonts
and the Raven mark from [`studio/brand/mark/raven.svg`](../../../studio/brand/mark/raven.svg).
Run `bun run design:sync` after changing them in the vault.

## Commands

- `bun run db`: start Postgres (Docker, port 5435). `bun run db:reset` wipes it.
- `bun run dev`: Rust server (:7580) plus web app (:5175, proxies `/api`).
  It sets `RAVEN_VERSION` to the commit, so calls carry their version.
- `bun run dev:desktop`: the Tauri app against the running dev server.
  `bun run build:desktop` packages it; the packaged app asks for a server
  (the Rust port, 7580, not Vite's).
- `bun run seed`: the local account and a few dozen calls; needs
  `bun run dev`. The password is in `scripts/seed.ts`.
- `bun run check`: fmt, clippy, Rust tests, design copies, Biome,
  typecheck, web build. It must pass before you finish.
  `RAVEN_CHECK_SKIP_DESKTOP=1` leaves the desktop app out.
- `cargo run -p raven-server -- ingest-key <app>`: a new key an app sends
  its calls with. `reset-password <email>` sets a new password.

## How data comes in

- Raven records its own calls: `telemetry::record_calls` makes a span per
  request named `METHOD /route`, `telemetry::timed` a child span per
  database query, and the `Capture` layer keeps the logs. A background
  writer stores them, so a request never waits on its own record.
- Other apps send OpenTelemetry over HTTP as JSON (`otlp.rs`), with a
  Bearer key; Raven stores only its hash.
- `health::watch` checks every app every 30 seconds and clears records
  older than 30 days.

## The desktop app

The same web app in a Tauri window, as Glyph's. It keeps its session
token in the system keychain and sends it as a bearer token with
`x-raven-client: desktop`; the server allows the `tauri://localhost`
origin. Signing in with Code Villain happens in the browser, and comes
back through a `raven://signed-in` link with a one-time code only this
app can trade (`oidc.rs` says how). Deep links only reach an app macOS
has registered from a real folder: try a test build, with its own
identifier, from `~/Applications`.

## Rules

- Wrap every query in `timed(...)`, so Technical shows Raven's own
  queries too. sqlx 0.9 refuses SQL built at runtime: write each
  statement as a `const`.
- Wire field names are snake_case. Times go out as ISO times, never
  formatted: the page shows them in the reader's time zone.
- Change the schema with a new numbered migration; don't edit one that
  has been committed.
- A level's status always names its reason, and names the action to look
  into when calls are why.
