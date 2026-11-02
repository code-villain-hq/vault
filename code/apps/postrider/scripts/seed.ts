// Fills the stand-in mail and calendar servers (`bun run db`) with the
// storyboard's morning, and adds both to the workspace: the Code Villain
// and Lazarski accounts with their Inboxes and a few folders (Drafts, Junk
// and Trash among them), and their calendars, with today's events. Needs
// `bun run dev` running. Safe to run again: a folder that has mail isn't
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
  /** Who it went to, when not the account: mail it sent. */
  to?: string;
  subject: string;
  body: string;
  /** Days before today, and the time it came. */
  day: number;
  at: string;
  read?: boolean;
  /** A PDF that came with it, and the line it says. */
  file?: { name: string; says: string };
  /** More header lines, such as `List-Unsubscribe`. */
  headers?: string[];
};

// Where a one-click unsubscribe goes on this Mac. Nothing listens there
// unless you test it, and then Postrider says it couldn't reach them.
const oneClick = "http://127.0.0.1:7899/unsubscribe";

/**
 * A sender's newsletters of the last 90 days: one every `every` days, at
 * `at`, the newest `first` days ago, with every `opened`th read (0: none).
 * `leave` is how it takes unsubscribing: a mail, one click, or a page.
 */
function newsletters(sender: {
  from: string;
  subjects: string[];
  body: string;
  every: number;
  first?: number;
  at: string;
  opened: number;
  leave: "mail" | "click" | "page";
}): Mail[] {
  const address = sender.from.match(/<(.+)>/)?.[1] ?? sender.from;
  const domain = address.split("@")[1];
  const links = {
    mail: `<mailto:leave@${domain}?subject=unsubscribe>`,
    click: `<${oneClick}/${domain}>, <mailto:leave@${domain}>`,
    page: `<https://${domain}/abmelden>`,
  }[sender.leave];
  const headers = [`List-Id: <news.${domain}>`, `List-Unsubscribe: ${links}`];
  if (sender.leave === "click") headers.push("List-Unsubscribe-Post: List-Unsubscribe=One-Click");
  const mail: Mail[] = [];
  for (let day = sender.first ?? 0, n = 0; day < 90; day += sender.every, n++) {
    mail.push({
      from: sender.from,
      subject: sender.subjects[n % sender.subjects.length] ?? "",
      body: sender.body,
      day,
      at: sender.at,
      read: sender.opened > 0 && n % sender.opened === 0,
      headers,
    });
  }
  return mail;
}

const accounts: {
  name: string;
  address: string;
  calendar: string;
  mail: Mail[];
  /** Its other folders, by their paths, and their mail. */
  folders: Record<string, Mail[]>;
}[] = [
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
    folders: {
      Sent: [
        {
          from: "Michael Lazarski <contact@codevillain.test>",
          to: "Anna Weber <anna@weber.test>",
          subject: "Re: Layer trial",
          body: "Hi Anna,\n\ngood to hear the trial went well. I'll write about teams this week.\n\nMichael",
          day: 1,
          at: "17:20",
          read: true,
        },
        {
          from: "Michael Lazarski <contact@codevillain.test>",
          to: "Uberspace <hallo@uberspace.test>",
          subject: "Umzug auf U8",
          body: "Hallo,\n\nwann wird hideout auf U8 umgezogen?\n\nMichael",
          day: 4,
          at: "09:30",
          read: true,
        },
      ],
      Drafts: [],
      Junk: [
        {
          from: "Prize Desk <winner@prizes.test>",
          subject: "You have won a MacBook!!!",
          body: "Click here to claim your prize before midnight.",
          day: 0,
          at: "03:12",
        },
      ],
      Trash: [
        {
          from: "Newsletter <news@shop.test>",
          subject: "Autumn sale: 20% off",
          body: "Only this week.",
          day: 5,
          at: "11:00",
          read: true,
        },
      ],
      Archive: [
        {
          from: "Pocket ID <id@codevillain.test>",
          subject: "Welcome to Pocket ID",
          body: "Your account at id.codevillain.test is ready.",
          day: 12,
          at: "14:02",
          read: true,
        },
      ],
      Rechnungen: [
        {
          from: "Netcup <noreply@netcup.test>",
          subject: "Ihre Rechnung für Oktober",
          body: "Ihre Rechnung über 12,34 € liegt im Kundenbereich.",
          day: 2,
          at: "06:10",
        },
      ],
      "Projekte/Layer": [
        {
          from: "Anna Weber <anna@weber.test>",
          subject: "Layer trial: what we noticed",
          body: "Hi Michael,\n\nthree things we noticed in the trial: restoring panes, ⌘K focus and the theme picker.\n\nAnna",
          day: 6,
          at: "11:45",
          read: true,
        },
      ],
      Newsletters: [
        ...newsletters({
          from: "The Pragmatic Engineer <gergely@pragmatic.test>",
          subjects: [
            "The Pulse: AI tools in teams",
            "Inside a platform team",
            "The Pulse: hiring is back",
          ],
          body: "This week: what we learned from 400 engineers about their tools.",
          every: 7,
          first: 1,
          at: "16:00",
          opened: 1,
          leave: "click",
        }),
        ...newsletters({
          from: "This Week in Rust <twir@rust.test>",
          subjects: ["This Week in Rust 620", "This Week in Rust 621", "This Week in Rust 622"],
          body: "Hello and welcome to another issue of This Week in Rust!",
          every: 7,
          first: 2,
          at: "05:00",
          opened: 2,
          leave: "mail",
        }),
        ...newsletters({
          from: "Product Hunt Daily <hello@producthunt.test>",
          subjects: ["Today's top products", "5 AI tools you missed", "The best of the week"],
          body: "Here's what launched today.",
          every: 1,
          first: 0,
          at: "09:15",
          opened: 0,
          leave: "click",
        }),
      ],
    },
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
    folders: {
      Gesendet: [
        {
          from: "Michael Lazarski <michael@lazarski.test>",
          to: "Kristina <kristina@lazarski.test>",
          subject: "Re: Samstag",
          body: "Ja, 18 Uhr passt. Ich bringe Nachtisch mit.",
          day: 0,
          at: "08:05",
          read: true,
        },
      ],
      Archiv: [
        {
          from: "DB Fernverkehr <buchung@bahn.test>",
          subject: "Ihre Fahrkarte: Hamburg → Berlin, So 16. Nov",
          body: "Vielen Dank für Ihre Buchung.",
          day: 9,
          at: "19:12",
          read: true,
        },
      ],
      Newsletters: [
        ...newsletters({
          from: "Zalando <news@zalando.test>",
          subjects: [
            "Neu eingetroffen: Herbstjacken",
            "Nur heute: 20 % extra",
            "Deine Lieblingsmarken im Sale",
          ],
          body: "Entdecke die neuen Styles der Saison.",
          every: 1,
          at: "10:30",
          opened: 0,
          leave: "mail",
        }),
        ...newsletters({
          from: "LinkedIn <messages-noreply@linkedin.test>",
          subjects: [
            "Du bist in 12 Suchen aufgetaucht",
            "Neue Jobs für dich",
            "Anna Weber hat etwas geteilt",
          ],
          body: "Sieh dir an, was in deinem Netzwerk passiert.",
          every: 2,
          at: "08:45",
          opened: 0,
          leave: "click",
        }),
        ...newsletters({
          from: "MediaMarkt <newsletter@mediamarkt.test>",
          subjects: ["Tiefpreisspätschicht", "Die besten Deals der Woche"],
          body: "Nur solange der Vorrat reicht.",
          every: 7,
          first: 3,
          at: "18:00",
          opened: 0,
          leave: "mail",
        }),
        ...newsletters({
          from: "Spotify <no-reply@spotify.test>",
          subjects: ["Neue Musik für dich", "Dein Mix der Woche"],
          body: "Frische Songs, ausgewählt für dich.",
          every: 7,
          first: 4,
          at: "12:00",
          opened: 0,
          leave: "page",
        }),
        ...newsletters({
          from: "BahnBonus <bahnbonus@bahn.test>",
          subjects: ["Ihre Punkte im Überblick", "Prämien im November"],
          body: "Ihr aktueller Punktestand und neue Prämien.",
          every: 14,
          first: 5,
          at: "09:00",
          opened: 3,
          leave: "page",
        }),
        ...newsletters({
          from: "heise online <newsletter@heise.test>",
          subjects: ["heise-Newsletter: Die Nachrichten des Tages", "c't-Newsletter"],
          body: "Die wichtigsten Meldungen des Tages.",
          every: 1,
          at: "06:30",
          opened: 1,
          leave: "click",
        }),
        ...newsletters({
          from: "Babylon Kino <programm@babylon.test>",
          subjects: ["Das Programm der Woche"],
          body: "Diese Woche im Babylon: Filme, Gäste und Reihen.",
          every: 7,
          first: 1,
          at: "11:00",
          opened: 1,
          leave: "page",
        }),
      ],
      Rechnungen: [
        {
          from: "Uberspace <hallo@uberspace.test>",
          subject: "Rechnung November",
          body: "Für November buchen wir 10 € ab. Die Rechnung hängt an.",
          file: { name: "Rechnung November.pdf", says: "Uberspace - Rechnung November: 10 EUR" },
          day: 1,
          at: "08:00",
        },
        {
          from: "Telekom <rechnung@telekom.test>",
          subject: "Ihre Mobilfunk-Rechnung",
          body: "Ihre Rechnung für Oktober ist da.",
          day: 8,
          at: "05:30",
          read: true,
        },
      ],
    },
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
  const head = [
    `From: ${mail.from}`,
    `To: ${mail.to ?? to}`,
    `Subject: ${header(mail.subject)}`,
    `Date: ${date.toUTCString().replace("GMT", "+0000")}`,
    `Message-ID: <${crypto.randomUUID()}@postrider.test>`,
    ...(mail.headers ?? []),
    "MIME-Version: 1.0",
  ];
  const text = [
    "Content-Type: text/plain; charset=utf-8",
    "Content-Transfer-Encoding: 8bit",
    "",
    mail.body,
    "",
  ];
  if (!mail.file) return [...head, ...text].join("\r\n");
  const boundary = `postrider-${crypto.randomUUID()}`;
  return [
    ...head,
    `Content-Type: multipart/mixed; boundary="${boundary}"`,
    "",
    `--${boundary}`,
    ...text,
    `--${boundary}`,
    `Content-Type: application/pdf; name="${mail.file.name}"`,
    `Content-Disposition: attachment; filename="${mail.file.name}"`,
    "Content-Transfer-Encoding: base64",
    "",
    ...(pdf(mail.file.says)
      .toString("base64")
      .match(/.{1,76}/g) ?? []),
    `--${boundary}--`,
    "",
  ].join("\r\n");
}

/** A one-page PDF that says `line` (ASCII), for a mail's file. */
function pdf(line: string): Buffer {
  const content = `BT /F1 14 Tf 72 760 Td (${line.replace(/[()\\]/g, "\\$&")}) Tj ET`;
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
  ];
  let body = "%PDF-1.4\n";
  const offsets = objects.map((object, i) => {
    const at = body.length;
    body += `${i + 1} 0 obj\n${object}\nendobj\n`;
    return at;
  });
  const xref = body.length;
  body += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  body += offsets.map((at) => `${String(at).padStart(10, "0")} 00000 n \n`).join("");
  body += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(body, "latin1");
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

/** Fills the account's folders that are still empty; whether it filled the Inbox. */
async function fillMail(address: string, folders: Record<string, Mail[]>): Promise<boolean> {
  const client = await Imap.open();
  try {
    await client.run(`LOGIN "${address}" "${password}"`);
    let filled = false;
    for (const [folder, mail] of Object.entries(folders)) {
      // There already, it says NO.
      if (folder !== "INBOX") await client.run(`CREATE "${folder}"`).catch(() => undefined);
      const status = await client.run(`STATUS "${folder}" (MESSAGES)`);
      if (!/MESSAGES 0\)/.test(status)) continue;
      // Oldest first, as it would have come.
      for (const one of [...mail].reverse()) {
        const flags = one.read ? "(\\Seen)" : "()";
        const date = arrived(when(one.day, one.at));
        await client.run(`APPEND "${folder}" ${flags} ${date}`, message(address, one));
      }
      if (folder === "INBOX") filled = true;
    }
    await client.run("LOGOUT").catch(() => undefined);
    return filled;
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
  const filled = await fillMail(account.address, { INBOX: account.mail, ...account.folders });
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
