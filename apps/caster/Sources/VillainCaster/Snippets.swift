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
        Field(defaultsKey: "snippet.workEmail", title: "Work Email", keywords: "mail job company"),
        Field(defaultsKey: "snippet.privateEmail", title: "Private Email", keywords: "mail personal home"),
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

/// Plain titled window with one text field per snippet, opened from the
/// status item's right-click menu.
final class SettingsWindowController: NSWindowController {
    static let shared = SettingsWindowController()

    private var inputs: [String: NSTextField] = [:]

    private init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 230),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "Villain Caster Settings"
        super.init(window: window)

        let grid = NSGridView()
        grid.rowSpacing = 10
        grid.columnSpacing = 12
        for field in Snippets.fields {
            let label = NSTextField(labelWithString: field.title + ":")
            label.alignment = .right
            let input = NSTextField()
            input.widthAnchor.constraint(greaterThanOrEqualToConstant: 300).isActive = true
            inputs[field.defaultsKey] = input
            grid.addRow(with: [label, input])
        }
        let saveButton = NSButton(title: "Save", target: self, action: #selector(save))
        saveButton.keyEquivalent = "\r"
        grid.addRow(with: [NSGridCell.emptyContentView, saveButton])

        let content = NSView()
        grid.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(grid)
        NSLayoutConstraint.activate([
            grid.topAnchor.constraint(equalTo: content.topAnchor, constant: 20),
            grid.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
            grid.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
            grid.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -20),
        ])
        window.contentView = content
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show() {
        for (key, input) in inputs {
            input.stringValue = Snippets.value(for: key) ?? ""
        }
        window?.center()
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate()
    }

    @objc private func save() {
        for (key, input) in inputs {
            Snippets.set(input.stringValue, for: key)
        }
        window?.close()
    }
}
