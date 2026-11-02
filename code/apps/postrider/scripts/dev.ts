// Runs the server and the web app together, with prefixed output: Postrider
// for the person on this Mac, as the desktop app runs it, reading the
// stand-in mail and calendar servers (`bun run db`, then `bun run seed`).
// Stops them all when one exits or on Ctrl-C.

import { runPrefixed } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

// The stand-ins' password, so nothing goes to the keychain. Set it empty to
// use the keychain, for your own accounts.
process.env.POSTRIDER_DEV_PASSWORD ??= "dev";

await runPrefixed(root, [
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "postrider-server"] },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "apps/web", "dev"] },
]);
