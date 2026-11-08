# The cask in code-villain/homebrew-tap on git.codevillain.eu, written from
# this file by `bun run release:publish` (scripts/release.ts): it fills in the
# version and the .dmg's SHA-256 (docs/releasing.md). Change the cask here,
# not in the tap, which is overwritten.
cask "villain-layer" do
  version "@VERSION@"
  sha256 "@SHA256@"

  # From GitHub's mirror of the vault, not Forgejo, whose proxy cuts a
  # download at about 48 s: brew's failed every time (docs/releasing.md).
  url "https://github.com/code-villain-hq/vault/releases/download/layer-v#{version}/Layer-#{version}-macos-arm64.dmg"
  name "Layer"
  desc "Agent development environment"
  homepage "https://git.codevillain.eu/code-villain/vault/src/branch/main/code/apps/layer"

  livecheck do
    url "https://git.codevillain.eu/code-villain/vault/releases"
    regex(/layer-v?(\d+(?:\.\d+)+)/i)
  end

  # macOS 26 is the oldest Layer has been tested on. Lower this
  # once an older one has been tried.
  depends_on arch: :arm64
  depends_on macos: :tahoe

  app "Layer.app"

  # Layer is signed ad hoc, not notarised. On Nov 8 2026 macOS 27.2 called
  # brew's quarantined 0.13.0 "damaged" and offered no Open Anyway, while
  # Almanac, signed the same way, had been offered it. Clearing brew's
  # quarantine flag on the installed app lets it open. Remove this once
  # releases are notarised (docs/releasing.md).
  postflight_steps do
    run "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "{{appdir}}/Layer.app"]
  end

  # Replacing the bundle under a running app kills it, and every agent
  # running in it. `uninstall quit:` would not help: quitting ends them too.
  # This runs before an upgrade or uninstall touches anything, so refusing
  # here leaves the installed app as it was.
  uninstall_preflight_steps do
    run "/bin/sh",
        args: [
          "-c",
          "if /usr/bin/pgrep -f 'Layer[.]app/Contents/MacOS/villain-layer' >/dev/null; then " \
          "echo 'Layer is running. Quit it first, from a terminal outside it: replacing " \
          "the app while it runs kills it, and every agent running in it.' >&2; exit 1; fi",
        ]
  end

  # Not ~/.villain-worktrees: the task folders hold work that may not be
  # pushed. Not the keychain item, which brew cannot remove.
  zap trash: [
    "~/Library/Application Support/eu.codevillain.villain-layer",
    "~/Library/Caches/eu.codevillain.villain-layer",
    "~/Library/Logs/villain-layer",
    "~/Library/Preferences/eu.codevillain.villain-layer.plist",
    "~/Library/WebKit/eu.codevillain.villain-layer",
  ]

  caveats <<~EOS
    Layer is not notarised by Apple yet. brew clears macOS's download
    flag on it after each install, so macOS doesn't stop it as "damaged".
    macOS still asks for keychain access again after each upgrade.

    Its tokens stay in the keychain (item eu.codevillain.villain-layer)
    after `brew uninstall --zap`. Delete them in Keychain Access.
  EOS
end
