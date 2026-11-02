# Postrider

_A Code Villain product. First build: one Inbox and today, read-only._

Postrider is mail and calendar in one window. It opens on one Inbox for
every account, each in its own colour, with today's events above it. It
reads mail over IMAP and the calendar over CalDAV, from servers such as
Uberspace's Dovecot and Radicale, and keeps the accounts' passwords in the
macOS keychain.

[`storyboards/a-day-with-your-mail.html`](storyboards/a-day-with-your-mail.html)
is the day it's built for. Step 1 works: the unified Inbox, today, and
Settings → Accounts. Next come opening a mail, replying, archiving, Sieve
rules, newsletters, the week, and asking Postrider with Claude Code on
this Mac.

## Run it

```bash
bun install
bun run db      # Postgres, and stand-in mail and calendar servers, in Docker
bun run seed    # two accounts with the storyboard's mail and today's events
bun run dev     # server on :7880, web app on http://localhost:5178
```

The stand-ins take any user name with the password `dev`, and so does
`bun run dev`: nothing goes to the keychain. To read your own accounts in
development, start it with `POSTRIDER_DEV_PASSWORD=` (empty), and add them
in Settings → Accounts.

## The desktop app

```bash
bun run build:desktop
```

The app runs Postrider on this Mac, on its own Postgres and server. Add an
account in Settings → Accounts with its IMAP server (port 993, over TLS)
and password; Postrider signs in once, then keeps the password in the
keychain. A calendar server takes its address, user name and password the
same way.
