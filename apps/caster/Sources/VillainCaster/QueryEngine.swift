import AppKit

/// What the panel should render for a query.
enum QueryOutput {
    case empty
    case list([ResultItem])
    /// Shown inline to the right of the input; `copyValue` is what ⏎ copies.
    case inline(display: String, copyValue: String?)
}

/// A built-in action that competes with apps in the fuzzy search.
private struct Command {
    let title: String
    let subtitle: String
    let icon: NSImage?
    /// Irreversible — never offered in the empty-query list, where row 0
    /// is preselected and a stray ⌘Space ⏎ would run it.
    let isDestructive: Bool
    let isAvailable: () -> Bool
    let run: () -> Void

    init(title: String, subtitle: String, symbol: String, isDestructive: Bool = false,
         isAvailable: @escaping () -> Bool, run: @escaping () -> Void) {
        self.title = title
        self.subtitle = subtitle
        self.icon = symbolIcon(symbol, title)
        self.isDestructive = isDestructive
        self.isAvailable = isAvailable
        self.run = run
    }
}

/// SF Symbol at result-list size; build once, not per keystroke.
func symbolIcon(_ name: String, _ description: String) -> NSImage? {
    NSImage(systemSymbolName: name, accessibilityDescription: description)?
        .withSymbolConfiguration(.init(pointSize: 16, weight: .regular))
}

final class QueryEngine {
    private let appIndex = AppIndex()
    private var generation = 0
    private var pendingNetworkWork: DispatchWorkItem?
    /// True while the current query's network result hasn't arrived yet.
    private(set) var isLoading = false
    /// App icons are surprisingly expensive to fetch; cache evicts itself
    /// under memory pressure.
    private let iconCache = NSCache<NSString, NSImage>()
    private static let snippetIcon = symbolIcon("doc.on.clipboard", "Snippet")

    private let commands: [Command] = [
        Command(title: "Maximize Window",
                subtitle: "Monitor size minus the menu bar",
                symbol: "macwindow",
                isAvailable: { true },
                run: { WindowManager.maximizeFocusedWindow() }),
        Command(title: "Move Window to Next Display",
                subtitle: "Send the focused window to the other monitor",
                symbol: "rectangle.on.rectangle",
                isAvailable: { WindowManager.hasSecondDisplay },
                run: { WindowManager.moveFocusedWindowToNextDisplay() }),
        Command(title: "Sleep",
                subtitle: "Put the Mac to sleep",
                symbol: "moon.zzz",
                isAvailable: { true },
                run: { SystemActions.sleep() }),
        Command(title: "Lock Screen",
                subtitle: "Sleep displays and lock the session",
                symbol: "lock",
                isAvailable: { true },
                run: { SystemActions.lockScreen() }),
        Command(title: "Empty Trash",
                subtitle: "Ask Finder to empty the trash",
                symbol: "trash",
                isDestructive: true,
                isAvailable: { true },
                run: { SystemActions.emptyTrash() }),
        Command(title: "Toggle Dark Mode",
                subtitle: "Switch between light and dark appearance",
                symbol: "circle.lefthalf.filled",
                isAvailable: { true },
                run: { SystemActions.toggleDarkMode() }),
        Command(title: "Quit Villain Caster",
                subtitle: "Close this launcher",
                symbol: "power",
                isAvailable: { true },
                run: { NSApp.terminate(nil) }),
    ]

    func refreshApps() {
        appIndex.scanIfNeeded()
    }

    /// Routes the query to the right handler. `deliver` may be called more
    /// than once (loading placeholder, then final results); stale async
    /// results are dropped via the generation counter.
    func query(_ raw: String, deliver: @escaping (QueryOutput) -> Void) {
        generation += 1
        let gen = generation
        pendingNetworkWork?.cancel()
        isLoading = false

        let text = raw.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else {
            let frequent = frequentItems()
            deliver(frequent.isEmpty ? .empty : .list(frequent))
            return
        }

        if Help.matches(text) {
            deliver(.list(Help.items()))
            return
        }

        if let currencyQuery = CurrencyQuery.parse(text) {
            deliver(.inline(display: "= …", copyValue: nil))
            debounced(gen) { finish in
                Currency.convert(currencyQuery) { display, copyValue in
                    finish(.inline(display: display, copyValue: copyValue))
                }
            } deliver: { deliver($0) }
            return
        }

        if let time = TimeLookup.lookup(text) {
            deliver(.inline(display: time.display, copyValue: time.copyValue))
            return
        }

        if let emojiItems = EmojiSearch.search(text) {
            deliver(.list(emojiItems))
            return
        }

        if Weather.matches(text) {
            deliver(.list([ResultItem(icon: nil, title: "Fetching weather…", subtitle: nil, action: nil)]))
            debounced(gen) { finish in
                Weather.fetch { finish(.list($0)) }
            } deliver: { deliver($0) }
            return
        }

        if let value = Calculator.evaluate(text) {
            deliver(.inline(display: "= \(value)", copyValue: value))
            return
        }

        if let webItem = WebSearch.match(text) {
            deliver(.list([webItem]))
            return
        }

        // Frecency boost: items you actually launch outrank same-fuzzy-score
        // neighbors ("zen" → Zen Browser above an app literally named Zen),
        // and the boost fades for things not used in a while.
        let frecency = UsageStore.decayedAll()
        func boost(_ usageKey: String) -> Int {
            Int(min(frecency[usageKey] ?? 0, 20) * 10)
        }

        // Rank lightweight candidates first; build rows (and fetch app icons)
        // only for the few that are shown.
        var scored: [(score: Int, title: String, make: () -> ResultItem)] = []
        for (score, entry) in appIndex.searchScored(text) {
            scored.append((score + boost("app:\(entry.url.path)"), entry.name, { self.appItem(entry) }))
        }
        for command in commands where command.isAvailable() {
            guard let score = Fuzzy.score(query: text, target: command.title) else { continue }
            scored.append((score + boost("command:\(command.title)"), command.title, { self.commandItem(command) }))
        }
        for field in Snippets.fields {
            guard Snippets.value(for: field.defaultsKey) != nil,
                  let score = Fuzzy.score(query: text, target: field.title + " " + field.keywords)
            else { continue }
            scored.append((score + boost("snippet:\(field.defaultsKey)"), field.title, { self.snippetItem(field) }))
        }
        scored.sort { $0.score == $1.score ? $0.title < $1.title : $0.score > $1.score }
        deliver(.list(scored.prefix(8).map { $0.make() }))
    }

    // MARK: - Item builders

    private func appIcon(forPath path: String) -> NSImage {
        if let cached = iconCache.object(forKey: path as NSString) { return cached }
        let icon = NSWorkspace.shared.icon(forFile: path)
        iconCache.setObject(icon, forKey: path as NSString)
        return icon
    }

    private func appItem(_ entry: AppEntry) -> ResultItem {
        ResultItem(
            icon: appIcon(forPath: entry.url.path),
            title: entry.name,
            subtitle: nil,
            usageKey: "app:\(entry.url.path)",
            action: { Self.launch(entry.url) }
        )
    }

    /// Launches the app at exactly this URL. Matching running apps by URL
    /// (not bundle id) and disabling substitution keeps sibling builds with
    /// the same bundle id apart — e.g. Zen Browser vs Zen Twilight, where
    /// the default behavior would focus whichever one is already running.
    private static func launch(_ url: URL) {
        // Finder is always "running"; activate() alone shows nothing when it
        // has no windows. Prefer Dock-style reopen; if Automation is denied,
        // fall through to activate / openApplication.
        if url.lastPathComponent == "Finder.app", SystemActions.openFinder() {
            return
        }
        if let running = NSWorkspace.shared.runningApplications
            .first(where: { $0.bundleURL?.standardizedFileURL.path == url.standardizedFileURL.path }) {
            running.activate()
            return
        }
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.allowsRunningApplicationSubstitution = false
        NSWorkspace.shared.openApplication(at: url, configuration: configuration)
    }

    private func commandItem(_ command: Command) -> ResultItem {
        ResultItem(
            icon: command.icon,
            title: command.title,
            subtitle: command.subtitle,
            usageKey: "command:\(command.title)",
            action: command.run
        )
    }

    private func snippetItem(_ field: Snippets.Field) -> ResultItem {
        ResultItem(
            icon: Self.snippetIcon,
            title: field.title,
            subtitle: Snippets.value(for: field.defaultsKey),
            usageKey: "snippet:\(field.defaultsKey)",
            action: {
                if let value = Snippets.value(for: field.defaultsKey) {
                    Clipboard.copy(value)
                }
            }
        )
    }

    /// Most-executed items, shown when the input is empty.
    private func frequentItems() -> [ResultItem] {
        var items: [ResultItem] = []
        for key in UsageStore.top(10) {
            if key.hasPrefix("app:") {
                let path = String(key.dropFirst(4))
                guard FileManager.default.fileExists(atPath: path) else { continue }
                var name = (path as NSString).lastPathComponent
                if name.hasSuffix(".app") { name = String(name.dropLast(4)) }
                items.append(appItem(AppEntry(name: name, url: URL(fileURLWithPath: path))))
            } else if key.hasPrefix("command:") {
                let title = String(key.dropFirst(8))
                guard let command = commands.first(where: { $0.title == title }),
                      !command.isDestructive, command.isAvailable() else { continue }
                items.append(commandItem(command))
            } else if key.hasPrefix("snippet:") {
                let defaultsKey = String(key.dropFirst(8))
                guard let field = Snippets.fields.first(where: { $0.defaultsKey == defaultsKey }),
                      Snippets.value(for: defaultsKey) != nil else { continue }
                items.append(snippetItem(field))
            }
            if items.count == 3 { break }
        }
        return items
    }

    /// Runs a network fetch after a short debounce so keystrokes don't spam
    /// the APIs. Results are only delivered if the query hasn't changed.
    private func debounced(_ gen: Int,
                           _ fetch: @escaping (@escaping (QueryOutput) -> Void) -> Void,
                           deliver: @escaping (QueryOutput) -> Void) {
        let work = DispatchWorkItem { [weak self] in
            fetch { output in
                DispatchQueue.main.async {
                    guard let self, gen == self.generation else { return }
                    self.isLoading = false
                    deliver(output)
                }
            }
        }
        pendingNetworkWork = work
        isLoading = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25, execute: work)
    }
}
