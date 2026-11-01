// A stand-in for claude-agent-acp in the server's tests (`claude_code.rs`).
// It speaks ACP on stdin and stdout and answers the way Claude Code would
// with Forge AI's tools: it calls them over the MCP server the session
// names, reads the service's code when it may, and finishes. Each session's
// options are appended to the file named first, one JSON line each, for the
// test to read.

import { appendFileSync } from "node:fs";

type Session = { url: string; authorization: string; allowed: string[] };

const log = process.argv[2];
if (!log) throw new Error("Usage: bun claude-code.ts <file for the sessions' options>");
let session: Session | undefined;

function send(message: object) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function say(text: string) {
  send({
    jsonrpc: "2.0",
    method: "session/update",
    params: {
      sessionId: "s1",
      update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text } },
    },
  });
}

async function mcp(method: string, params: object) {
  if (!session) throw new Error("no session");
  const response = await fetch(session.url, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: session.authorization },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  return response.json();
}

async function tool(name: string, args: object) {
  const said = await mcp("tools/call", { name, arguments: args });
  if (said.result?.isError) throw new Error(said.result.content[0].text);
  return said.result.content[0].text as string;
}

async function answer() {
  const allowed = session?.allowed ?? [];
  await mcp("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "fake" },
  });
  if (allowed.includes("mcp__forge__read_file")) {
    const code = await tool("read_file", { path: "src/limits.rs" });
    const limited = code.includes("rate_limit") ? "is limited" : "isn't limited";
    await tool("finish", { summary: `Checked on this Mac: the API ${limited}.` });
  } else {
    await tool("finish", { summary: "Checked on this Mac." });
  }
  say("Done.");
}

for await (const line of console) {
  const { id, method, params } = JSON.parse(line);
  if (method === "initialize") {
    send({
      jsonrpc: "2.0",
      id,
      result: { protocolVersion: 1, agentInfo: { name: "fake", version: "0.0.0-test" } },
    });
  } else if (method === "session/new") {
    const server = params.mcpServers[0];
    const options = params._meta?.claudeCode?.options;
    session = {
      url: server?.url ?? "",
      authorization:
        server?.headers.find((h: { name: string }) => h.name === "Authorization")?.value ?? "",
      allowed: options?.allowedTools ?? [],
    };
    appendFileSync(
      log,
      `${JSON.stringify({ system: params._meta?.systemPrompt ?? "", tools: options?.tools, allowed: session.allowed })}\n`,
    );
    send({ jsonrpc: "2.0", id, result: { sessionId: "s1" } });
  } else if (method === "session/prompt") {
    await answer();
    send({ jsonrpc: "2.0", id, result: { stopReason: "end_turn" } });
  }
}
