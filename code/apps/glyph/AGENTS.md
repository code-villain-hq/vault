# Glyph: notes for coding agents

Glyph translates an app's i18n strings with local models, and a team reviews
what the models propose. It is built the way Forge is: a Rust server on
Postgres with the studio's sync engine (`code/kit/sync`, the one Forge uses), a React
app that keeps a local copy of the workspace, and a Tauri shell around it.
When something here works like Forge, Forge's code is the reference; keep
the two alike rather than letting them drift apart.

## Shared design and brand

The vault's [`studio/design/`](../../../studio/design/README.md) holds the shared design
system. Read [`studio/design/principles.md`](../../../studio/design/principles.md) before
any UI change and name the principle the change serves. Glyph uses
Nightshade in night and day, Atkinson Hyperlegible Next for text, Atkinson
Hyperlegible Mono for keys and code, and Grenze Gotisch only for the
wordmark. The shared controls are the `ui-*` classes of
[`studio/design/components.css`](../../../studio/design/components.css); use them, never a
local copy of a recipe. `studio/design/lint` fails CI on literal colours, sizes or
radii in CSS or classes.

The web app bundles snapshots of the tokens, `components.css` and the
fonts in `apps/web/src/design/`, and the Glyph mark from
[`studio/brand/mark/glyph.svg`](../../../studio/brand/mark/glyph.svg) as its icon. Run
`bun run design:sync` after changing them in the vault; don't edit the
snapshots by hand. `bun run check` fails when they've drifted.

## Commands

- `bun run db`: start Postgres (Docker, port 5434). `bun run db:reset` wipes it.
- `bun run dev`: Rust server (:7480) plus web app (:5174, proxies `/api`).
- `bun run dev:fake-model`: `bun run dev` with a stand-in model on :7483
  (no download, no GPU). It answers in a moment with the source text, the
  target language and the glossary terms it was told to use.
- `bun run dev:desktop`: Tauri app against the running dev server.
  `bun run build:desktop` packages it. The app runs Glyph itself, on its own
  Postgres (7486) and server (7485), for the workspaces on this Mac, and
  signs in to servers for the others. Its data, and the models added in
  Settings (`models.toml`), are in
  `~/Library/Application Support/eu.codevillain.glyph`.
- `bun run app`: the whole app in Docker at http://localhost:7490 (one
  image: server plus built web app). `bun run app:logs` follows it.
- `bun run seed`: local test accounts (an admin, a member and a guest of
  "Villain Corp", the guest in its first language) and the made-up strings in `demo/`; the password is in
  `scripts/seed-demo.ts`. Needs `bun run dev` running, with passwords on
  (`GLYPH_OIDC_ISSUER` empty; README, "Signing in with Code Villain").
- `bun run gen:types`: regenerate `packages/types` from the Rust models.
- `bun run check`: fmt, clippy, Rust tests, types and design copies up to
  date, Biome, typecheck, bun tests, web build. It must pass before you
  finish. CI runs it on every push that touches Glyph
  (`.forgejo/workflows/glyph.yml`).
  `GLYPH_CHECK_SKIP_DESKTOP=1` leaves the desktop app out.

Rust tests need Postgres running (`bun run db`); `DATABASE_URL` comes from
`.cargo/config.toml`.

## Models

The server reads its models from a TOML file (`GLYPH_MODELS`, default
`./models.toml`; start from `models.example.toml`). Each entry names an
OpenAI-compatible server by `endpoint`. With `model_path`, or `hf_repo` and
`hf_file`, a signed-in user can start and stop it from Settings, which runs
`llm_binary` (llama-server) on the endpoint's port. A language pair picks a
model by name; a pair without one uses the first.

On the desktop app's own server, Settings → Models adds and removes models,
written to `models.toml` in the app's data folder; Glyph picks the port
(7481 to 7499) and the context size. Any other server refuses both
(`POST /api/models`, `DELETE /api/models/{name}`) with 403: its models stay
in its file, since the file names programs for the server to run. A
packaged app's PATH is short, so llama-server is also looked for in
`/opt/homebrew/bin` and `/usr/local/bin`.

Every model call is told the glossary terms its text contains, the
placeholder rules and the key's context. A text that leaves out a term is
retried with a correction prompt, and still flagged in Review if it misses.

## Runs and suggestions

Translating is a run (`TranslationRun`): the app asks the server for one
(`POST /api/w/{id}/runs`) and a worker translates key by key. Its progress
and results are synced records, written through the sync engine, so every
client sees them arrive. A Suggest run makes a `Suggestion` per key for
review; accepting one writes the translation and deletes the suggestion in
one transaction, and rejecting it deletes it. A Fill run writes
translations straight away.

## Import and export

Import, export and the namespace operations (rename, delete) run in the app,
on its local copy, and write back as sync transactions in batches. Import
takes i18next JSON, flat or nested; keys written `namespace:key` go to their
namespace. Export gives the file i18next loads for one namespace, or one
file with `namespace:key` keys for all of them, plus a meta file with each
text's confidence and origin.

## Layout

- `crates/domain`: models, validation, sync protocol types (no database).
- `crates/sync`: Glyph's rules for the studio's sync engine (`code/kit/sync`):
  admins' models, texts written by hand, what a language takes with it.
- `crates/llm`: model client, prompts, reading the models file, starting
  llama-server.
- `crates/server`: axum server: HTTP API, WebSocket feed, accounts,
  invites, runs, models. `serve.rs` runs it on a listener until told to
  stop, and stops the models it started, for the server binary and the
  desktop app alike; `local.rs` is the one person of the desktop app's own
  Glyph, who has no password.
- `packages/types`: generated from Rust. Never edit by hand.
- `apps/web`: React + Base UI + Tailwind. Its workspaces come from places
  (`server.ts`): this Mac, the server it came from, or servers the desktop
  app signed in to, each with its own session.
- `apps/desktop`: Tauri shell. `this_mac.rs` starts Glyph on Postgres for
  the workspaces on this Mac, with `code/kit/desktop`.
- `migrations`: SQL schema, applied by the server on start.
- `demo`: made-up strings and glossary for the seed. Real strings never go
  in this repository: it is public.
- `scripts`: dev, check, seeding, type generation, design sync.

## Rules

- Every write to a synced model goes through `glyph_sync::apply_transaction`,
  whether it comes from the app or a run. Never write those tables any
  other way, or clients won't hear about it.
- Clients may only delete a `Suggestion`; the server makes them. `Language`
  and `LanguagePair` writes are for admins.
- Wire field names are snake_case, the same as the columns.
- Change the schema with a new numbered migration; don't edit one that has
  been committed. If it changes synced records directly, end it with
  `UPDATE workspaces SET sync_epoch = gen_random_uuid();` so clients resync.
- Changed a Rust type that's exported to TypeScript? Run `bun run gen:types`.
- Workspace endpoints go under `/api/w/{workspace_id}` and take the `Member`
  extractor, which checks membership. Admin-only ones call `require_admin()`.
- Test behaviour, not implementation: the sync engine against a real
  Postgres (`#[sqlx::test]`), the HTTP API through the router
  (`crates/server/tests`), with a stand-in model where a run needs one.

## Guests

A guest translates only the languages an admin adds them to
(`language_access`, set with `glyph_sync::set_languages`), and reads the
source language. They see those languages' translations, suggestions,
glossary terms, pairs and runs, and the people who share a language with
them, plus the admins and members. They change no source text, start no
run for another language, and see no Members. `model.rs`'s `guest_view`
holds what they see; `rules.rs` (`check_guest_language`) what they write.

## Adding a synced model

1. A migration for its table, with `id`, `workspace_id`, `created_at` and
   `updated_at` columns.
2. In `crates/domain`: the record, create and patch types, a `ModelKind`
   variant, and the `match` arms the compiler asks for.
3. In `crates/sync/src/rules.rs`: any fields only the server may set.
   In `crates/domain`'s `guest_view`: what a guest sees of it.
4. `bun run gen:types`, then add it to `Schema` in `apps/web/src/sync.ts`.
