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
bun run dev     # server on :7580, web app on http://localhost:5175
bun run seed    # a local account and some calls; password in scripts/seed.ts
```

`bun run dev:desktop` opens the desktop app on the dev server, and
`bun run build:desktop` packages it. The packaged app asks which Raven to
use and signs in with Code Villain through the browser.

The first person to open Raven sets it up with a password; after that,
nobody else can sign up with one. With `RAVEN_OIDC_ISSUER`,
`RAVEN_OIDC_CLIENT_ID`, `RAVEN_OIDC_CLIENT_SECRET` and `RAVEN_PUBLIC_URL`
set, people sign in with Code Villain and passwords are off.

Raven watches itself, Forge and Glyph. It checks itself from the start.
For the others, set `RAVEN_FORGE_HEALTH_URL` and `RAVEN_GLYPH_HEALTH_URL`;
locally they default to their dev servers (7380, 7480). An app that isn't
running shows as down, which is the truth.

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
- **Data in:** Raven records its own calls the same way. Other apps send
  OpenTelemetry over HTTP as JSON to `/v1/traces` and `/v1/logs`, with a
  key from `cargo run -p raven-server -- ingest-key <app>`. Forge and
  Glyph don't send yet.
- **Desktop app:** Raven in its own window on macOS, like Glyph. It
  keeps its session in the keychain and signs in with Code Villain
  through the browser and a `raven://` link.
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
