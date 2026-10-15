import AppKit

/// Prefix web searches: "g swift nspanel" → Google, "yt …" → YouTube,
/// "gh …" → GitHub.
enum WebSearch {
    private static let icon = symbolIcon("magnifyingglass", "Search")

    private static let engines: [String: (name: String, searchURL: String)] = [
        "g": ("Google", "https://www.google.com/search?q="),
        "yt": ("YouTube", "https://www.youtube.com/results?search_query="),
        "gh": ("GitHub", "https://github.com/search?q="),
    ]

    static func match(_ text: String) -> ResultItem? {
        let parts = text.split(separator: " ", maxSplits: 1)
        guard parts.count == 2,
              let engine = engines[parts[0].lowercased()] else { return nil }
        let query = String(parts[1]).trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return nil }

        var allowed = CharacterSet.urlQueryAllowed
        allowed.remove(charactersIn: "&+=?")
        guard let encoded = query.addingPercentEncoding(withAllowedCharacters: allowed),
              let url = URL(string: engine.searchURL + encoded) else { return nil }

        return ResultItem(
            icon: icon,
            title: "Search \(engine.name) for “\(query)”",
            subtitle: nil,
            action: { NSWorkspace.shared.open(url) }
        )
    }
}
