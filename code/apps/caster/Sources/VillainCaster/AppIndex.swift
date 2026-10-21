import Foundation

struct AppEntry {
    let name: String
    let url: URL
    /// Lowercased name, precomputed once at scan time — fuzzy matching
    /// runs on every keystroke over every app.
    let searchChars: [Character]

    init(name: String, url: URL) {
        self.name = name
        self.url = url
        self.searchChars = Array(name.lowercased())
    }
}

final class AppIndex {
    private var apps: [AppEntry] = []
    private var lastScan: Date = .distantPast
    private let scanQueue = DispatchQueue(label: "villaincaster.appscan", qos: .userInitiated)
    private let lock = NSLock()

    func scanIfNeeded() {
        guard Date().timeIntervalSince(lastScan) > 60 else { return }
        lastScan = Date()
        scanQueue.async { [weak self] in
            guard let self else { return }
            let found = Self.scanAll()
            self.lock.lock()
            self.apps = found
            self.lock.unlock()
        }
    }

    /// Unsorted matches; the caller ranks them together with commands.
    func searchScored(_ query: String) -> [(score: Int, entry: AppEntry)] {
        lock.lock()
        let snapshot = apps
        lock.unlock()

        let queryChars = Array(query.lowercased())
        return snapshot.compactMap { entry in
            Fuzzy.score(queryChars: queryChars, targetChars: entry.searchChars).map { ($0, entry) }
        }
    }

    private static func scanAll() -> [AppEntry] {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let roots = [
            "/Applications",
            "/System/Applications", // includes Utilities via recursion
            "\(home)/Applications",
        ]
        var found: [String: AppEntry] = [:]
        for root in roots {
            collect(root, depth: 0, into: &found)
        }
        // Finder lives in CoreServices; index it directly rather than scanning
        // the whole CoreServices tree, which clutters results.
        let finder = "/System/Library/CoreServices/Finder.app"
        if FileManager.default.fileExists(atPath: finder) {
            found[finder] = AppEntry(name: "Finder", url: URL(fileURLWithPath: finder))
        }
        return Array(found.values)
    }

    private static func collect(_ dir: String, depth: Int, into found: inout [String: AppEntry]) {
        let fm = FileManager.default
        guard let items = try? fm.contentsOfDirectory(atPath: dir) else { return }
        for item in items where !item.hasPrefix(".") {
            let path = dir + "/" + item
            if item.hasSuffix(".app") {
                found[path] = AppEntry(name: String(item.dropLast(4)), url: URL(fileURLWithPath: path))
            } else if depth < 2 {
                var isDirectory: ObjCBool = false
                if fm.fileExists(atPath: path, isDirectory: &isDirectory), isDirectory.boolValue {
                    collect(path, depth: depth + 1, into: &found)
                }
            }
        }
    }
}

enum Fuzzy {
    static func score(query: String, target: String) -> Int? {
        score(queryChars: Array(query.lowercased()), targetChars: Array(target.lowercased()))
    }

    /// Subsequence match with bonuses for prefix, word-boundary and
    /// consecutive hits. Returns nil when the query is not a subsequence.
    /// Takes pre-lowercased char arrays so hot callers can precompute them.
    static func score(queryChars q: [Character], targetChars t: [Character]) -> Int? {
        guard !q.isEmpty else { return nil }

        var score = 0
        var qi = 0
        var lastMatch = -2
        for (ti, char) in t.enumerated() {
            guard qi < q.count else { break }
            if char == q[qi] {
                var bonus = 10
                if ti == 0 {
                    bonus += 100
                } else if !t[ti - 1].isLetter && !t[ti - 1].isNumber {
                    bonus += 40
                }
                if ti == lastMatch + 1 { bonus += 25 }
                score += bonus
                lastMatch = ti
                qi += 1
            }
        }
        guard qi == q.count else { return nil }
        // Slightly favor shorter names so "Music" beats "Music Converter Pro".
        return score - max(0, t.count - q.count)
    }
}
