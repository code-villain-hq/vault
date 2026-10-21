// The verification gate: everything here must pass before a change is done.
// The server's tests need Postgres (`bun run db`).
import { readdir } from "node:fs/promises";
import { $ } from "bun";

const root = new URL("..", import.meta.url).pathname;
// The studio's sync client, which the web app builds on (`code/kit/sync`).
const kitClient = `${root}../../kit/sync/client`;
// The parts every studio web app shares (`code/kit/web`).
const kitWeb = `${root}../../kit/web`;
const biome = `${root}web/node_modules/.bin/biome`;

const steps: [string, () => Promise<unknown>][] = [
  ["guard", () => $`bun scripts/guard.ts`.cwd(root)],
  ["design copies are current", () => $`bun scripts/sync-design.ts --check`.cwd(root)],
  ["bun test (app)", () => $`bun test scripts`.cwd(root)],
  ["tsc (app)", () => $`bunx tsc`.cwd(root)],
  ["app build", () => $`bunx vite build`.cwd(root).quiet()],
  ["cargo fmt (server)", () => $`cargo fmt -p layer-domain -p layer-sync -p layer-server --check`.cwd(root)],
  ["cargo clippy", () => $`cargo clippy --workspace --all-targets -- -D warnings`.cwd(root)],
  ["cargo test (app)", () => $`cargo test -p villain-layer --lib --quiet`.cwd(root)],
  ["cargo test (server)", () => $`cargo test -p layer-domain -p layer-sync -p layer-server --quiet`.cwd(root)],
  [
    "generated types are current",
    async () => {
      const before = await snapshot(`${root}packages/types/src`);
      await $`bun scripts/gen-types.ts`.cwd(root).quiet();
      if (before !== (await snapshot(`${root}packages/types/src`))) {
        throw new Error(
          "packages/types was out of date; it has been regenerated, review and commit it",
        );
      }
    },
  ],
  ["biome (web)", () => $`${biome} check .`.cwd(`${root}web`)],
  // The kit's client, with its own settings: Biome checks only under its root.
  ["biome (sync kit)", () => $`${biome} check .`.cwd(kitClient)],
  ["biome (web kit)", () => $`${biome} check .`.cwd(kitWeb)],
  ["tsc (web)", () => $`bunx tsc -p web --noEmit`.cwd(root)],
  ["tsc (web kit)", () => $`bunx tsc -p ${kitWeb} --noEmit`.cwd(root)],
  [
    "bun test (web, sync kit)",
    async () => {
      await $`bun test src`.cwd(`${root}web`);
      await $`bun test`.cwd(kitClient);
      await $`bun test`.cwd(kitWeb);
    },
  ],
  ["web build", () => $`bun run --cwd web build`.cwd(root).quiet()],
];

for (const [name, run] of steps) {
  process.stdout.write(`• ${name}\n`);
  try {
    await run();
  } catch (error) {
    if (error instanceof Error && !("exitCode" in error)) console.error(error.message);
    console.error(`\n✗ ${name} failed`);
    process.exit(1);
  }
}
console.log("\n✓ all checks passed");

async function snapshot(dir: string): Promise<string> {
  const files = (await readdir(dir, { recursive: true })).filter((f) => f.endsWith(".ts")).sort();
  const contents = await Promise.all(files.map((f) => Bun.file(`${dir}/${f}`).text()));
  return JSON.stringify(files.map((f, i) => [f, contents[i]]));
}
