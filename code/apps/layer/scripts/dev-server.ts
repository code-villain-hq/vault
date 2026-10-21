// Runs Layer's server and its web app together, with prefixed output.
// Stops both when either exits or on Ctrl-C. (`bun run dev` is the desktop
// app's Vite, for the mock harness.)

const root = new URL("..", import.meta.url).pathname;

const processes = [
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
