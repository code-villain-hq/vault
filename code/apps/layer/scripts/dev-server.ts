// Runs Layer's server and its web app together, with prefixed output, for
// Acme Inc as dev-id stands in for its account (code/kit/dev-id), or with
// your own settings (layer.toml, or LAYER_OIDC_ISSUER in the environment).
// Stops them all when one exits or on Ctrl-C. (`bun run dev` is the desktop
// app's Vite, for the mock harness.)

import { devCompany, runPrefixed } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

process.env.LAYER_PUBLIC_URL ||= "http://localhost:5176";

const dev = await devCompany(
  "layer",
  root,
  process.env.DATABASE_URL ?? "postgres://layer:layer@localhost:5436/layer",
);
console.log(
  dev
    ? "Layer for Acme Inc: sign in as anyone in kit/dev-id/people.toml."
    : "Layer with your own settings.",
);

await runPrefixed(root, [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId }] : []),
  // Pages asked of the server (the desktop app's Members…) go on to Vite.
  {
    name: "server",
    color: 35,
    cmd: ["cargo", "run", "-p", "layer-server"],
    env: { LAYER_WEB_DEV_URL: "http://localhost:5176" },
  },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "web", "dev"] },
]);
