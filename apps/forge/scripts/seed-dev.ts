// Creates local test accounts through the API: an admin with a "Forge"
// workspace, and a member who joined it by invite link. Safe to run again.
// Needs `bun run dev` running. Local development only.

export const BASE = process.env.FORGE_URL ?? "http://localhost:5173";

/** Test-only password for the local seed accounts. */
export const DEV_PASSWORD = "forge-dev-password";
export const DEV_ADMIN = { name: "Ada Admin", email: "admin@forge.test" };
export const DEV_MEMBER = { name: "Max Member", email: "member@forge.test" };

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

  console.log(
    `Seeded ${DEV_ADMIN.email} (admin) and ${DEV_MEMBER.email} (member) in "${workspace.name}".`,
  );
  console.log(
    `Password for both: see DEV_PASSWORD in scripts/seed-dev.ts. Open ${BASE}/${workspace.url_key}`,
  );
}
