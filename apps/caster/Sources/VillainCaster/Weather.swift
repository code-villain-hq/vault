import Foundation

enum Weather {
    /// Only a query that starts with "weather", so "g weather berlin" stays
    /// a web search.
    static func matches(_ text: String) -> Bool {
        let lower = text.lowercased()
        return lower == "weather" || lower.hasPrefix("weather ")
    }

    /// Location from IP (ipapi.co), then current conditions from open-meteo.
    /// Both free, no API key.
    static func fetch(completion: @escaping ([ResultItem]) -> Void) {
        URLSession.shared.dataTask(with: URL(string: "https://ipapi.co/json/")!) { data, _, _ in
            guard let data,
                  let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let lat = object["latitude"] as? Double,
                  let lon = object["longitude"] as? Double
            else {
                completion([ResultItem(icon: nil, title: "Location lookup failed",
                                       subtitle: "ipapi.co unreachable or rate-limited", action: nil)])
                return
            }
            let city = object["city"] as? String ?? "Current location"
            fetchConditions(lat: lat, lon: lon, city: city, completion: completion)
        }.resume()
    }

    private static func fetchConditions(lat: Double, lon: Double, city: String,
                                        completion: @escaping ([ResultItem]) -> Void) {
        let url = URL(string: "https://api.open-meteo.com/v1/forecast?latitude=\(lat)&longitude=\(lon)"
            + "&current=temperature_2m,apparent_temperature,weather_code,wind_speed_10m"
            + "&daily=weather_code,temperature_2m_max,temperature_2m_min"
            + "&timezone=auto&forecast_days=4")!
        URLSession.shared.dataTask(with: url) { data, _, _ in
            guard let data,
                  let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let current = object["current"] as? [String: Any],
                  let temp = current["temperature_2m"] as? Double,
                  let code = (current["weather_code"] as? NSNumber)?.intValue
            else {
                completion([ResultItem(icon: nil, title: "Weather fetch failed",
                                       subtitle: "open-meteo.com unreachable", action: nil)])
                return
            }
            let feels = current["apparent_temperature"] as? Double ?? temp
            let wind = current["wind_speed_10m"] as? Double ?? 0
            let (emoji, description) = describe(code)

            var items = [ResultItem(
                icon: nil,
                title: "\(emoji) \(Int(temp.rounded()))° \(description) — \(city)",
                subtitle: "Feels like \(Int(feels.rounded()))° · wind \(Int(wind.rounded())) km/h",
                action: nil
            )]
            items.append(contentsOf: forecastRows(from: object["daily"] as? [String: Any]))
            completion(items)
        }.resume()
    }

    /// One row per day for the next three days (skipping today).
    private static func forecastRows(from daily: [String: Any]?) -> [ResultItem] {
        guard let daily,
              let dates = daily["time"] as? [String],
              let codeNumbers = daily["weather_code"] as? [NSNumber],
              let highs = daily["temperature_2m_max"] as? [Double],
              let lows = daily["temperature_2m_min"] as? [Double]
        else { return [] }

        let dateParser = DateFormatter()
        dateParser.dateFormat = "yyyy-MM-dd"
        let weekdayFormatter = DateFormatter()
        weekdayFormatter.dateFormat = "EEEE"

        var rows: [ResultItem] = []
        let days = min(4, dates.count, codeNumbers.count, highs.count, lows.count)
        for i in stride(from: 1, to: days, by: 1) { // tolerates short/empty arrays
            let (emoji, description) = describe(codeNumbers[i].intValue)
            let weekday = dateParser.date(from: dates[i]).map(weekdayFormatter.string(from:)) ?? dates[i]
            rows.append(ResultItem(
                icon: nil,
                title: "\(emoji) \(weekday) — \(description)",
                subtitle: "\(Int(lows[i].rounded()))° to \(Int(highs[i].rounded()))°",
                action: nil
            ))
        }
        return rows
    }

    /// WMO weather interpretation codes.
    private static func describe(_ code: Int) -> (String, String) {
        switch code {
        case 0: return ("☀️", "Clear")
        case 1: return ("🌤", "Mostly clear")
        case 2: return ("⛅️", "Partly cloudy")
        case 3: return ("☁️", "Overcast")
        case 45, 48: return ("🌫", "Fog")
        case 51...57: return ("🌦", "Drizzle")
        case 61...67: return ("🌧", "Rain")
        case 71...77: return ("🌨", "Snow")
        case 80...82: return ("🌧", "Rain showers")
        case 85, 86: return ("🌨", "Snow showers")
        case 95...99: return ("⛈", "Thunderstorm")
        default: return ("🌡", "Weather")
        }
    }
}
