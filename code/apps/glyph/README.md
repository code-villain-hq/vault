# Glyph

*A Code Villain product.*

Glyph translates an app's i18n strings with local models, and a team
reviews what the models propose before it ships.

- Keys in namespaces, as i18next has them, in every language the
  workspace adds.
- Models you run yourself: any OpenAI-compatible server, such as
  llama-server with a Nordic fine-tune. Glyph can start and stop them.
- A glossary per language pair: the terms a translation must use, and the
  other forms that count. Models are told them, and texts that leave one
  out are flagged.
- Review: suggestions with their confidence, accepted one by one or in bulk.
- Import and export i18next JSON, flat or nested.
- Everyone sees the same workspace live. The app keeps a local copy, so
  it opens at once.

## Stack

The same as Forge and Layer: a Rust server (axum) on Postgres, with the
studio's sync engine (`code/kit/sync`); a React app (Vite, Bun); and a
Tauri desktop shell.

## Running it

Needs Rust, Bun and Docker.

```bash
bun install
bun run db               # Postgres in Docker, port 5434
bun run dev:fake-model   # Glyph for Acme Inc on :5174, with a stand-in model
bun run seed             # in another terminal: demo strings, Gus in a language
```

Then open http://localhost:5174. For a real model, copy
`models.example.toml` to `models.toml` and run `bun run dev` instead.

`bun run dev` runs Glyph for a made-up company, Acme Inc: dev-id
(`code/kit/dev-id`) stands in for its account, so you sign in as anyone in
its `people.toml` (Ada Admin, Max Member, Gus Guest, or Nell New, who isn't
let in). Acme's data has a database of its own, `glyph_acme`. With a
`glyph.toml`, or `GLYPH_OIDC_ISSUER` in `.env`, it runs with those settings
and the `glyph` database instead.

`bun run app` runs the whole thing in Docker at http://localhost:7490.
Copy `models.example.toml` to `models.toml` first: Docker mounts it. It
needs a company's settings (below) in a `.env` file next to
`docker-compose.yml`, which it reads as `GLYPH_COMPANY`,
`GLYPH_OIDC_ISSUER` and so on. It also takes `GLYPH_LISTEN`, the address
Docker publishes it on (default `127.0.0.1:7490`).

## Running it for a company

A server runs for one company, from one settings file: copy
`glyph.example.toml` to `glyph.toml` (or pass `glyph-server --config
<file>`), and fill in the company's name, Glyph's address, the database,
the company's account and a SCIM token. Every setting can come from the
environment too, which wins: `[oidc] issuer` is `GLYPH_OIDC_ISSUER`. A
missing or unknown setting stops the server, naming it.

In the company's account (Pocket ID, or any OpenID Connect provider with
SCIM):

1. Register Glyph as a confidential client with the callback
   `<public_url>/api/auth/oidc/callback`.
2. Make the groups `glyph-users`, `glyph-admins` and `glyph-guests` (or
   the names under `[groups]`), and put people in them.
3. Turn on SCIM for Glyph's client, with the endpoint
   `<public_url>/api/scim/v2` and the settings' token.

On its first start Glyph makes the company's workspace, with English as
its source language once its first person is in. The account sends its
people within minutes, and they sign in with "Sign in with <company>".
Nobody signs up, and there are no invites or passwords: someone in none of
the groups is told to ask the company's admin. A guest's languages are
still added in Members.

See [AGENTS.md](AGENTS.md) for the commands, the layout and the rules.

## License

MIT
