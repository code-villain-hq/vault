// The verification gate: everything here must pass before a change is done.
import { readdir } from "node:fs/promises";
import { $ } from "bun";

const root = new URL("..", import.meta.url).pathname;
// The studio's sync client, which Glyph builds on (`code/kit/sync`).
const kitClient = `${root}../../kit/sync/client`;
// CI checks the desktop app in a job of its own, so the main check doesn't
// wait for Tauri to build.
const crates = process.env.GLYPH_CHECK_SKIP_DESKTOP
  ? ["--workspace", "--exclude", "glyph-desktop"]
  : ["--workspace"];

const steps: [string, () => Promise<unknown>][] = [
  ["cargo fmt", () => $`cargo fmt --all --check`.cwd(root)],
  ["cargo clippy", () => $`cargo clippy ${crates} --all-targets -- -D warnings`.cwd(root)],
  ["cargo test", () => $`cargo test ${crates} --quiet`.cwd(root)],
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
  // Shared tokens are generated, and Glyph bundles a copy: neither may drift.
  [
    "design tokens are current",
    async () => {
      await $`python3 build.py --check`.cwd(`${root}../../../studio/design/tokens`);
      await $`bun scripts/sync-design.ts --check`.cwd(root);
    },
  ],
  ["biome", () => $`bunx biome check .`.cwd(root)],
  // The kit's client, with its own settings: Biome checks only under its root.
  ["biome (sync kit)", () => $`${root}node_modules/.bin/biome check .`.cwd(kitClient)],
  ["typecheck", () => $`bun run typecheck`.cwd(root)],
  [
    "bun test",
    async () => {
      await $`bun test`.cwd(root);
      await $`bun test`.cwd(kitClient);
    },
  ],
  ["web build", () => $`bun run --cwd apps/web build`.cwd(root).quiet()],
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
