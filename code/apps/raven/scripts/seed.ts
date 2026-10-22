// Readies Acme Inc's workspace for trying Raven: its people come from dev-id
// (code/kit/dev-id/people.toml). This calls a few pages, so Raven has calls
// of its own to show, and lets Gus Guest see Raven itself. Safe to run
// again. Needs `bun run dev` running, for Acme Inc. Local development only.

import { signInAs } from "../../../kit/dev-id/dev";

const server = process.env.RAVEN_URL ?? "http://localhost:5175";

/** Waits up to ten seconds, two of dev-id's rounds, for `check` to find something. */
async function until<T>(what: string, check: () => Promise<T | undefined>) {
  for (let i = 0; i < 20; i++) {
    const found = await check();
    if (found) return found;
    await Bun.sleep(500);
  }
  throw new Error(`gave up waiting for ${what}`);
}

// Ada is dev-id's admin.
const { session: cookie, me } = await until("Ada", () =>
  signInAs(server, "ada").catch(() => undefined),
);
const workspace = (me as { workspaces: { id: string; name: string; url_key: string }[] })
  .workspaces[0];
if (!workspace) throw new Error("Ada isn't in Acme Inc's workspace yet: is dev-id running?");
const w = `/api/w/${workspace.id}`;

// Some ordinary use, and a few calls that miss, so the pages have something on them.
const paths = [
  "/api/me",
  `${w}/apps`,
  `${w}/apps/raven/health`,
  `${w}/apps/raven/technical`,
  `${w}/apps/raven/technical?window=86400`,
  `${w}/apps/nobody/health`,
];
for (let round = 0; round < 6; round++) {
  for (const path of paths) await fetch(`${server}${path}`, { headers: { cookie } });
}
console.log(`Called ${paths.length * 6} pages.`);

// Gus Guest sees Raven itself, for trying what a guest sees.
const members = (await (await fetch(`${server}${w}/members`, { headers: { cookie } })).json()) as {
  user: { id: string; email: string };
}[];
const guest = members.find((m) => m.user.email === "gus@acme.test");
if (!guest) throw new Error("Gus isn't in the workspace: check people.toml");
const snapshot = (await (
  await fetch(`${server}${w}/sync/bootstrap`, { headers: { cookie } })
).json()) as { records: { App?: { id: string; name: string }[] } };
const first = snapshot.records.App?.[0];
if (!first) throw new Error("the workspace watches nothing yet");
const added = await fetch(`${server}${w}/members/${guest.user.id}/access`, {
  method: "PUT",
  headers: { "content-type": "application/json", cookie },
  body: JSON.stringify({ items: [first.id] }),
});
if (!added.ok) throw new Error(`couldn't let Gus see ${first.name}`);

console.log(`Gus Guest sees ${first.name}, in "${workspace.name}".`);
console.log(`Open ${server}/${workspace.url_key} and sign in as anyone in people.toml.`);
