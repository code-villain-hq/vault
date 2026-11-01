// Runs the server and the web app together, with prefixed output, for Acme
// Inc as dev-id stands in for its account (code/kit/dev-id), or with your
// own settings (glyph.toml, or GLYPH_OIDC_ISSUER in .env).
// Stops them all when one exits or on Ctrl-C.

import { devCompany, runPrefixed } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

process.env.GLYPH_PUBLIC_URL ||= "http://localhost:5174";

const dev = await devCompany(
  "glyph",
  root,
  process.env.DATABASE_URL ?? "postgres://glyph:glyph@localhost:5434/glyph",
);
console.log(
  dev
    ? "Glyph for Acme Inc: sign in as anyone in kit/dev-id/people.toml."
    : "Glyph with your own settings.",
);

await runPrefixed(root, [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId }] : []),
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "glyph-server"] },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "apps/web", "dev"] },
]);
