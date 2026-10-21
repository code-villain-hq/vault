// A local test account, for development only. Needs `bun run dev` running,
// with passwords on (RAVEN_OIDC_ISSUER empty), and no account yet: Raven
// takes a password sign-up for its first account only, which sets it up
// with a workspace. Everyone after joins by invite.
// Then it calls a few pages, so Raven has calls of its own to show.

const server = process.env.RAVEN_URL ?? "http://127.0.0.1:7580";
export const account = {
  name: "Local Villain",
  email: "villain@raven.test",
  password: "raven-local-only",
  workspace_name: "Code Villain",
};

const signup = await fetch(`${server}/api/auth/signup`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(account),
});
if (signup.status === 403) {
  console.log(`Raven has an account already; sign in as ${account.email} if it's this one.`);
} else if (!signup.ok) {
  console.error(`Couldn't sign up: ${signup.status} ${await signup.text()}`);
  process.exit(1);
} else {
  console.log(`Signed up ${account.email} (password in scripts/seed.ts).`);
}

const login = await fetch(`${server}/api/auth/login`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ email: account.email, password: account.password }),
});
const cookie = login.headers.get("set-cookie")?.split(";")[0];
if (!login.ok || !cookie) {
  console.error(`Couldn't sign in: ${login.status} ${await login.text()}`);
  process.exit(1);
}
const me = (await (await fetch(`${server}/api/me`, { headers: { cookie } })).json()) as {
  workspaces: { id: string }[];
};
const w = `/api/w/${me.workspaces[0]?.id}`;

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
