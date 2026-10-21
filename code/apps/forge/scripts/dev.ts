// Runs the server and the web app together, with prefixed output.
// Stops both when either exits or on Ctrl-C.

const root = new URL("..", import.meta.url).pathname;

// In development people reach Forge through Vite, so that's where Code Villain
// sign-in sends them back. Set here rather than in `.env`, which `bun run app`
// reads too, and whose Forge is at 7390.
process.env.FORGE_PUBLIC_URL ||= "http://localhost:5173";

const processes = [
  { name: "server", color: 35, cmd: ["cargo", "run", "-p", "forge-server"] },
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
