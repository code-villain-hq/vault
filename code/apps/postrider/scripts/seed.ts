// Fills the stand-in mail and calendar servers (`bun run db`) with the
// storyboard's morning, and adds both to the workspace: the Code Villain
// and Lazarski accounts, and their calendars, with today's events. Needs
// `bun run dev` running. Safe to run again: an Inbox that has mail isn't
// filled twice, and today's events are written over; `bun run db:reset`
// starts them over. Local development only.

import { connect, type Socket } from "node:net";

const root = new URL("..", import.meta.url).pathname;
const server = process.env.POSTRIDER_URL ?? "http://127.0.0.1:7880";
const imap = { host: "127.0.0.1", port: 3143 };
const caldav = "http://127.0.0.1:5232";
// The stand-ins' one password (docker-compose.yml).
const password = "dev";

type Mail = {
  from: string;
  subject: string;
  body: string;
  /** Days before today, and the time it came. */
  day: number;
  at: string;
  read?: boolean;
};

const accounts: { name: string; address: string; calendar: string; mail: Mail[] }[] = [
  {
    name: "Code Villain",
    address: "contact@codevillain.test",
    calendar: "code-villain",
    mail: [
      {
        from: "Anna Weber <anna@weber.test>",
        subject: "Layer for our team?",
        body: "Hi Michael,\n\nwe tried Layer on three Macs last week and wondered whether it works for a team of six. Is there a plan for that?\n\nAnna",
        day: 0,
        at: "08:12",
      },
      {
        from: "Code Villain Git <git@codevillain.test>",
        subject: "[code-villain/vault] CI passed on main",
        body: "Run #812 for b13c00a finished in 6 minutes.",
        day: 0,
        at: "07:40",
      },
      {
        from: "Pocket ID <id@codevillain.test>",
        subject: "New sign-in to your account",
        body: "A new sign-in from Safari on macOS.",
        day: 2,
        at: "21:04",
        read: true,
      },
    ],
  },
  {
    name: "Lazarski",
    address: "michael@lazarski.test",
    calendar: "lazarski",
    mail: [
      {
        from: "Kristina <kristina@lazarski.test>",
        subject: "Samstag",
        body: "Hast du Samstag Zeit? Mama fragt, ob wir um 18 Uhr zum Essen kommen.",
        day: 0,
        at: "07:58",
      },
      {
        from: "GitHub <notifications@github.test>",
        subject: "[code-villain-hq/vault] Bump vite from 7.3 to 7.4",
        body: "Dependabot opened a pull request to bump vite from 7.3 to 7.4.",
        day: 0,
        at: "07:41",
      },
      {
        from: "GitHub <notifications@github.test>",
        subject: "[villain-layer] Pane loses focus after ⌘K (#212)",
        body: "A new issue was opened by a Layer user: after ⌘K the pane loses focus.",
        day: 0,
        at: "06:55",
      },
      {
        from: "Netcup <noreply@netcup.test>",
        subject: "codevillain.eu wurde verlängert",
        body: "Ihre Domain wurde um ein Jahr verlängert.",
        day: 1,
        at: "16:20",
        read: true,
      },
      {
        from: "DB Fernverkehr <buchung@bahn.test>",
        subject: "Ihre Fahrkarte: Berlin → Hamburg, Fr 14. Nov",
        body: "Vielen Dank für Ihre Buchung.",
        day: 1,
        at: "11:05",
        read: true,
      },
      {
        from: "Mama <mama@lazarski.test>",
        subject: "Fotos vom Wochenende",
        body: "Hier die Bilder, die ich versprochen hatte.",
        day: 2,
        at: "15:30",
        read: true,
      },
      {
        from: "Uberspace <hallo@uberspace.test>",
        subject: "Wartung am 12. November",
        body: "Am Mittwoch, 12. November, zwischen 6 und 7 Uhr warten wir den Server.",
        day: 3,
        at: "10:00",
        read: true,
      },
    ],
  },
];

const events: { calendar: string; title: string; from: string; to: string }[] = [
  { calendar: "lazarski", title: "Zahnarzt", from: "10:00", to: "11:00" },
  { calendar: "lazarski", title: "Mittag", from: "12:30", to: "13:30" },
  { calendar: "lazarski", title: "Bouldern", from: "18:30", to: "20:00" },
  { calendar: "code-villain", title: "Call: Layer", from: "20:15", to: "21:00" },
];

/** Today, `days` before, at `at` on this Mac's clock. */
function when(days: number, at: string): Date {
  const [hours, minutes] = at.split(":").map(Number);
  const date = new Date();
  date.setDate(date.getDate() - days);
  date.setHours(hours ?? 0, minutes ?? 0, 0, 0);
  return date;
}

// Mail: appended over IMAP, so the read ones arrive read.

function header(text: string): string {
  // Non-ASCII headers go as RFC 2047 words.
  return /^[\x20-\x7e]*$/.test(text) ? text : `=?UTF-8?B?${Buffer.from(text).toString("base64")}?=`;
}

/** IMAP's date-time, as it arrived: Postrider sorts by that, not by `Date:`. */
function arrived(date: Date): string {
  const months = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ");
  const two = (n: number) => String(Math.abs(n)).padStart(2, "0");
  const offset = -date.getTimezoneOffset();
  const zone = `${offset < 0 ? "-" : "+"}${two(Math.trunc(offset / 60))}${two(offset % 60)}`;
  const time = `${two(date.getHours())}:${two(date.getMinutes())}:00`;
  return `"${two(date.getDate())}-${months[date.getMonth()]}-${date.getFullYear()} ${time} ${zone}"`;
}

function message(to: string, mail: Mail): string {
  const date = when(mail.day, mail.at);
  return [
    `From: ${mail.from}`,
    `To: ${to}`,
    `Subject: ${header(mail.subject)}`,
    `Date: ${date.toUTCString().replace("GMT", "+0000")}`,
    `Message-ID: <${crypto.randomUUID()}@postrider.test>`,
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=utf-8",
    "Content-Transfer-Encoding: 8bit",
    "",
    mail.body,
    "",
  ].join("\r\n");
}

/** A small IMAP client: enough to sign in, look and append. */
class Imap {
  private buffer = "";
  private waiting: (() => void) | undefined;
  private tag = 0;

  private constructor(private socket: Socket) {
    socket.setEncoding("utf8");
    socket.on("data", (chunk) => {
      this.buffer += chunk;
      this.waiting?.();
    });
  }

  static async open(): Promise<Imap> {
    const socket = connect(imap);
    await new Promise<void>((resolve, reject) => {
      socket.once("connect", resolve);
      socket.once("error", reject);
    });
    const client = new Imap(socket);
    await client.until(/^\* OK.*\r\n/m);
    return client;
  }

  /** Waits for `pattern` in what the server said, and takes up to it. */
  private async until(pattern: RegExp): Promise<string> {
    for (;;) {
      const match = pattern.exec(this.buffer);
      if (match) {
        const said = this.buffer.slice(0, match.index + match[0].length);
        this.buffer = this.buffer.slice(said.length);
        return said;
      }
      await new Promise<void>((resolve) => (this.waiting = resolve));
    }
  }

  async run(command: string, literal?: string): Promise<string> {
    const tag = `s${++this.tag}`;
    if (literal === undefined) {
      this.socket.write(`${tag} ${command}\r\n`);
    } else {
      this.socket.write(`${tag} ${command} {${Buffer.byteLength(literal)}}\r\n`);
      await this.until(/^\+.*\r\n/m);
      this.socket.write(`${literal}\r\n`);
    }
    const said = await this.until(new RegExp(`^${tag} (OK|NO|BAD).*\\r\\n`, "m"));
    if (!new RegExp(`^${tag} OK`, "m").test(said)) throw new Error(`IMAP ${command}: ${said}`);
    return said;
  }

  close() {
    this.socket.end();
  }
}

async function fillInbox(address: string, mail: Mail[]): Promise<boolean> {
  const client = await Imap.open();
  try {
    await client.run(`LOGIN "${address}" "${password}"`);
    const status = await client.run("STATUS INBOX (MESSAGES)");
    if (!/MESSAGES 0\)/.test(status)) return false;
    // Oldest first, as it would have come.
    for (const one of [...mail].reverse()) {
      const flags = one.read ? "(\\Seen)" : "()";
      const date = arrived(when(one.day, one.at));
      await client.run(`APPEND INBOX ${flags} ${date}`, message(address, one));
    }
    await client.run("LOGOUT").catch(() => undefined);
    return true;
  } finally {
    client.close();
  }
}

// The day: one calendar per account in Radicale, today's events in them.

function icalTime(date: Date): string {
  return date.toISOString().replace(/[-:]/g, "").replace(/\.\d+/, "");
}

async function dav(method: string, path: string, body?: string, headers?: Record<string, string>) {
  return fetch(`${caldav}${path}`, {
    method,
    body,
    headers: {
      authorization: `Basic ${btoa(`michael:${password}`)}`,
      "content-type": "text/xml; charset=utf-8",
      ...headers,
    },
  });
}

async function fillCalendar(slug: string, name: string) {
  const path = `/michael/${slug}/`;
  const found = await dav("PROPFIND", path, undefined, { depth: "0" });
  if (found.status === 404) {
    const made = await dav(
      "MKCALENDAR",
      path,
      `<?xml version="1.0" encoding="utf-8"?>
<c:mkcalendar xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:set><d:prop><d:displayname>${name}</d:displayname></d:prop></d:set>
</c:mkcalendar>`,
    );
    if (!made.ok) throw new Error(`couldn't make ${name}'s calendar: ${made.status}`);
  }
  for (const event of events.filter((e) => e.calendar === slug)) {
    const uid = `${event.title.toLowerCase().replace(/[^a-z]+/g, "-")}@postrider.test`;
    const ics = [
      "BEGIN:VCALENDAR",
      "VERSION:2.0",
      "PRODID:-//Code Villain//Postrider seed//EN",
      "BEGIN:VEVENT",
      `UID:${uid}`,
      `DTSTAMP:${icalTime(new Date())}`,
      `DTSTART:${icalTime(when(0, event.from))}`,
      `DTEND:${icalTime(when(0, event.to))}`,
      `SUMMARY:${event.title}`,
      "END:VEVENT",
      "END:VCALENDAR",
      "",
    ].join("\r\n");
    const put = await dav("PUT", `${path}${uid}.ics`, ics, {
      "content-type": "text/calendar; charset=utf-8",
    });
    if (!put.ok) throw new Error(`couldn't write ${event.title}: ${put.status}`);
  }
}

// The workspace: both accounts and the calendar server, as the form adds them.

const token = (
  await Bun.file(`${root}.dev-session`)
    .text()
    .catch(() => "")
).trim();
if (!token) throw new Error("no .dev-session: start `bun run dev` first");
const auth = { authorization: `Bearer ${token}`, "content-type": "application/json" };

async function api(method: string, path: string, body?: unknown) {
  const response = await fetch(`${server}${path}`, {
    method,
    headers: auth,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) throw new Error(`${method} ${path}: ${await response.text()}`);
  return response.json();
}

const me = (await api("GET", "/api/me")) as { workspaces: { id: string; name: string }[] };
const workspace = me.workspaces[0];
if (!workspace) throw new Error("the dev server made no workspace");
const w = `/api/w/${workspace.id}`;

for (const account of accounts) {
  const filled = await fillInbox(account.address, account.mail);
  console.log(
    `${account.address}: ${filled ? `${account.mail.length} mails` : "has mail already"}`,
  );
  await fillCalendar(account.calendar, account.name);
  await api("POST", `${w}/accounts`, {
    name: account.name,
    address: account.address,
    host: imap.host,
    port: imap.port,
    password,
  });
}
await api("POST", `${w}/calendar`, { url: `${caldav}/`, username: "michael", password });
console.log(`${events.length} events today. Both accounts are in "${workspace.name}".`);
