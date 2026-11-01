// The verification gate: everything here must pass before a change is done.
import { $ } from "bun";
import { assertTypesCurrent, runChecks } from "../../../kit/dev-id/check";

const root = new URL("..", import.meta.url).pathname;
// The studio's sync client, which Raven builds on (`code/kit/sync`).
const kitClient = `${root}../../kit/sync/client`;
// The parts every studio web app shares (`code/kit/web`).
const kitWeb = `${root}../../kit/web`;
const vp = `${root}node_modules/.bin/vp`;
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
  // Oxfmt, Oxlint and TypeScript's own errors, as vite.config.ts says.
  ["vp check", () => $`${vp} check`.cwd(root)],
  // The kits, each with its own vite.config.ts: Vite+ checks only under
  // the folder it runs in.
  ["vp check (sync kit)", () => $`${vp} check`.cwd(kitClient)],
  ["vp check (web kit)", () => $`${vp} check`.cwd(kitWeb)],
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
