import Foundation

/// System-level commands invoked from the launcher.
/// Empty trash and dark mode use AppleScript — macOS shows a one-time
/// Automation permission prompt (Finder / System Events) on first use.
enum SystemActions {
    static func sleep() {
        run("/usr/bin/pmset", ["sleepnow"])
    }

    /// Sleeps the displays; the session locks if "require password after
    /// sleep" is enabled (the default).
    static func lockScreen() {
        run("/usr/bin/pmset", ["displaysleepnow"])
    }

    static func emptyTrash() {
        runAppleScriptInBackground("tell application \"Finder\" to empty trash")
    }

    /// Dock-style reopen: opens a window when Finder has none, otherwise
    /// brings an existing window forward. Plain activate() is a no-op when
    /// Finder is running with zero windows. Returns false if Automation is
    /// denied or osascript fails — callers should fall back to activate().
    @discardableResult
    static func openFinder() -> Bool {
        runAppleScript("""
        tell application "Finder"
          reopen
          activate
        end tell
        """)
    }

    static func toggleDarkMode() {
        runAppleScriptInBackground("""
        tell application "System Events" to tell appearance preferences \
        to set dark mode to not dark mode
        """)
    }

    private static func run(_ path: String, _ arguments: [String]) {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: path)
        task.arguments = arguments
        try? task.run()
    }

    /// Fire-and-forget: osascript takes tens of ms, and the first run blocks
    /// on the Automation prompt — keep that off the main thread.
    private static func runAppleScriptInBackground(_ script: String) {
        DispatchQueue.global(qos: .userInitiated).async { runAppleScript(script) }
    }

    @discardableResult
    private static func runAppleScript(_ script: String) -> Bool {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        task.arguments = ["-e", script]
        do {
            try task.run()
            task.waitUntilExit()
            return task.terminationStatus == 0
        } catch {
            return false
        }
    }
}
