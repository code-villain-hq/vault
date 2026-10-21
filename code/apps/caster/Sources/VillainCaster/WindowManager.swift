import AppKit
import ApplicationServices

/// Moves/resizes the focused window of the frontmost app via the
/// Accessibility API. Works because the launcher panel is non-activating:
/// the user's app stays frontmost while our panel is open.
///
/// Requires Accessibility permission (System Settings → Privacy & Security
/// → Accessibility). First use triggers the system prompt.
enum WindowManager {

    /// PID of the app that was frontmost when the launcher executed a
    /// command. Set by PanelController so we don't lose the target after hide.
    static var targetPID: pid_t?

    /// Fills the display under the menu bar and over the Dock (monitor
    /// minus the top bar only — not macOS Full Screen / a new Space).
    @discardableResult
    static func maximizeFocusedWindow() -> Bool {
        guard ensureAccessibility() else { return false }
        guard let window = focusedWindow(), let axFrame = frame(of: window) else { return false }
        guard let screen = screenContaining(axFrame: axFrame) else { return false }
        setFrame(maximizeRect(for: screen), on: window)
        return true
    }

    /// Moves the window to the next display, keeping its size (clamped) and
    /// relative position within the screen.
    @discardableResult
    static func moveFocusedWindowToNextDisplay() -> Bool {
        let screens = NSScreen.screens
        guard screens.count > 1 else { return false }
        guard ensureAccessibility() else { return false }
        guard let window = focusedWindow(), let axFrame = frame(of: window) else { return false }
        guard let currentScreen = screenContaining(axFrame: axFrame),
              let currentIndex = screens.firstIndex(of: currentScreen) else { return false }

        // Usable area below the menu bar, so a maximized window stays fully
        // on-screen when the target display is shorter.
        let sourceAX = maximizeRect(for: currentScreen)
        let targetAX = maximizeRect(for: screens[(currentIndex + 1) % screens.count])

        let size = CGSize(
            width: min(axFrame.width, targetAX.width),
            height: min(axFrame.height, targetAX.height)
        )
        let relX = sourceAX.width > axFrame.width
            ? (axFrame.minX - sourceAX.minX) / (sourceAX.width - axFrame.width) : 0
        let relY = sourceAX.height > axFrame.height
            ? (axFrame.minY - sourceAX.minY) / (sourceAX.height - axFrame.height) : 0
        let origin = CGPoint(
            x: targetAX.minX + relX.clamped01 * (targetAX.width - size.width),
            y: targetAX.minY + relY.clamped01 * (targetAX.height - size.height)
        )
        setFrame(CGRect(origin: origin, size: size), on: window)
        return true
    }

    static var hasSecondDisplay: Bool {
        NSScreen.screens.count > 1
    }

    // MARK: - Geometry (AX / Quartz space)

    /// Monitor minus menu bar, in AX coordinates (CGDisplayBounds-based).
    private static func maximizeRect(for screen: NSScreen) -> CGRect {
        let bounds = cgBounds(for: screen) ?? cocoaToAX(screen.frame)
        let menuGap = max(0, screen.frame.maxY - screen.visibleFrame.maxY)
        return CGRect(
            x: bounds.minX,
            y: bounds.minY + menuGap,
            width: bounds.width,
            height: bounds.height - menuGap
        )
    }

    private static func cgBounds(for screen: NSScreen) -> CGRect? {
        guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber
        else { return nil }
        return CGDisplayBounds(CGDirectDisplayID(number.uint32Value))
    }

    private static func screenContaining(axFrame: CGRect) -> NSScreen? {
        let center = CGPoint(x: axFrame.midX, y: axFrame.midY)
        if let exact = NSScreen.screens.first(where: { cgBounds(for: $0)?.contains(center) == true }) {
            return exact
        }
        return NSScreen.screens.max { a, b in
            let aArea = cgBounds(for: a)?.intersection(axFrame).area ?? 0
            let bArea = cgBounds(for: b)?.intersection(axFrame).area ?? 0
            return aArea < bArea
        } ?? NSScreen.main
    }

    // MARK: - Accessibility plumbing

    /// True when trusted; otherwise shows the system permission prompt once.
    private static func ensureAccessibility() -> Bool {
        if AXIsProcessTrusted() { return true }
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
        return false
    }

    private static func focusedWindow() -> AXUIElement? {
        let pid = targetPID ?? NSWorkspace.shared.frontmostApplication?.processIdentifier
        guard let pid else { return nil }
        let axApp = AXUIElementCreateApplication(pid)
        var window: CFTypeRef?
        if AXUIElementCopyAttributeValue(axApp, kAXFocusedWindowAttribute as CFString, &window) == .success,
           window != nil {
            return (window as! AXUIElement)
        }
        // Some apps (terminals) report no focused window — use the front window.
        var windows: CFTypeRef?
        guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windows) == .success,
              let list = windows as? [AXUIElement],
              let first = list.first
        else { return nil }
        return first
    }

    private static func frame(of window: AXUIElement) -> CGRect? {
        var positionRef: CFTypeRef?
        var sizeRef: CFTypeRef?
        guard AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &positionRef) == .success,
              AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &sizeRef) == .success
        else { return nil }
        var position = CGPoint.zero
        var size = CGSize.zero
        AXValueGetValue(positionRef as! AXValue, .cgPoint, &position)
        AXValueGetValue(sizeRef as! AXValue, .cgSize, &size)
        return CGRect(origin: position, size: size)
    }

    /// Move onto the destination display first, then size — size-first gets
    /// clamped when the window still sits on another screen (multi-monitor).
    private static func setFrame(_ rect: CGRect, on window: AXUIElement) {
        var position = rect.origin
        var size = rect.size
        if let value = AXValueCreate(.cgPoint, &position) {
            AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, value)
        }
        if let value = AXValueCreate(.cgSize, &size) {
            AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value)
        }
        if let value = AXValueCreate(.cgPoint, &position) {
            AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, value)
        }
        if let value = AXValueCreate(.cgSize, &size) {
            AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value)
        }
    }

    // MARK: - Cocoa ↔ AX (fallback when CGDisplayBounds is unavailable)

    private static var primaryHeight: CGFloat {
        NSScreen.screens.first?.frame.maxY ?? 0
    }

    private static func cocoaToAX(_ rect: CGRect) -> CGRect {
        CGRect(x: rect.minX, y: primaryHeight - rect.minY - rect.height,
               width: rect.width, height: rect.height)
    }
}

private extension CGRect {
    var area: CGFloat { isNull ? 0 : width * height }
}

private extension CGFloat {
    var clamped01: CGFloat { Swift.min(Swift.max(self, 0), 1) }
}
