# Releasing

A release is built on your Mac and published on the vault's Forgejo, at
<https://git.codevillain.eu/code-villain/vault/releases>.
[`scripts/release.ts`](../scripts/release.ts) does it in two steps:

- `bun run release` runs `bun run check`, builds the app for Apple silicon,
  and puts `Layer-<version>-macos-arm64.dmg` with its SHA-256 on a **draft**
  release for the commit. Nothing is public yet, not even the tag.
- `bun run release:publish` publishes the draft, which makes the tag
  `layer-v<version>` on Forgejo, puts the same .dmg on that tag's release
  on the GitHub mirror, and puts it in the Homebrew tap
  ([below](#homebrew)). Run it where the release was built: the .dmg it
  mirrors is the one in `target/`, checked against the release's SHA-256.
  Forgejo publishes a release at once when its tag already exists, so the
  script never pushes the tag itself.

Tags carry the `layer-` prefix because the vault holds more than one app.
Releases up to 0.8.0 were universal (Intel too) and are on the old GitHub
repository; from 0.9.0 they are Apple silicon only.

## Cutting one

1. **Bump the version** in `src-tauri/Cargo.toml`, and only there. The app's
   About line, the bundle and the MCP server all read it from Cargo, and the
   tag is made from it.
2. Run `cargo check -p villain-layer`, so `Cargo.lock` follows.
3. Commit (*Release 0.9.0.*) and push `main`. The script refuses a tree with
   uncommitted changes, a `main` that differs from `origin/main`, and a tag
   that already exists.
4. `bun run release`. It takes as long as `check` plus a release build.
5. Open the draft under Releases on Forgejo, read the notes (the commits to
   `code/apps/layer` since the last tag), and edit them there if needed.
6. `bun run release:publish`.

A draft that is wrong can be deleted on Forgejo and cut again once the fix
is on `main`: there is no tag until it is published.

## Once per Mac

**A Forgejo token.** On git.codevillain.eu: Settings → Applications →
Generate new token, with *repository: read and write*. Store it in the
keychain, where the script reads it; `-w` last makes `security` ask for it
without echoing it or leaving it in your shell history:

```bash
security add-generic-password -s git.codevillain.eu -a release -w
```

**Git over SSH** to git.codevillain.eu, as for pushing the vault: the script
pushes the tap with your own `git`. It never pushes the tag (above).

**`gh`, signed in** with access to code-villain-hq/vault, for the mirror's
release.

## Homebrew

The cask downloads the .dmg from the release of the same tag on the
vault's GitHub mirror, [code-villain-hq/vault](https://github.com/code-villain-hq/vault/releases),
not from Forgejo. Uberspace's proxy cuts any transfer at about 48 seconds,
and Layer's .dmg is near 100 MB since it bundles Postgres: at 1.4 MB/s,
brew's download stopped at 70 MB every time. A browser download that is cut
can be resumed. `release:publish` makes the mirror's release with `gh`,
once the mirror has the tag; if it has not synced yet, it says so, and
running it again carries on.

The cask is written here, in
[`packaging/homebrew/villain-layer.rb`](../packaging/homebrew/villain-layer.rb),
and published to
[code-villain/homebrew-tap](https://git.codevillain.eu/code-villain/homebrew-tap)
by `release:publish`. It fills in the version and the .dmg's SHA-256, has
`brew style` and `brew audit --online` check the result in a throwaway tap,
and pushes it. Change the cask here: the tap's copy is overwritten. A
prerelease is never put in the tap.

The cask refuses to upgrade or uninstall while Layer is running, for the
same reason `release:mac` does. It requires macOS 26, the oldest the tests
have run on, and Apple silicon.

Installing it the way everyone else will:

```bash
brew tap code-villain/tap https://git.codevillain.eu/code-villain/homebrew-tap
brew trust --cask code-villain/tap/villain-layer
brew install --cask villain-layer
```

The tap is not on GitHub, so `brew tap` needs its address. Homebrew runs
nothing from a tap it has not been told to trust, and installing by the full
name does not keep it trusted. Without `brew trust`, `brew upgrade` does not
load the cask, and `brew uninstall` skips the check that the app is not
running.

Once releases are notarised, delete the first paragraph of the cask's
`caveats`.

## Signing and notarisation

Without them, macOS stops a downloaded app with "Apple could not verify
…", and the only way past is System Settings → Privacy & Security → Open
Anyway. Right-click → Open no longer works for that. An unsigned app also
asks for keychain access again after every update, because *Always Allow*
is tied to the exact binary. The script signs ad hoc until the variables
below are set, and the release notes then tell people how to open it.

What it takes:

1. **An Apple Developer Program membership** (99 USD a year). An
   individual membership signs as your legal name: the certificate reads
   *Developer ID Application: <your name> (TEAMID)*, and macOS shows that
   name. Signing as *Code Villain* needs an organisation membership, which
   needs a legal entity with a D-U-N-S number.
2. **A Developer ID Application certificate** in your login keychain. Create
   it in Xcode (Settings → Accounts → Manage Certificates → + → Developer ID
   Application). On this Mac it stays in the keychain: no `.p12` to export.
3. **An app-specific password** for notarisation, from account.apple.com →
   Sign-In and Security → App-Specific Passwords. Keep it in the keychain:
   `security add-generic-password -s notarisation -a <apple id> -w`.
4. **Four variables** for the run, all of them or none. With some set and
   not others the script stops, rather than ship an app that is signed but
   not notarised, which macOS stops just the same. `security find-identity
   -v -p codesigning` prints the identity as
   `Developer ID Application: <name> (TEAMID)`.

   ```bash
   APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (TEAMID)" \
   APPLE_ID=<apple id> \
   APPLE_PASSWORD="$(security find-generic-password -s notarisation -w)" \
   APPLE_TEAM_ID=TEAMID \
   bun run release
   ```

With all four, the build is signed and notarised, and the script checks the
result the way a Mac that downloads it will (`codesign`, `stapler
validate`, `spctl`) before it drafts anything.

Check the first one on a Mac that has never run the app: download it,
open it, and it should start without a warning. Then check the parts that
the hardened runtime, which notarisation requires, could stop: a terminal
pane starts an agent, a token is saved and read back, a notification
arrives, and an agent reaches the app's MCP tools.

## Moving the identifier

The identifier, `eu.codevillain.villain-layer`, names the config folder
and the keychain item. Changing it again means another one-time copy, as
`previous.rs` does for the last change (DISK-4). Without that, every user
opens an empty app.
