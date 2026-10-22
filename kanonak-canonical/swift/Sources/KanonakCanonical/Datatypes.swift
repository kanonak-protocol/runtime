// Carriers + per-carrier canonical lexical forms (canonicalFormVersion "1").
// An independent conformant Swift port of kanonak.org/canonical-form, verified
// byte-for-byte against the golden vectors. IEEE lexicals follow the
// TypeScript reference's ECMAScript Number::toString semantics exactly.

import Foundation

public let canonicalFormVersion = "1"

public struct CanonicalError: Error, CustomStringConvertible {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var description: String { message }
}

// ---------------------------------------------------------------------------
// Carriers
// ---------------------------------------------------------------------------

public enum Carrier: String {
    case integer, decimal, double, float, boolean, string
    case anyURI, langString, dateTime, date, time, hexBinary, base64Binary
}

private let xsdCarrier: [String: Carrier] = [
    "integer": .integer, "long": .integer, "int": .integer, "short": .integer,
    "byte": .integer, "unsignedLong": .integer, "unsignedInt": .integer,
    "unsignedShort": .integer, "unsignedByte": .integer,
    "nonNegativeInteger": .integer, "positiveInteger": .integer,
    "nonPositiveInteger": .integer, "negativeInteger": .integer,
    "decimal": .decimal, "double": .double, "float": .float,
    "boolean": .boolean, "string": .string, "normalizedString": .string,
    "token": .string, "anyURI": .anyURI, "dateTime": .dateTime,
    "date": .date, "time": .time, "hexBinary": .hexBinary,
    "base64Binary": .base64Binary,
]


/// Carrier for a datatype URI, or nil (out-of-set → raw-token tier).
public func carrierOf(_ datatypeUri: String) -> Carrier? {
    // Routing key comes from the total `lenientVersionlessKey`, not the throwing
    // parser (pinned by the lenientKeyVectors in coordinate-vectors.json): a
    // structurally unparseable datatype routes to no carrier — raw token
    // preserved, never guessed.
    guard let key = lenientVersionlessKey(datatypeUri) else { return nil }
    if key == "kanonak.org/core-rdf/langString" { return .langString }
    let prefix = "kanonak.org/core-xsd/"
    guard key.hasPrefix(prefix) else { return nil }
    return xsdCarrier[String(key.dropFirst(prefix.count))]
}

// ---------------------------------------------------------------------------
// Per-carrier canonical lexical forms
// ---------------------------------------------------------------------------

private let integerRe = try! Regex("^[+-]?[0-9]+$")
private let decimalRe = try! Regex("^([+-]?)([0-9]*)(?:\\.([0-9]*))?$")
private let ieeeRe = try! Regex("^[+-]?([0-9]+\\.?[0-9]*|\\.[0-9]+)([eE][+-]?[0-9]+)?$")
private let hexRe = try! Regex("^([0-9A-Fa-f]{2})*$")
private let base64Re = try! Regex("^[A-Za-z0-9+/]*={0,2}$")
private let dateTimeRe = try! Regex(
    "^(-?[0-9]{4,})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})(\\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})?$")
private let dateRe = try! Regex("^(-?[0-9]{4,})-([0-9]{2})-([0-9]{2})(Z|[+-][0-9]{2}:[0-9]{2})?$")
private let timeRe = try! Regex("^([0-9]{2}):([0-9]{2}):([0-9]{2})(\\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})?$")

private func wholeMatch(_ re: Regex<AnyRegexOutput>, _ s: String) -> [String?]? {
    guard let m = try? re.wholeMatch(in: s) else { return nil }
    return m.output.map { $0.substring.map(String.init) }
}

private func matches(_ re: Regex<AnyRegexOutput>, _ s: String) -> Bool {
    (try? re.wholeMatch(in: s)) != nil
}

private func trimmed(_ raw: String) -> String {
    raw.trimmingCharacters(in: .whitespacesAndNewlines)
}

public func canonicalInteger(_ raw: String) throws -> String {
    let t = trimmed(raw)
    guard matches(integerRe, t) else {
        throw CanonicalError("canonicalInteger: '\(raw)' invalid")
    }
    var sign = "", digits = t
    if t.hasPrefix("-") { sign = "-"; digits = String(t.dropFirst()) }
    else if t.hasPrefix("+") { digits = String(t.dropFirst()) }
    var stripped = String(digits.drop(while: { $0 == "0" }))
    if stripped.isEmpty { stripped = "0" }
    if stripped == "0" { return "0" }
    return sign + stripped
}

public func canonicalDecimal(_ raw: String) throws -> String {
    let t = trimmed(raw)
    guard let m = wholeMatch(decimalRe, t) else {
        throw CanonicalError("canonicalDecimal: '\(raw)' invalid")
    }
    let intRaw = m[2] ?? ""
    let fracRaw = m[3] ?? ""
    if intRaw.isEmpty && fracRaw.isEmpty {
        throw CanonicalError("canonicalDecimal: '\(raw)' invalid")
    }
    let sign = (m[1] == "-") ? "-" : ""
    var intPart = String(intRaw.drop(while: { $0 == "0" }))
    if intPart.isEmpty { intPart = "0" }
    var fracPart = fracRaw
    while fracPart.hasSuffix("0") { fracPart.removeLast() }
    let magnitude = fracPart.isEmpty ? intPart : "\(intPart).\(fracPart)"
    if magnitude == "0" { return "0" }
    return sign + magnitude
}

/// The shortest round-trip decimal token of a finite double, rendered with
/// ECMAScript `Number::toString` notation rules — the formatting the
/// TypeScript reference (`String(n)`) produces. Public because the codec's
/// scalar-lexical bridge needs the same token for programmatic Double values.
public func ecmaScriptDoubleToken(_ d: Double) -> String {
    if d == 0 { return "0" } // covers -0: the canonical form of a zero is "0"
    // Swift's description is the shortest round-trip decimal; re-notate it
    // per ECMAScript rules (plain while -6 < n <= 21, else exponential).
    var s = "\(d)"
    var sign = ""
    if s.hasPrefix("-") { sign = "-"; s.removeFirst() }
    var mantissa = s, exp = 0
    if let e = s.firstIndex(where: { $0 == "e" || $0 == "E" }) {
        mantissa = String(s[..<e])
        exp = Int(s[s.index(after: e)...]) ?? 0
    }
    var intPart = mantissa, fracPart = ""
    if let dot = mantissa.firstIndex(of: ".") {
        intPart = String(mantissa[..<dot])
        fracPart = String(mantissa[mantissa.index(after: dot)...])
    }
    let combined = intPart + fracPart
    let lead = combined.prefix(while: { $0 == "0" }).count
    var digits = String(combined.dropFirst(lead))
    // n: the decimal exponent with value = 0.digits × 10^n.
    let n = intPart.count - lead + exp
    while digits.hasSuffix("0") { digits.removeLast() }
    let k = digits.count
    let body: String
    if k <= n && n <= 21 {
        body = digits + String(repeating: "0", count: n - k)
    } else if 0 < n && n <= 21 {
        let cut = digits.index(digits.startIndex, offsetBy: n)
        body = "\(digits[..<cut]).\(digits[cut...])"
    } else if -6 < n && n <= 0 {
        body = "0." + String(repeating: "0", count: -n) + digits
    } else {
        let head = k == 1 ? digits : "\(digits.first!).\(digits.dropFirst())"
        let e = n - 1
        body = "\(head)e\(e >= 0 ? "+" : "-")\(abs(e))"
    }
    return sign + body
}

private func canonicalIeee(_ raw: String, single: Bool, label: String) throws -> String {
    let t = trimmed(raw)
    if t == "NaN" { return "NaN" }
    if t == "INF" { return "INF" }
    if t == "-INF" { return "-INF" }
    guard matches(ieeeRe, t), var n = Double(t) else {
        throw CanonicalError("canonical\(label): '\(raw)' invalid")
    }
    if single { n = Double(Float(n)) }
    guard n.isFinite else {
        throw CanonicalError("canonical\(label): '\(raw)' out of the finite range")
    }
    return ecmaScriptDoubleToken(n)
}

public func canonicalDouble(_ raw: String) throws -> String {
    try canonicalIeee(raw, single: false, label: "Double")
}

public func canonicalFloat(_ raw: String) throws -> String {
    try canonicalIeee(raw, single: true, label: "Float")
}

public func canonicalBoolean(_ raw: String) throws -> String {
    switch trimmed(raw) {
    case "true", "1": return "true"
    case "false", "0": return "false"
    default: throw CanonicalError("canonicalBoolean: '\(raw)' invalid")
    }
}

public func canonicalString(_ raw: String) -> String {
    raw.precomposedStringWithCanonicalMapping
}

public func canonicalLanguageTag(_ tag: String) throws -> String {
    let subs = trimmed(tag).split(separator: "-", omittingEmptySubsequences: false).map(String.init)
    guard let first = subs.first, !first.isEmpty else {
        throw CanonicalError("canonicalLanguageTag: '\(tag)' invalid")
    }
    var out: [String] = []
    for (i, sub) in subs.enumerated() {
        if i == 0 {
            out.append(sub.lowercased())
        } else if sub.count == 4 && isAlpha(sub) {
            out.append(sub.prefix(1).uppercased() + sub.dropFirst().lowercased())
        } else if sub.count == 2 && isAlpha(sub) {
            out.append(sub.uppercased())
        } else {
            out.append(sub.lowercased())
        }
    }
    return out.joined(separator: "-")
}

private func isAlpha(_ s: String) -> Bool {
    !s.isEmpty && s.allSatisfy { ("A"..."Z").contains($0) || ("a"..."z").contains($0) }
}

public func canonicalHexBinary(_ raw: String) throws -> String {
    let t = trimmed(raw)
    guard matches(hexRe, t) else {
        throw CanonicalError("canonicalHexBinary: '\(raw)' invalid")
    }
    return t.uppercased()
}

public func canonicalBase64(_ raw: String) throws -> String {
    let stripped = String(raw.unicodeScalars.filter { !CharacterSet.whitespacesAndNewlines.contains($0) })
    guard matches(base64Re, stripped), stripped.count % 4 == 0,
          let bytes = Data(base64Encoded: stripped) else {
        throw CanonicalError("canonicalBase64: '\(raw)' invalid")
    }
    return bytes.base64EncodedString()
}

// -- temporal: XSD 1.1's rules, on the proleptic Gregorian calendar with a year 0 ---------
// A year of any length is kept as its sign and digits, so a UTC shift across a year boundary has no range limit
// (runtime#36). A leap year needs only the year's remainder by 400, and a shift moves the year by one at most.

/// A year of any length: its sign and its digits, without leading zeros. Year 0 is "0", with no sign.
private struct Year {
    let negative: Bool
    let digits: String

    init(negative: Bool, digits: String) {
        self.negative = negative && digits != "0"
        self.digits = digits
    }

    init(parsing raw: String) {
        let unsigned = raw.hasPrefix("-") ? String(raw.dropFirst()) : raw
        let significant = String(unsigned.drop(while: { $0 == "0" }))
        self.init(negative: raw.hasPrefix("-"), digits: significant.isEmpty ? "0" : significant)
    }

    /// The year's remainder by 400, never negative.
    var remainderBy400: Int {
        let r = digits.utf8.reduce(0) { ($0 * 10 + Int($1 - UInt8(ascii: "0"))) % 400 }
        return negative && r != 0 ? 400 - r : r
    }

    var isLeap: Bool {
        let r = remainderBy400
        return r % 4 == 0 && (r % 100 != 0 || r == 0)
    }

    /// At least four digits wide, its sign kept.
    var text: String {
        let padded = digits.count < 4 ? String(repeating: "0", count: 4 - digits.count) + digits : digits
        return (negative ? "-" : "") + padded
    }

    var next: Year {
        negative ? Year(negative: true, digits: decrementDigits(digits)) : Year(negative: false, digits: incrementDigits(digits))
    }

    var previous: Year {
        (negative || digits == "0")
            ? Year(negative: true, digits: incrementDigits(digits))
            : Year(negative: false, digits: decrementDigits(digits))
    }
}

private func incrementDigits(_ digits: String) -> String {
    var bytes = Array(digits.utf8)
    var i = bytes.count - 1
    while i >= 0 {
        if bytes[i] == UInt8(ascii: "9") {
            bytes[i] = UInt8(ascii: "0")
            i -= 1
        } else {
            bytes[i] += 1
            return String(decoding: bytes, as: UTF8.self)
        }
    }
    return "1" + String(decoding: bytes, as: UTF8.self)
}

/// One less than a positive number written in digits.
private func decrementDigits(_ digits: String) -> String {
    var bytes = Array(digits.utf8)
    var i = bytes.count - 1
    while i >= 0 {
        if bytes[i] == UInt8(ascii: "0") {
            bytes[i] = UInt8(ascii: "9")
            i -= 1
        } else {
            bytes[i] -= 1
            break
        }
    }
    let text = String(String(decoding: bytes, as: UTF8.self).drop(while: { $0 == "0" }))
    return text.isEmpty ? "0" : text
}

private let daysInMonthTable = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]

private func daysInMonth(_ year: Year, _ month: Int) -> Int {
    month == 2 && year.isLeap ? 29 : daysInMonthTable[month - 1]
}

private struct Civil {
    var year: Year
    var month: Int
    var day: Int
}

private func twoDigits<S: StringProtocol>(_ field: S) -> Int {
    guard let n = Int(field) else { preconditionFailure("the pattern matched two digits, not '\(field)'") }
    return n
}

private func civilOf(_ yy: String, _ mo: String, _ dd: String, _ what: String, _ raw: String) throws -> Civil {
    let year = Year(parsing: yy)
    let month = twoDigits(mo), day = twoDigits(dd)
    guard (1...12).contains(month) else {
        throw CanonicalError("\(what): '\(raw)' has month \(mo); a month is 01 to 12")
    }
    guard day >= 1 && day <= daysInMonth(year, month) else {
        throw CanonicalError("\(what): '\(raw)' has day \(dd), which month \(mo) of year \(year.text) doesn't have")
    }
    return Civil(year: year, month: month, day: day)
}

private func nextDay(_ c: Civil) -> Civil {
    if c.day < daysInMonth(c.year, c.month) { return Civil(year: c.year, month: c.month, day: c.day + 1) }
    if c.month < 12 { return Civil(year: c.year, month: c.month + 1, day: 1) }
    return Civil(year: c.year.next, month: 1, day: 1)
}

private func previousDay(_ c: Civil) -> Civil {
    if c.day > 1 { return Civil(year: c.year, month: c.month, day: c.day - 1) }
    if c.month > 1 { return Civil(year: c.year, month: c.month - 1, day: daysInMonth(c.year, c.month - 1)) }
    return Civil(year: c.year.previous, month: 12, day: 31)
}

/// Minutes east of UTC, and 0 with no offset. XSD 1.1 bounds an offset at 14:00.
private func offsetMinutes(_ tz: String?, _ what: String, _ raw: String) throws -> Int {
    guard let tz, !tz.isEmpty, tz != "Z" else { return 0 }
    let hours = twoDigits(tz.dropFirst().prefix(2)), minutes = twoDigits(tz.suffix(2))
    guard minutes <= 59 && hours <= 14 && !(hours == 14 && minutes > 0) else {
        throw CanonicalError("\(what): '\(raw)' has offset \(tz); an offset is at most 14:00 either way")
    }
    return (tz.hasPrefix("-") ? -1 : 1) * (hours * 60 + minutes)
}

/// XSD 1.1: no 60th second, and 24:00:00 only as the first instant of the next day.
private func clockOf(
    _ hh: String, _ mi: String, _ ss: String, _ fraction: String, _ what: String, _ raw: String
) throws -> (hour: Int, minute: Int) {
    let hour = twoDigits(hh), minute = twoDigits(mi), second = twoDigits(ss)
    let endOfDay = hour == 24 && minute == 0 && second == 0 && fraction.isEmpty
    guard minute <= 59 && second <= 59 && (hour <= 23 || endOfDay) else {
        throw CanonicalError(
            "\(what): '\(raw)' has time \(hh):\(mi):\(ss)\(fraction); XSD 1.1 allows 00:00:00 to 23:59:59, and 24:00:00 exactly")
    }
    return (hour, minute)
}

private func canonicalFraction(_ frac: String?) -> String {
    guard var t = frac, !t.isEmpty else { return "" }
    if t.hasPrefix(".") { t.removeFirst() }
    while t.hasSuffix("0") { t.removeLast() }
    return t.isEmpty ? "" : ".\(t)"
}

private func canonicalTz(_ tz: String?) -> String {
    guard let tz, !tz.isEmpty else { return "" }
    if tz == "Z" || tz == "+00:00" || tz == "-00:00" { return "Z" }
    return tz
}

private func pad2(_ n: Int) -> String { n < 10 ? "0\(n)" : "\(n)" }

public func canonicalDateTime(_ raw: String) throws -> String {
    let what = "canonicalDateTime"
    guard let m = wholeMatch(dateTimeRe, trimmed(raw)) else {
        throw CanonicalError("\(what): '\(raw)' invalid")
    }
    let fraction = canonicalFraction(m[7])
    let tz = m[8]
    var civil = try civilOf(m[1]!, m[2]!, m[3]!, what, raw)
    let clock = try clockOf(m[4]!, m[5]!, m[6]!, fraction, what, raw)
    let offset = try offsetMinutes(tz, what, raw)
    var minutes = clock.hour * 60 + clock.minute - offset
    if minutes < 0 {
        minutes += 1440
        civil = previousDay(civil)
    } else if minutes >= 1440 {
        minutes -= 1440
        civil = nextDay(civil)
    }
    let zone = (tz ?? "").isEmpty ? "" : "Z"
    return "\(civil.year.text)-\(pad2(civil.month))-\(pad2(civil.day))"
        + "T\(pad2(minutes / 60)):\(pad2(minutes % 60)):\(m[6]!)\(fraction)\(zone)"
}

public func canonicalDate(_ raw: String) throws -> String {
    let what = "canonicalDate"
    guard let m = wholeMatch(dateRe, trimmed(raw)) else {
        throw CanonicalError("\(what): '\(raw)' invalid")
    }
    let civil = try civilOf(m[1]!, m[2]!, m[3]!, what, raw)
    _ = try offsetMinutes(m[4], what, raw)
    return "\(civil.year.text)-\(m[2]!)-\(m[3]!)\(canonicalTz(m[4]))"
}

public func canonicalTime(_ raw: String) throws -> String {
    let what = "canonicalTime"
    guard let m = wholeMatch(timeRe, trimmed(raw)) else {
        throw CanonicalError("\(what): '\(raw)' invalid")
    }
    let fraction = canonicalFraction(m[4])
    let clock = try clockOf(m[1]!, m[2]!, m[3]!, fraction, what, raw)
    _ = try offsetMinutes(m[5], what, raw)
    return "\(clock.hour == 24 ? "00" : m[1]!):\(m[2]!):\(m[3]!)\(fraction)\(canonicalTz(m[5]))"
}

public func canonicalScalarLexical(_ carrier: Carrier, _ raw: String) throws -> String {
    switch carrier {
    case .integer: return try canonicalInteger(raw)
    case .decimal: return try canonicalDecimal(raw)
    case .double: return try canonicalDouble(raw)
    case .float: return try canonicalFloat(raw)
    case .boolean: return try canonicalBoolean(raw)
    case .string, .anyURI: return canonicalString(raw)
    case .langString: throw CanonicalError("canonicalLangString: a language string is a structured value, an embedded { lexicalForm, language }, so its tag is hashed with it; the langString carrier has no slot for the tag and refuses the scalar '\(raw)'")
    case .dateTime: return try canonicalDateTime(raw)
    case .date: return try canonicalDate(raw)
    case .time: return try canonicalTime(raw)
    case .hexBinary: return try canonicalHexBinary(raw)
    case .base64Binary: return try canonicalBase64(raw)
    }
}
