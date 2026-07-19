// A lexical-preserving JSON bridge for the node contract. The codec's node is
// a plain [String: Any]; numeric field values must survive as their original
// tokens (the input the canonical form normalizes), so numbers parse into
// JSONNumber — the Swift twin of Go's json.Number — never into a lossy Double.
// Foundation's JSONSerialization is deliberately not used: it widens numbers
// and represents booleans as NSNumber, whose Bool-vs-number disambiguation
// differs between Darwin and corelibs-foundation.

import Foundation
import KanonakCanonical

/// A JSON number carried as its original lexical token.
public struct JSONNumber: Equatable, Hashable, CustomStringConvertible {
    public let token: String
    public init(_ token: String) { self.token = token }
    public var description: String { token }
}

public struct JSONError: Error, CustomStringConvertible {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var description: String { message }
}

/// Parse JSON text into the node-contract value space: String, Bool,
/// JSONNumber, [Any], [String: Any], and NSNull for null.
public func parseJSON(_ data: Data) throws -> Any {
    var parser = JSONParser(bytes: [UInt8](data))
    let value = try parser.parseValue()
    parser.skipWhitespace()
    guard parser.atEnd else { throw JSONError("json: trailing characters after value") }
    return value
}

public func parseJSON(_ text: String) throws -> Any {
    try parseJSON(Data(text.utf8))
}

private struct JSONParser {
    let bytes: [UInt8]
    var i = 0

    var atEnd: Bool { i >= bytes.count }

    mutating func skipWhitespace() {
        while i < bytes.count, bytes[i] == 0x20 || bytes[i] == 0x09 || bytes[i] == 0x0A || bytes[i] == 0x0D {
            i += 1
        }
    }

    mutating func parseValue() throws -> Any {
        skipWhitespace()
        guard i < bytes.count else { throw JSONError("json: unexpected end of input") }
        switch bytes[i] {
        case UInt8(ascii: "{"): return try parseObject()
        case UInt8(ascii: "["): return try parseArray()
        case UInt8(ascii: "\""): return try parseString()
        case UInt8(ascii: "t"): try expect("true"); return true
        case UInt8(ascii: "f"): try expect("false"); return false
        case UInt8(ascii: "n"): try expect("null"); return NSNull()
        default: return try parseNumber()
        }
    }

    mutating func expect(_ literal: String) throws {
        for c in literal.utf8 {
            guard i < bytes.count, bytes[i] == c else {
                throw JSONError("json: invalid literal at offset \(i)")
            }
            i += 1
        }
    }

    mutating func parseObject() throws -> [String: Any] {
        i += 1 // {
        var out: [String: Any] = [:]
        skipWhitespace()
        if i < bytes.count, bytes[i] == UInt8(ascii: "}") { i += 1; return out }
        while true {
            skipWhitespace()
            guard i < bytes.count, bytes[i] == UInt8(ascii: "\"") else {
                throw JSONError("json: expected object key at offset \(i)")
            }
            let key = try parseString()
            skipWhitespace()
            guard i < bytes.count, bytes[i] == UInt8(ascii: ":") else {
                throw JSONError("json: expected ':' at offset \(i)")
            }
            i += 1
            out[key] = try parseValue()
            skipWhitespace()
            guard i < bytes.count else { throw JSONError("json: unterminated object") }
            if bytes[i] == UInt8(ascii: ",") { i += 1; continue }
            if bytes[i] == UInt8(ascii: "}") { i += 1; return out }
            throw JSONError("json: expected ',' or '}' at offset \(i)")
        }
    }

    mutating func parseArray() throws -> [Any] {
        i += 1 // [
        var out: [Any] = []
        skipWhitespace()
        if i < bytes.count, bytes[i] == UInt8(ascii: "]") { i += 1; return out }
        while true {
            out.append(try parseValue())
            skipWhitespace()
            guard i < bytes.count else { throw JSONError("json: unterminated array") }
            if bytes[i] == UInt8(ascii: ",") { i += 1; continue }
            if bytes[i] == UInt8(ascii: "]") { i += 1; return out }
            throw JSONError("json: expected ',' or ']' at offset \(i)")
        }
    }

    mutating func parseString() throws -> String {
        i += 1 // "
        var scalars = String.UnicodeScalarView()
        var buffer: [UInt8] = []
        func flush() throws {
            guard !buffer.isEmpty else { return }
            guard let s = String(bytes: buffer, encoding: .utf8) else {
                throw JSONError("json: invalid UTF-8 in string")
            }
            scalars.append(contentsOf: s.unicodeScalars)
            buffer.removeAll(keepingCapacity: true)
        }
        while i < bytes.count {
            let c = bytes[i]
            if c == UInt8(ascii: "\"") {
                i += 1
                try flush()
                return String(scalars)
            }
            if c == UInt8(ascii: "\\") {
                try flush()
                i += 1
                guard i < bytes.count else { throw JSONError("json: unterminated escape") }
                switch bytes[i] {
                case UInt8(ascii: "\""): scalars.append("\"")
                case UInt8(ascii: "\\"): scalars.append("\\")
                case UInt8(ascii: "/"): scalars.append("/")
                case UInt8(ascii: "b"): scalars.append("\u{08}")
                case UInt8(ascii: "f"): scalars.append("\u{0C}")
                case UInt8(ascii: "n"): scalars.append("\n")
                case UInt8(ascii: "r"): scalars.append("\r")
                case UInt8(ascii: "t"): scalars.append("\t")
                case UInt8(ascii: "u"):
                    let hi = try parseHex4()
                    if hi >= 0xD800 && hi <= 0xDBFF {
                        guard i + 1 < bytes.count, bytes[i + 1] == UInt8(ascii: "\\"),
                              i + 2 < bytes.count, bytes[i + 2] == UInt8(ascii: "u") else {
                            throw JSONError("json: unpaired surrogate")
                        }
                        i += 2
                        let lo = try parseHex4()
                        guard lo >= 0xDC00 && lo <= 0xDFFF else {
                            throw JSONError("json: invalid low surrogate")
                        }
                        let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                        scalars.append(Unicode.Scalar(cp)!)
                    } else if hi >= 0xDC00 && hi <= 0xDFFF {
                        throw JSONError("json: unpaired surrogate")
                    } else {
                        scalars.append(Unicode.Scalar(hi)!)
                    }
                default:
                    throw JSONError("json: invalid escape at offset \(i)")
                }
                i += 1
                continue
            }
            buffer.append(c)
            i += 1
        }
        throw JSONError("json: unterminated string")
    }

    mutating func parseHex4() throws -> UInt32 {
        var v: UInt32 = 0
        for _ in 0..<4 {
            i += 1
            guard i < bytes.count else { throw JSONError("json: unterminated \\u escape") }
            let c = bytes[i]
            let digit: UInt32
            switch c {
            case UInt8(ascii: "0")...UInt8(ascii: "9"): digit = UInt32(c - UInt8(ascii: "0"))
            case UInt8(ascii: "a")...UInt8(ascii: "f"): digit = UInt32(c - UInt8(ascii: "a")) + 10
            case UInt8(ascii: "A")...UInt8(ascii: "F"): digit = UInt32(c - UInt8(ascii: "A")) + 10
            default: throw JSONError("json: invalid \\u escape")
            }
            v = v << 4 | digit
        }
        return v
    }

    mutating func parseNumber() throws -> JSONNumber {
        let start = i
        if i < bytes.count, bytes[i] == UInt8(ascii: "-") { i += 1 }
        while i < bytes.count {
            let c = bytes[i]
            if (c >= UInt8(ascii: "0") && c <= UInt8(ascii: "9"))
                || c == UInt8(ascii: ".") || c == UInt8(ascii: "e")
                || c == UInt8(ascii: "E") || c == UInt8(ascii: "+") || c == UInt8(ascii: "-") {
                i += 1
            } else {
                break
            }
        }
        guard i > start, let token = String(bytes: bytes[start..<i], encoding: .utf8), token != "-" else {
            throw JSONError("json: invalid number at offset \(start)")
        }
        return JSONNumber(token)
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Render a node-contract value as compact JSON (object keys sorted for
/// determinism; JSONNumber tokens emitted verbatim).
public func writeJSON(_ value: Any) throws -> String {
    var out = ""
    try writeValue(&out, value)
    return out
}

private func writeValue(_ out: inout String, _ value: Any) throws {
    switch value {
    case let s as String:
        appendJSONString(&out, s)
    case let b as Bool:
        out.append(b ? "true" : "false")
    case let n as JSONNumber:
        out.append(n.token)
    case let n as Int:
        out.append(String(n))
    case let n as Int64:
        out.append(String(n))
    case let d as Double:
        out.append(ecmaScriptDoubleToken(d))
    case is NSNull:
        out.append("null")
    case let arr as [Any]:
        out.append("[")
        for (i, item) in arr.enumerated() {
            if i > 0 { out.append(",") }
            try writeValue(&out, item)
        }
        out.append("]")
    case let obj as [String: Any]:
        out.append("{")
        for (i, key) in obj.keys.sorted().enumerated() {
            if i > 0 { out.append(",") }
            appendJSONString(&out, key)
            out.append(":")
            try writeValue(&out, obj[key]!)
        }
        out.append("}")
    default:
        throw JSONError("json: unsupported value \(type(of: value))")
    }
}

private func appendJSONString(_ out: inout String, _ s: String) {
    out.append("\"")
    for c in s.unicodeScalars {
        switch c {
        case "\"": out.append("\\\"")
        case "\\": out.append("\\\\")
        case "\u{08}": out.append("\\b")
        case "\u{0C}": out.append("\\f")
        case "\n": out.append("\\n")
        case "\r": out.append("\\r")
        case "\t": out.append("\\t")
        default:
            if c.value < 0x20 {
                out.append(String(format: "\\u%04x", c.value))
            } else {
                out.unicodeScalars.append(c)
            }
        }
    }
    out.append("\"")
}
