// Installs the runner as a launch agent on this Mac (see README.md): keeps
// the runner secret and the agent's key, writes the launch agent, and
// (re)starts it. `--new-secret` asks for the secret again, after "New
// secret" in Forge; `--new-key` for the key, after a new key in Forge.
// `FORGE_URL` is the Forge server the runs talk to (http://localhost:7380).

import { chmod, mkdir } from "node:fs/promises";
import { homedir } from "node:os";
import { $ } from "bun";
import { mcpConfig } from "./runner";

const label = "studio.villain.forge-runner";
const home = homedir();
const root = new URL("../..", import.meta.url).pathname;
const secretFile = `${home}/.config/forge/runner-secret`;
const mcpConfigFile = `${home}/.config/forge/runner-mcp.json`;
const plistFile = `${home}/Library/LaunchAgents/${label}.plist`;
const logFile = `${home}/Library/Logs/forge/runner.log`;

if (!(await Bun.file(secretFile).exists()) || process.argv.includes("--new-secret")) {
  const secret = prompt("Runner secret from Settings → Agents (forge_runner_…):")?.trim();
  if (!secret?.startsWith("forge_runner_")) {
    console.error("That isn't a runner secret; nothing installed.");
    process.exit(1);
  }
  await mkdir(`${home}/.config/forge`, { recursive: true });
  await Bun.write(secretFile, `${secret}\n`);
  await chmod(secretFile, 0o600);
}

// The runs act as this key's agent ("Claude Code"), not as whatever the
// checkout's own Forge MCP server signs in as (the chat session's agent).
if (!(await Bun.file(mcpConfigFile).exists()) || process.argv.includes("--new-key")) {
  const key = prompt("Key for the runner's agent from Settings → Agents (forge_…):")?.trim();
  if (!key?.startsWith("forge_") || key.startsWith("forge_runner_")) {
    console.error("That isn't an agent's key; nothing installed.");
    process.exit(1);
  }
  const forgeUrl = process.env.FORGE_URL ?? "http://localhost:7380";
  await mkdir(`${home}/.config/forge`, { recursive: true });
  await Bun.write(mcpConfigFile, `${JSON.stringify(mcpConfig(forgeUrl, key), null, 2)}\n`);
  await chmod(mcpConfigFile, 0o600);
}

await mkdir(`${home}/Library/Logs/forge`, { recursive: true });
await Bun.write(
  plistFile,
  `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!-- Claude Code's Forge runner (scripts/agent-runner), kept running. -->
<plist version="1.0">
<dict>
  <key>Label</key><string>${label}</string>
  <key>WorkingDirectory</key><string>${xml(root)}</string>
  <key>ProgramArguments</key>
  <array><string>${xml(process.execPath)}</string><string>scripts/agent-runner/main.ts</string></array>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>${xml(process.env.PATH ?? "")}</string></dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>15</integer>
  <key>StandardOutPath</key><string>${xml(logFile)}</string>
  <key>StandardErrorPath</key><string>${xml(logFile)}</string>
</dict>
</plist>
`,
);

const domain = `gui/${process.getuid?.()}`;
await $`launchctl bootout ${domain}/${label}`.quiet().nothrow();
await $`launchctl bootstrap ${domain} ${plistFile}`;
console.log(`Installed ${label}, running ${root}scripts/agent-runner/main.ts.`);
console.log(`Runner URL for Settings → Agents: http://localhost:3100/wake. Logs: ${logFile}`);

function xml(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
