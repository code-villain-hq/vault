import Foundation

/// Offline timezone lookup: "time in tokyo", "time nyc", "time cet",
/// bare "time" for local. Cities come from the system tz database,
/// plus a few aliases the database doesn't know.
enum TimeLookup {
    static func lookup(_ text: String) -> (display: String, copyValue: String)? {
        let lower = text.lowercased()
        guard lower == "time" || lower.hasPrefix("time ") else { return nil }
        var place = String(lower.dropFirst(4)).trimmingCharacters(in: .whitespaces)
        if place.hasPrefix("in ") { place = String(place.dropFirst(3)) }

        let zone: TimeZone?
        if place.isEmpty {
            zone = .current
        } else {
            zone = resolve(place)
        }
        guard let zone else { return nil }

        let now = Date()
        // Reused — creating DateFormatters per keystroke is expensive.
        // Main-thread only, so mutating the timezone is safe.
        timeFormatter.timeZone = zone
        weekdayFormatter.timeZone = zone

        let time = timeFormatter.string(from: now)
        let diffSeconds = zone.secondsFromGMT(for: now) - TimeZone.current.secondsFromGMT(for: now)
        let diffText = diffSeconds == 0
            ? "same as here"
            : String(format: "%+g h", Double(diffSeconds) / 3600)
        let cityName = zone.identifier.split(separator: "/").last
            .map { $0.replacingOccurrences(of: "_", with: " ") } ?? zone.identifier

        return ("= \(time) \(weekdayFormatter.string(from: now)) · \(cityName) \(diffText)", time)
    }

    private static let timeFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter
    }()

    private static let weekdayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "EEE"
        return formatter
    }()

    /// Shorthands and cities that aren't tz-database city names.
    private static let aliases: [String: String] = [
        "nyc": "America/New_York",
        "la": "America/Los_Angeles",
        "sf": "America/Los_Angeles",
        "san francisco": "America/Los_Angeles",
        "sthlm": "Europe/Stockholm",
        "gothenburg": "Europe/Stockholm",
        "beijing": "Asia/Shanghai",
        "delhi": "Asia/Kolkata",
        "mumbai": "Asia/Kolkata",
    ]

    /// Lowercased city name → identifier, from the system database
    /// ("America/New_York" → "new york").
    private static let cities: [(name: String, id: String)] = TimeZone.knownTimeZoneIdentifiers.map {
        (String($0.split(separator: "/").last ?? "").replacingOccurrences(of: "_", with: " ").lowercased(), $0)
    }

    private static func resolve(_ place: String) -> TimeZone? {
        if let id = aliases[place] { return TimeZone(identifier: id) }
        if let zone = TimeZone(abbreviation: place.uppercased()) { return zone } // CET, PST, UTC…
        if let hit = cities.first(where: { $0.name == place }) { return TimeZone(identifier: hit.id) }
        // Loose matches need some length or every letter combo hits a city.
        if place.count >= 3, let hit = cities.first(where: { $0.name.hasPrefix(place) }) {
            return TimeZone(identifier: hit.id)
        }
        if place.count >= 4, let hit = cities.first(where: { $0.name.contains(place) }) {
            return TimeZone(identifier: hit.id)
        }
        return nil
    }
}
