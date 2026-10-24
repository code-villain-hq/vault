# Raven

*A Code Villain product. First build: Health and Technical work; Product
is decided but not built.*

Raven watches the studio's running apps and tells you what it sees, on
three levels. Each level has its own reader and speaks that reader's
language:

| Level | Reader | Question | Words it uses |
| --- | --- | --- | --- |
| Health | Anyone | Is it up, and is it well? | Up, slow, failing, since when |
| Technical | Developer | How does it behave? | Errors, response times, queries, logs, versions |
| Product | PO / PM | How is it used? | Events, counts, durations, funnels |

You start at Health and only go deeper when something catches your eye.

## Run it

```bash
bun install
bun run db      # Postgres in Docker, on 127.0.0.1:5435
bun run dev     # Raven for Acme Inc: server on :7580, web app on http://localhost:5175
bun run seed    # some calls, and Gus Guest let in to Raven itself
```

`bun run dev` runs Raven for a made-up company, Acme Inc: dev-id
(`code/kit/dev-id`) stands in for its account, so you sign in as anyone in
its `people.toml` (Ada Admin, Max Member, Gus Guest, or Nell New, who isn't
let in). Acme's data has a database of its own, `raven_acme`. With a
`raven.toml`, or `RAVEN_OIDC_ISSUER` in `.env`, it runs with those settings
and the `raven` database instead.

`bun run dev:desktop` opens the desktop app on the dev server, and
`bun run build:desktop` packages it. The packaged app runs Raven on this
Mac, or signs in to a server through the browser.

## Running it for a company

A server runs for one company, from one settings file: copy
`raven.example.toml` to `raven.toml` (or pass `raven-server --config
<file>`), and fill in the company's name, Raven's address, the database,
the company's account and a SCIM token. Every setting can come from the
environment too, which wins: `[oidc] issuer` is `RAVEN_OIDC_ISSUER`. A
missing or unknown setting stops the server, naming it.

In the company's account (Pocket ID, or any OpenID Connect provider with
SCIM):

1. Register Raven as a confidential client with the callback
   `<public_url>/api/auth/oidc/callback`.
2. Make the groups `raven-users`, `raven-admins` and `raven-guests` (or
   the names under `[groups]`), and put people in them.
3. Turn on SCIM for Raven's client, with the endpoint
   `<public_url>/api/scim/v2` and the settings' token.

On its first start Raven makes the company's workspace, and Raven itself
comes with the first person. The account sends its people within minutes,
and they sign in with "Sign in with <company>". Nobody signs up, and there
are no invites or passwords: someone in none of the groups is told to ask
the company's admin.

`bun run app` runs the whole app in Docker at http://localhost:7590, with
the settings in `.env`. Elsewhere, build the image from `code/` and give it
the settings file:

```sh
docker build -f apps/raven/Dockerfile -t raven .
docker run -d --name raven -p 7580:7580 \
  -v ./raven.toml:/etc/raven/raven.toml:ro -e RAVEN_CONFIG=/etc/raven/raven.toml raven
```

Put it behind https at `public_url`; the image serves the web app too.

A workspace is a team's apps. Its admins add the apps it watches in
Settings → Apps, with each one's health address, and make the key the app
sends its calls with. Raven checks itself from the start. An app that
isn't running shows as down, which is the truth.

## What works

- **Health:** up, needs a look, or down, in one sentence. Down when the
  last two checks failed; needs a look when only the last one did, when
  calls failed in the last 15 minutes, or when an action is slower than
  its own last 7 days. The last 24 hours as half-hour bars, uptime and
  the latest checks.
- **Technical:** every action the server does, worst first: calls a
  minute, failed share, typical (p50) and slow end (p95) against its own
  week. Errors grouped by message, with first and last seen and the
  version they started in. Windows of 15 minutes, an hour, a day, a week.
- **An action:** its slow end and typical time as lines, its calls as
  bars with the failed part marked, and its slowest calls.
- **A call:** where its time went, step by step, down to each database
  query, with the logs it wrote and the app's logs just before.
- **Workspaces:** each watches its team's apps, with members and roles
  as in Forge. The apps and members sync, so Settings works
  offline; calls, logs and checks are read live, and say so when the
  server can't be reached.
- **Data in:** Raven records its own calls the same way. Other apps send
  OpenTelemetry over HTTP as JSON to `/v1/traces` and `/v1/logs`, with
  the key from Settings → Apps. Forge and Glyph don't send yet.
- **Desktop app:** Raven in its own window on macOS, like Glyph. It runs
  Raven on this Mac, watching the apps on this Mac: itself, the installed
  Forge and Glyph and their dev servers. For a server it keeps its session
  in the keychain and signs in with the company's account through the
  browser and a `raven://` link.
- **Kept:** 30 days of calls, logs and checks, in Postgres.

## What's decided, not built

- **Product:** the PO defines events in Raven: a name, what it means in
  plain words, and the properties it carries. Raven shows each event as
  arriving, not sent yet, or sent but not defined. From events the PO
  builds metrics: counts, durations between two steps, funnels with the
  biggest drop named, and splits by a property.
- **"How long does X take"** depends on the level. In Technical it's a
  query or a response. In Product it's the time between two steps a
  person takes.
- **One sender:** Forge and Glyph share one small Rust piece that sends
  calls, logs and events, so one instrumented moment feeds both
  Technical and Product.
- **Version marks on charts**, and daily totals kept longer than 30 days.
- **Not in it:** dashboard builders, session replay, feature flags.

## Storyboards

Open them in a browser; add `#day` to the address for day.

- [`storyboards/po-path.html`](storyboards/po-path.html): the PO defines
  an event, sees it arrive and builds a funnel.
- [`storyboards/developer-path.html`](storyboards/developer-path.html):
  Health turns yellow, Technical shows what's off, a slow call opens up,
  and an error shows its logs.

## Open

- Whether the PO sees people's names or only counts.
- Retention ("do people come back?") as a fourth kind of metric.
- Alerts: who hears about a red or yellow app, and how.
- A Health page across all apps, once there are more than three.

App health summaries are shared between clients for up to 15 seconds. The seven-day latency baseline refreshes at most every five minutes per app; recent action statistics are still queried live.
