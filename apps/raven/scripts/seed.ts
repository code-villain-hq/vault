// A local test account, for development only. Needs `bun run dev` running,
// with passwords on (RAVEN_OIDC_ISSUER empty), and no account yet: Raven
// takes a password sign-up for its first account only.
// Then it calls a few pages, so Raven has calls of its own to show.

const server = process.env.RAVEN_URL ?? "http://127.0.0.1:7580";
export const account = {
  name: "Local Villain",
  email: "villain@raven.test",
  password: "raven-local-only",
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

// Some ordinary use, and a few calls that miss, so the pages have something on them.
const paths = [
  "/api/me",
  "/api/apps",
  "/api/apps/raven/health",
  "/api/apps/raven/technical",
  "/api/apps/forge/health",
  "/api/apps/glyph/technical",
  "/api/apps/nobody/health",
];
for (let round = 0; round < 6; round++) {
  for (const path of paths) await fetch(`${server}${path}`, { headers: { cookie } });
}
console.log(`Called ${paths.length * 6} pages.`);
