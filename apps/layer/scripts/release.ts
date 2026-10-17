#!/usr/bin/env bun
/**
 * A Layer release, built on this Mac and published on the vault's Forgejo
 * (docs/releasing.md).
 *
 *     bun run release           check, build, tag layer-v<version>, and
 *                               draft the release with its .dmg
 *     bun run release:publish   publish that draft and put it in the
 *                               Homebrew tap
 *
 * The draft is there to be read before anyone can download it. Apple silicon
 * only. Signed ad hoc, unless APPLE_SIGNING_IDENTITY, APPLE_ID,
 * APPLE_PASSWORD and APPLE_TEAM_ID are all set: then it is signed with the
 * Developer ID in the keychain and notarised.
 *
 * The Forgejo token is read from the keychain (service git.codevillain.eu,
 * account release), never from a file or the environment.
 */
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

process.chdir(new URL("..", import.meta.url).pathname);

const FORGEJO = "https://git.codevillain.eu";
const REPO = "code-villain/vault";
const TAP = "git.codevillain.eu:code-villain/homebrew-tap.git";
const SIGNING = ["APPLE_SIGNING_IDENTITY", "APPLE_ID", "APPLE_PASSWORD", "APPLE_TEAM_ID"];

function fail(message: string): never {
  console.error(`\nrelease: ${message}`);
  process.exit(1);
}

function run(cmd: string[], options: { cwd?: string; env?: Record<string, string | undefined> } = {}) {
  console.log(`\n$ ${cmd.join(" ")}`);
  const done = Bun.spawnSync(cmd, { cwd: options.cwd, env: options.env ?? process.env, stdout: "inherit", stderr: "inherit" });
  if (done.exitCode !== 0) fail(`${cmd.slice(0, 2).join(" ")} failed.`);
}

function read(cmd: string[], cwd?: string): string {
  const done = Bun.spawnSync(cmd, { cwd });
  if (done.exitCode !== 0) fail(`${cmd.join(" ")}: ${done.stderr.toString().trim()}`);
  return done.stdout.toString().trim();
}

const version =
  readFileSync("src-tauri/Cargo.toml", "utf8").match(/^version = "(.+)"$/m)?.[1] ?? fail("src-tauri/Cargo.toml has no version.");
const tag = `layer-v${version}`;
const dmgName = `Layer-${version}-macos-arm64.dmg`;

let cachedToken: string | undefined;
function token(): string {
  if (cachedToken) return cachedToken;
  const found = Bun.spawnSync(["security", "find-generic-password", "-s", "git.codevillain.eu", "-a", "release", "-w"]);
  if (found.exitCode !== 0) {
    fail(
      "there is no Forgejo token in the keychain. Make one (docs/releasing.md) and store it with\n" +
        "  security add-generic-password -s git.codevillain.eu -a release -w",
    );
  }
  cachedToken = found.stdout.toString().trim();
  return cachedToken;
}

async function api(method: string, path: string, body?: BodyInit, json = true): Promise<any> {
  const headers: Record<string, string> = { Authorization: `token ${token()}` };
  if (body && json) headers["Content-Type"] = "application/json";
  const res = await fetch(`${FORGEJO}/api/v1/repos/${REPO}${path}`, { method, headers, body });
  if (!res.ok) fail(`Forgejo answered ${method} ${path} with ${res.status}: ${await res.text()}`);
  return res.status === 204 ? null : res.json();
}

// ---------------------------------------------------------------------------
// bun run release

async function draft() {
  if (process.arch !== "arm64") fail("the release is built for Apple silicon; run it on an Apple silicon Mac.");
  token();

  // What is released is what everyone can read on main.
  if (read(["git", "branch", "--show-current"]) !== "main") fail("switch to main first.");
  if (read(["git", "status", "--porcelain"])) fail("the vault has uncommitted changes. Commit or stash them first.");
  run(["git", "fetch", "--quiet", "--tags", "origin", "main"]);
  if (read(["git", "rev-parse", "HEAD"]) !== read(["git", "rev-parse", "origin/main"])) {
    fail("main and origin/main differ. Push or pull first: a release is cut from a commit that is on Forgejo.");
  }
  if (read(["git", "ls-remote", "--tags", "origin", `refs/tags/${tag}`])) {
    fail(`${tag} already exists. Bump the version in src-tauri/Cargo.toml first.`);
  }
  const releases: any[] = await api("GET", "/releases?limit=50");
  if (releases.some((r) => r.tag_name === tag)) {
    fail(`there is already a release for ${tag} on Forgejo. Delete the draft there, or bump the version.`);
  }

  const set = SIGNING.filter((name) => process.env[name]);
  const env: Record<string, string | undefined> = { ...process.env };
  for (const name of SIGNING) if (!env[name]) delete env[name];
  let signing: "developer-id" | "ad-hoc";
  if (set.length === SIGNING.length) {
    signing = "developer-id";
  } else if (set.length === 0) {
    // Ad hoc, not unsigned: macOS offers Open Anyway for an app it cannot
    // check, but calls an unsigned download "damaged".
    env.APPLE_SIGNING_IDENTITY = "-";
    signing = "ad-hoc";
  } else {
    // Signed but not notarised is stopped by Gatekeeper all the same.
    fail(`${set.length} of ${SIGNING.join(", ")} are set. Set all of them to sign and notarise, or none to sign ad hoc.`);
  }

  run(["bun", "run", "check"]);
  run(["bun", "tauri", "build", "--bundles", "app,dmg"], { env });

  const bundle = "src-tauri/target/release/bundle";
  if (signing === "developer-id") {
    // What a Mac that downloads it will check.
    const app = `${bundle}/macos/Layer.app`;
    run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", app]);
    run(["xcrun", "stapler", "validate", app]);
    run(["spctl", "--assess", "--type", "execute", "--verbose=4", app]);
  }

  const built = readdirSync(`${bundle}/dmg`).filter((f) => f.endsWith(".dmg") && f.includes(version));
  if (built.length !== 1) fail(`expected one ${version} .dmg in ${bundle}/dmg, found ${built.length}.`);
  const out = mkdtempSync(join(tmpdir(), "layer-release-"));
  copyFileSync(`${bundle}/dmg/${built[0]}`, join(out, dmgName));
  writeFileSync(join(out, "SHA256SUMS.txt"), `${read(["shasum", "-a", "256", dmgName], out)}\n`);

  const previous = read(["git", "tag", "--list", "layer-v*", "--sort=-v:refname"]).split("\n").filter(Boolean)[0];
  const changes = previous ? read(["git", "log", "--format=- %s", `${previous}..HEAD`, "--", "."]) : "";
  const notes = [
    "For Macs with Apple silicon.",
    "",
    "Open the .dmg and drag Layer to Applications. Quit Layer before replacing it:",
    "replacing the app while it runs kills it, and every agent running in it.",
    ...(signing === "ad-hoc"
      ? [
          "",
          'This build is not notarised by Apple, so macOS stops it the first time with "Apple',
          'could not verify…". Choose Done, then System Settings → Privacy & Security → Open',
          "Anyway. macOS also asks for keychain access again after each update.",
        ]
      : []),
    ...(changes ? ["", `## Since ${previous}`, "", changes] : []),
  ].join("\n");

  // No tag is pushed: Forgejo publishes a release at once when its tag
  // already exists, draft or not (0.9.0 went out that way). A draft names
  // the commit instead, and Forgejo makes the tag when it is published.
  const release = await api(
    "POST",
    "/releases",
    JSON.stringify({
      tag_name: tag,
      target_commitish: read(["git", "rev-parse", "HEAD"]),
      name: `Layer ${version}`,
      body: notes,
      draft: true,
      prerelease: false,
    }),
  );
  if (!release.draft) fail(`Forgejo published ${tag} instead of drafting it. Check it on Forgejo before anything else.`);
  for (const name of [dmgName, "SHA256SUMS.txt"]) {
    const form = new FormData();
    form.append("attachment", Bun.file(join(out, name)), name);
    await api("POST", `/releases/${release.id}/assets?name=${encodeURIComponent(name)}`, form, false);
    console.log(`Uploaded ${name}.`);
  }

  console.log(`\nDrafted Layer ${version} (${signing}): ${FORGEJO}/${REPO}/releases`);
  console.log("Read it there, then run: bun run release:publish");
}

// ---------------------------------------------------------------------------
// bun run release:publish

async function publish() {
  // `?draft=true` filters drafts out rather than in, so read them all.
  const releases: any[] = await api("GET", "/releases?limit=50");
  const release = releases.find((r) => r.tag_name === tag) ?? fail(`there is no draft for ${tag}. Run bun run release first.`);
  if (release.draft) {
    await api("PATCH", `/releases/${release.id}`, JSON.stringify({ draft: false }));
    console.log(`Published Layer ${version}: ${FORGEJO}/${REPO}/releases/tag/${tag}`);
  }
  run(["git", "fetch", "--quiet", "--tags", "origin"]);
  // A prerelease would be what everyone gets from `brew upgrade`.
  if (release.prerelease) return console.log("A prerelease is not put in the tap.");

  const sums = release.assets.find((a: any) => a.name === "SHA256SUMS.txt") ?? fail(`${tag} has no SHA256SUMS.txt.`);
  const listed = await (await fetch(sums.browser_download_url)).text();
  const sha = listed.split("\n").find((line) => line.trim().endsWith(dmgName))?.split(/\s+/)[0];
  if (!sha) fail(`${tag}'s SHA256SUMS.txt has no line for ${dmgName}.`);
  const cask = readFileSync("packaging/homebrew/villain-layer.rb", "utf8")
    .replace("@VERSION@", version)
    .replace("@SHA256@", sha);

  // What `brew install` will read: its style, and --online downloads the
  // .dmg and checks it against the SHA-256.
  const check = "code-villain/check";
  Bun.spawnSync(["brew", "untap", "--force", check]);
  run(["brew", "tap-new", "--no-git", check]);
  const casks = join(read(["brew", "--repository", check]), "Casks");
  mkdirSync(casks, { recursive: true });
  writeFileSync(join(casks, "villain-layer.rb"), cask);
  run(["brew", "trust", "--cask", `${check}/villain-layer`]);
  run(["brew", "style", "--cask", `${check}/villain-layer`]);
  run(["brew", "audit", "--cask", "--online", `${check}/villain-layer`]);
  run(["brew", "untap", "--force", check]);

  const tap = mkdtempSync(join(tmpdir(), "layer-tap-"));
  run(["git", "clone", "--quiet", TAP, tap]);
  mkdirSync(join(tap, "Casks"), { recursive: true });
  writeFileSync(join(tap, "Casks/villain-layer.rb"), cask);
  run(["git", "add", "Casks/villain-layer.rb"], { cwd: tap });
  if (Bun.spawnSync(["git", "diff", "--cached", "--quiet"], { cwd: tap }).exitCode === 0) {
    return console.log("The tap already has this cask.");
  }
  run(["git", "commit", "--quiet", "-m", `villain-layer ${version}`], { cwd: tap });
  run(["git", "push", "--quiet"], { cwd: tap });
  console.log(`The tap has Layer ${version}.`);
}

if (process.argv[2] === "publish") await publish();
else if (process.argv[2] === undefined) await draft();
else fail(`unknown command ${process.argv[2]}. Run it with no argument to draft, or with publish.`);
