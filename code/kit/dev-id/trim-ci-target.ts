// Keeps a CI job's Rust build (its `CARGO_TARGET_DIR`) from growing without
// end. Every workflow runs it after the check: `bun trim-ci-target.ts <dir>`.
//
// The runner checks the vault out into a new folder for every job, and Cargo
// names the build of a crate outside the workspace (a kit) after the folder
// it came from, and every crate that depends on it after that build. So each
// job builds the vault's own crates anew, under new names, and Cargo never
// deletes the old ones: it reached 134 GB in five days. This removes the
// vault's own crates and keeps the crates.io builds, which are what the
// cache is for. If those still pass 20 GB (dependency updates, a new Rust),
// it removes the whole build.
import { existsSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { $ } from "bun";

const target = process.argv[2];
if (!target) {
  console.error("usage: bun trim-ci-target.ts <CARGO_TARGET_DIR>");
  process.exit(2);
}
if (!existsSync(target)) process.exit(0);

const limitKb = 20 * 1024 * 1024;
const codeRoot = new URL("../..", import.meta.url).pathname;

const own = new Set<string>();
for (const file of (await $`git ls-files -- '*Cargo.toml'`.cwd(codeRoot).text()).split("\n")) {
  if (!file) continue;
  const manifest = Bun.TOML.parse(await Bun.file(join(codeRoot, file)).text()) as {
    package?: { name?: string };
  };
  if (manifest.package?.name) own.add(manifest.package.name);
}

const sizeKb = async () => Number((await $`du -sk ${target}`.text()).split("\t")[0]);
const gb = (kb: number) => `${(kb / 1024 / 1024).toFixed(1)} GB`;
const before = await sizeKb();

// A profile's (debug, release) build of one crate is a unit: its folder in
// .fingerprint is `<package>-<hash>`, and the same hash names its files in
// deps/ and examples/ and its build script's folders in build/.
for (const profile of readdirSync(target)) {
  const dir = join(target, profile);
  const fingerprints = join(dir, ".fingerprint");
  if (!existsSync(fingerprints)) continue;
  const hashes = new Set<string>();
  for (const unit of readdirSync(fingerprints)) {
    const dash = unit.lastIndexOf("-");
    if (!own.has(unit.slice(0, dash))) continue;
    hashes.add(unit.slice(dash + 1));
    rmSync(join(fingerprints, unit), { recursive: true, force: true });
  }
  for (const sub of ["build", "deps", "examples"]) {
    if (!existsSync(join(dir, sub))) continue;
    for (const file of readdirSync(join(dir, sub))) {
      const hash = /-([0-9a-f]{16})(\.|$)/.exec(file)?.[1];
      if (hash && hashes.has(hash)) rmSync(join(dir, sub, file), { recursive: true, force: true });
    }
  }
  // Cargo builds only the workspace's crates incrementally.
  rmSync(join(dir, "incremental"), { recursive: true, force: true });
}

const after = await sizeKb();
if (after > limitKb) {
  rmSync(target, { recursive: true, force: true });
  console.log(`${target}: ${gb(before)}, still ${gb(after)} without the vault's crates: removed`);
} else {
  console.log(`${target}: ${gb(before)} → ${gb(after)}, the crates.io builds kept`);
}
