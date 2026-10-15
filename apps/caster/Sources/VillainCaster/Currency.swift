import Foundation

struct CurrencyQuery {
    let amount: Double
    let from: String
    let to: String

    /// Matches inputs like "32 sek to eur", "1500sek in usd",
    /// "5 euro to dollar", "10 £ in yen".
    static func parse(_ text: String) -> CurrencyQuery? {
        let pattern = #"^([0-9]+(?:[.,][0-9]+)?)\s*(.+?)\s+(?:to|in)\s+(.+?)$"#
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]),
              let match = regex.firstMatch(in: text, range: NSRange(text.startIndex..., in: text))
        else { return nil }

        func group(_ i: Int) -> String {
            String(text[Range(match.range(at: i), in: text)!])
        }
        guard let amount = Double(Calculator.normalizeDecimals(group(1))),
              let from = Currency.normalize(group(2)),
              let to = Currency.normalize(group(3))
        else { return nil }
        return CurrencyQuery(amount: amount, from: from, to: to)
    }
}

enum Currency {
    /// Common names, plurals and symbols → ISO 4217 code.
    private static let aliases: [String: String] = [
        // Euro
        "euro": "EUR", "euros": "EUR", "€": "EUR",
        // US dollar
        "dollar": "USD", "dollars": "USD", "$": "USD", "buck": "USD", "bucks": "USD",
        "us dollar": "USD", "us dollars": "USD",
        // Scandinavian (kr defaults to SEK)
        "kr": "SEK", "krona": "SEK", "kronor": "SEK",
        "swedish krona": "SEK", "swedish kronor": "SEK",
        "krone": "NOK", "kroner": "NOK", "norwegian krone": "NOK",
        "danish krone": "DKK", "danish kroner": "DKK",
        // Pound
        "pound": "GBP", "pounds": "GBP", "quid": "GBP", "sterling": "GBP", "£": "GBP",
        "british pound": "GBP", "british pounds": "GBP",
        // Yen / yuan / won
        "yen": "JPY", "¥": "JPY",
        "yuan": "CNY", "rmb": "CNY", "renminbi": "CNY",
        "won": "KRW", "₩": "KRW",
        // Franc
        "franc": "CHF", "francs": "CHF", "swiss franc": "CHF", "swiss francs": "CHF",
        // Ruble
        "ruble": "RUB", "rubles": "RUB", "rouble": "RUB", "roubles": "RUB", "₽": "RUB",
        // Other Europe
        "zloty": "PLN", "złoty": "PLN", "koruna": "CZK", "forint": "HUF",
        "lira": "TRY", "leu": "RON", "lei": "RON", "lev": "BGN", "hryvnia": "UAH", "₴": "UAH",
        // Rest of world
        "rupee": "INR", "rupees": "INR", "₹": "INR",
        "rand": "ZAR", "real": "BRL", "reais": "BRL",
        "peso": "MXN", "pesos": "MXN", "baht": "THB",
        "shekel": "ILS", "shekels": "ILS", "₪": "ILS",
        "dirham": "AED", "riyal": "SAR",
        "australian dollar": "AUD", "aussie dollar": "AUD",
        "canadian dollar": "CAD", "new zealand dollar": "NZD",
        "hong kong dollar": "HKD", "singapore dollar": "SGD",
    ]

    /// Resolves a user-typed currency token to an ISO code: alias lookup
    /// first, then any bare 3-letter token passes through as a code.
    static func normalize(_ raw: String) -> String? {
        let token = raw.lowercased().trimmingCharacters(in: .whitespaces)
        if let code = aliases[token] { return code }
        if token.count == 3, token.allSatisfy({ $0.isLetter }) {
            return token.uppercased()
        }
        return nil
    }

    /// Converts via the Frankfurter API (ECB reference rates, no API key).
    /// The ECB only covers ~30 currencies (and dropped e.g. RUB in 2022),
    /// so on a miss it falls back to open.er-api.com (160+ currencies).
    /// Calls back with an inline display string and the value ⏎ should copy.
    static func convert(_ query: CurrencyQuery,
                        completion: @escaping (_ display: String, _ copyValue: String?) -> Void) {
        guard query.from != query.to else {
            deliver(query.amount, query, completion)
            return
        }
        let url = URL(string: "https://api.frankfurter.dev/v1/latest?amount=\(query.amount)&base=\(query.from)&symbols=\(query.to)")!
        URLSession.shared.dataTask(with: url) { data, _, _ in
            if let data,
               let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let rates = object["rates"] as? [String: Double],
               let value = rates[query.to] {
                deliver(value, query, completion)
            } else {
                convertViaFallback(query, completion: completion)
            }
        }.resume()
    }

    private static func convertViaFallback(_ query: CurrencyQuery,
                                           completion: @escaping (String, String?) -> Void) {
        let url = URL(string: "https://open.er-api.com/v6/latest/\(query.from)")!
        URLSession.shared.dataTask(with: url) { data, _, _ in
            guard let data,
                  let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let rates = object["rates"] as? [String: Double],
                  let rate = rates[query.to]
            else {
                completion("no rate", nil)
                return
            }
            deliver(query.amount * rate, query, completion)
        }.resume()
    }

    private static func deliver(_ value: Double, _ query: CurrencyQuery,
                                _ completion: (String, String?) -> Void) {
        let formatted = formatAmount(value)
        completion("= \(formatted) \(query.to)", formatted)
    }

    /// 2 decimals for normal amounts; keep 4 significant digits for tiny
    /// ones so "1 rub to eur" doesn't round 0.0095 up to 0.01.
    private static func formatAmount(_ value: Double) -> String {
        if abs(value) >= 1 {
            return Calculator.format((value * 100).rounded() / 100)
        }
        return String(format: "%.4g", value)
    }
}
