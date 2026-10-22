// Runs Layer's server and its web app together, with prefixed output, for
// Acme Inc as dev-id stands in for its account (code/kit/dev-id), or with
// your own settings (layer.toml, or LAYER_OIDC_ISSUER in the environment).
// Stops them all when one exits or on Ctrl-C. (`bun run dev` is the desktop
// app's Vite, for the mock harness.)

import { devCompany } from "../../../kit/dev-id/dev";

const root = new URL("..", import.meta.url).pathname;

// In development people reach Layer through Vite, so that's where signing in
// sends them back.
process.env.LAYER_PUBLIC_URL ||= "http://localhost:5176";

// The developer's own Layer data stays in `layer`; Acme's is in `layer_acme`.
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

const processes = [
  ...(dev?.devId ? [{ name: "dev-id", color: 33, cmd: dev.devId, env: {} }] : []),
  // Pages asked of the server (the desktop app's Members…) go on to Vite.
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "layer-server"], env: { LAYER_WEB_DEV_URL: "http://localhost:5176" } },
  { name: "web", color: 36, cmd: ["bun", "run", "--cwd", "web", "dev"], env: {} },
].map(({ name, color, cmd, env }) => {
  const child = Bun.spawn(cmd, { cwd: root, env: { ...process.env, ...env }, stdout: "pipe", stderr: "pipe" });
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
