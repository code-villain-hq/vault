// `bun run dev` with Forge's AI features switched on against a stand-in for
// the Messages API, so they can be tried and worked on without an API key or
// spending anything. The stand-in answers with scripted tool calls: triage
// searches once, then suggests a priority; a drift check lists the files,
// calls every spec requirement unclear, then finishes. Nothing it says is
// meant to be right.

const PORT = 4010;
let count = 0;

function reply(name: string, input: unknown) {
  count += 1;
  return Response.json({
    id: `msg_fake_${count}`,
    type: "message",
    role: "assistant",
    model: "fake",
    content: [{ type: "tool_use", id: `toolu_fake_${count}`, name, input }],
    stop_reason: "tool_use",
    usage: { input_tokens: 0, output_tokens: 0 },
  });
}

// The requirements the drift check's context lists, each called unclear.
function requirements(context: string) {
  const found = [];
  for (const block of context.split("\nRequirements of ").slice(1)) {
    const spec = block.slice(1, block.indexOf('" to give a verdict on:'));
    for (const line of block.split("\n").slice(1)) {
      if (!line.startsWith("- ")) break;
      found.push({
        spec,
        requirement: line.slice(2),
        verdict: "unclear",
        evidence: "A stand-in model gave this verdict, not Claude.",
      });
    }
  }
  return found;
}

Bun.serve({
  port: PORT,
  async fetch(request) {
    const body = (await request.json()) as {
      system?: unknown;
      messages: { role: string; content: unknown }[];
    };
    const triage = JSON.stringify(body.system ?? "").includes("triage");
    const turns = body.messages.filter((m) => m.role === "assistant").length;
    if (triage) {
      if (turns === 0) return reply("search_issues", { query: "the", include_closed: true });
      return reply("suggest", {
        explanation: "A stand-in model suggested this, not Claude.",
        priority: "medium",
      });
    }
    if (turns === 0) return reply("list_files", {});
    const verdicts = requirements(String(body.messages[0]?.content ?? ""));
    if (turns === 1 && verdicts.length > 0) return reply("report_verdicts", { verdicts });
    return reply("finish", { summary: "A stand-in model looked at the file list, not Claude." });
  },
});
console.log(`Stand-in Messages API on :${PORT}`);

const root = new URL("..", import.meta.url).pathname;
const dev = Bun.spawn(["bun", "scripts/dev.ts"], {
  cwd: root,
  stdout: "inherit",
  stderr: "inherit",
  env: {
    ...process.env,
    ANTHROPIC_API_KEY: "fake",
    ANTHROPIC_BASE_URL: `http://127.0.0.1:${PORT}`,
  },
});
process.on("SIGINT", () => dev.kill());
process.on("SIGTERM", () => dev.kill());
process.exit(await dev.exited);
