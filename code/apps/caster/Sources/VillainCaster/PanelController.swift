import AppKit

// Forge's command menu (code/apps/forge/apps/web/src/command-menu.tsx) scaled
// up a step: a launcher sits alone on the desktop, not inside an app.
private let panelWidth: CGFloat = 680
private let inputHeight: CGFloat = 60
private let rowHeight: CGFloat = 44
private let maxVisibleRows = 10
private let listInset: CGFloat = 8
/// Row content sits this far in from the row's edge.
private let rowContentInset: CGFloat = 10
private let iconSize: CGFloat = 22
/// An NSTextField draws its text 2pt inside its frame.
private let inputSidePadding: CGFloat = listInset + rowContentInset - 2
/// The query in its role; rows at the body size (studio/brand/type.md).
private let inputFont = Nightshade.font(TypeTokens.query)
private let rowFont = Nightshade.font(TypeTokens.body)
private let screenshotPadding: CGFloat = 48

/// The panel itself: a solid `dialog` tone with `radius-lg` corners. No
/// border and no shadow: the tone alone sets it off.
private final class PanelBackground: NSView {
    override func draw(_ dirtyRect: NSRect) {
        Nightshade.dialog.setFill()
        NSBezierPath(roundedRect: bounds, xRadius: Nightshade.radiusLg, yRadius: Nightshade.radiusLg).fill()
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
    private let chrome = PanelBackground()
    private let field = NSTextField()
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()
    /// Screenshot feedback, at the right of the input.
    private let inlineResultField = NSTextField(labelWithString: "")
    private let answerCard = AnswerCard()
    private let engine = QueryEngine()
    private var results: [ResultItem] = []
    private var answer: Answer?
    /// ⏎ pressed while a lookup (currency) was still loading: copy the
    /// value when it arrives instead of silently copying nothing.
    private var copyWhenReady = false
    /// Query restored on next open when the panel closed without executing.
    private var storedText: String?
    /// One line of the input, for vertical centering: a taller frame makes
    /// NSTextField top-align its text. Measured on a sample line, not the
    /// field, whose wrapped placeholder would measure several lines.
    private let fieldHeight: CGFloat = {
        let sample = NSTextField(labelWithString: "Hg")
        sample.font = inputFont
        return ceil(sample.cell?.cellSize.height ?? 20)
    }()

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

    /// ⌘S / ⌘⇧S → Desktop PNG of the panel region (padded), with the desktop
    /// and the panel's shadow around it. Without Screen Recording only the
    /// panel itself is drawn.
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
        do {
            try renderPNG(to: url)
            showScreenshotFeedback("📸 saved (panel only — grant Screen Recording)")
        } catch {
            NSLog("Caster screenshot failed: \(error)")
        }
    }

    /// The panel drawn on its own, corners transparent.
    func renderPNG(to url: URL) throws {
        let bounds = chrome.bounds
        guard let rep = chrome.bitmapImageRepForCachingDisplay(in: bounds),
              let data = { chrome.cacheDisplay(in: bounds, to: rep); return rep.representation(using: .png, properties: [:]) }()
        else { throw CocoaError(.fileWriteUnknown) }
        try data.write(to: url)
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
        // The system shadow brings a light rim with it: a border.
        panel.hasShadow = false
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.delegate = self

        chrome.autoresizingMask = [.width, .height]
        panel.contentView = chrome
    }

    private func configureField() {
        // The query is the panel's one input, so it sits on the dialog tone
        // like an inline editor, without a field fill.
        field.font = inputFont
        field.textColor = Nightshade.fg
        field.placeholderAttributedString = NSAttributedString(
            string: "Search apps, calculate, convert…",
            attributes: [.font: inputFont, .foregroundColor: Nightshade.faint]
        )
        field.isBezeled = false
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.lineBreakMode = .byTruncatingTail
        field.delegate = self
        chrome.addSubview(field)

        inlineResultField.font = inputFont
        inlineResultField.textColor = Nightshade.muted
        inlineResultField.alignment = .right
        inlineResultField.lineBreakMode = .byTruncatingHead
        inlineResultField.isHidden = true
        chrome.addSubview(inlineResultField)

        answerCard.isHidden = true
        answerCard.onActivate = { [weak self] in self?.executeSelected() }
        chrome.addSubview(answerCard)
    }

    private func configureTable() {
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("result"))
        column.width = panelWidth - listInset * 2
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
        // No scroller: a legacy one (a mouse attached, or "always show")
        // takes a strip off every row. Arrows and the wheel still scroll.
        scrollView.hasVerticalScroller = false
        scrollView.drawsBackground = false
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

    /// `present: false` lays the panel out without ordering it in or taking
    /// keys, for the preview.
    func show(present: Bool = true) {
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
        guard present else { return }
        panel.makeKeyAndOrderFront(nil)
        // Focusing selects any restored text, so typing replaces it.
        panel.makeFirstResponder(field)
        (field.currentEditor() as? NSTextView)?.insertionPointColor = Nightshade.fg
    }

    /// Opens with `text` typed, as if restored.
    func show(query text: String, present: Bool = true) {
        storedText = text
        show(present: present)
    }

    func hide() {
        storedText = restorableText()
        panel.orderOut(nil)
    }

    /// Text worth restoring next time: an unfinished search. One-shot
    /// lookups (math/currency/time — an answer showing — weather, help) are not.
    private func restorableText() -> String? {
        let text = field.stringValue.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty,
              answer == nil,
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
        // Space, not a line, separates the query from the list or the answer.
        let listAreaHeight: CGFloat = answer != nil ? AnswerCard.height + listInset
            : rows > 0 ? visibleListHeight + listInset : 0
        let totalHeight = inputHeight + listAreaHeight

        var frame = panel.frame
        let top = frame.maxY
        frame.size.height = totalHeight
        frame.origin.y = top - totalHeight
        if frame != panel.frame {
            panel.setFrame(frame, display: true)
        }
        chrome.frame = NSRect(origin: .zero, size: frame.size)

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
        answerCard.frame = NSRect(x: listInset, y: listInset,
                                  width: panelWidth - listInset * 2, height: AnswerCard.height)
        answerCard.isHidden = answer == nil
        scrollView.frame = NSRect(x: listInset, y: listInset,
                                  width: panelWidth - listInset * 2, height: visibleListHeight)
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
            if self.copyWhenReady, case .answer(let answer) = output, let copyValue = answer.copyValue {
                self.copyWhenReady = false
                Clipboard.copy(copyValue)
            }
            self.inlineResultField.isHidden = true
            switch output {
            case .empty:
                self.results = []
                self.answer = nil
            case .list(let items):
                self.results = items
                self.answer = nil
            case .answer(let answer):
                self.results = []
                self.answer = answer
                self.answerCard.configure(with: answer)
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
        if let answer {
            if let copyValue = answer.copyValue {
                Clipboard.copy(copyValue)
            } else if engine.isLoading {
                copyWhenReady = true
            }
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

/// The active option: `fg` at `highlight-tint`, `radius-md`, no border.
/// The text keeps its colours.
final class ResultRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {
        guard isSelected else { return }
        Nightshade.highlight.setFill()
        NSBezierPath(roundedRect: bounds, xRadius: Nightshade.radiusMd, yRadius: Nightshade.radiusMd).fill()
    }

    // Keep the row's own colours when selected; the default emphasis would
    // switch the text to white.
    override var isEmphasized: Bool {
        get { false }
        set {}
    }
}

/// One line, like Forge's command rows: icon, title in `fg`, then the
/// detail in `muted`.
final class ResultCell: NSTableCellView {
    private let iconView = NSImageView()
    private let label = NSTextField(labelWithString: "")

    init(identifier: NSUserInterfaceItemIdentifier) {
        super.init(frame: .zero)
        self.identifier = identifier
        label.lineBreakMode = .byTruncatingTail
        iconView.imageScaling = .scaleProportionallyDown
        // Symbols draw in `muted`; app icons keep their own colours.
        iconView.contentTintColor = Nightshade.muted
        addSubview(iconView)
        addSubview(label)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(with item: ResultItem) {
        iconView.image = item.icon
        let text = NSMutableAttributedString(string: item.title, attributes: [
            .font: rowFont,
            .foregroundColor: Nightshade.fg,
        ])
        if let subtitle = item.subtitle {
            text.append(NSAttributedString(string: "   " + subtitle, attributes: [
                .font: rowFont,
                .foregroundColor: Nightshade.muted,
            ]))
        }
        label.attributedStringValue = text
        needsLayout = true
    }

    override func layout() {
        super.layout()
        let hasIcon = iconView.image != nil
        iconView.frame = NSRect(x: rowContentInset, y: (bounds.height - iconSize) / 2,
                                width: hasIcon ? iconSize : 0, height: iconSize)
        let textX: CGFloat = hasIcon ? rowContentInset + iconSize + 10 : rowContentInset
        let height = ceil(label.cell?.cellSize.height ?? 18)
        label.frame = NSRect(x: textX, y: (bounds.height - height) / 2,
                             width: bounds.width - textX - rowContentInset, height: height)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        needsLayout = true
    }
}
