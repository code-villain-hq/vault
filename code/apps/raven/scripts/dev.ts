// Runs the server and the web app together, with prefixed output, for Acme
// Inc as dev-id stands in for its account (code/kit/dev-id), or with your
// own settings (raven.toml, or RAVEN_OIDC_ISSUER in .env).
// Stops them all when one exits or on Ctrl-C.

import { devCompany, gitShortCommit, runPrefixed } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

process.env.RAVEN_PUBLIC_URL ||= "http://localhost:5175";

const dev = await devCompany(
  "raven",
  root,
  process.env.DATABASE_URL ?? "postgres://raven:raven@localhost:5435/raven",
);
console.log(
  dev
    ? "Raven for Acme Inc: sign in as anyone in kit/dev-id/people.toml."
    : "Raven with your own settings.",
);

// Raven marks its own calls with the commit it runs, so a change that made
// something slower shows where it started.
process.env.RAVEN_VERSION ||= gitShortCommit(root);

await runPrefixed(root, [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId }] : []),
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "raven-server"] },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "apps/web", "dev"] },
]);
