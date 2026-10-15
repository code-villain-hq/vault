import AppKit
import Carbon.HIToolbox

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var panelController: PanelController!
    private var statusItem: NSStatusItem!
    private let hotKey = HotKeyManager()

    func applicationDidFinishLaunching(_ notification: Notification) {
        panelController = PanelController()
        setUpStatusItem()
        setUpEditMenu()
        hotKey.onPress = { [weak self] in
            self?.panelController.toggle()
        }
        let registered = hotKey.register(keyCode: UInt32(kVK_Space), modifiers: UInt32(cmdKey))
        if !registered {
            NSLog("Villain Caster: could not register ⌘Space. Disable Spotlight's shortcut in System Settings → Keyboard → Keyboard Shortcuts → Spotlight.")
        }
    }

    private func setUpStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        guard let button = statusItem.button else { return }
        button.image = MenuBarIcon.image()
        button.target = self
        button.action = #selector(statusItemClicked)
        button.sendAction(on: [.leftMouseUp, .rightMouseUp])
    }

    @objc private func statusItemClicked() {
        if NSApp.currentEvent?.type == .rightMouseUp {
            let menu = NSMenu()
            let settingsItem = NSMenuItem(title: "Settings…",
                                          action: #selector(openSettings),
                                          keyEquivalent: ",")
            settingsItem.target = self
            menu.addItem(settingsItem)
            menu.addItem(.separator())
            menu.addItem(NSMenuItem(title: "Quit Villain Caster",
                                    action: #selector(NSApplication.terminate(_:)),
                                    keyEquivalent: "q"))
            // Attach the menu only for this click so left-click keeps toggling the panel.
            statusItem.menu = menu
            statusItem.button?.performClick(nil)
            statusItem.menu = nil
        } else {
            panelController.toggle()
        }
    }

    @objc private func openSettings() {
        SettingsWindowController.shared.show()
    }

    /// Accessory apps get no menu bar, but a main menu is still needed for
    /// standard Edit key equivalents (⌘V etc.) to reach text fields in
    /// regular windows like Settings.
    private func setUpEditMenu() {
        let mainMenu = NSMenu()
        let editItem = NSMenuItem()
        mainMenu.addItem(editItem)
        let edit = NSMenu(title: "Edit")
        editItem.submenu = edit
        edit.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        edit.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
        edit.addItem(.separator())
        edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        NSApp.mainMenu = mainMenu
    }
}
