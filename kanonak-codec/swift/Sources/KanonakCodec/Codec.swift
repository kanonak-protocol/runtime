// The generic, ontology-independent Kanonak codec runtime (Swift port). Given
// a CodecSchema (the per-package metadata a generated SDK embeds) and a set of
// typed nodes, it builds the canonical input model and content-addresses it
// via KanonakCanonical — the same content-form the TypeScript/Python
// references and the `kanonak hash` CLI produce. It also (de)serializes the
// normalized-JSON wire form.
//
// A node is a plain [String: Any] (the $-envelope plus alias-collapsed
// local-name fields). Values: String, Bool, JSONNumber (lexical-preserving —
// what parseJSON produces), Int/Int64/Double for programmatic construction,
// [Any] for lists, [String: Any] for references/embeddeds, NSNull for null.

import Foundation
import KanonakCanonical

public struct CodecError: Error, CustomStringConvertible {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var description: String { message }
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// A single modeled property of a class. `range` is the range class's URI —
/// present for object props; maps an embedded value's fields when the embedded
/// carries no explicit $type (inference only, never materialized as a statement).
public struct CodecProp: Decodable {
    public let predicate: String
    public let kind: String // "datatype" | "object"
    public let datatype: String?
    public let range: String?

    public init(predicate: String, kind: String, datatype: String? = nil, range: String? = nil) {
        self.predicate = predicate
        self.kind = kind
        self.datatype = datatype
        self.range = range
    }
}

/// A modeled type: its URI and its properties keyed by local name.
public struct CodecClass: Decodable {
    public let typeUri: String
    public let props: [String: CodecProp]

    public init(typeUri: String, props: [String: CodecProp]) {
        self.typeUri = typeUri
        self.props = props
    }
}

/// The per-package metadata a generated SDK embeds.
public struct CodecSchema: Decodable {
    public let typePredicate: String
    public let labelPredicate: String
    public let packageTypeUri: String
    public let classes: [String: CodecClass]

    public init(typePredicate: String, labelPredicate: String, packageTypeUri: String,
                classes: [String: CodecClass]) {
        self.typePredicate = typePredicate
        self.labelPredicate = labelPredicate
        self.packageTypeUri = packageTypeUri
        self.classes = classes
    }

    public static func fromJSON(_ json: String) throws -> CodecSchema {
        try JSONDecoder().decode(CodecSchema.self, from: Data(json.utf8))
    }

    /// Merge the imported SDKs' generated codec schemas into this one
    /// (transitively, by induction) — durable versioned URIs make the merge
    /// collision-free, so mixed-package graphs hash through this one codec
    /// with no consumer-side schema merging. Own classes win: this package is
    /// authoritative for its own URIs.
    public func merging(imports: [CodecSchema]) -> CodecSchema {
        var merged: [String: CodecClass] = [:]
        for imported in imports {
            merged.merge(imported.classes) { _, new in new }
        }
        merged.merge(classes) { _, own in own }
        return CodecSchema(typePredicate: typePredicate, labelPredicate: labelPredicate,
                           packageTypeUri: packageTypeUri, classes: merged)
    }
}

/// Identifies the package being built/hashed.
public struct PackageContext {
    public let publisher: String
    public let packageName: String
    public let version: String
    public let label: String?

    public init(publisher: String, packageName: String, version: String, label: String? = nil) {
        self.publisher = publisher
        self.packageName = packageName
        self.version = version
        self.label = label
    }
}

// ---------------------------------------------------------------------------
// $types validation
// ---------------------------------------------------------------------------

// The $-envelope keys excluded from statement/field emission. $name carries an
// embedded value's authored dict-key — hash-relevant. $types carries a
// multi-typed node's FULL type set.
private let envelopeKeys: Set<String> = ["$type", "$types", "$id", "$name", "$contentHash", "$version", "$extra"]

/// Validate a node-or-embedded's $types envelope and return the validated set,
/// or nil when the node is single-typed. Invariants: sorted by UTF-8 bytes, at
/// least two members, no duplicates, and $type (the dispatch key) a member.
/// Enforced wherever the envelope is touched — serialize, deserialize, and
/// canonicalization — so a producer fails at emit time and a reader never
/// masks a nondeterministic emitter by silently repairing the set.
func validatedTypes(_ m: [String: Any], where context: String) throws -> [String]? {
    guard let raw = m["$types"], !(raw is NSNull) else { return nil }
    guard let list = raw as? [Any] else {
        throw CodecError("codec: \(context): $types must be a list of non-empty type URIs")
    }
    var types: [String] = []
    for item in list {
        guard let s = item as? String, !s.isEmpty else {
            throw CodecError("codec: \(context): $types must be a list of non-empty type URIs")
        }
        types.append(s)
    }
    if types.count < 2 {
        throw CodecError(
            "codec: \(context): $types with \(types.count) member(s) is forbidden — a single-typed node carries "
            + "only $type (a second encoding of the same content would be hash-ambiguous)")
    }
    for i in 1..<types.count {
        if types[i - 1] == types[i] {
            throw CodecError("codec: \(context): $types carries duplicate member \(types[i])")
        }
        if utf8OrderedAfter(types[i - 1], types[i]) {
            throw CodecError(
                "codec: \(context): $types is not sorted by UTF-8 bytes (\(types[i - 1]) sorts after \(types[i])) — "
                + "ordering is the producer's job, never the reader's")
        }
    }
    let primary = m["$type"] as? String ?? ""
    if !primary.isEmpty, types.contains(primary) { return types }
    throw CodecError("codec: \(context): $type (\"\(primary)\") must be present and a member of $types")
}

private func utf8OrderedAfter(_ a: String, _ b: String) -> Bool {
    // a > b in UTF-8 byte order.
    let ab = [UInt8](a.utf8), bb = [UInt8](b.utf8)
    for (x, y) in zip(ab, bb) where x != y { return x > y }
    return ab.count > bb.count
}

/// Recursively validate every $types envelope in a wire value (the node itself
/// and any embedded node at any depth). Shared by serialize (the producer
/// fails at emit time) and deserialize (the strict reader rejects, never repairs).
func assertTypesEnvelopes(_ value: Any, where context: String) throws {
    if let list = value as? [Any] {
        for (i, item) in list.enumerated() {
            try assertTypesEnvelopes(item, where: "\(context)[\(i)]")
        }
        return
    }
    if let m = value as? [String: Any] {
        if m["$types"] != nil {
            _ = try validatedTypes(m, where: context)
        }
        for (key, item) in m where key != "$types" {
            try assertTypesEnvelopes(item, where: "\(context).\(key)")
        }
    }
}

// ---------------------------------------------------------------------------
// Canonicalization
// ---------------------------------------------------------------------------

/// The raw lexical token of a scalar — the input the canonical form
/// normalizes. Bool -> "true"/"false", String -> as-is, JSONNumber -> its
/// original token; Int/Int64/Double supported for programmatic nodes.
/// Anything else fails loudly.
func lexical(_ value: Any) throws -> String {
    switch value {
    case let b as Bool: return b ? "true" : "false"
    case let s as String: return s
    case let n as JSONNumber: return n.token
    case let n as Int: return String(n)
    case let n as Int64: return String(n)
    case let d as Double: return ecmaScriptDoubleToken(d)
    default:
        throw CodecError("codec: unsupported scalar lexical \(type(of: value)) (\(value))")
    }
}

/// Build a canonical Value for a single (non-list) raw field value.
private func canonicalValue(_ prop: CodecProp, _ raw: Any, _ schema: CodecSchema) throws -> Value {
    if prop.kind == "object" {
        // A node: a reference ({"$ref"}) or an embedded resource.
        guard let m = raw as? [String: Any] else {
            throw CodecError(
                "codec: object property \(prop.predicate) expects a reference ({\"$ref\": ...}) or an "
                + "embedded node (a dictionary), got \(type(of: raw))")
        }
        if let ref = m["$ref"] {
            guard let uri = ref as? String else {
                throw CodecError("codec: object $ref must be a string, got \(type(of: ref))")
            }
            return .ref(uri)
        }
        return try embeddedValue(prop, m, schema)
    }
    let lex = try lexical(raw)
    guard let datatype = prop.datatype, let carrier = carrierOf(datatype) else {
        return .raw(lex)
    }
    return .typed(carrier: carrier, lexical: lex)
}

/// Canonicalize an embedded value: a dictionary with no $id, an optional $name
/// (the authored dict-key — hash-relevant), an optional $type, and
/// schema-mapped fields. An explicit $type emits a type statement inside the
/// embedded (hash-relevant even when it equals the range-derived type);
/// without it, fields map via the containing property's range and NO type
/// statement is emitted — range-derived typing is inference only.
private func embeddedValue(_ prop: CodecProp, _ m: [String: Any], _ schema: CodecSchema) throws -> Value {
    if m["$id"] != nil {
        throw CodecError(
            "codec: an embedded value under \(prop.predicate) must not carry $id — to point at a "
            + "named resource, pass a reference ({\"$ref\": ...})")
    }
    let types = try validatedTypes(m, where: "embedded value under \(prop.predicate)")
    let explicitType = m["$type"] as? String ?? ""
    let clsUri = explicitType.isEmpty ? (prop.range ?? "") : explicitType
    if clsUri.isEmpty {
        throw CodecError(
            "codec: cannot map embedded value under \(prop.predicate): it carries no $type and the "
            + "property declares no range")
    }
    guard let cls = schema.classes[clsUri] else {
        throw CodecError("codec: no schema for embedded type \(clsUri)")
    }

    var stmts = try fieldStatements(m, cls, schema)
    if let types {
        // A multi-typed embedded ($types implies an explicit $type): one type
        // statement per member, in $types (UTF-8 sorted) order — all hash-relevant.
        for member in types {
            stmts.append(Statement(predicate: schema.typePredicate, value: .ref(member)))
        }
    } else if !explicitType.isEmpty {
        stmts.append(Statement(predicate: schema.typePredicate, value: .ref(explicitType)))
    }
    var name: String?
    if let n = m["$name"] as? String, !n.isEmpty { name = n }
    return .embedded(name: name, statements: stmts)
}

/// The canonical statements for one node-or-embedded's modeled fields plus its
/// $extra — everything except the type triple (subjects always carry one;
/// embeddeds only when explicitly typed).
private func fieldStatements(_ source: [String: Any], _ cls: CodecClass, _ schema: CodecSchema) throws -> [Statement] {
    var out: [Statement] = []

    for (key, raw) in source {
        if envelopeKeys.contains(key) || raw is NSNull { continue }
        guard let prop = cls.props[key] else {
            out.append(Statement(predicate: key, value: .raw(try lexical(raw))))
            continue
        }
        if let list = raw as? [Any] {
            // An empty list contributes NO statement — absent and empty are
            // identical at the canonical layer (the wire serialize still
            // preserves the empty list).
            if list.isEmpty { continue }
            let items = try list.map { try canonicalValue(prop, $0, schema) }
            out.append(Statement(predicate: prop.predicate, value: .list(items)))
        } else {
            out.append(Statement(predicate: prop.predicate, value: try canonicalValue(prop, raw, schema)))
        }
    }

    if let extra = source["$extra"] as? [String: Any] {
        for (predicate, raw) in extra {
            if raw is NSNull { continue }
            out.append(Statement(predicate: predicate, value: .raw(try lexical(raw))))
        }
    }
    return out
}

/// The canonical statements for one subject node: the rdf:type triple(s), then
/// its fields.
private func subjectStatements(_ node: [String: Any], _ schema: CodecSchema) throws -> [Statement] {
    var id = node["$id"] as? String ?? ""
    if id.isEmpty { id = "(no $id)" }
    let types = try validatedTypes(node, where: "node \(id)")
    guard let typeUri = node["$type"] as? String, !typeUri.isEmpty else {
        throw CodecError("codec: node is missing $type")
    }
    guard let cls = schema.classes[typeUri] else {
        throw CodecError("codec: no schema for type \(typeUri)")
    }

    // The rdf:type triple(s) every subject carries: one per $types member for a
    // multi-typed node (in $types' UTF-8 sorted order), else the single $type.
    let members = types ?? [typeUri]
    var out = members.map { Statement(predicate: schema.typePredicate, value: .ref($0)) }
    out.append(contentsOf: try fieldStatements(node, cls, schema))
    return out
}

/// Build the canonical input model: a subject per node plus the synthesized
/// package-wrapper subject (raw label + Package type), exactly the subject set
/// `kanonak hash` produces for the equivalent authored package.
public func buildPackage(_ nodes: [[String: Any]], schema: CodecSchema, pkg: PackageContext) throws -> Package {
    var subjects: [Subject] = []
    subjects.reserveCapacity(nodes.count + 1)
    for node in nodes {
        guard let id = node["$id"] as? String, !id.isEmpty else {
            throw CodecError("codec: node is missing $id")
        }
        subjects.append(Subject(uri: id, statements: try subjectStatements(node, schema)))
    }

    let pkgUri = "\(pkg.publisher)/\(pkg.packageName)@\(pkg.version)/\(pkg.packageName)"
    var pkgStmts: [Statement] = []
    if let label = pkg.label {
        pkgStmts.append(Statement(predicate: schema.labelPredicate, value: .raw(label)))
    }
    pkgStmts.append(Statement(predicate: schema.typePredicate, value: .ref(schema.packageTypeUri)))
    subjects.append(Subject(uri: pkgUri, statements: pkgStmts))

    return Package(subjects: subjects)
}

/// The canonical form (the {subjects:[...]} JSON) of a package built from nodes.
public func canonicalForm(_ nodes: [[String: Any]], schema: CodecSchema, pkg: PackageContext) throws -> String {
    try KanonakCanonical.canonicalForm(buildPackage(nodes, schema: schema, pkg: pkg))
}

/// The sha256: content hash of a package built from nodes — matches `kanonak hash`.
public func contentHash(_ nodes: [[String: Any]], schema: CodecSchema, pkg: PackageContext) throws -> String {
    try KanonakCanonical.canonicalHash(buildPackage(nodes, schema: schema, pkg: pkg))
}

// ---------------------------------------------------------------------------
// Wire form
// ---------------------------------------------------------------------------

/// Render a typed node to its normalized-JSON wire form. $extra entries ride
/// as sibling fields after the modeled ones; a modeled field wins a name
/// collision. Null values are dropped. An invalid $types envelope (at any
/// depth) is a producer bug and fails at emit time.
public func serialize(_ node: [String: Any]) throws -> [String: Any] {
    var context = node["$id"] as? String ?? ""
    if context.isEmpty { context = node["$type"] as? String ?? "" }
    if context.isEmpty { context = "(node)" }
    try assertTypesEnvelopes(node, where: "serialize \(context)")
    var out: [String: Any] = [:]
    for (key, val) in node {
        if key == "$extra" || val is NSNull { continue }
        out[key] = val
    }
    if let extra = node["$extra"] as? [String: Any] {
        for (key, val) in extra {
            if val is NSNull { continue }
            if out[key] == nil { out[key] = val }
        }
    }
    return out
}

/// Parse normalized JSON into a typed node. $-envelope keys and fields modeled
/// on the node's $type stay top-level; every other key is collected into
/// $extra so a strongly-typed consumer round-trips it losslessly.
public func deserialize(_ jsonObj: [String: Any], schema: CodecSchema) throws -> [String: Any] {
    guard let typeUri = jsonObj["$type"] as? String else {
        throw CodecError("codec: cannot deserialize: missing string $type")
    }
    // Reader-side $types validation, at every depth: an unsorted / singleton /
    // duplicate / non-member set is REJECTED, never silently repaired —
    // determinism belongs to the producer, and a lenient reader would mask a
    // nondeterministic emitter.
    var context = jsonObj["$id"] as? String ?? ""
    if context.isEmpty { context = typeUri }
    try assertTypesEnvelopes(jsonObj, where: "deserialize \(context)")
    guard let cls = schema.classes[typeUri] else {
        throw CodecError("codec: cannot deserialize: no schema for type \(typeUri)")
    }

    var node: [String: Any] = ["$type": typeUri]
    var extra: [String: Any] = [:]
    for (key, val) in jsonObj {
        if key == "$type" { continue }
        if key.hasPrefix("$") || cls.props[key] != nil {
            node[key] = val
        } else {
            extra[key] = val
        }
    }
    if !extra.isEmpty { node["$extra"] = extra }
    return node
}

// ---------------------------------------------------------------------------
// Dictionary-path helpers
// ---------------------------------------------------------------------------

/// A reference to a named resource by its canonical URI.
public func ref(_ uri: String) -> [String: Any] { ["$ref": uri] }

/// A reference to a named resource by the node itself — resolved through its $id.
public func ref(_ node: [String: Any]) throws -> [String: Any] {
    guard let id = node["$id"] as? String, !id.isEmpty else {
        throw CodecError(
            "codec: ref(node) requires a node with a non-empty $id — "
            + "to carry the value inline instead, use embed")
    }
    return ["$ref": id]
}

/// An embedded value, carried inline (derived identity, no $id); `name` is the
/// authored dict-key — HASH-RELEVANT (rides $name).
public func embed(_ value: [String: Any], name: String? = nil) -> [String: Any] {
    var out = value
    if let name { out["$name"] = name }
    return out
}
