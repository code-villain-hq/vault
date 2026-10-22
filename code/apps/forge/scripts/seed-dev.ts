// Creates local test accounts through the API: an admin with a "Forge"
// workspace, a member who joined it by invite link, and a guest in its first
// team. Safe to run again; the guest is put back in that team alone.
// Needs `bun run dev` running. Local development only.

export const BASE = process.env.FORGE_URL ?? "http://localhost:5173";

/** Test-only password for the local seed accounts. */
export const DEV_PASSWORD = "forge-dev-password";
export const DEV_ADMIN = { name: "Ada Admin", email: "admin@forge.test" };
export const DEV_MEMBER = { name: "Max Member", email: "member@forge.test" };
export const DEV_GUEST = { name: "Gus Guest", email: "guest@forge.test" };

export async function call(path: string, body: unknown, cookie?: string, method = "POST") {
  const response = await fetch(`${BASE}${path}`, {
    method,
    headers: { "content-type": "application/json", ...(cookie ? { cookie } : {}) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const session = response.headers.get("set-cookie")?.split(";")[0];
  return { status: response.status, body: await response.json().catch(() => null), session };
}

/** Signs up, or signs in if the account exists. Returns the session cookie and workspaces. */
export async function account(
  person: { name: string; email: string },
  extra: Record<string, string>,
) {
  const signup = await call("/api/auth/signup", { ...person, password: DEV_PASSWORD, ...extra });
  if (signup.status === 200) return signup;
  const login = await call("/api/auth/login", { email: person.email, password: DEV_PASSWORD });
  if (login.status !== 200)
    throw new Error(`couldn't sign in ${person.email}: ${JSON.stringify(login.body)}`);
  return login;
}

if (import.meta.main) {
  const admin = await account(DEV_ADMIN, { workspace_name: "Forge" });
  const workspace = admin.body.workspaces.find((w: { name: string }) => w.name === "Forge");
  const invite = await call(`/api/w/${workspace.id}/invites`, { role: "member" }, admin.session);
  await account(DEV_MEMBER, { invite_token: invite.body.token });

  const guestInvite = await call(
    `/api/w/${workspace.id}/invites`,
    { role: "guest" },
    admin.session,
  );
  const guest = await account(DEV_GUEST, { invite_token: guestInvite.body.token });
  const snapshot = await call(
    `/api/w/${workspace.id}/sync/bootstrap`,
    undefined,
    admin.session,
    "GET",
  );
  const team = snapshot.body.records.Team?.[0];
  if (team) {
    const access = await call(
      `/api/w/${workspace.id}/members/${guest.body.user.id}/access`,
      { items: [team.id] },
      admin.session,
      "PUT",
    );
    if (access.status !== 204) throw new Error(`couldn't add the guest to ${team.name}`);
  }

  console.log(
    `Seeded ${DEV_ADMIN.email} (admin), ${DEV_MEMBER.email} (member) and ${DEV_GUEST.email} ` +
      `(guest in ${team?.name ?? "no team"}) in "${workspace.name}".`,
  );
  console.log(
    `Password for all three: see DEV_PASSWORD in scripts/seed-dev.ts. Open ${BASE}/${workspace.url_key}`,
  );
}
