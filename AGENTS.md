# Code Villain Vault: notes for coding agents

The vault holds the whole studio. `studio/` is the company: its look, its words, its decisions. `code/` is everything that builds.

- `studio/brand/`: decisions about look and voice. Change these when the feeling should change. A new palette or a new tone starts here.
- `studio/design/`: principles, tokens, and reference layouts taken from `studio/brand/`. Apps implement them in their own toolkit. Caster is AppKit; Forge, Layer, Glyph, Raven and the site are web; the Code Villain theme (`code/apps/theme`) dresses VS Code, Cursor, Neovim, WezTerm and the Starship prompt. They share the decisions. Read `studio/design/principles.md` before any UI change, in any app, and `studio/design/behaviour.md` before changing what an app does: sign-in, errors, empty states, keys, settings, sync. An issue isn't done until its app follows both.
- `studio/content/`: blog posts and social posts. The site renders the blog. The posts live in `studio/content/blog`.
- `code/apps/<app>/`: one product. Read `code/apps/<app>/AGENTS.md` before working there, and run commands from that folder.

Nightshade is every app's default theme, in night and day. An app's theme picker, like Layer's, can add themes that fill the same colour names. The studio surfaces — the mark, the site, the blog, social — follow `studio/brand/` and stay night.

A change to how Code Villain feels starts in `studio/brand/` or `studio/design/`, then the apps catch up. A change to one product's behavior stays in that app.

A pull request for a Forge issue says "Fixes <ID>" in its body. Pull requests pushed to `refs/for/main` (AGit) have no branch for Forge to read, so without it the issue never reaches review and isn't closed by the merge.

## How an app is built

Every app except Caster, the theme and the site is built the same way:

- A server and its clients: a Rust server on Postgres, a React web app, and a Tauri desktop app.
- Workspaces, as Forge has them: a team's space with its members and roles.
- One sync engine. Each client keeps a local copy of a workspace and works on it offline.
- A desktop app that runs on its own, with no server. It starts its own server on its own Postgres for the workspaces on that Mac.

The sync engine lives in `code/kit/sync` (its `AGENTS.md` says how an app uses it). Forge, Glyph and Raven use it. Layer uses it for the setup a team shares: its desktop app keeps that in a workspace, and the rest of its state in local files.

The desktop app's own server starts with `code/kit/desktop`: Postgres 18 bundled in the app, on loopback, and the app's server beside it. Forge, Glyph and Raven have it (`apps/desktop/src-tauri/src/this_mac.rs`), and Layer, one per profile (`src-tauri/src/workspace.rs`).

## Running an app for a company

A server runs for one company, from one settings file: `<app>.toml` (`<app>.example.toml` shows it), or `--config <file>`, with the environment over it (`[oidc] issuer` is `<APP>_OIDC_ISSUER`). `code/kit/settings` reads it; a missing or unknown setting stops the server, naming it. Required: `company`, `public_url`, the database, the company's account (`[oidc] issuer`, `client_id`, `client_secret`) and a SCIM token (`[scim] token`). Optional: `[groups] users`, `admins` and `guests`, by default `<app>-users`, `-admins` and `-guests`. `studio/design/behaviour.md` ("Who gets in") says what people see. Every app installs the same way: the settings file, a container image (`Dockerfile`, built from `code/`), and its README's "Running it for a company".

- The account signs people in over OpenID Connect (`crates/server/src/oidc.rs`) and is the only way in: no passwords, sign-up or invites. The studio's own is Pocket ID at id.codevillain.eu; a company brings its own.
- It also sends its people and groups over SCIM 2.0 to `/api/scim/v2`, with `code/kit/scim` (its `AGENTS.md` says how). On its first start the server makes the company's workspace, and the groups its settings name say who's in it and as what (`crates/server/src/company.rs`, `groups.rs`).
- In the account: register the app as a confidential client with the callback `<public url>/api/auth/oidc/callback`, allow only the app's groups on it, and turn on SCIM with the endpoint `<public url>/api/scim/v2` and the settings' token.
- The desktop app's own server, for the workspaces on this Mac, signs in to nothing and reads no settings file.
- Local development runs every app for a made-up company, Acme Inc, with `code/kit/dev-id` standing in for its account (sign-in at 127.0.0.1:7280, people in `people.toml`). Each app's `bun run dev` starts it, on a database of the app's own (`<app>_acme`); with your own `<app>.toml` or `<APP>_OIDC_ISSUER` in `.env`, it uses those instead.
- For the next app, copy Forge's: `config.rs`, `company.rs`, `groups.rs`, `oidc.rs` and their tests, the migrations for `users.oidc_sub` and one company, `/api/auth/providers`; on the web side `useProviders`, the sign-in button, `DesktopSignIn` and the sign-in problems in `auth.tsx`; for the desktop app the `<app>://` deep link (tauri-plugin-deep-link 2.6.1) and `openInBrowser`/`onAppLink` in `platform.ts`; and `scripts/dev.ts`, `forge.example.toml` and the `Dockerfile`.

## Specs are checked against the code

Forge's specs live in Forge, not in this repository. Sessions here get Forge's tools from `.mcp.json` (the token comes from `scripts/forge-mcp-headers`); find what the specs say with one `search_specs` that lists the word and its alternatives. A scheduled drift check (Claude Code on the studio's Mac) reads each new commit on `main`. For every Forge service a commit touches, it compares the service's specs with the code. Where the code no longer does what a requirement or scenario says, it files `Spec drift: "<spec>" no longer matches the code` in that team's Triage, or comments on the one already open. It files nothing for:

- behaviour no requirement deals with: that is only named in the check's run summary;
- a point that open work already covers: an issue on that service in Triage, Backlog, Todo, In Progress or In Review, about the same requirement. When a commit from another issue meets part of that work, the check says so once, in a comment on the covering issue.

So when a change alters behaviour a spec describes, write the spec change with it: on the issue's spec change, or by asking the PO. Otherwise the change comes back as a drift issue. The check names other issues by key and title only, and never edits specs, code or other issues.
