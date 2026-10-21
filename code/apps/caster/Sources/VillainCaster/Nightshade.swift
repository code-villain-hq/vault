import AppKit
import CoreText

/// Nightshade in AppKit: the generated tokens in Design/ as colours and
/// fonts. Night follows the system's dark appearance, day its light one.
enum Nightshade {
    static let bg = color(\.bg)
    static let panel = color(\.panel)
    static let dialog = color(\.dialog)
    static let field = color(\.field)
    static let fieldHover = color(\.fieldHover)
    static let fg = color(\.fg)
    static let muted = color(\.muted)
    static let faint = color(\.faint)
    static let primary = color(\.primary)
    static let primaryHover = color(\.primaryHover)
    static let onPrimary = color(\.onPrimary)
    static let accent = color(\.accent)
    /// The selected row: a faint poison tint.
    static let selected = color(\.selected)
    /// A key hint's fill: `fg` at `chip-tint`, like a chip.
    static let chip = color(\.fg, alpha: ColorTokens.Opacity.chipTint)
    /// The active option in a command list: `fg` at `highlight-tint`.
    static let highlight = color(\.fg, alpha: ColorTokens.Opacity.highlightTint)
    /// The field's focus glow: `accent` at 22%.
    static let focusGlow = color(\.accent, alpha: 0.22)
    /// A problem's text: `status-danger-ink`.
    static let dangerInk = color(\.statusDangerInk)

    static func isNight(_ appearance: NSAppearance) -> Bool {
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
    }

    static func color(_ role: KeyPath<ColorTokens.Theme, String>, alpha: Double = 1) -> NSColor {
        NSColor(name: nil) { appearance in
            let theme = isNight(appearance) ? ColorTokens.night : ColorTokens.day
            return NSColor(hex: theme[keyPath: role]).withAlphaComponent(alpha)
        }
    }

    // MARK: - Type

    /// Registers the bundled faces for this process. `make app` puts them in
    /// Contents/Resources/Fonts; `make run` next to the binary.
    static func registerFonts() {
        guard let folder = Bundle.main.resourceURL?.appendingPathComponent("Fonts"),
              let files = try? FileManager.default.contentsOfDirectory(
                  at: folder, includingPropertiesForKeys: nil)
        else { return }
        for file in files where file.pathExtension == "ttf" {
            CTFontManagerRegisterFontsForURL(file as CFURL, .process, nil)
        }
    }

    /// A type role at its own weight, or at `weight` when a control needs
    /// the role's size at another of the face's weights, or at `size` where a
    /// shared measure has no role (the command input's 15pt).
    static func font(_ role: TypeTokens.Role, weight: Int? = nil, size: Double? = nil) -> NSFont {
        let weight = weight ?? role.weight
        let size = size ?? role.size
        let descriptor = NSFontDescriptor(fontAttributes: [
            .family: role.face,
            .variation: [wghtAxis: weight],
        ])
        if let font = NSFont(descriptor: descriptor, size: size), font.familyName == role.face {
            return font
        }
        // Unbundled (a bare `swift build`): the system face at the same size.
        return .systemFont(ofSize: size, weight: weight >= 700 ? .bold : weight >= 600 ? .semibold : .regular)
    }

    /// The OpenType `wght` axis tag.
    private static let wghtAxis = NSNumber(value: 0x7767_6874)

    // MARK: - Shape

    static let radiusSm = CGFloat(ShapeTokens.radiusSm)
    static let radiusMd = CGFloat(ShapeTokens.radiusMd)
    static let radiusLg = CGFloat(ShapeTokens.radiusLg)
}

extension NSColor {
    /// `#RRGGBB` in sRGB, as the tokens write it.
    convenience init(hex: String) {
        let value = UInt32(hex.dropFirst(), radix: 16) ?? 0
        self.init(srgbRed: CGFloat((value >> 16) & 0xFF) / 255,
                  green: CGFloat((value >> 8) & 0xFF) / 255,
                  blue: CGFloat(value & 0xFF) / 255,
                  alpha: 1)
    }
}
