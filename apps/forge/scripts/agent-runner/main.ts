// Claude Code's runner on the PO's Mac: Forge POSTs a wake-up here when
// there's work for the agent, and this starts a Claude Code run with the
// "forge-pickup" task's prompt. It also checks for work every hour, for
// wake-ups missed while the Mac slept. Setup: README.md next to this.

import { homedir } from "node:os";
import { createRunner, describe, type RunOutcome, type Wakeup } from "./runner";

const home = homedir();
const root = new URL("../..", import.meta.url).pathname;

const config = {
  port: Number(process.env.FORGE_RUNNER_PORT ?? 3100),
  secretFile: process.env.FORGE_RUNNER_SECRET_FILE ?? `${home}/.config/forge/runner-secret`,
  prompt:
    process.env.FORGE_RUNNER_PROMPT ?? `${home}/.claude/scheduled-tasks/forge-pickup/SKILL.md`,
  // Where Claude Code runs: the main checkout.
  checkout: process.env.FORGE_RUNNER_CHECKOUT ?? root,
  // The runs' only MCP server, Forge with the agent's own key: the
  // checkout's own Forge server is the chat session's agent (FOR-43).
  mcpConfig: process.env.FORGE_RUNNER_MCP_CONFIG ?? `${home}/.config/forge/runner-mcp.json`,
  claude: process.env.FORGE_RUNNER_CLAUDE ?? "claude",
  permissionMode: process.env.FORGE_RUNNER_PERMISSION_MODE ?? "auto",
  timeoutMinutes: Number(process.env.FORGE_RUNNER_TIMEOUT_MINUTES ?? 120),
  checkEveryMinutes: Number(process.env.FORGE_RUNNER_CHECK_EVERY_MINUTES ?? 60),
};

const secret = (await Bun.file(config.secretFile).text()).trim();
if (!secret) throw new Error(`no runner secret in ${config.secretFile}`);
if (!(await Bun.file(config.mcpConfig).exists())) {
  throw new Error(`no MCP settings in ${config.mcpConfig}; run \`bun run runner:install\``);
}

const runner = createRunner({ secret, run, log });
Bun.serve({ hostname: "127.0.0.1", port: config.port, fetch: runner.fetch });
setInterval(runner.checkIn, config.checkEveryMinutes * 60_000);
log(`listening on http://127.0.0.1:${config.port}/wake, running Claude Code in ${config.checkout}`);

async function run(wakeups: Wakeup[]): Promise<RunOutcome> {
  // Read on every run, so edits to the task apply without a restart.
  const task = stripFrontmatter(await Bun.file(config.prompt).text());
  const why =
    wakeups.length > 0
      ? `Forge woke you (${describe(wakeups)}).`
      : "The Forge runner started you to check for work.";
  const prompt = [
    `${why} This is an unattended run started by the Forge runner: nobody is`,
    "watching, so don't ask in the terminal; where the instructions say to ask,",
    "ask in Forge.",
    "",
    task,
  ].join("\n");

  const child = Bun.spawn(
    [
      config.claude,
      "--print",
      "--permission-mode",
      config.permissionMode,
      "--strict-mcp-config",
      "--mcp-config",
      config.mcpConfig,
      "--name",
      `Forge runner: ${wakeups.length > 0 ? describe(wakeups) : "check for work"}`,
      prompt,
    ],
    { cwd: config.checkout, stdout: "pipe", stderr: "inherit", env: process.env },
  );
  const timeout = setTimeout(() => {
    log(`run took over ${config.timeoutMinutes} minutes; stopping it`);
    child.kill();
  }, config.timeoutMinutes * 60_000);
  const output = await new Response(child.stdout).text();
  const code = await child.exited;
  clearTimeout(timeout);

  process.stdout.write(`${output.trimEnd()}\n`);
  if (code !== 0) return "failed";
  return output.includes("Nothing to pick up.") ? "idle" : "worked";
}

function stripFrontmatter(text: string): string {
  return text.replace(/^---\n[\s\S]*?\n---\n/, "").trim();
}

function log(message: string) {
  process.stdout.write(`[${new Date().toISOString()}] ${message}\n`);
}
