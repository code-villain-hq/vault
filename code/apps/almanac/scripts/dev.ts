// Runs the server and the web app together, with prefixed output, for Acme
// Inc as dev-id stands in for its account (code/kit/dev-id), or with your
// own settings (almanac.toml, or ALMANAC_OIDC_ISSUER in .env).
// Stops them all when one exits or on Ctrl-C.

import { devCompany, gitShortCommit, runPrefixed } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

process.env.ALMANAC_PUBLIC_URL ||= "http://localhost:5177";

const dev = await devCompany(
  "almanac",
  root,
  process.env.DATABASE_URL ?? "postgres://almanac:almanac@localhost:5437/almanac",
);
console.log(
  dev
    ? "Almanac for Acme Inc: sign in as anyone in kit/dev-id/people.toml."
    : "Almanac with your own settings.",
);

process.env.ALMANAC_VERSION ||= gitShortCommit(root);

await runPrefixed(root, [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId }] : []),
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "almanac-server"] },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "apps/web", "dev"] },
]);
