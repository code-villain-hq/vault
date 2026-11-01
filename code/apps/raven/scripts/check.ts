// The verification gate: everything here must pass before a change is done.
import { $ } from "bun";
import { assertTypesCurrent, runChecks } from "../../../kit/dev-id/check";

const root = new URL("..", import.meta.url).pathname;
// The studio's sync client, which Raven builds on (`code/kit/sync`).
const kitClient = `${root}../../kit/sync/client`;
// The parts every studio web app shares (`code/kit/web`).
const kitWeb = `${root}../../kit/web`;
// RAVEN_CHECK_SKIP_DESKTOP leaves the desktop app out, for a quicker check
// that doesn't wait for Tauri to build.
const crates = process.env.RAVEN_CHECK_SKIP_DESKTOP
  ? ["--workspace", "--exclude", "raven-desktop"]
  : ["--workspace"];

await runChecks([
  ["cargo fmt", () => $`cargo fmt --all --check`.cwd(root)],
  ["cargo clippy", () => $`cargo clippy ${crates} --all-targets -- -D warnings`.cwd(root)],
  ["cargo test", () => $`cargo test ${crates} --quiet`.cwd(root)],
  [
    "generated types are current",
    () =>
      assertTypesCurrent(`${root}packages/types/src`, () =>
        $`bun scripts/gen-types.ts`.cwd(root).quiet(),
      ),
  ],
  // Shared tokens are generated, and Raven bundles a copy: neither may drift.
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
  ["biome (web kit)", () => $`${root}node_modules/.bin/biome check .`.cwd(kitWeb)],
  ["typecheck", () => $`bun run typecheck`.cwd(root)],
  [
    "bun test",
    async () => {
      await $`bun test`.cwd(root);
      await $`bun test`.cwd(kitClient);
      await $`bun test`.cwd(kitWeb);
    },
  ],
  ["web build", () => $`bun run --cwd apps/web build`.cwd(root).quiet()],
]);
