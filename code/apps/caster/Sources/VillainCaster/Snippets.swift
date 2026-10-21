import AppKit

/// Personal text snippets ("work email" → ⏎ copies the address). Values are
/// edited in the settings window and stored in UserDefaults; only filled-in
/// fields appear in search.
enum Snippets {
    struct Field {
        let defaultsKey: String
        let title: String
        let keywords: String
    }

    static let fields: [Field] = [
        Field(defaultsKey: "snippet.workEmail", title: "Work email", keywords: "mail job company"),
        Field(defaultsKey: "snippet.privateEmail", title: "Private email", keywords: "mail personal home"),
        Field(defaultsKey: "snippet.phone", title: "Phone", keywords: "number mobile telephone cell"),
        Field(defaultsKey: "snippet.address", title: "Address", keywords: "street home location"),
    ]

    static func value(for key: String) -> String? {
        guard let value = UserDefaults.standard.string(forKey: key)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !value.isEmpty else { return nil }
        return value
    }

    static func set(_ value: String, for key: String) {
        UserDefaults.standard.set(value.trimmingCharacters(in: .whitespacesAndNewlines), forKey: key)
    }
}

/// One field per snippet on the `panel` tone, opened from the status item's
/// right-click menu. A field saves when you leave it or press Enter
/// (studio/design/behaviour.md), and the window ends with Caster's version.
final class SettingsWindowController: NSWindowController, NSTextFieldDelegate, NSWindowDelegate {
    static let shared = SettingsWindowController()

    private var inputs: [String: NSTextField] = [:]

    private init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 200),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "Caster settings"
        window.titlebarAppearsTransparent = true
        window.backgroundColor = Nightshade.panel
        super.init(window: window)
        window.delegate = self

        let grid = NSGridView()
        grid.rowSpacing = 8
        grid.columnSpacing = 12
        grid.rowAlignment = .firstBaseline
        for field in Snippets.fields {
            let label = NSTextField(labelWithString: field.title)
            label.font = Nightshade.font(TypeTokens.ui)
            label.textColor = Nightshade.muted
            label.alignment = .right
            let box = FieldBox()
            box.widthAnchor.constraint(greaterThanOrEqualToConstant: 300).isActive = true
            box.input.delegate = self
            inputs[field.defaultsKey] = box.input
            grid.addRow(with: [label, box])
        }
        grid.column(at: 0).xPlacement = .trailing
        grid.rowAlignment = .none
        for row in 0..<grid.numberOfRows { grid.row(at: row).yPlacement = .center }

        let version = NSTextField(labelWithString: Self.nameAndVersion)
        version.font = Nightshade.font(TypeTokens.uiSmall)
        version.textColor = Nightshade.faint
        grid.addRow(with: [NSGridCell.emptyContentView, version])
        grid.row(at: grid.numberOfRows - 1).topPadding = 8

        let content = NSView()
        grid.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(grid)
        NSLayoutConstraint.activate([
            // Clear of the transparent title bar.
            grid.topAnchor.constraint(equalTo: content.topAnchor, constant: 44),
            grid.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
            grid.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
            grid.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -20),
        ])
        window.contentView = content
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func fill() {
        for (key, input) in inputs {
            input.stringValue = Snippets.value(for: key) ?? ""
        }
        if let first = Snippets.fields.first.flatMap({ inputs[$0.defaultsKey] }) {
            window?.makeFirstResponder(first)
        }
    }

    func show() {
        fill()
        window?.center()
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate()
    }

    /// The window's content drawn on its own, for the preview.
    func renderPNG(to url: URL) throws {
        guard let frame = window?.contentView?.superview else { throw CocoaError(.fileWriteUnknown) }
        let bounds = frame.bounds
        guard let rep = frame.bitmapImageRepForCachingDisplay(in: bounds) else { throw CocoaError(.fileWriteUnknown) }
        frame.cacheDisplay(in: bounds, to: rep)
        guard let data = rep.representation(using: .png, properties: [:]) else { throw CocoaError(.fileWriteUnknown) }
        try data.write(to: url)
    }

    /// "Caster 0.2.0"; just "Caster" from a bare `swift build`, which has
    /// no Info.plist.
    private static var nameAndVersion: String {
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
        return ["Caster", version].compactMap { $0 }.joined(separator: " ")
    }

    // Leaving a field (Tab, a click elsewhere) and Enter both end editing.
    func controlTextDidEndEditing(_ obj: Notification) {
        guard let input = obj.object as? NSTextField,
              let key = inputs.first(where: { $0.value === input })?.key
        else { return }
        Snippets.set(input.stringValue, for: key)
    }

    // Closing the window is leaving the field too: end its editing so it saves.
    func windowWillClose(_ notification: Notification) {
        window?.makeFirstResponder(nil)
    }
}

/// A field from studio/design/components.md: a `field` tone with no border, 28pt
/// tall, `radius-md`; `field-hover` on hover. While editing, one ring on
/// the edge: an `accent` border plus a 3pt `accent` glow at 22%. The box
/// keeps 3pt around the field for that glow.
final class FieldBox: NSView {
    let input = FocusField()
    private static let glow: CGFloat = 3
    private var isHovered = false { didSet { needsDisplay = true } }
    private var isEditing = false { didSet { needsDisplay = true } }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        input.isBezeled = false
        input.isBordered = false
        input.drawsBackground = false
        input.focusRingType = .none
        input.font = Nightshade.font(TypeTokens.ui)
        input.textColor = Nightshade.fg
        input.lineBreakMode = .byTruncatingTail
        input.cell?.isScrollable = true
        input.translatesAutoresizingMaskIntoConstraints = false
        input.onFocus = { [weak self] focused in self?.isEditing = focused }
        addSubview(input)
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 28 + Self.glow * 2),
            input.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.glow + 8),
            input.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.glow - 8),
            input.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                                       owner: self, userInfo: nil))
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // A click anywhere in the tone starts editing, not only on the text.
    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(input)
    }

    override func mouseEntered(with event: NSEvent) { isHovered = true }
    override func mouseExited(with event: NSEvent) { isHovered = false }

    override func draw(_ dirtyRect: NSRect) {
        let field = bounds.insetBy(dx: Self.glow, dy: Self.glow)
        let radius = Nightshade.radiusMd
        if isEditing {
            Nightshade.focusGlow.setFill()
            NSBezierPath(roundedRect: bounds, xRadius: radius + Self.glow, yRadius: radius + Self.glow).fill()
        }
        (isHovered && !isEditing ? Nightshade.fieldHover : Nightshade.field).setFill()
        let shape = NSBezierPath(roundedRect: field, xRadius: radius, yRadius: radius)
        shape.fill()
        if isEditing {
            Nightshade.accent.setStroke()
            let edge = NSBezierPath(roundedRect: field.insetBy(dx: 0.5, dy: 0.5), xRadius: radius - 0.5, yRadius: radius - 0.5)
            edge.lineWidth = 1
            edge.stroke()
        }
    }
}

/// Reports when it takes focus and when its editing session ends: the
/// window's first responder is not observable, and the field editor stands
/// in for the field while it edits.
final class FocusField: NSTextField {
    var onFocus: ((Bool) -> Void)?

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became { onFocus?(true) }
        return became
    }

    override func textDidEndEditing(_ notification: Notification) {
        super.textDidEndEditing(notification)
        onFocus?(false)
    }
}

/// The primary button from studio/design/components.md: 28pt tall, the `control`
/// role, `radius-md`, no border. Fill `primary` with `on-primary` text;
/// `primary-hover` on hover. Keyboard focus is a 2pt `accent` ring, 2pt out,
/// drawn in a layer so it can sit outside the button.
final class PrimaryButton: NSButton {
    private var isHovered = false { didSet { needsDisplay = true } }
    private let ring = CAShapeLayer()

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became { showRing(true) }
        return became
    }

    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned { showRing(false) }
        return resigned
    }

    private func showRing(_ on: Bool) {
        wantsLayer = true
        layer?.masksToBounds = false
        if ring.superlayer == nil { layer?.addSublayer(ring) }
        // The stroke's centre sits 3pt out: a 2pt gap, then the 2pt ring.
        let rect = bounds.insetBy(dx: -3, dy: -3)
        ring.path = CGPath(roundedRect: rect, cornerWidth: Nightshade.radiusMd + 3, cornerHeight: Nightshade.radiusMd + 3, transform: nil)
        ring.fillColor = nil
        ring.lineWidth = 2
        effectiveAppearance.performAsCurrentDrawingAppearance {
            ring.strokeColor = Nightshade.accent.cgColor
        }
        ring.isHidden = !on
    }

    convenience init(title: String, target: AnyObject?, action: Selector?) {
        self.init(frame: .zero)
        self.title = title
        self.target = target
        self.action = action
        isBordered = false
        focusRingType = .none
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 28).isActive = true
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                                       owner: self, userInfo: nil))
    }

    private var label: NSAttributedString {
        NSAttributedString(string: title, attributes: [
            .font: Nightshade.font(TypeTokens.control),
            .foregroundColor: Nightshade.onPrimary,
        ])
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: ceil(label.size().width) + 28, height: 28)
    }

    override func mouseEntered(with event: NSEvent) { isHovered = true }
    override func mouseExited(with event: NSEvent) { isHovered = false }

    override func draw(_ dirtyRect: NSRect) {
        let pressed = isHighlighted || isHovered
        (pressed ? Nightshade.primaryHover : Nightshade.primary).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: Nightshade.radiusMd, yRadius: Nightshade.radiusMd).fill()
        let text = label
        let size = text.size()
        text.draw(at: NSPoint(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2))
    }
}
