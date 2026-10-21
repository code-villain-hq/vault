# Forge

An AI-first issue tracker in the spirit of Linear. Forge can read your code, so
an AI can keep issues, specs and decisions in step with what the code actually
does, and coding agents can use Forge instead of keeping specs in the repo.

What it's for and how it's built is written down in Forge itself, as specs
and decisions on Forge's services: start with the specs "Product: what Forge
is and why" and "Architecture overview".

## What works

- **Tracking.** Teams with their own workflows; issues with sub-issues,
  relations, labels, priorities, assignees, due dates, Markdown descriptions,
  comments, @mentions, attachments and full history; projects and
  initiatives; triage; an inbox. List and board layouts, filters, bulk
  edits.
- **Fast and local-first.** Every change shows at once and syncs live to
  everyone; the app keeps working offline and catches up.
- **Keyboard-first.** ⌘K for everything and search; `C` new issue, `F`
  filter, `X` select, `G` then a letter to go places; `?` lists them all.
- **Knowledge.** Services (parts of the code), domains (areas of the
  product) with their specs, and decisions, linked to issues. Specs follow
  OpenSpec's convention and change when the issue that changes them is done.
- **Forge AI** (needs `ANTHROPIC_API_KEY`). Drift checks read a service's
  code and history and suggest fixes where the tracker no longer matches it;
  new issues in triage get suggested properties and duplicates. People
  accept or dismiss every suggestion.
- **Coding agents.** An MCP server with full read and write access, acting
  as an agent member (Settings → Agents shows the `claude mcp add` command).
- **GitHub** (needs a GitHub App, see Settings → GitHub). Pull requests and
  commits link to the issues they mention; a pull request moves the issues
  it works on to review once it could be merged, and merging it completes
  them; repositories are cloned for drift checks.
- **Forgejo** (Settings → Forgejo: a server, the repositories to watch and a
  token). Forge asks Forgejo about their pull requests about once a minute,
  links them to the issues they name and moves those issues, as with GitHub.
- **Accounts.** Code Villain sign-in (the studio's account, see below), or
  without it email and password or GitHub; invite links, admin and member
  roles.
- **Apps.** The web app, and a desktop app with native notifications and the
  unread count on the dock icon.

## Getting started

Needs Rust, [bun](https://bun.sh) and Docker.

```sh
bun install
bun run db            # Postgres in Docker, on port 5433
bun run dev           # server on :7380, web app on http://localhost:5173
bun run dev:desktop   # desktop app (with `bun run dev` running)
bun run seed          # local test accounts, see scripts/seed-dev.ts
bun run check         # everything that must pass before a change is done
```

`bun run db:reset` deletes the local database and starts it fresh; open clients
notice and download a new copy.

`bun run build:desktop` builds the desktop app (in
`target/release/bundle`). It asks which server to sign in to, by default the
Docker setup below.

To try drift checks, start the server with `ANTHROPIC_API_KEY` set, give a
service a repository in the folder containing the vault (for example
`villain-vault`) and some path globs from its root (like
`apps/forge/crates/**`), and press "Check for drift".

## Running it for the team

```sh
bun run app           # builds the image and runs it with Postgres
```

Forge is then at http://localhost:7390, with the same database as
development. Only this machine can reach it (and Postgres); to let
teammates in, put it behind https and set `FORGE_LISTEN=0.0.0.0:7390`; if
the proxy in front of it sets `X-Forwarded-For`, also set
`FORGE_TRUST_FORWARDED_FOR=1`, so limits per client (such as on signing up)
see each client's own address. `bun run app:logs` follows its logs. Put `ANTHROPIC_API_KEY` (for
drift checks) and the `FORGE_GITHUB_*` settings (Settings → GitHub lists
them) in a `.env` file next to
`docker-compose.yml`, then run `bun run app` again.

`bun run backup` saves the database and the attachments outside the
repository, to `FORGE_BACKUP_DIR` or else `backup/forge` next to the vault;
`scripts/backup.ts` says how to restore them.

## Signing in with Code Villain

Set `FORGE_OIDC_ISSUER` (`https://id.codevillain.eu`),
`FORGE_OIDC_CLIENT_ID`, `FORGE_OIDC_CLIENT_SECRET` and `FORGE_PUBLIC_URL`
(this Forge's address; Pocket ID sends people back to
`<FORGE_PUBLIC_URL>/api/auth/oidc/callback`, which the client there must
list). With them, Code Villain is the only way in: passwords and GitHub
sign-in are off, and so are `bun run seed` and `bun run reset-password`.
Without them, as in plain local development, passwords stay on. Bun and
`docker compose` both read a `.env` next to `docker-compose.yml`, so one
there turns it on for `bun run dev` and `bun run app` alike.
