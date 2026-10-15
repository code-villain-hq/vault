import AppKit

/// Status item glyph: a Doom-style forged faceplate as a template image,
/// with the two eye slits (lower at the inner end, like the app icon's
/// mask) cut out. Drawn in code so it stays crisp and works without a
/// bundle (`make run`).
enum MenuBarIcon {
    static func image() -> NSImage {
        let image = NSImage(size: NSSize(width: 18, height: 18), flipped: true) { _ in
            NSColor.black.set()

            // Flat brow, straight cheeks, jaw tapering to a squared chin.
            let mask = polygon([
                CGPoint(x: 4.4, y: 2), CGPoint(x: 13.6, y: 2), CGPoint(x: 15, y: 3.4),
                CGPoint(x: 14.7, y: 9.6), CGPoint(x: 12.6, y: 14.2), CGPoint(x: 10.4, y: 16.4),
                CGPoint(x: 7.6, y: 16.4), CGPoint(x: 5.4, y: 14.2), CGPoint(x: 3.3, y: 9.6),
                CGPoint(x: 3, y: 3.4),
            ])
            mask.windingRule = .evenOdd
            let leftEye = [
                CGPoint(x: 4.4, y: 6.6), CGPoint(x: 8.3, y: 7.9),
                CGPoint(x: 8.2, y: 9.7), CGPoint(x: 4.8, y: 8.8),
            ]
            mask.append(polygon(leftEye))
            mask.append(polygon(leftEye.map { CGPoint(x: 18 - $0.x, y: $0.y) }))
            mask.fill()
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Villain Caster"
        return image
    }

    private static func polygon(_ points: [CGPoint]) -> NSBezierPath {
        let path = NSBezierPath()
        path.move(to: points[0])
        for point in points.dropFirst() { path.line(to: point) }
        path.close()
        return path
    }
}
