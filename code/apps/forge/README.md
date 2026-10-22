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
- **People.** A server runs for one company: its account signs people in
  and its groups say who's in, as admin, member or guest (see below).
- **Apps.** The web app, and a desktop app with native notifications and the
  unread count on the dock icon.

## Getting started

Needs Rust, [bun](https://bun.sh) and Docker.

```sh
bun install
bun run db            # Postgres in Docker, on port 5433
bun run dev           # Forge for Acme Inc on http://localhost:5173, see below
bun run dev:desktop   # desktop app (with `bun run dev` running)
bun run seed          # Gus Guest in Acme's first team alone
bun run check         # everything that must pass before a change is done
```

`bun run dev` runs Forge for a made-up company, Acme Inc: dev-id
(`code/kit/dev-id`) stands in for its account, so you sign in as anyone in
its `people.toml` (Ada Admin, Max Member, Gus Guest, or Nell New, who isn't
let in). Acme's data has a database of its own, `forge_acme`. With a
`forge.toml`, or `FORGE_OIDC_ISSUER` in `.env`, it runs with those settings
and the `forge` database instead.

`bun run db:reset` deletes the local databases and starts them fresh; open
clients notice and download a new copy.

`bun run build:desktop` builds the desktop app (in
`target/release/bundle`). It offers Start on this Mac, which runs Forge's
own Postgres (7386) and server (7385) for the workspaces on this Mac, or
signing in to a server, by default the Docker setup below.

To try drift checks, start the server with `ANTHROPIC_API_KEY` set, give a
service a repository in the folder containing the vault (for example
`villain-vault`) and some path globs from its root (like
`code/apps/forge/crates/**`), and press "Check for drift".

## Running it for the team

```sh
bun run app           # builds the image and runs it with Postgres
```

Forge is then at http://localhost:7390, with the same database as
development. It needs a company's settings (below) in a `.env` file next to
`docker-compose.yml`, which it reads as `FORGE_COMPANY`, `FORGE_OIDC_ISSUER`
and so on. Only this machine can reach it (and Postgres); to let teammates
in, put it behind https and set `FORGE_LISTEN=0.0.0.0:7390`. `bun run
app:logs` follows its logs.

`bun run backup` saves the database and the attachments outside the
repository, to `FORGE_BACKUP_DIR` or else `backup/forge` next to the vault;
`scripts/backup.ts` says how to restore them.

## Running it for a company

A server runs for one company, from one settings file: copy
`forge.example.toml` to `forge.toml` (or pass `forge-server --config
<file>`), and fill in the company's name, Forge's address, the database,
the company's account and a SCIM token. Every setting can come from the
environment too, which wins: `[oidc] issuer` is `FORGE_OIDC_ISSUER`. A
missing or unknown setting stops the server, naming it.

In the company's account (Pocket ID, or any OpenID Connect provider with
SCIM):

1. Register Forge as a confidential client with the callback
   `<public_url>/api/auth/oidc/callback`.
2. Make the groups `forge-users`, `forge-admins` and `forge-guests` (or
   the names under `[groups]`), and put people in them.
3. Turn on SCIM for Forge's client, with the endpoint
   `<public_url>/api/scim/v2` and the settings' token.

On its first start Forge makes the company's workspace. The account sends
its people within minutes, and they sign in with "Sign in with <company>".
Nobody signs up, and there are no invites or passwords: someone in none of
the groups is told to ask the company's admin.

With Docker, build the image from `code/` and give it the settings file:

```sh
docker build -f apps/forge/Dockerfile -t forge .
docker run -d --name forge -p 7380:7380 -v forge-data:/var/lib/forge \
  -v ./forge.toml:/etc/forge/forge.toml:ro -e FORGE_CONFIG=/etc/forge/forge.toml forge
```

Put it behind https at `public_url`; the image serves the web app too.
