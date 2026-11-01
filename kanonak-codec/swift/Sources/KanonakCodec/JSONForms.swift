// The Codable binding of the JSON forms of integer and decimal values
// (runtime#54): what a generated struct types a field with, so JSONDecoder and
// JSONEncoder read and write the forms `deserialize` and `serialize` do. A wide
// integer and a decimal are written as a string of their digits. `int` and
// narrower stay Int32 and the rest, since Codable already refuses a string, a
// fraction or a value past their width.

import Foundation

/// One of `xsd:integer` and its four sign-restricted forms, whose range a
/// `KanonakInteger` checks.
public protocol IntegerDatatype {
    static var localName: String { get }
}

public enum XsdInteger: IntegerDatatype {
    public static var localName: String { "integer" }
}

public enum NonNegativeInteger: IntegerDatatype {
    public static var localName: String { "nonNegativeInteger" }
}

public enum PositiveInteger: IntegerDatatype {
    public static var localName: String { "positiveInteger" }
}

public enum NonPositiveInteger: IntegerDatatype {
    public static var localName: String { "nonPositiveInteger" }
}

public enum NegativeInteger: IntegerDatatype {
    public static var localName: String { "negativeInteger" }
}

/// A JSON value as the forms read it: a string, a boolean, or a number as its
/// digits. JSONDecoder gives no number's token, so it is read as a `Decimal`,
/// which holds 38 significant digits exactly, where a `Double` would round
/// `1.0000000000000000001` to a whole number. A number with more is read as
/// `Decimal` rounds it; `deserialize` judges the token itself.
private func jsonFormValue(_ decoder: Decoder, _ localName: String) throws -> Any {
    let container = try decoder.singleValueContainer()
    if let text = try? container.decode(String.self) { return text }
    if let flag = try? container.decode(Bool.self) { return flag }
    if let number = try? container.decode(Decimal.self) { return JSONNumber(number.description) }
    throw DecodingError.dataCorrupted(DecodingError.Context(
        codingPath: decoder.codingPath,
        debugDescription: "codec: xsd:\(localName): read from a JSON string or number; got neither, or a number past what Decimal holds [json-number-form]"))
}

/// The codec's refusal as JSONDecoder reports one, its message unchanged.
private func decodingRefusal(_ decoder: Decoder, _ error: Error) -> DecodingError {
    DecodingError.dataCorrupted(DecodingError.Context(
        codingPath: decoder.codingPath, debugDescription: "\(error)", underlyingError: error))
}

private func readForm(_ decoder: Decoder, _ localName: String) throws -> Any {
    let value = try jsonFormValue(decoder, localName)
    do { return try readJSONForm(localName, value) } catch { throw decodingRefusal(decoder, error) }
}

private func encodeDigits(_ digits: String, _ encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(digits)
}

/// An `xsd:integer`, or one of its sign-restricted forms, by `D`: its canonical
/// digits, any number of them, inside `D`'s range. JSON carries it as a string
/// of its digits, and it reads from a string or a whole number within
/// ±(2^53 − 1).
public struct KanonakInteger<D: IntegerDatatype>: Codable, Hashable, CustomStringConvertible {
    public let digits: String

    /// From a lexical form, such as `"+007"`, refused outside `D`'s range.
    public init(_ lexical: String) throws {
        digits = try readJSONForm(D.localName, lexical) as! String
    }

    public init<T: BinaryInteger>(_ value: T) throws {
        digits = try writeJSONForm(D.localName, value) as! String
    }

    public init(from decoder: Decoder) throws {
        digits = try readForm(decoder, D.localName) as! String
    }

    public func encode(to encoder: Encoder) throws { try encodeDigits(digits, encoder) }

    public var description: String { digits }
}

/// An `xsd:long`. JSON carries it as a string of its digits, and it reads from
/// a string or a whole number within ±(2^53 − 1).
public struct KanonakLong: Codable, Hashable, CustomStringConvertible {
    public let value: Int64

    public init(_ value: Int64) { self.value = value }

    /// From a lexical form, refused outside `long`'s range.
    public init(_ lexical: String) throws {
        value = try readJSONForm("long", lexical) as! Int64
    }

    public init(from decoder: Decoder) throws {
        value = try readForm(decoder, "long") as! Int64
    }

    public func encode(to encoder: Encoder) throws { try encodeDigits(String(value), encoder) }

    public var description: String { String(value) }
}

/// An `xsd:unsignedLong`. JSON carries it as a string of its digits, and it
/// reads from a string or a whole number up to 2^53 − 1.
public struct KanonakUnsignedLong: Codable, Hashable, CustomStringConvertible {
    public let value: UInt64

    public init(_ value: UInt64) { self.value = value }

    /// From a lexical form, refused outside `unsignedLong`'s range.
    public init(_ lexical: String) throws {
        value = try readJSONForm("unsignedLong", lexical) as! UInt64
    }

    public init(from decoder: Decoder) throws {
        value = try readForm(decoder, "unsignedLong") as! UInt64
    }

    public func encode(to encoder: Encoder) throws { try encodeDigits(String(value), encoder) }

    public var description: String { String(value) }
}

/// An `xsd:decimal`: its canonical digits, any number of them, which
/// Foundation's `Decimal` can't always hold. JSON carries it as a string of
/// its digits, and it reads from a string only, never a number, which may
/// already have been rounded.
public struct KanonakDecimal: Codable, Hashable, CustomStringConvertible {
    public let digits: String

    /// From a lexical form, such as `"1.50"`, which reads as `1.5`.
    public init(_ lexical: String) throws {
        digits = try readJSONForm("decimal", lexical) as! String
    }

    public init(from decoder: Decoder) throws {
        digits = try readForm(decoder, "decimal") as! String
    }

    public func encode(to encoder: Encoder) throws { try encodeDigits(digits, encoder) }

    public var description: String { digits }
}
