# dev-id: notes for coding agents

A server runs for one company, signed in by its account (OpenID Connect)
and given its people and groups (SCIM). dev-id stands in for that account
in local development, for every app: the company is Acme Inc, and its people
are in `people.toml`.

- Sign-in at http://127.0.0.1:7280, as Pocket ID does it: discovery,
  `/authorize` (a page that asks who you are; `login_hint=<id or email>`
  skips it), `/token` with PKCE. Clients `forge`, `glyph`, `raven` and
  `layer`, all with the secret `dev`. ID tokens are HS256 with a known key:
  apps don't check the signature of a token they fetched themselves.
- SCIM: every five seconds it sends each app at its local port (7380, 7480,
  7580, 7680) the people in one of its groups (`forge-users`, `-admins`,
  `-guests`) and all their groups, with the token `dev`. An app that isn't
  running is skipped. Edit `people.toml` while it runs; it sends the change.
- Each app's `bun run dev` starts it, or uses the one already running,
  with `devCompany(app, root, database)` from `dev.ts`: it makes the app's
  own database (`<app>_acme`) and sets the settings below, unless the
  developer has their own (`<app>.toml`, `<APP>_CONFIG` or
  `<APP>_OIDC_ISSUER`). Seed scripts sign in with `signInAs(base, who)`,
  which follows the sign-in without a browser.

Its settings in an app, as `bun run dev` sets them:

```toml
company = "Acme Inc"
public_url = "http://localhost:5173"   # where people reach the app

[oidc]
issuer = "http://127.0.0.1:7280"
client_id = "forge"
client_secret = "dev"

[scim]
token = "dev"
```

Development only: anyone can sign in as anyone. It never runs on a server.
Tests don't use it: each app's `tests/oidc_login.rs` keeps a provider of its
own, for the cases a real provider gets wrong (unverified emails, another
app's token, expired tokens).

`cargo run` here, or `cargo run --manifest-path ../../kit/dev-id/Cargo.toml`
from an app.
