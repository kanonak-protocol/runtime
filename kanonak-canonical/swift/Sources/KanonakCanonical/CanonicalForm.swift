// The canonical value model + wire form: compact JSON with UTF-8 byte
// ordering, RFC 8785 escaping, and a fixed per-blob field order; the content
// address is the SHA-256 of those bytes.

import Crypto
import Foundation

// ---------------------------------------------------------------------------
// Value model
// ---------------------------------------------------------------------------

public indirect enum Value {
    case typed(carrier: Carrier, lexical: String)
    case raw(String)
    case ref(String)
    case embedded(name: String?, statements: [Statement])
    case list([Value])
}

public struct Statement {
    public let predicate: String
    public let value: Value
    public init(predicate: String, value: Value) {
        self.predicate = predicate
        self.value = value
    }
}

public struct Subject {
    public let uri: String
    public let statements: [Statement]
    public init(uri: String, statements: [Statement]) {
        self.uri = uri
        self.statements = statements
    }
}

public struct Package {
    public let subjects: [Subject]
    public init(subjects: [Subject]) { self.subjects = subjects }
}

// ---------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------

/// UTF-8 byte-wise lexicographic order — Swift's String `<` compares Unicode
/// canonical order, which is NOT the contract; the canonical form orders by
/// encoded bytes.
func utf8Less(_ a: String, _ b: String) -> Bool {
    var ai = a.utf8.makeIterator()
    var bi = b.utf8.makeIterator()
    while true {
        switch (ai.next(), bi.next()) {
        case (nil, nil): return false
        case (nil, _): return true
        case (_, nil): return false
        case let (x?, y?):
            if x != y { return x < y }
        }
    }
}

public func canonicalForm(_ pkg: Package) throws -> String {
    var b = "{\"subjects\":["
    // Stable order on equal URIs (index-decorated), byte-wise on distinct ones.
    let subjects = pkg.subjects.enumerated().sorted { l, r in
        if l.element.uri != r.element.uri { return utf8Less(l.element.uri, r.element.uri) }
        return l.offset < r.offset
    }
    for (i, s) in subjects.enumerated() {
        if i > 0 { b.append(",") }
        b.append("{\"uri\":")
        emitJSONString(&b, s.element.uri)
        b.append(",\"statements\":[")
        try emitStatements(&b, s.element.statements)
        b.append("]}")
    }
    b.append("]}")
    return b
}

public func canonicalHash(_ pkg: Package) throws -> String {
    let form = try canonicalForm(pkg)
    let digest = SHA256.hash(data: Data(form.utf8))
    return "sha256:" + digest.map { String(format: "%02x", $0) }.joined()
}

private func serializeStatement(_ st: Statement) throws -> String {
    var b = "{\"predicate\":"
    emitJSONString(&b, st.predicate)
    b.append(",")
    try emitValueTail(&b, st.value)
    b.append("}")
    return b
}

// Order by predicate UTF-8 bytes; equal predicates (possible since multi-typed
// subjects — several type statements share the type predicate) order by the
// serialized statement blob's UTF-8 bytes. The tie-break makes the declared
// invariance under statement ordering TRUE for same-predicate statements
// rather than an accident of sort stability; no distinct-predicate ordering
// is affected.
private func emitStatements(_ b: inout String, _ stmts: [Statement]) throws {
    var rendered: [(predicate: String, serialized: String)] = []
    rendered.reserveCapacity(stmts.count)
    for st in stmts {
        rendered.append((st.predicate, try serializeStatement(st)))
    }
    rendered.sort { l, r in
        if l.predicate != r.predicate { return utf8Less(l.predicate, r.predicate) }
        return utf8Less(l.serialized, r.serialized)
    }
    for (i, st) in rendered.enumerated() {
        if i > 0 { b.append(",") }
        b.append(st.serialized)
    }
}

private func emitValueTail(_ b: inout String, _ v: Value) throws {
    switch v {
    case let .typed(carrier, lexical):
        let lex = try canonicalScalarLexical(carrier, lexical)
        b.append("\"type\":\"typed\",\"carrier\":")
        emitJSONString(&b, carrier.rawValue)
        b.append(",\"value\":")
        emitJSONString(&b, lex)
    case let .raw(token):
        b.append("\"type\":\"string\",\"value\":")
        emitJSONString(&b, token)
    case let .ref(uri):
        b.append("\"type\":\"ref\",\"value\":")
        emitJSONString(&b, uri)
    case let .embedded(name, statements):
        b.append("\"type\":\"embedded\"")
        if let name {
            b.append(",\"name\":")
            emitJSONString(&b, name)
        }
        b.append(",\"statements\":[")
        try emitStatements(&b, statements)
        b.append("]")
    case let .list(items):
        b.append("\"type\":\"list\",\"items\":[")
        for (i, item) in items.enumerated() {
            if i > 0 { b.append(",") }
            b.append("{")
            try emitValueTail(&b, item)
            b.append("}")
        }
        b.append("]")
    }
}

/// RFC 8785 / JSON.stringify escaping.
func emitJSONString(_ b: inout String, _ s: String) {
    b.append("\"")
    for c in s.unicodeScalars {
        switch c {
        case "\"": b.append("\\\"")
        case "\\": b.append("\\\\")
        case "\u{08}": b.append("\\b")
        case "\u{0C}": b.append("\\f")
        case "\n": b.append("\\n")
        case "\r": b.append("\\r")
        case "\t": b.append("\\t")
        default:
            if c.value < 0x20 {
                b.append(String(format: "\\u%04x", c.value))
            } else {
                b.unicodeScalars.append(c)
            }
        }
    }
    b.append("\"")
}
