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

/// `publisher/package/name` carrier key from a datatype URI.
private func carrierKey(_ uri: String) -> String {
    guard let idx = uri.lastIndex(of: "/") else { return uri }
    let name = String(uri[uri.index(after: idx)...])
    let head = String(uri[..<idx])
    guard let slash = head.firstIndex(of: "/") else { return uri }
    let publisher = String(head[..<slash])
    var pkg = String(head[head.index(after: slash)...])
    if let at = pkg.firstIndex(of: "@") { pkg = String(pkg[..<at]) }
    return "\(publisher)/\(pkg)/\(name)"
}

/// Carrier for a datatype URI, or nil (out-of-set → raw-token tier).
public func carrierOf(_ datatypeUri: String) -> Carrier? {
    let key = carrierKey(datatypeUri)
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

// -- temporal ---------------------------------------------------------------

private func canonicalYear(_ raw: String) -> String {
    let neg = raw.hasPrefix("-")
    var digits = String((neg ? String(raw.dropFirst()) : raw).drop(while: { $0 == "0" }))
    if digits.isEmpty { digits = "0" }
    if digits.count < 4 { digits = String(repeating: "0", count: 4 - digits.count) + digits }
    return (neg ? "-" : "") + digits
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

private func tzOffsetMinutes(_ tz: String) -> Int {
    if tz == "Z" { return 0 }
    let sign = tz.hasPrefix("-") ? -1 : 1
    let hh = Int(tz.dropFirst().prefix(2)) ?? 0
    let mm = Int(tz.suffix(2)) ?? 0
    return sign * (hh * 60 + mm)
}

// Proleptic-Gregorian civil-date arithmetic (Howard Hinnant's algorithms) —
// deterministic and free of any calendar/timezone library, and valid across
// negative years.
private func daysFromCivil(_ y: Int, _ m: Int, _ d: Int) -> Int {
    let yy = y - (m <= 2 ? 1 : 0)
    let era = (yy >= 0 ? yy : yy - 399) / 400
    let yoe = yy - era * 400
    let doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
    return era * 146097 + doe - 719468
}

private func civilFromDays(_ z0: Int) -> (y: Int, m: Int, d: Int) {
    let z = z0 + 719468
    let era = (z >= 0 ? z : z - 146096) / 146097
    let doe = z - era * 146097
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365
    let y = yoe + era * 400
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
    let mp = (5 * doy + 2) / 153
    let d = doy - (153 * mp + 2) / 5 + 1
    let m = mp + (mp < 10 ? 3 : -9)
    return (y + (m <= 2 ? 1 : 0), m, d)
}

private func floorDiv(_ a: Int, _ b: Int) -> Int {
    let q = a / b
    return (a % b != 0 && (a < 0) != (b < 0)) ? q - 1 : q
}

private func pad2(_ n: Int) -> String { n < 10 ? "0\(n)" : "\(n)" }

public func canonicalDateTime(_ raw: String) throws -> String {
    guard let m = wholeMatch(dateTimeRe, trimmed(raw)) else {
        throw CanonicalError("canonicalDateTime: '\(raw)' invalid")
    }
    let fraction = canonicalFraction(m[7])
    guard let tz = m[8], !tz.isEmpty else {
        return "\(canonicalYear(m[1]!))-\(m[2]!)-\(m[3]!)T\(m[4]!):\(m[5]!):\(m[6]!)\(fraction)"
    }
    let y = Int(m[1]!)!, mo = Int(m[2]!)!, dd = Int(m[3]!)!
    let hh = Int(m[4]!)!, mi = Int(m[5]!)!
    let total = (daysFromCivil(y, mo, dd) * 24 + hh) * 60 + mi - tzOffsetMinutes(tz)
    let day = floorDiv(total, 1440)
    let rem = total - day * 1440
    let (cy, cm, cd) = civilFromDays(day)
    return "\(canonicalYear(String(cy)))-\(pad2(cm))-\(pad2(cd))"
        + "T\(pad2(rem / 60)):\(pad2(rem % 60)):\(m[6]!)\(fraction)Z"
}

public func canonicalDate(_ raw: String) throws -> String {
    guard let m = wholeMatch(dateRe, trimmed(raw)) else {
        throw CanonicalError("canonicalDate: '\(raw)' invalid")
    }
    return "\(canonicalYear(m[1]!))-\(m[2]!)-\(m[3]!)\(canonicalTz(m[4]))"
}

public func canonicalTime(_ raw: String) throws -> String {
    guard let m = wholeMatch(timeRe, trimmed(raw)) else {
        throw CanonicalError("canonicalTime: '\(raw)' invalid")
    }
    var hh = m[1]!
    let fraction = canonicalFraction(m[4])
    if hh == "24" && m[2]! == "00" && m[3]! == "00" && fraction.isEmpty { hh = "00" }
    return "\(hh):\(m[2]!):\(m[3]!)\(fraction)\(canonicalTz(m[5]))"
}

public func canonicalScalarLexical(_ carrier: Carrier, _ raw: String) throws -> String {
    switch carrier {
    case .integer: return try canonicalInteger(raw)
    case .decimal: return try canonicalDecimal(raw)
    case .double: return try canonicalDouble(raw)
    case .float: return try canonicalFloat(raw)
    case .boolean: return try canonicalBoolean(raw)
    case .string, .anyURI, .langString: return canonicalString(raw)
    case .dateTime: return try canonicalDateTime(raw)
    case .date: return try canonicalDate(raw)
    case .time: return try canonicalTime(raw)
    case .hexBinary: return try canonicalHexBinary(raw)
    case .base64Binary: return try canonicalBase64(raw)
    }
}
