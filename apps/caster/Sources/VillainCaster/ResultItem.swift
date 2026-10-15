import AppKit

struct ResultItem {
    var icon: NSImage?
    var title: String
    var subtitle: String?
    /// Stable id for usage tracking ("app:<path>" / "command:<title>" /
    /// "snippet:<defaultsKey>");
    /// nil for untracked rows like weather or loading placeholders.
    var usageKey: String? = nil
    var action: (() -> Void)?
}

/// Frecency store: every execution adds 1 to a score that decays
/// exponentially (half-life 7 days). A daily-used app keeps a high score;
/// something launched a lot last month fades on its own. Backs both the
/// empty-query top list and the search-ranking boost.
enum UsageStore {
    private static let defaultsKey = "usageFrecency"
    private static let legacyKey = "usageCounts"
    private static let halfLife: Double = 7 * 24 * 3600

    /// Below this an entry no longer affects ranking (one launch, ~6 weeks
    /// unused); pruned so the store doesn't grow forever.
    private static let pruneBelow = 0.01

    static func record(_ id: String) {
        let now = Date().timeIntervalSince1970
        var entries = load().filter { decayedScore($0.value, now: now) >= pruneBelow }
        let current = entries[id].map { decayedScore($0, now: now) } ?? 0
        entries[id] = [current + 1, now]
        UserDefaults.standard.set(entries, forKey: defaultsKey)
    }

    /// Current decayed score per id — snapshot once per query, not per item.
    static func decayedAll() -> [String: Double] {
        let now = Date().timeIntervalSince1970
        return load().mapValues { decayedScore($0, now: now) }
    }

    static func top(_ limit: Int) -> [String] {
        decayedAll()
            .sorted { $0.value == $1.value ? $0.key < $1.key : $0.value > $1.value }
            .prefix(limit)
            .map(\.key)
    }

    // Stored as [id: [score, lastUsedEpoch]].
    private static func load() -> [String: [Double]] {
        let defaults = UserDefaults.standard
        if let stored = defaults.dictionary(forKey: defaultsKey) as? [String: [Double]] {
            return stored
        }
        // Migrate the old plain-count format: count becomes the score,
        // stamped now.
        if let legacy = defaults.dictionary(forKey: legacyKey) as? [String: Int] {
            let now = Date().timeIntervalSince1970
            let migrated = legacy.mapValues { [Double($0), now] }
            defaults.set(migrated, forKey: defaultsKey)
            defaults.removeObject(forKey: legacyKey)
            return migrated
        }
        return [:]
    }

    private static func decayedScore(_ entry: [Double], now: Double) -> Double {
        guard entry.count == 2 else { return 0 }
        return entry[0] * pow(0.5, max(0, now - entry[1]) / halfLife)
    }
}

enum Clipboard {
    static func copy(_ string: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(string, forType: .string)
    }
}
