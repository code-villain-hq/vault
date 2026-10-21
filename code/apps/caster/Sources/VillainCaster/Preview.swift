import AppKit

/// Renders Caster's surfaces to PNGs and quits, for checking the look
/// without ⌘Space or a screen recording grant:
///
///     .build/release/VillainCaster -preview "33 / 3" -previewTheme day -previewOut panel.png
///
/// `-preview` takes a query, `settings` for the settings window, or
/// `menubar` for the status icon on a dark and a light bar. No hot key or
/// status item is registered, so it runs beside an installed Caster.
enum Preview {
    static func run(panel: PanelController) -> Bool {
        let defaults = UserDefaults.standard
        guard let subject = defaults.string(forKey: "preview") else { return false }
        let out = URL(fileURLWithPath: defaults.string(forKey: "previewOut") ?? "caster-preview.png")
        let night = defaults.string(forKey: "previewTheme") != "day"
        let appearance = NSAppearance(named: night ? .darkAqua : .aqua)!
        NSApp.appearance = appearance

        // Network lookups (currency, weather) get a moment to arrive.
        let delay: Double = subject == "menubar" || subject == "settings" ? 0.3 : 2.5
        switch subject {
        // Laid out but never ordered in: the preview must not take the
        // keyboard from whatever app is in front.
        case "settings": SettingsWindowController.shared.fill()
        case "menubar": break
        default:
            panel.show(query: subject, present: false)
            // The app index fills in the background; ask again once it has.
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                panel.show(query: subject, present: false)
            }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            do {
                switch subject {
                case "menubar": try menuBar(to: out)
                case "settings": try SettingsWindowController.shared.renderPNG(to: out)
                default: try panel.renderPNG(to: out)
                }
                print("Wrote \(out.path) in \(Nightshade.font(TypeTokens.ui).familyName ?? "?")")
            } catch {
                print("Preview failed: \(error)")
            }
            NSApp.terminate(nil)
        }
        return true
    }

    /// The icon at 8x on a dark bar, then a light one.
    private static func menuBar(to url: URL) throws {
        let scale: CGFloat = 8, side: CGFloat = 22 * scale
        let bars: [(NSAppearance.Name, String)] = [(.darkAqua, "#1E1E1E"), (.aqua, "#ECECEC")]
        let image = NSImage(size: NSSize(width: side * 2, height: side), flipped: false) { _ in
            for (index, bar) in bars.enumerated() {
                let tile = NSRect(x: side * CGFloat(index), y: 0, width: side, height: side)
                NSColor(hex: bar.1).setFill()
                tile.fill()
                let icon = MenuBarIcon.image(for: NSAppearance(named: bar.0)!)
                icon.draw(in: tile.insetBy(dx: 2 * scale, dy: 2 * scale))
            }
            return true
        }
        guard let tiff = image.tiffRepresentation,
              let data = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:])
        else { throw CocoaError(.fileWriteUnknown) }
        try data.write(to: url)
    }
}
