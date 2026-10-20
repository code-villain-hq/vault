# Code Villain Vault: notes for coding agents

The vault holds the whole studio. Apps are one part of it.

- `brand/`: decisions about look and voice. Change these when the feeling should change. A new palette or a new tone starts here.
- `design/`: principles, tokens, and reference layouts taken from `brand/`. Apps implement them in their own toolkit. Caster is AppKit; Forge, Layer, Glyph and the site are web; the Code Villain theme (`apps/theme`) dresses VS Code, Cursor, Neovim and WezTerm. They share the decisions. Read `design/principles.md` before any UI change, in any app.
- `content/`: blog posts and social posts. The site renders the blog. The posts live in `content/blog`.
- `apps/<app>/`: one product. Read `apps/<app>/AGENTS.md` before working there, and run commands from that folder.

Nightshade is every app's default theme, in night and day. An app's theme picker, like Layer's, can add themes that fill the same colour names. The studio surfaces — the mark, the site, the blog, social — follow `brand/` and stay night.

A change to how Code Villain feels starts in `brand/` or `design/`, then the apps catch up. A change to one product's behavior stays in that app.

## Signing in with Code Villain

One studio account signs people in to every app: Pocket ID at id.codevillain.eu, over OpenID Connect. The provider says who someone is; each app keeps its own workspaces, roles and invites. Forge and Glyph have it. For the next app:

- Copy Forge's `crates/server/src/oidc.rs` and `tests/oidc_login.rs`, the `users.oidc_sub` migration, the `passwords_open` guard and `/api/auth/providers`. On the web side copy `useProviders`, `CodeVillainButton`, `DesktopSignIn` and `OIDC_PROBLEMS` from `auth.tsx`. For the desktop app copy the `<app>://` deep link (tauri-plugin-deep-link 2.6.1) and `openInBrowser`/`onAppLink` in `platform.ts`. Glyph shows the changes a copy needs.
- In Pocket ID, register a confidential client with the callback `<public url>/api/auth/oidc/callback`, and a group for the people allowed in.
- Set `<APP>_OIDC_ISSUER`, `<APP>_OIDC_CLIENT_ID`, `<APP>_OIDC_CLIENT_SECRET` and `<APP>_PUBLIC_URL`. With them set, passwords are off. Without them the app keeps its passwords, as local development does.

## Specs are checked against the code

Forge's specs live in Forge, not in this repository. A scheduled drift check (Claude Code on the studio's Mac) reads each new commit on `main`. For every Forge service a commit touches, it compares the service's specs with the code. Where the code no longer does what a requirement or scenario says, it files `Spec drift: "<spec>" no longer matches the code` in that team's Triage, or comments on the one already open. It files nothing for:

- behaviour no requirement deals with: that is only named in the check's run summary;
- a point that open work already covers: an issue on that service in Triage, Backlog, Todo, In Progress or In Review, about the same requirement. When a commit from another issue meets part of that work, the check says so once, in a comment on the covering issue.

So when a change alters behaviour a spec describes, write the spec change with it: on the issue's spec change, or by asking the PO. Otherwise the change comes back as a drift issue. The check names other issues by key and title only, and never edits specs, code or other issues.
