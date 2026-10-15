import Foundation

enum Calculator {
    /// Returns a formatted result string if the input is a valid arithmetic
    /// expression, nil otherwise. Supports + - * / % ^ and parentheses.
    static func evaluate(_ input: String) -> String? {
        let allowed = CharacterSet(charactersIn: "0123456789.,+-*/()^% ")
        guard input.rangeOfCharacter(from: allowed.inverted) == nil else { return nil }
        guard input.rangeOfCharacter(from: .decimalDigits) != nil else { return nil }
        // Require an operator so bare numbers don't produce a pointless result.
        guard input.rangeOfCharacter(from: CharacterSet(charactersIn: "+-*/^%(")) != nil else { return nil }
        // The parser recurses per "(" / unary "-"; cap input so a pasted
        // wall of parentheses can't overflow the stack.
        guard input.count <= 1000 else { return nil }

        var parser = Parser(normalizeDecimals(input))
        guard let value = parser.parse(), value.isFinite else { return nil }
        return format(value)
    }

    /// Commas → decimal points, except in "." locales where a comma followed
    /// by exactly three digits is a thousands separator ("1,000" = 1000).
    static func normalizeDecimals(_ input: String) -> String {
        var text = input
        if Locale.current.decimalSeparator != "," {
            text = text.replacingOccurrences(of: #"(?<=\d),(?=\d{3}(?!\d))"#, with: "",
                                             options: .regularExpression)
        }
        return text.replacingOccurrences(of: ",", with: ".")
    }

    static func format(_ value: Double) -> String {
        if value == value.rounded() && abs(value) < 1e15 {
            return String(Int64(value))
        }
        return String(format: "%.10g", value)
    }
}

private struct Parser {
    private let chars: [Character]
    private var pos = 0

    init(_ input: String) {
        chars = Array(input.filter { $0 != " " })
    }

    mutating func parse() -> Double? {
        guard let value = expression(), pos == chars.count else { return nil }
        return value
    }

    // expression := term (('+' | '-') term)*
    private mutating func expression() -> Double? {
        guard var lhs = term() else { return nil }
        while let op = peek(), op == "+" || op == "-" {
            pos += 1
            guard let rhs = term() else { return nil }
            lhs = op == "+" ? lhs + rhs : lhs - rhs
        }
        return lhs
    }

    // term := factor (('*' | '/' | '%') factor)*
    private mutating func term() -> Double? {
        guard var lhs = factor() else { return nil }
        while let op = peek(), op == "*" || op == "/" || op == "%" {
            pos += 1
            guard let rhs = factor() else { return nil }
            switch op {
            case "*": lhs *= rhs
            case "/": lhs /= rhs
            default: lhs = lhs.truncatingRemainder(dividingBy: rhs)
            }
        }
        return lhs
    }

    // factor := '-' factor | primary ('^' factor)?
    private mutating func factor() -> Double? {
        if peek() == "-" {
            pos += 1
            guard let value = factor() else { return nil }
            return -value
        }
        guard let base = primary() else { return nil }
        if peek() == "^" {
            pos += 1
            guard let exponent = factor() else { return nil }
            return pow(base, exponent)
        }
        return base
    }

    // primary := number | '(' expression ')'
    private mutating func primary() -> Double? {
        if peek() == "(" {
            pos += 1
            guard let value = expression(), peek() == ")" else { return nil }
            pos += 1
            return value
        }
        return number()
    }

    private mutating func number() -> Double? {
        let start = pos
        while let c = peek(), c.isNumber || c == "." { pos += 1 }
        guard pos > start else { return nil }
        return Double(String(chars[start..<pos]))
    }

    private func peek() -> Character? {
        pos < chars.count ? chars[pos] : nil
    }
}
