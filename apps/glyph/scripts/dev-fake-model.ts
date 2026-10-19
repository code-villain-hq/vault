// `bun run dev` with a stand-in model, so runs, suggestions and Find
// variants work without llama-server or a download. It answers like an
// OpenAI-style server: the text with the target language's name after it,
// at a confidence that varies by text, so Review has some to look at.

const root = new URL("..", import.meta.url).pathname;
const PORT = 7483;

/** A stable number from 0 to 1 for a text. */
function spread(text: string): number {
  let hash = 0;
  for (const char of text) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) >>> 0;
  return (hash % 1000) / 1000;
}

/** The text a prompt asks about, from either the first prompt or a correction. */
function textIn(prompt: string): string {
  const line = prompt.match(/^(?:Text to translate|Source text): (.*)$/m)?.[1] ?? '""';
  try {
    return JSON.parse(line) as string;
  } catch {
    return line;
  }
}

function answer(prompt: string): string {
  const variants = prompt.match(/The word "(.+?)" is the canonical/);
  if (variants?.[1]) {
    const term = variants[1];
    return JSON.stringify({ variants: [term, `${term}en`, `${term}er`] });
  }
  const target = prompt.match(/(?:Translate|translation) from .+? to (.+?)[.\s]/)?.[1] ?? "?";
  const text = textIn(prompt);
  // Uses each glossary term the text has, so Review sees few glossary misses.
  const terms = [...prompt.matchAll(/^ {2}- "(.+?)" → "(.+?)"/gm)]
    .filter(([, from]) => from && text.toLowerCase().includes(from.toLowerCase()))
    .map(([, , to]) => to);
  const unique = [...new Set(terms)];
  return JSON.stringify({
    translation: `${text} [${target}]${unique.length ? ` ${unique.join(" ")}` : ""}`,
    confidence: Math.round((0.45 + spread(text) * 0.54) * 100) / 100,
  });
}

Bun.serve({
  port: PORT,
  hostname: "127.0.0.1",
  async fetch(request) {
    const { pathname } = new URL(request.url);
    if (pathname === "/health") return new Response("ok");
    if (pathname === "/v1/chat/completions" && request.method === "POST") {
      const body = (await request.json()) as { messages?: { content?: string }[] };
      const prompt = body.messages?.map((m) => m.content ?? "").join("\n") ?? "";
      await Bun.sleep(150);
      return Response.json({ choices: [{ message: { content: answer(prompt) } }] });
    }
    return new Response("not found", { status: 404 });
  },
});

const models = `${root}target/fake-models.toml`;
await Bun.write(
  models,
  `# Written by scripts/dev-fake-model.ts.\n[[models]]\nname = "fake"\nendpoint = "http://localhost:${PORT}"\n`,
);
console.log(`fake model on http://localhost:${PORT}`);

const dev = Bun.spawn(["bun", "scripts/dev.ts"], {
  cwd: root,
  stdout: "inherit",
  stderr: "inherit",
  env: { ...process.env, GLYPH_MODELS: models },
});
const stop = () => dev.kill();
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
process.exit(await dev.exited);
