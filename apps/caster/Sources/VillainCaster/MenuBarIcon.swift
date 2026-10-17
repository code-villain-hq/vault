import AppKit

/// Status item glyph: Caster's mark without its tile. The C from
/// brand/mark/caster.svg with the strike behind it, cut where the letter
/// crosses. In colour, not a template, because the strike is poison: night
/// `fg` and bright poison on a dark menu bar, day `fg` and deep poison on a
/// light one (brand/mark/README.md, "Menu bar glyph"). Drawn in code so it
/// stays crisp and works without a bundle (`make run`).
enum MenuBarIcon {
    /// The mark's C, in its 200-unit tile coordinates (y down). `make
    /// design-check` fails when it drifts from brand/mark/caster.svg.
    private static let letter = "M93.83 73.28Q93.42 78.08 92.95 83.05Q92.48 88.01 92.27 93.49Q92.07 98.96 92.07 105.56Q92.07 118.07 94.03 124.89Q96.00 131.72 99.70 134.40Q103.40 137.09 108.63 137.09L130.23 137.06L130.23 149.90Q127.80 150.50 123.52 151.22Q119.25 151.94 114.70 152.46Q110.15 152.99 106.73 153.35Q103.32 153.71 102.53 153.71Q91.50 153.71 83.71 149.09Q75.93 144.47 71.83 134.70Q67.73 124.94 67.73 109.49Q67.73 100.70 69.01 93.41Q70.28 86.12 72.23 81.14Q74.48 78.38 78.68 75.01Q82.88 71.63 87.78 68.51ZM99.78 137.39L99.78 68.87Q99.78 67.58 100.63 64.94Q101.48 62.30 102.63 59.09L107.82 59.33L107.82 128.84Q107.82 129.74 106.77 132.06Q105.72 134.39 104.07 137.39ZM130.17 91.73L118.83 91.73L116.67 74.15Q115.23 73.58 112.95 73.01Q110.67 72.44 108.55 72.02Q106.43 71.60 105.27 71.60Q103.88 71.60 103.06 71.62Q102.23 71.63 101.48 71.78L102.63 59.09Q103.25 58.79 103.88 58.71Q104.52 58.64 105.12 58.64Q108.30 58.64 113.11 59.16Q117.93 59.69 123.06 60.35Q128.18 61.01 131.97 61.58Z"

    // On the tile: the bar's top at 108, 15 tall; the cut 7 either side.
    private static let barTop: CGFloat = 108
    private static let barHeight: CGFloat = 15
    private static let cut: CGFloat = 7

    private static let size: CGFloat = 18
    /// The C fills 16 of the 18 points.
    private static let letterHeight: CGFloat = 16

    static func image(for appearance: NSAppearance) -> NSImage {
        let night = Nightshade.isNight(appearance)
        let ink = NSColor(hex: night ? ColorTokens.night.fg : ColorTokens.day.fg)
        let strike = NSColor(hex: night ? ColorTokens.Palette.poison : ColorTokens.Palette.poisonDeep)

        let image = NSImage(size: NSSize(width: size, height: size), flipped: true) { _ in
            let path = svgPath(letter)
            let bounds = path.bounds
            let scale = letterHeight / bounds.height
            // Applied last to first: centre the C on the origin, scale it,
            // centre it in the icon.
            var transform = AffineTransform(translationByX: size / 2, byY: size / 2)
            transform.scale(scale)
            transform.translate(x: -bounds.midX, y: -bounds.midY)
            path.transform(using: transform)

            let context = NSGraphicsContext.current
            let bar = NSRect(x: 0, y: size / 2 + (barTop - bounds.midY) * scale,
                             width: size, height: barHeight * scale)
            strike.setFill()
            bar.fill()
            // Cut the bar around the letter's strokes, then set the letter.
            context?.compositingOperation = .clear
            path.lineWidth = cut * 2 * scale
            path.lineJoinStyle = .round
            path.stroke()
            path.fill()
            context?.compositingOperation = .sourceOver
            ink.setFill()
            path.fill()
            return true
        }
        image.isTemplate = false
        image.accessibilityDescription = "Caster"
        return image
    }

    /// Absolute M, L, Q and Z: all the outliner writes.
    private static func svgPath(_ d: String) -> NSBezierPath {
        let path = NSBezierPath()
        let scanner = Scanner(string: d)
        scanner.charactersToBeSkipped = CharacterSet(charactersIn: " ,")
        var command: Character = "M"
        func point() -> CGPoint? {
            guard let x = scanner.scanDouble(), let y = scanner.scanDouble() else { return nil }
            return CGPoint(x: x, y: y)
        }
        while !scanner.isAtEnd {
            if let letter = scanner.scanCharacter(), letter.isLetter {
                command = letter
            } else {
                scanner.currentIndex = d.index(before: scanner.currentIndex)
            }
            switch command {
            case "M": if let p = point() { path.move(to: p) }
            case "L": if let p = point() { path.line(to: p) }
            case "Q":
                if let c = point(), let p = point() {
                    // Quadratic as cubic: control points two-thirds of the way to c.
                    let start = path.currentPoint
                    path.curve(to: p,
                               controlPoint1: CGPoint(x: start.x + 2 / 3 * (c.x - start.x), y: start.y + 2 / 3 * (c.y - start.y)),
                               controlPoint2: CGPoint(x: p.x + 2 / 3 * (c.x - p.x), y: p.y + 2 / 3 * (c.y - p.y)))
                }
            case "Z", "z": path.close()
            default: return path
            }
        }
        return path
    }
}
