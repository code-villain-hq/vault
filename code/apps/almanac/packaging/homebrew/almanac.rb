# The cask in code-villain/homebrew-tap on git.codevillain.eu, written from
# this file by `bun run release:publish` (scripts/release.ts): it fills in the
# version and the .dmg's SHA-256. Change the cask here, not in the tap, which
# is overwritten.
cask "almanac" do
  version "@VERSION@"
  sha256 "@SHA256@"

  # From GitHub's mirror of the vault, not Forgejo, whose proxy cuts a
  # download at about 48 s (code/apps/layer/docs/releasing.md).
  url "https://github.com/code-villain-hq/vault/releases/download/almanac-v#{version}/Almanac-#{version}-macos-arm64.dmg"
  name "Almanac"
  desc "Keeps what you learn, by topic, with Claude"
  homepage "https://www.codevillain.eu/almanac/"

  livecheck do
    url "https://git.codevillain.eu/code-villain/vault/releases"
    regex(/almanac-v?(\d+(?:\.\d+)+)/i)
  end

  # macOS 26 is the oldest Almanac has been tried on. Lower this once an
  # older one has been.
  depends_on arch: :arm64
  depends_on macos: :tahoe

  app "Almanac.app"

  # Quitting stops its own Postgres cleanly, before the bundle it runs from
  # is replaced.
  uninstall quit: "eu.codevillain.almanac"

  # Application Support holds the topics on this Mac, in its own Postgres.
  # Not the keychain items, which brew cannot remove.
  zap trash: [
    "~/Library/Application Support/eu.codevillain.almanac",
    "~/Library/Caches/eu.codevillain.almanac",
    "~/Library/Preferences/eu.codevillain.almanac.plist",
    "~/Library/WebKit/eu.codevillain.almanac",
  ]

  caveats <<~EOS
    Almanac is not notarised by Apple yet, so macOS stops it the first
    time with "Apple could not verify…". Choose Done, then System Settings →
    Privacy & Security → Open Anyway.

    It answers with Claude Code, as you're signed in to it, through its
    adapter. Install that once:
      npm install -g @agentclientprotocol/claude-agent-acp

    `brew uninstall --zap` deletes the topics kept on this Mac. Sign-ins to
    servers stay in the keychain (items named Almanac); delete them in
    Keychain Access.
  EOS
end
