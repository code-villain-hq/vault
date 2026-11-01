# Company sign-in: notes for coding agents

`code-villain-auth` is the shared OpenID Connect sign-in and session helpers
every studio app server uses. Forge, Glyph, Raven, Almanac and Layer call it.

## What it does

- Discovers the company's provider, runs the browser and desktop PKCE flow,
  and hands the desktop app a one-time code (`OidcLogin`, `start`,
  `callback`, `desktop`).
- Links a provider account to a row in `users` (`LinkPolicy`), and only lets
  someone in when they are in a workspace.
- Builds session tokens and cookies (`start_session`, `session_cookie`,
  `session_token`), named per app.
- The desktop app's own person (`this_mac_person`, `ensure_this_mac_person`,
  `THIS_MAC_EMAIL`): no password, no real email.

## Using it in an app

1. Depend on `code-villain-auth = { path = "../../kit/auth" }`.
2. Implement [`OidcHost`](src/oidc.rs) for the app's `AppState`: cookie and
   header names, `LinkPolicy`, `session_cookie`, and `signed_in` after a
   desktop handoff.
3. Build `OidcLogin::new(…, host.user_agent())` from settings (or
   `from_parts` with the issuer, client id/secret and public URL).
4. Mount `start`, `callback` and `desktop` at `/api/auth/oidc*`.

Forge uses `LinkPolicy::PeopleOnly` so agents can't sign in. The others use
`LinkPolicy::AnyUser`.

## Changing it

A change here changes every app that uses it. `cargo test` here, then
`bun run check` in each app.
