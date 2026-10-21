# Claude Code's runner

Forge wakes an agent when there's work for it and doesn't run the agent
itself (decision "Forge wakes agents; their owners run them"; specs
"Connecting agents", "Waking agents"). This is the runner for the agent "Claude Code"
on the PO's Mac. It's a small endpoint (`main.ts`) on
`http://127.0.0.1:3100/wake` that runs as a launch agent, like the dev server
and the GitHub webhook relay.

- It checks `X-Forge-Signature` against the runner secret. It answers 202 at
  once and then starts `claude --print` in the main checkout with the
  "forge-pickup" task's prompt
  (`~/.claude/scheduled-tasks/forge-pickup/SKILL.md`, read again on every
  run).
- The runs act as "Claude Code" and nobody else: their only MCP server is
  Forge, signed in with Claude Code's own key
  (`~/.config/forge/runner-mcp.json`). The checkout's own Forge MCP server is
  the chat session's agent, "Claude Code (chat)", so Forge can tell the two
  apart (FOR-43).
- It does one run at a time. Wake-ups that arrive during a run don't start a
  second one. When the run ends, it checks for more work: it runs again if
  wake-ups came in meanwhile, or if the run did something, until a run
  answers "Nothing to pick up." (at most 5 in a row). A failed run waits for
  the next wake-up.
- Every hour it also starts a run on its own, unless one is going, for
  wake-ups missed while the Mac slept (Forge retries for about an hour and a
  half). This replaces the "forge-pickup" scheduled task, so scheduled runs
  are "Claude Code" too and never overlap a woken one.
- Runs that go over 2 hours are stopped. Logs, including each run's final
  answer, are in `~/Library/Logs/forge/runner.log`.

## Setting it up

1. In Forge, under **Settings → Agents → Claude Code → Wake it when there's
   work**, enter `http://localhost:3100/wake`. Copy the secret: it's shown
   only once. Then make a new key for Claude Code, and copy that too.
2. From the main checkout, run `bun run runner:install` and paste the
   secret and the key. It keeps them in `~/.config/forge/runner-secret` and
   `~/.config/forge/runner-mcp.json` (readable only by you), writes
   `~/Library/LaunchAgents/studio.villain.forge-runner.plist` with this
   checkout, bun and your current `PATH` (which must find `claude`, `git`,
   `gh`, `cargo` and `docker`), and starts it.
3. Check it: `curl -s localhost:3100/` answers `{"running":false}`. Assign an
   issue to Claude Code; the Agents page should show the wake-up as
   delivered, and the log should show a run starting.
4. Turn off the "forge-pickup" scheduled task in the Claude app (don't
   delete it: its file is the runs' prompt). The runner's hourly check
   replaces it.

### The chat session is its own agent

So the chat session in the main checkout doesn't act as "Claude Code":

1. Under **Settings → Agents**, add the agent "Claude Code (chat)" and copy
   its key.
2. In the main checkout, point Claude Code's Forge MCP server at that key:
   `claude mcp remove forge`, then the `claude mcp add …` command Forge
   shows with the key.
3. Revoke the key the chat session used until now under Claude Code (the
   one that isn't the runner's), so nothing but the runner acts as Claude
   Code.

After **New secret** in Forge, run `bun run runner:install --new-secret`;
after a new key for Claude Code, `bun run runner:install --new-key`.
To stop the runner: `launchctl bootout gui/$(id -u)/studio.villain.forge-runner`.

Settings, as environment variables in the launch agent:
`FORGE_RUNNER_PORT` (3100), `FORGE_RUNNER_SECRET_FILE`,
`FORGE_RUNNER_PROMPT` (the task file), `FORGE_RUNNER_CHECKOUT` (where Claude
Code runs; this checkout), `FORGE_RUNNER_MCP_CONFIG` (the runs' MCP
settings), `FORGE_RUNNER_CLAUDE` (`claude`), `FORGE_RUNNER_PERMISSION_MODE`
(`auto`), `FORGE_RUNNER_TIMEOUT_MINUTES` (120),
`FORGE_RUNNER_CHECK_EVERY_MINUTES` (60). `runner:install` takes `FORGE_URL`
(`http://localhost:7380`) for the key's server.
