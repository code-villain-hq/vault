// Runs the server and the web app together, with prefixed output, for Acme
// Inc as dev-id stands in for its account (code/kit/dev-id), or with your
// own settings (glyph.toml, or GLYPH_OIDC_ISSUER in .env).
// Stops them all when one exits or on Ctrl-C.

import { devCompany } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

// In development people reach Glyph through Vite, so that's where signing in
// sends them back. Set here rather than in `.env`, which `bun run app`
// reads too, and whose Glyph is at 7490.
process.env.GLYPH_PUBLIC_URL ||= "http://localhost:5174";

// The developer's own Glyph data stays in `glyph`; Acme's is in `glyph_acme`.
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

const processes = [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId }] : []),
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "glyph-server"] },
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
