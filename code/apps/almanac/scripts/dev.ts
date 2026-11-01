// Runs the server and the web app together, with prefixed output, for Acme
// Inc as dev-id stands in for its account (code/kit/dev-id), or with your
// own settings (almanac.toml, or ALMANAC_OIDC_ISSUER in .env).
// Stops them all when one exits or on Ctrl-C.

import { devCompany } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

// In development people reach Almanac through Vite, so that's where signing in
// sends them back.
process.env.ALMANAC_PUBLIC_URL ||= "http://localhost:5177";

// The developer's own Almanac data stays in `almanac`; Acme's is in `almanac_acme`.
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

// Almanac marks its own calls with the commit it runs, so a change that made
// something slower shows where it started.
const commit = Bun.spawnSync(["git", "rev-parse", "--short", "HEAD"], { cwd: root });
const version = commit.success ? commit.stdout.toString().trim() : "";
process.env.ALMANAC_VERSION ||= version;

const processes = [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId }] : []),
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "almanac-server"] },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "apps/web", "dev"] },
].map(({ name, color, cmd }) => {
  const child = Bun.spawn(cmd, { cwd: root, stdout: "pipe", stderr: "pipe", env: process.env });
  const prefix = `\x1b[${color}m${name.padEnd(6)}\x1b[0m │ `;
  pipe(child.stdout, prefix);
  pipe(child.stderr, prefix);
  return child;
});

async function pipe(stream: ReadableStream<Uint8Array>, prefix: string) {
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of stream) {
    buffer += decoder.decode(chunk, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";
    for (const line of lines) process.stdout.write(`${prefix}${line}\n`);
  }
  if (buffer) process.stdout.write(`${prefix}${buffer}\n`);
}

function stopAll() {
  for (const child of processes) child.kill();
}

process.on("SIGINT", stopAll);
process.on("SIGTERM", stopAll);
await Promise.race(processes.map((child) => child.exited));
stopAll();
