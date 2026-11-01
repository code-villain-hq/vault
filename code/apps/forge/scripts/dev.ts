// Runs the server and the web app together, with prefixed output, for Acme
// Inc as dev-id stands in for its account (code/kit/dev-id), or with your
// own settings (forge.toml, or FORGE_OIDC_ISSUER in .env).
// Stops them all when one exits or on Ctrl-C.

import { devCompany, runPrefixed } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

// In development people reach Forge through Vite, so that's where signing in
// sends them back. Set here rather than in `.env`, which `bun run app`
// reads too, and whose Forge is at 7390.
process.env.FORGE_PUBLIC_URL ||= "http://localhost:5173";

// The developer's own Forge data stays in `forge`; Acme's is in `forge_acme`.
const dev = await devCompany(
  "forge",
  root,
  process.env.DATABASE_URL ?? "postgres://forge:forge@localhost:5433/forge",
);
console.log(
  dev
    ? "Forge for Acme Inc: sign in as anyone in kit/dev-id/people.toml."
    : "Forge with your own settings.",
);

await runPrefixed(root, [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId }] : []),
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "forge-server"] },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "apps/web", "dev"] },
]);
