import AppKit

/// One calculated answer under the query, as on the homepage: a poison tile
/// with the kind's symbol, the value large, its detail beneath, and the ⏎
/// key that copies it. The card is the screen's one action, so the tile is
/// the screen's one poison fill (brand/color.md, "Where poison goes"); the
/// card itself wears the `selected` tint.
final class AnswerCard: NSView {
    static let height: CGFloat = 76
    var onActivate: (() -> Void)?
    private var answer: Answer?

    private static let inset: CGFloat = 16
    private static let tile: CGFloat = 44
    private static let key: CGFloat = 30
    private static let valueFont = Nightshade.font(TypeTokens.title)
    private static let detailFont = Nightshade.font(TypeTokens.ui, size: 15)

    func configure(with answer: Answer) {
        self.answer = answer
        setAccessibilityLabel("\(answer.value), \(answer.detail)")
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        onActivate?()
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let answer else { return }
        Nightshade.selected.setFill()
        NSBezierPath(roundedRect: bounds, xRadius: Nightshade.radiusMd, yRadius: Nightshade.radiusMd).fill()

        // The tile: poison with dark ink.
        let tile = NSRect(x: Self.inset, y: (bounds.height - Self.tile) / 2, width: Self.tile, height: Self.tile)
        Nightshade.primary.setFill()
        NSBezierPath(roundedRect: tile, xRadius: Nightshade.radiusMd, yRadius: Nightshade.radiusMd).fill()
        let symbol = NSImage(systemSymbolName: answer.kind.symbol, accessibilityDescription: answer.kind.label)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: CGFloat(TypeTokens.body.size), weight: .semibold)
                .applying(NSImage.SymbolConfiguration(paletteColors: [Nightshade.onPrimary])))
        if let symbol {
            symbol.draw(in: NSRect(x: tile.midX - symbol.size.width / 2, y: tile.midY - symbol.size.height / 2,
                                   width: symbol.size.width, height: symbol.size.height))
        }

        // The ⏎ key: a key hint, `radius-sm` on the chip tint.
        let keyRect = NSRect(x: bounds.width - Self.inset - Self.key, y: (bounds.height - Self.key) / 2,
                             width: Self.key, height: Self.key)
        Nightshade.chip.setFill()
        NSBezierPath(roundedRect: keyRect, xRadius: Nightshade.radiusSm, yRadius: Nightshade.radiusSm).fill()
        let keyGlyph = NSAttributedString(string: "↵", attributes: [.font: Self.detailFont, .foregroundColor: Nightshade.muted])
        let keySize = keyGlyph.size()
        keyGlyph.draw(at: NSPoint(x: keyRect.midX - keySize.width / 2, y: keyRect.midY - keySize.height / 2))

        // The value over its detail, centred as a pair beside the tile.
        let textX = tile.maxX + 14
        let textWidth = keyRect.minX - 14 - textX
        let value = NSAttributedString(string: answer.value, attributes: [
            .font: Self.valueFont, .foregroundColor: Nightshade.fg, .paragraphStyle: Self.truncating,
        ])
        let detail = NSAttributedString(string: answer.detail, attributes: [
            .font: Self.detailFont, .foregroundColor: Nightshade.muted, .paragraphStyle: Self.truncating,
        ])
        let valueHeight = ceil(value.size().height), detailHeight = ceil(detail.size().height)
        let top = (bounds.height + valueHeight + detailHeight + 2) / 2
        value.draw(with: NSRect(x: textX, y: top - valueHeight, width: textWidth, height: valueHeight),
                   options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        detail.draw(with: NSRect(x: textX, y: top - valueHeight - 2 - detailHeight, width: textWidth, height: detailHeight),
                    options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    private static let truncating: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        return style
    }()
}
