// The typed SDK-facing surface: the $-envelope as data, the explicit
// reference-or-embedded union, and the Codable bridge from a typed model to
// the node contract. A generated struct CARRIES a KanonakNode (its custom
// Codable flattens the envelope keys into the same JSON object), types its
// object properties as Ref<T>, and binds through toNode — native Codable,
// one contract with the dictionary path.

import Foundation

/// The $-envelope as data — the value a generated typed model carries so an
/// instance holds its own identity and serializes straight to the
/// normalized-JSON wire form. Envelope keys are reserved (never ontology
/// statements).
public struct KanonakNode: Codable, Equatable {
    /// The resource's canonical URI. Required to form a subject.
    public var id: String?

    /// The durable class URI — the value of the synthesized type triple.
    public var type: String?

    /// A multi-typed node's FULL type set — present only when the node carries
    /// more than one type statement. Sorted by UTF-8 bytes, at least two
    /// members, no duplicates, `type` a member; each member emits one type
    /// statement in canonical form. Exposed ONLY as the $types envelope —
    /// deliberately no unprefixed accessor, because an ontology can model a
    /// property literally named "types"; the $ prefix exists to avoid exactly
    /// that collision.
    public var types: [String]?

    /// An embedded value's authored dict-key — HASH-RELEVANT (serialized into
    /// the canonical form). Only meaningful when the instance is used as an
    /// embedded value (via Ref.embed(_:named:)); nil for subjects.
    public var name: String?

    /// Package provenance on read; ignored if echoed back on write.
    public var packageContentHash: String?

    /// Package provenance on read; ignored if echoed back on write.
    public var packageVersion: String?

    public enum CodingKeys: String, CodingKey {
        case id = "$id"
        case type = "$type"
        case types = "$types"
        case name = "$name"
        case packageContentHash = "$contentHash"
        case packageVersion = "$version"
    }

    public init(id: String? = nil, type: String? = nil, types: [String]? = nil,
                name: String? = nil, packageContentHash: String? = nil,
                packageVersion: String? = nil) {
        self.id = id
        self.type = type
        self.types = types
        self.name = name
        self.packageContentHash = packageContentHash
        self.packageVersion = packageVersion
    }
}

/// Adopted by generated typed structs so the runtime can read/write an
/// instance's envelope — what lets Ref.to(resource) resolve identity and
/// Ref.embed(_:named:) carry the authored dict-key.
public protocol KanonakResource {
    var kanonakNode: KanonakNode { get set }
}

/// An object property's value: EXACTLY ONE of a reference to a named resource
/// (its canonical URI) or an embedded node (the value itself, carried inline —
/// derived identity, no $id). The typed twin of the wire form's
/// {"$ref": uri} vs embedded-node distinction; the choice between the arms is
/// authorial and hash-relevant, so it is explicit here, never inferred. An
/// embedded value's fields map via the containing property's declared range
/// when it carries no explicit $type — that range-derived typing is inference
/// only, never materialized as a statement.
public enum Ref<T: Codable>: Codable {
    case reference(String)
    case embedded(T)

    /// A reference to a named resource by its canonical URI.
    public static func to(_ uri: String) -> Ref<T> { .reference(uri) }

    /// A reference to a named resource by the instance itself — resolved
    /// through the target's envelope id. The target must already carry its
    /// identity; an embedded (id-less) value cannot be referenced.
    public static func to(_ resource: some KanonakResource) throws -> Ref<T> {
        guard let id = resource.kanonakNode.id, !id.isEmpty else {
            throw CodecError(
                "codec: Ref.to(resource) requires a resource with a non-empty envelope $id — "
                + "to carry the value inline instead, use Ref.embed")
        }
        return .reference(id)
    }

    /// An embedded value, carried inline (derived identity, no $id).
    public static func embed(_ value: T) -> Ref<T> { .embedded(value) }

    /// An embedded value with its authored dict-key name — HASH-RELEVANT
    /// (rides $name).
    public static func embed(_ value: T, named name: String) -> Ref<T> where T: KanonakResource {
        var v = value
        v.kanonakNode.name = name
        return .embedded(v)
    }

    /// The referenced resource's canonical URI — the reference arm (else nil).
    public var uri: String? {
        if case let .reference(uri) = self { return uri }
        return nil
    }

    /// The embedded value — the embedded arm (else nil).
    public var value: T? {
        if case let .embedded(v) = self { return v }
        return nil
    }

    private struct RefEnvelope: Codable {
        let ref: String
        enum CodingKeys: String, CodingKey { case ref = "$ref" }
    }

    public init(from decoder: Decoder) throws {
        if let envelope = try? RefEnvelope(from: decoder) {
            self = .reference(envelope.ref)
            return
        }
        self = .embedded(try T(from: decoder))
    }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case let .reference(uri):
            try RefEnvelope(ref: uri).encode(to: encoder)
        case let .embedded(value):
            try value.encode(to: encoder)
        }
    }
}

/// A typed instance's codec node (the dictionary contract). The bridge is
/// native Codable: the instance serializes to its normalized-JSON wire form
/// (envelope-as-data + Ref values), and the wire form maps onto the node
/// contract through the SAME split deserialize defines — so the typed path
/// and the dictionary path are one contract, not two.
public func toNode(_ typed: any Encodable, schema: CodecSchema) throws -> [String: Any] {
    let wire: Data
    do {
        wire = try JSONEncoder().encode(typed)
    } catch {
        throw CodecError("codec: typed value failed to serialize: \(error)")
    }
    guard let obj = try parseJSON(wire) as? [String: Any] else {
        throw CodecError("codec: a typed instance must serialize to a JSON object (the wire node form)")
    }
    return try deserialize(obj, schema: schema)
}

/// The canonical form of a package built from typed instances.
public func canonicalFormTyped(_ models: [any Encodable], schema: CodecSchema, pkg: PackageContext) throws -> String {
    try canonicalForm(models.map { try toNode($0, schema: schema) }, schema: schema, pkg: pkg)
}

/// The sha256: content hash of a package built from typed instances.
public func contentHashTyped(_ models: [any Encodable], schema: CodecSchema, pkg: PackageContext) throws -> String {
    try contentHash(models.map { try toNode($0, schema: schema) }, schema: schema, pkg: pkg)
}
