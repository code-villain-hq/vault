// The verification gate: everything here must pass before a change is done.
import { $ } from "bun";

const root = new URL("..", import.meta.url).pathname;
// Leaves the desktop shell out, for a quick check of the server and web app.
const crates = process.env.RAVEN_CHECK_SKIP_DESKTOP
  ? ["--workspace", "--exclude", "raven-desktop"]
  : ["--workspace"];

const steps: [string, () => Promise<unknown>][] = [
  ["cargo fmt", () => $`cargo fmt --all --check`.cwd(root)],
  ["cargo clippy", () => $`cargo clippy ${crates} --all-targets -- -D warnings`.cwd(root)],
  ["cargo test", () => $`cargo test ${crates} --quiet`.cwd(root)],
  // Shared tokens are generated, and Raven bundles a copy: neither may drift.
  [
    "design tokens are current",
    async () => {
      await $`python3 build.py --check`.cwd(`${root}../../../studio/design/tokens`);
      await $`bun scripts/sync-design.ts --check`.cwd(root);
    },
  ],
  ["biome", () => $`bunx biome check .`.cwd(root)],
  ["typecheck", () => $`bun run typecheck`.cwd(root)],
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
