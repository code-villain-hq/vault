import AppKit

private let panelWidth: CGFloat = 640
private let inputHeight: CGFloat = 60
private let rowHeight: CGFloat = 50
private let maxVisibleRows = 8
private let listTopPadding: CGFloat = 8
private let listBottomPadding: CGFloat = 14
private let listSidePadding: CGFloat = 14
private let inputSidePadding: CGFloat = 20
private let panelCornerRadius: CGFloat = 18
private let screenshotPadding: CGFloat = 48

/// Clips Liquid Glass to a rounded shape. Declares corners via the macOS 27
/// cornerConfiguration API and applies a matching layer mask so glass bloom
/// can't draw a square halo on light desktops.
private final class GlassClipView: NSView {
    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.cornerCurve = .continuous
        layer?.masksToBounds = true
        layer?.cornerRadius = panelCornerRadius
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var cornerConfiguration: NSViewCornerConfiguration? {
        .uniformCorners(radius: .fixed(panelCornerRadius))
    }

    override func viewDidChangeEffectiveCornerRadii() {
        super.viewDidChangeEffectiveCornerRadii()
        layer?.cornerRadius = effectiveCornerRadii?.topLeft ?? panelCornerRadius
    }
}

final class LauncherPanel: NSPanel {
    var onScreenshot: (() -> Void)?
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }

    // The main menu's Edit items only fire while the app is active; this
    // nonactivating panel is key without activating us, so route
    // ⌘V/⌘C/⌘X/⌘A to the field editor here.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let key = event.charactersIgnoringModifiers?.lowercased()

        // ⌘S / ⌘⇧S (Shift makes charactersIgnoringModifiers "S").
        if mods.contains(.command),
           !mods.contains(.option), !mods.contains(.control),
           key == "s" {
            onScreenshot?()
            return true
        }

        if mods == .command {
            switch key {
            case "v":
                return NSApp.sendAction(#selector(NSText.paste(_:)), to: nil, from: self)
            case "c":
                return NSApp.sendAction(#selector(NSText.copy(_:)), to: nil, from: self)
            case "x":
                return NSApp.sendAction(#selector(NSText.cut(_:)), to: nil, from: self)
            case "a":
                return NSApp.sendAction(#selector(NSText.selectAll(_:)), to: nil, from: self)
            default:
                break
            }
        }
        return super.performKeyEquivalent(with: event)
    }

    // Escape must close no matter which view has focus; the text field's
    // delegate only sees it while the field editor is first responder.
    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { // Escape
            onCancel?()
            return
        }
        super.keyDown(with: event)
    }
}

final class PanelController: NSObject, NSTextFieldDelegate, NSTableViewDataSource, NSTableViewDelegate, NSWindowDelegate {
    private let panel: LauncherPanel
    /// Rounded clip host — Liquid Glass blooms past its cornerRadius into a
    /// rectangular rim that reads as a square halo on light desktops.
    private let glassHost = GlassClipView()
    private let glass = NSGlassEffectView()
    /// Field/list live here; NSGlassEffectView only glass-treats contentView.
    private let chrome = NSView()
    private let field = NSTextField()
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()
    private let divider = NSBox()
    private let inlineResultField = NSTextField(labelWithString: "")
    private let engine = QueryEngine()
    private var results: [ResultItem] = []
    private var inlineCopyValue: String?
    /// ⏎ pressed while an inline lookup (currency) was still loading: copy
    /// the value when it arrives instead of silently copying nothing.
    private var copyWhenReady = false
    /// Query restored on next open when the panel closed without executing.
    private var storedText: String?
    /// Input line height for vertical centering — a taller frame makes
    /// NSTextField top-align its text. Font is fixed, so compute once.
    private lazy var fieldHeight: CGFloat = ceil(field.cell?.cellSize(forBounds:
        NSRect(x: 0, y: 0, width: 100, height: 100)).height ?? 30)

    override init() {
        panel = LauncherPanel(
            contentRect: NSRect(x: 0, y: 0, width: panelWidth, height: inputHeight),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        super.init()
        configurePanel()
        configureField()
        configureTable()
        panel.onScreenshot = { [weak self] in self?.captureScreenshot() }
        panel.onCancel = { [weak self] in self?.hide() }
        engine.refreshApps()
    }

    /// ⌘S / ⌘⇧S → Desktop PNG of the panel region (padded). Screen grab keeps
    /// composited glass; window-only capture would lose the blur to alpha.
    private func captureScreenshot() {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        let url = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Desktop/villain-caster-\(formatter.string(from: Date())).png")

        let padded = panel.frame.insetBy(dx: -screenshotPadding, dy: -screenshotPadding)
        let screen = panel.screen?.frame ?? NSScreen.screens.first?.frame ?? .zero
        let clipped = padded.intersection(screen)
        // Cocoa (bottom-left) → screencapture -R (top-left).
        let primaryHeight = NSScreen.screens.first?.frame.maxY ?? 0
        let region = "\(Int(clipped.minX)),\(Int(primaryHeight - clipped.maxY)),\(Int(clipped.width)),\(Int(clipped.height))"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let task = Process()
            task.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            task.arguments = ["-x", "-R", region, url.path]
            try? task.run()
            task.waitUntilExit()
            let captured = task.terminationStatus == 0
                && FileManager.default.fileExists(atPath: url.path)

            DispatchQueue.main.async {
                guard let self else { return }
                if captured {
                    self.showScreenshotFeedback("📸 saved to Desktop")
                } else {
                    self.captureViewRender(to: url)
                }
            }
        }
    }

    private func captureViewRender(to url: URL) {
        let bounds = chrome.bounds
        guard let rep = chrome.bitmapImageRepForCachingDisplay(in: bounds) else { return }
        chrome.cacheDisplay(in: bounds, to: rep)
        guard let data = rep.representation(using: .png, properties: [:]) else { return }
        do {
            try data.write(to: url)
            showScreenshotFeedback("📸 saved (no blur — grant Screen Recording)")
        } catch {
            NSLog("Villain Caster screenshot failed: \(error)")
        }
    }

    private func showScreenshotFeedback(_ message: String) {
        inlineResultField.stringValue = message
        inlineResultField.isHidden = false
        relayout()
    }

    // MARK: - Setup

    private func configurePanel() {
        panel.level = .floating
        // canJoinAllApplications: float above other apps' full-screen Spaces.
        panel.collectionBehavior = [
            .canJoinAllSpaces, .canJoinAllApplications, .fullScreenAuxiliary, .transient
        ]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false // system shadow sits in clear corner pockets
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.delegate = self

        glassHost.autoresizingMask = [.width, .height]
        glassHost.invalidateCornerConfiguration()
        chrome.autoresizingMask = [.width, .height]

        // .regular keeps text legible; .clear washes out on bright desktops.
        glass.style = .regular
        glass.cornerRadius = panelCornerRadius
        glass.autoresizingMask = [.width, .height]
        glass.effectIsInteractive = true
        glass.contentView = chrome
        glassHost.addSubview(glass)
        panel.contentView = glassHost
    }

    private func configureField() {
        field.font = .systemFont(ofSize: 24, weight: .light)
        field.placeholderString = "Search apps, calculate, convert…"
        field.isBezeled = false
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.lineBreakMode = .byTruncatingTail
        field.delegate = self
        chrome.addSubview(field)

        // Soft hairline — system .separator reads near-black on Liquid Glass.
        divider.boxType = .custom
        divider.titlePosition = .noTitle
        divider.borderWidth = 0
        divider.fillColor = NSColor.labelColor.withAlphaComponent(0.1)
        divider.isHidden = true
        chrome.addSubview(divider)

        inlineResultField.font = .systemFont(ofSize: 24, weight: .light)
        inlineResultField.textColor = .secondaryLabelColor
        inlineResultField.alignment = .right
        inlineResultField.lineBreakMode = .byTruncatingHead
        inlineResultField.isHidden = true
        chrome.addSubview(inlineResultField)
    }

    private func configureTable() {
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("result"))
        column.width = panelWidth - listSidePadding * 2
        tableView.addTableColumn(column)
        tableView.headerView = nil
        tableView.backgroundColor = .clear
        tableView.rowHeight = rowHeight
        tableView.intercellSpacing = NSSize(width: 0, height: 0)
        // .plain kills the macOS 11+ "inset" style, which draws its own
        // rounded, inset selection on top of our drawSelection — the source
        // of mismatched corner radii that vary by row position.
        tableView.style = .plain
        tableView.selectionHighlightStyle = .regular
        tableView.dataSource = self
        tableView.delegate = self
        tableView.target = self
        tableView.action = #selector(rowClicked)

        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = false
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay
        scrollView.verticalScrollElasticity = .none
        scrollView.automaticallyAdjustsContentInsets = false
        scrollView.contentInsets = NSEdgeInsets()
        chrome.addSubview(scrollView)
    }

    // MARK: - Show / hide

    func toggle() {
        if panel.isVisible {
            hide()
        } else {
            show()
        }
    }

    func show() {
        guard let screen = NSScreen.main else { return }
        let visible = screen.visibleFrame
        let top = visible.minY + visible.height * 0.72
        panel.setFrame(
            NSRect(x: visible.midX - panelWidth / 2, y: top - inputHeight,
                   width: panelWidth, height: inputHeight),
            display: false
        )
        field.stringValue = storedText ?? ""
        engine.refreshApps()
        // Restored query or empty → most-used items. Every query path
        // delivers synchronously at least once, so the panel appears at its
        // final size.
        runQuery()
        panel.makeKeyAndOrderFront(nil)
        // Focusing selects any restored text, so typing replaces it.
        panel.makeFirstResponder(field)
    }

    func hide() {
        storedText = restorableText()
        panel.orderOut(nil)
    }

    /// Text worth restoring next time: an unfinished search. One-shot
    /// lookups (math/currency/time — inline visible — weather, help) are not.
    private func restorableText() -> String? {
        let text = field.stringValue.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty,
              inlineResultField.isHidden,
              !Weather.matches(text),
              !Help.matches(text)
        else { return nil }
        return field.stringValue
    }

    func windowDidResignKey(_ notification: Notification) {
        hide()
    }

    /// Resizes the panel downward from a fixed top edge as results change.
    private func relayout() {
        let rows = min(results.count, maxVisibleRows)
        let visibleListHeight = CGFloat(rows) * rowHeight
        let listAreaHeight: CGFloat = rows > 0
            ? 1 + listTopPadding + visibleListHeight + listBottomPadding
            : 0
        let totalHeight = inputHeight + listAreaHeight

        var frame = panel.frame
        let top = frame.maxY
        frame.size.height = totalHeight
        frame.origin.y = top - totalHeight
        if frame != panel.frame {
            panel.setFrame(frame, display: true)
        }
        let bounds = NSRect(origin: .zero, size: frame.size)
        glassHost.frame = bounds
        glass.frame = bounds
        chrome.frame = bounds

        let fieldY = totalHeight - inputHeight + (inputHeight - fieldHeight) / 2
        if inlineResultField.isHidden {
            field.frame = NSRect(x: inputSidePadding, y: fieldY,
                                 width: panelWidth - inputSidePadding * 2, height: fieldHeight)
        } else {
            // Split the input row: query on the left, "= result" on the right.
            inlineResultField.sizeToFit()
            let labelWidth = min(inlineResultField.frame.width, panelWidth * 0.5)
            inlineResultField.frame = NSRect(x: panelWidth - inputSidePadding - labelWidth, y: fieldY,
                                             width: labelWidth, height: fieldHeight)
            field.frame = NSRect(x: inputSidePadding, y: fieldY,
                                 width: panelWidth - inputSidePadding * 2 - labelWidth - 12, height: fieldHeight)
        }
        divider.frame = NSRect(x: listSidePadding, y: totalHeight - inputHeight - 1,
                               width: panelWidth - listSidePadding * 2, height: 1)
        divider.isHidden = rows == 0
        scrollView.frame = NSRect(x: listSidePadding, y: listBottomPadding,
                                  width: panelWidth - listSidePadding * 2, height: visibleListHeight)
        scrollView.isHidden = rows == 0
        // Keep the table exactly as wide as the visible clip area, otherwise
        // the selection highlight extends under the clipped edge and its
        // right corners appear square.
        let contentWidth = scrollView.contentSize.width
        if let column = tableView.tableColumns.first, column.width != contentWidth {
            column.width = contentWidth
        }
    }

    // MARK: - Text input

    func controlTextDidChange(_ obj: Notification) {
        runQuery()
    }

    private func runQuery() {
        copyWhenReady = false
        engine.query(field.stringValue) { [weak self] output in
            guard let self else { return }
            if self.copyWhenReady, case .inline(_, let copyValue?) = output {
                self.copyWhenReady = false
                Clipboard.copy(copyValue)
            }
            switch output {
            case .empty:
                self.results = []
                self.inlineCopyValue = nil
                self.inlineResultField.isHidden = true
            case .list(let items):
                self.results = items
                self.inlineCopyValue = nil
                self.inlineResultField.isHidden = true
            case .inline(let display, let copyValue):
                self.results = []
                self.inlineCopyValue = copyValue
                self.inlineResultField.stringValue = display
                self.inlineResultField.isHidden = false
            }
            self.tableView.reloadData()
            if !self.results.isEmpty {
                self.tableView.selectRowIndexes([0], byExtendingSelection: false)
                self.tableView.scrollRowToVisible(0)
            }
            self.relayout()
        }
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        switch commandSelector {
        case #selector(NSResponder.moveDown(_:)):
            moveSelection(by: 1)
            return true
        case #selector(NSResponder.moveUp(_:)):
            moveSelection(by: -1)
            return true
        case #selector(NSResponder.insertNewline(_:)):
            executeSelected()
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            hide()
            return true
        case #selector(NSResponder.insertTab(_:)),
             #selector(NSResponder.insertBacktab(_:)):
            return true // keep focus in the input field
        default:
            return false
        }
    }

    private func moveSelection(by delta: Int) {
        guard !results.isEmpty else { return }
        let current = tableView.selectedRow
        let next = current < 0 ? 0 : min(max(current + delta, 0), results.count - 1)
        tableView.selectRowIndexes([next], byExtendingSelection: false)
        tableView.scrollRowToVisible(next)
    }

    private func executeSelected() {
        if let copyValue = inlineCopyValue {
            hide()
            storedText = nil
            Clipboard.copy(copyValue)
            return
        }
        if !inlineResultField.isHidden, engine.isLoading {
            copyWhenReady = true
            hide()
            storedText = nil
            return
        }
        let row = tableView.selectedRow >= 0 ? tableView.selectedRow : 0
        guard row < results.count else {
            hide()
            return
        }
        let item = results[row]
        // Capture before hide — frontmost stays the user's app while our
        // nonactivating panel is key, but clear the target after running.
        WindowManager.targetPID = NSWorkspace.shared.frontmostApplication?.processIdentifier
        hide()
        storedText = nil // executed — next open starts fresh
        if let key = item.usageKey {
            UsageStore.record(key)
        }
        item.action?()
        WindowManager.targetPID = nil
    }

    @objc private func rowClicked() {
        guard tableView.clickedRow >= 0 else { return }
        tableView.selectRowIndexes([tableView.clickedRow], byExtendingSelection: false)
        executeSelected()
    }

    // MARK: - Table

    func numberOfRows(in tableView: NSTableView) -> Int {
        results.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let identifier = NSUserInterfaceItemIdentifier("ResultCell")
        let cell = tableView.makeView(withIdentifier: identifier, owner: self) as? ResultCell
            ?? ResultCell(identifier: identifier)
        cell.configure(with: results[row])
        return cell
    }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        let identifier = NSUserInterfaceItemIdentifier("ResultRow")
        if let reused = tableView.makeView(withIdentifier: identifier, owner: self) as? ResultRowView {
            return reused
        }
        let rowView = ResultRowView()
        rowView.identifier = identifier
        return rowView
    }
}

/// Glassy rounded highlight — plain white (dark) / black (light) at low
/// alpha, no border, like system glass selections. Keeps normal text colors.
final class ResultRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {
        guard isSelected else { return }
        let rect = bounds.insetBy(dx: 2, dy: 3)
        let isDark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let fill = isDark
            ? NSColor.white.withAlphaComponent(0.13)
            : NSColor.black.withAlphaComponent(0.07)
        fill.setFill()
        NSBezierPath(roundedRect: rect, xRadius: 9, yRadius: 9).fill()
    }
}

final class ResultCell: NSTableCellView {
    private let iconView = NSImageView()
    private let titleField = NSTextField(labelWithString: "")
    private let subtitleField = NSTextField(labelWithString: "")

    init(identifier: NSUserInterfaceItemIdentifier) {
        super.init(frame: .zero)
        self.identifier = identifier

        titleField.font = .systemFont(ofSize: 16)
        titleField.lineBreakMode = .byTruncatingTail
        subtitleField.font = .systemFont(ofSize: 13)
        subtitleField.textColor = .secondaryLabelColor
        subtitleField.lineBreakMode = .byTruncatingMiddle

        addSubview(iconView)
        addSubview(titleField)
        addSubview(subtitleField)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(with item: ResultItem) {
        iconView.image = item.icon
        titleField.stringValue = item.title
        subtitleField.stringValue = item.subtitle ?? ""
        subtitleField.isHidden = item.subtitle == nil
        needsLayout = true
    }

    override func layout() {
        super.layout()
        let hasIcon = iconView.image != nil
        let iconSize: CGFloat = 24
        iconView.frame = NSRect(x: 12, y: (bounds.height - iconSize) / 2,
                                width: hasIcon ? iconSize : 0, height: iconSize)
        let textX: CGFloat = hasIcon ? 12 + iconSize + 10 : 14
        let textWidth = bounds.width - textX - 14
        if subtitleField.isHidden {
            titleField.frame = NSRect(x: textX, y: (bounds.height - 21) / 2, width: textWidth, height: 21)
        } else {
            titleField.frame = NSRect(x: textX, y: bounds.height / 2 - 1, width: textWidth, height: 21)
            subtitleField.frame = NSRect(x: textX, y: bounds.height / 2 - 18, width: textWidth, height: 17)
        }
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        needsLayout = true
    }
}
