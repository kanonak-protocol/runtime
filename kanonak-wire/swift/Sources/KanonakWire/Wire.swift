// `KanonakWire` — the Swift port of the Kanonak wire kernel
// (`kanonak.org/wire-form`, `wireFormatVersion "1"`).
//
// A minimal, allocation-conscious binary reader/writer for hot-path wire
// protocols. Generated protocol codecs call this kernel; it contains only what
// is invariant across ALL protocols — bounds-checked cursor reads/writes,
// big-endian integers, strict text validation, and a rich error taxonomy. It
// knows nothing about any particular protocol.
//
// Zero-copy contract: `bytes(_:)`, `rest()`, and the byte view behind `utf8(_:)`
// return `ArraySlice<UInt8>` views into the source buffer, never copies —
// Swift's natural byte-view type (the counterpart of Rust's slice, Go's
// subslice, and TypeScript's `Uint8Array` subarray).
//
// Fail-loud contract: every failure is a `WireError` stating what was expected,
// what was found, and where. No silent fallbacks — no optional-nil returns, no
// partial values, no lossy decodes.
//
// Two representability notes, which is why the corresponding shared vectors are
// SKIPPED (with a reported count) rather than passed by this port:
//
//   - Writer numeric parameters are exact-width (`UInt8`/`UInt16`/`UInt32`), so
//     the type IS the range validation and out-of-range/non-integer arguments
//     cannot be constructed. (`wide-numeric-params`, `dynamic-numeric` — the
//     same position Rust, Go, and C# take.)
//   - A Swift `String` is a sequence of `Unicode.Scalar`s, and `Unicode.Scalar`
//     cannot hold a surrogate — so the lone-surrogate encoder trap that JS,
//     Java, C#, and Python must guard against is unrepresentable here, and
//     `utf8(_:)` on the writer is infallible. (`utf16-strings`.)

public let wireFormatVersion = "1"

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The error taxonomy of `wireFormatVersion "1"`. Raw values are the kind names
/// as they appear in the shared vectors.
public enum WireErrorKind: String {
    case truncated = "Truncated"
    case lengthOverrun = "LengthOverrun"
    case trailingBytes = "TrailingBytes"
    case invalidUtf8 = "InvalidUtf8"
    case invalidUuid = "InvalidUuid"
    case valueOutOfRange = "ValueOutOfRange"
    case unknownTag = "UnknownTag"
}

/// A wire kernel error: what was expected, what was found, and where.
public struct WireError: Error, CustomStringConvertible {
    public let kind: WireErrorKind
    /// Absolute byte offset where the failing read started (read-side errors).
    public let offset: Int?
    public let message: String

    public var description: String { message }

    public static func truncated(needed: Int, remaining: Int, offset: Int, context: String) -> WireError {
        WireError(
            kind: .truncated,
            offset: offset,
            message: "Truncated: \(context) needs \(needed) byte(s) at offset \(offset), \(remaining) remain"
        )
    }

    public static func lengthOverrun(declared: Int, remaining: Int, offset: Int, context: String) -> WireError {
        WireError(
            kind: .lengthOverrun,
            offset: offset,
            message: "LengthOverrun: \(context) at offset \(offset) declares \(declared) byte(s), "
                + "\(remaining) remain after the length field"
        )
    }

    public static func trailingBytes(count: Int, offset: Int) -> WireError {
        WireError(
            kind: .trailingBytes,
            offset: offset,
            message: "TrailingBytes: expected end of buffer at offset \(offset), \(count) byte(s) remain"
        )
    }

    public static func invalidUtf8(offset: Int, context: String) -> WireError {
        WireError(kind: .invalidUtf8, offset: offset, message: "InvalidUtf8: \(context) at offset \(offset)")
    }

    public static func invalidUuid(context: String) -> WireError {
        WireError(kind: .invalidUuid, offset: nil, message: "InvalidUuid: \(context)")
    }

    public static func valueOutOfRange(value: Any, typeName: String) -> WireError {
        WireError(
            kind: .valueOutOfRange,
            offset: nil,
            message: "ValueOutOfRange: \(value) is not a valid \(typeName)"
        )
    }

    /// Constructor for generated union dispatch on an unrecognized tag byte.
    public static func unknownTag(tag: UInt8, context: String) -> WireError {
        WireError(
            kind: .unknownTag,
            offset: nil,
            message: "UnknownTag: 0x\(format2Hex(tag)) is not a known \(context)"
        )
    }
}

private let hexLower: [Character] = Array("0123456789abcdef")

/// Two lowercase hex digits for a byte — no Foundation dependency.
private func format2Hex(_ byte: UInt8) -> String {
    String([hexLower[Int(byte >> 4)], hexLower[Int(byte & 0x0f)]])
}

private func hexValue(_ c: UInt8) -> UInt8? {
    switch c {
    case 0x30...0x39: return c - 0x30              // 0-9
    case 0x61...0x66: return c - 0x61 + 10         // a-f
    case 0x41...0x46: return c - 0x41 + 10         // A-F
    default: return nil
    }
}

// ---------------------------------------------------------------------------
// Strict UTF-8 validation
// ---------------------------------------------------------------------------

/// Is this byte sequence well-formed UTF-8 (RFC 3629)? Rejects overlong
/// encodings, the surrogate range (U+D800–U+DFFF), values above U+10FFFF, and
/// truncated sequences. Written out rather than delegated so the strictness is
/// auditable here and cannot vary with a platform's decoder: `String(decoding:
/// as: UTF8.self)` is LOSSY (it substitutes U+FFFD), which this contract forbids.
private func isWellFormedUTF8<C: Collection>(_ bytes: C) -> Bool where C.Element == UInt8 {
    var i = bytes.startIndex
    let end = bytes.endIndex

    func continuation(_ idx: C.Index, _ lo: UInt8, _ hi: UInt8) -> Bool {
        idx < end && bytes[idx] >= lo && bytes[idx] <= hi
    }

    while i < end {
        let b = bytes[i]
        let width: Int
        switch b {
        case 0x00...0x7f:
            width = 1
        case 0xc2...0xdf:
            let c1 = bytes.index(after: i)
            guard continuation(c1, 0x80, 0xbf) else { return false }
            width = 2
        case 0xe0:
            // Second byte A0..BF excludes the overlong 3-byte forms.
            let c1 = bytes.index(after: i)
            guard continuation(c1, 0xa0, 0xbf), continuation(bytes.index(after: c1), 0x80, 0xbf) else { return false }
            width = 3
        case 0xe1...0xec, 0xee...0xef:
            let c1 = bytes.index(after: i)
            guard continuation(c1, 0x80, 0xbf), continuation(bytes.index(after: c1), 0x80, 0xbf) else { return false }
            width = 3
        case 0xed:
            // Second byte 80..9F excludes the surrogate range U+D800–U+DFFF.
            let c1 = bytes.index(after: i)
            guard continuation(c1, 0x80, 0x9f), continuation(bytes.index(after: c1), 0x80, 0xbf) else { return false }
            width = 3
        case 0xf0:
            // Second byte 90..BF excludes the overlong 4-byte forms.
            let c1 = bytes.index(after: i)
            let c2 = bytes.index(after: c1)
            guard continuation(c1, 0x90, 0xbf), continuation(c2, 0x80, 0xbf),
                  continuation(bytes.index(after: c2), 0x80, 0xbf) else { return false }
            width = 4
        case 0xf1...0xf3:
            let c1 = bytes.index(after: i)
            let c2 = bytes.index(after: c1)
            guard continuation(c1, 0x80, 0xbf), continuation(c2, 0x80, 0xbf),
                  continuation(bytes.index(after: c2), 0x80, 0xbf) else { return false }
            width = 4
        case 0xf4:
            // Second byte 80..8F caps the range at U+10FFFF.
            let c1 = bytes.index(after: i)
            let c2 = bytes.index(after: c1)
            guard continuation(c1, 0x80, 0x8f), continuation(c2, 0x80, 0xbf),
                  continuation(bytes.index(after: c2), 0x80, 0xbf) else { return false }
            width = 4
        default:
            // 0x80–0xC1 (lone continuation / overlong 2-byte) and 0xF5–0xFF.
            return false
        }
        i = bytes.index(i, offsetBy: width)
    }
    return true
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// A bounds-checked cursor over an immutable byte buffer. Never copies.
public struct WireReader {
    private let buf: [UInt8]
    private var pos: Int

    public init(_ buf: [UInt8]) {
        self.buf = buf
        self.pos = 0
    }

    /// Count of unread bytes.
    public var remaining: Int { buf.count - pos }

    private func need(_ n: Int, _ context: String) throws {
        let left = remaining
        if left < n {
            throw WireError.truncated(needed: n, remaining: left, offset: pos, context: context)
        }
    }

    public mutating func u8() throws -> UInt8 {
        try need(1, "u8")
        let v = buf[pos]
        pos += 1
        return v
    }

    public mutating func u16BE() throws -> UInt16 {
        try need(2, "u16be")
        let v = UInt16(buf[pos]) << 8 | UInt16(buf[pos + 1])
        pos += 2
        return v
    }

    public mutating func u32BE() throws -> UInt32 {
        try need(4, "u32be")
        let v = UInt32(buf[pos]) << 24 | UInt32(buf[pos + 1]) << 16
            | UInt32(buf[pos + 2]) << 8 | UInt32(buf[pos + 3])
        pos += 4
        return v
    }

    /// Exactly n bytes as a zero-copy view of the source buffer.
    public mutating func bytes(_ n: Int) throws -> ArraySlice<UInt8> {
        try need(n, "bytes(\(n))")
        let v = buf[pos..<(pos + n)]
        pos += n
        return v
    }

    /// 16 bytes as a lowercase hyphenated UUID string. NO version/variant
    /// validation — any 16 bytes are legal.
    public mutating func uuid() throws -> String {
        try need(16, "uuid")
        var s = ""
        s.reserveCapacity(36)
        for i in 0..<16 {
            if i == 4 || i == 6 || i == 8 || i == 10 { s.append("-") }
            let byte = buf[pos + i]
            s.append(hexLower[Int(byte >> 4)])
            s.append(hexLower[Int(byte & 0x0f)])
        }
        pos += 16
        return s
    }

    /// n bytes decoded as STRICT UTF-8. Bounds are checked before validity, and
    /// the kernel never peeks past the requested extent — a multibyte character
    /// split by `n` is `InvalidUtf8`, not a longer read.
    public mutating func utf8(_ n: Int) throws -> String {
        let start = pos
        let view = try bytes(n)
        guard isWellFormedUTF8(view) else {
            pos = start  // the read did not take effect
            throw WireError.invalidUtf8(offset: start, context: "utf8(\(n))")
        }
        return String(decoding: view, as: UTF8.self)
    }

    /// u16be length L, then exactly L bytes (zero-copy view). L beyond the
    /// remaining bytes is `LengthOverrun`, not `Truncated` — the length field
    /// itself is what is suspect.
    public mutating func lenPrefixedBytes16() throws -> ArraySlice<UInt8> {
        let start = pos
        try need(2, "lenPrefixedBytes16")
        let declared = Int(UInt16(buf[pos]) << 8 | UInt16(buf[pos + 1]))
        let remainingAfterLength = buf.count - pos - 2
        if declared > remainingAfterLength {
            throw WireError.lengthOverrun(
                declared: declared,
                remaining: remainingAfterLength,
                offset: start,
                context: "lenPrefixedBytes16"
            )
        }
        pos += 2
        let v = buf[pos..<(pos + declared)]
        pos += declared
        return v
    }

    /// All remaining bytes (possibly empty), zero-copy. Never errors.
    public mutating func rest() -> ArraySlice<UInt8> {
        let v = buf[pos...]
        pos = buf.count
        return v
    }

    public func expectEnd() throws {
        let count = remaining
        if count > 0 {
            throw WireError.trailingBytes(count: count, offset: pos)
        }
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// An append-only buffer builder. Numeric parameters use exact-width types —
/// the type is the range validation.
public struct WireWriter {
    private var buf: [UInt8]

    public init() { buf = [] }

    /// Preallocate, so a generated encoder that knows its exact size writes once.
    public init(capacity: Int) {
        buf = []
        buf.reserveCapacity(capacity)
    }

    public mutating func u8(_ value: UInt8) {
        buf.append(value)
    }

    public mutating func u16BE(_ value: UInt16) {
        buf.append(UInt8(truncatingIfNeeded: value >> 8))
        buf.append(UInt8(truncatingIfNeeded: value))
    }

    public mutating func u32BE(_ value: UInt32) {
        buf.append(UInt8(truncatingIfNeeded: value >> 24))
        buf.append(UInt8(truncatingIfNeeded: value >> 16))
        buf.append(UInt8(truncatingIfNeeded: value >> 8))
        buf.append(UInt8(truncatingIfNeeded: value))
    }

    public mutating func bytes<C: Collection>(_ b: C) where C.Element == UInt8 {
        buf.append(contentsOf: b)
    }

    /// Hyphenated 8-4-4-4-12 hex, case-insensitive input; emits the 16 bytes.
    /// The full shape is validated BEFORE any parsing — hyphens at exactly
    /// positions 8/13/18/23 and hex digits everywhere else — so a stray hyphen
    /// is `InvalidUuid`, never a short or shifted parse.
    public mutating func uuid(_ s: String) throws {
        let b = Array(s.utf8)
        let shapeOK = b.count == 36 && b.enumerated().allSatisfy { i, c in
            switch i {
            case 8, 13, 18, 23: return c == 0x2d  // '-'
            default: return hexValue(c) != nil
            }
        }
        guard shapeOK else {
            throw WireError.invalidUuid(context: "\"\(s)\" is not a hyphenated 8-4-4-4-12 UUID")
        }
        var out = [UInt8]()
        out.reserveCapacity(16)
        var i = 0
        while i < 36 {
            if b[i] == 0x2d { i += 1; continue }
            out.append(hexValue(b[i])! << 4 | hexValue(b[i + 1])!)
            i += 2
        }
        buf.append(contentsOf: out)
    }

    /// UTF-8 encode. A Swift `String` is well-formed by construction (its
    /// scalars cannot be surrogates) — infallible, as in Rust.
    public mutating func utf8(_ s: String) {
        buf.append(contentsOf: s.utf8)
    }

    /// u16be length, then the bytes. A length above 0xFFFF is `ValueOutOfRange`.
    public mutating func lenPrefixedBytes16<C: Collection>(_ b: C) throws where C.Element == UInt8 {
        let count = b.count
        guard count <= 0xffff else {
            throw WireError.valueOutOfRange(value: count, typeName: "lenPrefixedBytes16 length")
        }
        u16BE(UInt16(count))
        bytes(b)
    }

    /// The written bytes.
    public func toBytes() -> [UInt8] { buf }
}
