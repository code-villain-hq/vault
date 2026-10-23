import Foundation
import ApplicationServices

/// System-level commands invoked from the launcher.
/// Empty trash and dark mode use AppleScript — macOS shows a one-time
/// Automation permission prompt (Finder / System Events) on first use.
enum SystemActions {
    static func sleep() {
        run("/usr/bin/pmset", ["sleepnow"])
    }

    enum LockError: LocalizedError {
        case permission, unavailable
        var errorDescription: String? {
            switch self {
            case .permission: "Couldn't lock the screen. Allow Caster in System Settings → Privacy & Security → Accessibility, then try again."
            case .unavailable: "Couldn't lock the screen. Try again."
            }
        }
    }

    /// Send macOS's Lock Screen shortcut, independent of sleep/password delay.
    /// Injection is separated so tests never post keys to the user's session.
    static func lockScreen(
        isTrusted: () -> Bool = { AXIsProcessTrustedWithOptions([kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary) },
        post: (CGEvent) -> Void = { $0.post(tap: .cghidEventTap) }
    ) throws {
        guard isTrusted() else { throw LockError.permission }
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 12, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: 12, keyDown: false)
        else { throw LockError.unavailable }
        for event in [down, up] {
            event.flags = [.maskControl, .maskCommand]
            post(event)
        }
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
