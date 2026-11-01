// A stand-in for claude-agent-acp in the server's tests (`claude_code.rs`),
// and for codex-acp, when the session comes without Claude Code's options.
// It speaks ACP on stdin and stdout and answers the way Claude Code would
// with Almanac's tools: it calls them over the MCP server the session names,
// picked by which tools the session allows (Codex's: those the server
// lists). Each session's options are appended to the file named first, one
// JSON line each, for the test to read.
// While that file's name with `.hang` exists, the next session never opens,
// as Claude Code's sometimes doesn't, and the file goes. Each prompt's
// blocks go to that file's name with `.prompts`, one JSON line each.

import { appendFileSync, existsSync, readFileSync, rmSync } from "node:fs";

type Session = { url: string; authorization: string; system: string; allowed: string[] };

const log = process.argv[2];
if (!log) throw new Error("Usage: bun claude-code.ts <file for the sessions' options>");
/** The brief a set-up writes around "Zimmerpflanzen, ich schicke Fotos". */
const BRIEF =
  "Zimmerpflanzen, ich schicke Fotos\nThe houseplants at home: watering, feeding, repotting.";
let session: Session | undefined;

function send(message: object) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function update(what: object) {
  send({ jsonrpc: "2.0", method: "session/update", params: { sessionId: "s1", update: what } });
}

function say(text: string) {
  update({ sessionUpdate: "agent_message_chunk", content: { type: "text", text } });
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
  // Claude Code tells the client about each tool it calls.
  update({ sessionUpdate: "tool_call", toolCallId: name, title: name, rawInput: args });
  const said = await mcp("tools/call", { name, arguments: args });
  if (said.result?.isError) throw new Error(said.result.content[0].text);
}

async function answer(prompt: string) {
  const allowed = session?.allowed ?? [];
  await mcp("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "fake" },
  });
  if (allowed.includes("mcp__almanac__set_up_topic")) {
    await tool("set_up_topic", {
      sticks_to: "The houseplants at home: watering, feeding, repotting.",
      icon: "sprout",
      group: "Home",
      template: "collection",
      cards: "Plants",
      starters: ["How often do I water the Monstera?"],
    });
    say("Set up.");
  } else if (allowed.includes("mcp__almanac__propose_topic")) {
    const rename = /rename (\S+) to (\S+)/i.exec(prompt);
    if (rename) {
      await tool("change_topic", {
        topic: rename[1],
        name: rename[2],
        said: `Renamed it ${rename[2]}.`,
      });
      say("Done.");
    } else {
      await tool("propose_topic", {
        name: "Plants",
        sticks_to: "The houseplants at home: watering, feeding, repotting.",
        icon: "sprout",
        group: "Home",
        template: "collection",
        cards: "Plants",
        starters: ["How often do I water the Monstera?", "When do I repot?"],
        fields: [{ name: "Light", kind: "text" }],
        points: [{ block: "Plants", text: "Monstera", cells: { light: "North window" } }],
        reminders: [
          { theme: "Monstera", title: "Water the Monstera", due_on: "2026-11-01", every_days: 7 },
        ],
      });
      say("Here's a topic for your plants.");
    }
    await tool("name_talk", { title: "Plants" });
  } else if (/\bin ([A-Z]\w+) note (.+)$/m.test(prompt)) {
    const [, topic, text] = /\bin ([A-Z]\w+) note (.+)$/m.exec(prompt) ?? [];
    try {
      await tool("add_block", { topic, kind: "paragraph", title: "Notes" });
      await tool("add_items", { topic, block: "Notes", items: [{ text }] });
      say("Noted there.");
    } catch (refused) {
      say(String(refused));
    }
  } else if (/\bcall ([A-Z]\w+) ([A-Z]\w+)\b/.test(prompt)) {
    const [, topic, name] = /\bcall ([A-Z]\w+) ([A-Z]\w+)\b/.exec(prompt) ?? [];
    try {
      await tool("change_topic", { topic, name, said: `Renamed it ${name}.` });
      say("Renamed.");
    } catch (refused) {
      say(String(refused));
    }
  } else if (/move ([bp]\d+) to (\S+)/i.test(prompt)) {
    const [, what, to] = /move ([bp]\d+) to (\S+)/i.exec(prompt) ?? [];
    await tool("move_to_sub_topic", { to, takes: [what] });
    say("Moved.");
  } else if (/tidy/i.test(prompt)) {
    await tool("change_points", {
      points: [{ point: "p1", text: "Water every 7 days, once the top is dry." }],
    });
    await tool("move", { points: ["p1"], to: "b1" });
    await tool("remove", { what: ["b2"] });
    say("Tidied.");
  } else if (/from now on/i.test(prompt)) {
    say("I will.");
    await tool("change_brief", {
      sticks_to: `${BRIEF}\nAnswers in English.`,
      said: "Answers in English.",
    });
  } else if (/washed/i.test(prompt)) {
    say("Ticked.");
    await tool("tick", { items: ["p1"] });
  } else if (/moved/i.test(prompt)) {
    say("Then Friday it is.");
    await tool("change_points", { points: [{ point: "p1", text: "Wash on Friday" }] });
    await tool("offer_reminder", {
      item: "p1",
      due_on: "2026-10-30",
      every_days: 7,
      replaces: ["r1"],
    });
  } else if (/wash/i.test(prompt)) {
    say("Thursday, then.");
    await tool("add_rows", {
      block: "To do",
      rows: [{ name: "Wash on Thursday", source: "You said" }],
    });
    await tool("offer_reminder", { item: "p1", due_on: "2026-10-29", every_days: 7 });
  } else {
    say("Water it every 7 days.");
    await tool("add_block", { kind: "paragraph", title: "Care" });
    await tool("add_items", {
      block: "Care",
      items: [
        {
          text: "Water every 7 days.",
          source: "Royal Horticultural Society",
          url: "https://www.rhs.org.uk/plants/monstera",
        },
      ],
    });
    say("Check that the top of the soil is dry first.");
  }
}

for await (const line of console) {
  const message = JSON.parse(line);
  const { id, method, params } = message;
  if (method === "initialize") {
    send({
      jsonrpc: "2.0",
      id,
      result: { protocolVersion: 1, agentInfo: { name: "fake", version: "0.0.0-test" } },
    });
  } else if (method === "session/new") {
    if (existsSync(`${log}.hang`)) {
      rmSync(`${log}.hang`);
      continue;
    }
    const server = params.mcpServers[0];
    const options = params._meta?.claudeCode?.options;
    session = {
      url: server?.url ?? "",
      authorization:
        server?.headers.find((h: { name: string }) => h.name === "Authorization")?.value ?? "",
      system: params._meta?.systemPrompt ?? "",
      allowed: options?.allowedTools ?? [],
    };
    if (options) {
      appendFileSync(
        log,
        `${JSON.stringify({ system: session.system, tools: options.tools, allowed: session.allowed })}\n`,
      );
    } else {
      // Codex: its prompt and tools came with its start.
      const config = JSON.parse(process.env.CODEX_CONFIG ?? "{}");
      if (config.model_instructions_file) {
        session.system = readFileSync(config.model_instructions_file, "utf8");
      }
      if (server) {
        const listed = await mcp("tools/list", {});
        session.allowed = listed.result.tools.map(
          (t: { name: string }) => `mcp__almanac__${t.name}`,
        );
      }
      appendFileSync(
        log,
        `${JSON.stringify({ system: session.system, config, home: process.env.CODEX_HOME, allowed: session.allowed })}\n`,
      );
    }
    send({ jsonrpc: "2.0", id, result: { sessionId: "s1" } });
  } else if (method === "session/prompt") {
    appendFileSync(`${log}.prompts`, `${JSON.stringify(params.prompt)}\n`);
    await answer(params.prompt[0].text);
    send({ jsonrpc: "2.0", id, result: { stopReason: "end_turn" } });
  }
}
