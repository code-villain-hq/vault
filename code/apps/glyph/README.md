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
bun run dev:fake-model   # server :7480, web :5174, a stand-in model
bun run seed             # in another terminal: test accounts and demo strings
```

Then open http://localhost:5174. For a real model, copy
`models.example.toml` to `models.toml` and run `bun run dev` instead.

`bun run app` runs the whole thing in Docker at http://localhost:7490.
Copy `models.example.toml` to `models.toml` first: Docker mounts it.

## Signing in with Code Villain

Set `GLYPH_OIDC_ISSUER` (`https://id.codevillain.eu`),
`GLYPH_OIDC_CLIENT_ID`, `GLYPH_OIDC_CLIENT_SECRET` and `GLYPH_PUBLIC_URL`
(Glyph's address; Pocket ID sends people back to
`<GLYPH_PUBLIC_URL>/api/auth/oidc/callback`), in your shell or a `.env`
next to `docker-compose.yml`. With them, Code Villain is the only way in
and `bun run seed` can't sign up its accounts. Without them, passwords stay
on. The server also reads `DATABASE_URL`, `GLYPH_ADDR` (default
`127.0.0.1:7480`), `GLYPH_WEB_DIR`, `GLYPH_MODELS` (default
`./models.toml`), `GLYPH_COOKIE_SECURE` (`0` for plain http) and
`GLYPH_TRUST_FORWARDED_FOR` (`crates/server/src/config.rs`). `bun run app`
also takes `GLYPH_LISTEN`, the address Docker publishes it on (default
`127.0.0.1:7490`).

With `GLYPH_SCIM_TOKEN` set too, the account's groups say who is in a
workspace. In Pocket ID, turn on SCIM for Glyph's client, with the
endpoint `<GLYPH_PUBLIC_URL>/api/scim/v2` and the same token, and allow
`glyph-users`, `glyph-admins` and `glyph-guests` on it. Then link the
workspace to `glyph` in Settings → Members → Group. Without the token, invites are the only way in.

See [AGENTS.md](AGENTS.md) for the commands, the layout and the rules.

## License

MIT
