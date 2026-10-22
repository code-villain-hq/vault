// Readies Acme Inc's workspace for trying Forge: its people come from dev-id
// (code/kit/dev-id/people.toml), and this puts Gus Guest in the first team
// alone. Safe to run again. Needs `bun run dev` running, for Acme Inc.
// Local development only.

import { signInAs } from "../../../kit/dev-id/dev";

export const BASE = process.env.FORGE_URL ?? "http://localhost:5173";

/** dev-id's admin. */
export const DEV_ADMIN = "ada";

export async function call(path: string, body: unknown, cookie?: string, method = "POST") {
  const response = await fetch(`${BASE}${path}`, {
    method,
    headers: { "content-type": "application/json", ...(cookie ? { cookie } : {}) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { status: response.status, body: await response.json().catch(() => null) };
}

/** Signs someone in through dev-id: their session cookie, `/api/me` and workspace. */
export async function account(who: string) {
  const { session, me } = await signInAs(BASE, who);
  const workspace = me.workspaces[0];
  if (!workspace) throw new Error(`${who} isn't in Acme Inc's workspace yet: is dev-id running?`);
  return { session, me, workspace };
}

/** Waits up to ten seconds, two of dev-id's rounds, for `check` to find something. */
async function until<T>(what: string, check: () => Promise<T | undefined>) {
  for (let i = 0; i < 20; i++) {
    const found = await check();
    if (found) return found;
    await Bun.sleep(500);
  }
  throw new Error(`gave up waiting for ${what}`);
}

if (import.meta.main) {
  const admin = await until("Ada", () => account(DEV_ADMIN).catch(() => undefined));
  const { workspace } = admin;
  const get = async (path: string) =>
    (await call(`/api/w/${workspace.id}${path}`, undefined, admin.session, "GET")).body;
  const team = await until(
    "the first team",
    async () => (await get("/sync/bootstrap")).records.Team?.[0],
  );
  const members: { user: { id: string; email: string } }[] = await get("/members");
  const guest = members.find((m) => m.user.email === "gus@acme.test");
  if (!guest) throw new Error("Gus isn't in the workspace: check people.toml");
  const access = await call(
    `/api/w/${workspace.id}/members/${guest.user.id}/access`,
    { items: [team.id] },
    admin.session,
    "PUT",
  );
  if (access.status !== 204) throw new Error(`couldn't add Gus to ${team.name}`);

  console.log(`Gus Guest is in ${team.name} alone, in "${workspace.name}".`);
  console.log(`Open ${BASE}/${workspace.url_key} and sign in as anyone in people.toml.`);
}
