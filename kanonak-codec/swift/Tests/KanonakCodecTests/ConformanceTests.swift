// Conformance: drive KanonakCodec with the shared codec vectors and assert the
// canonical form, content hash, and normalized-JSON serialize all match the
// authoritative (TypeScript-generated) expected values — on the dictionary
// contract AND the typed (KanonakNode/Ref) surface.

import Foundation
import XCTest
@testable import KanonakCodec

private func vectorsURL(_ name: String) -> URL {
    URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // -> KanonakCodecTests
        .deletingLastPathComponent()  // -> Tests
        .deletingLastPathComponent()  // -> swift
        .deletingLastPathComponent()  // -> kanonak-codec
        .appendingPathComponent("vectors")
        .appendingPathComponent(name)
}

private struct VectorFile {
    let schema: CodecSchema
    let cases: [[String: Any]]
}

private func loadVectors(_ name: String) throws -> VectorFile {
    let data = try Data(contentsOf: vectorsURL(name))
    guard let doc = try parseJSON(data) as? [String: Any],
          let schemaDict = doc["schema"] as? [String: Any],
          let cases = doc["cases"] as? [[String: Any]] else {
        throw CodecError("vectors \(name): unexpected shape")
    }
    let schema = try CodecSchema.fromJSON(try writeJSON(schemaDict))
    return VectorFile(schema: schema, cases: cases)
}

private func jsonEqual(_ a: Any, _ b: Any) -> Bool {
    switch (a, b) {
    case let (x as String, y as String): return x == y
    case let (x as Bool, y as Bool): return x == y
    case let (x as JSONNumber, y as JSONNumber): return x == y
    case (is NSNull, is NSNull): return true
    case let (x as [Any], y as [Any]):
        return x.count == y.count && zip(x, y).allSatisfy { jsonEqual($0, $1) }
    case let (x as [String: Any], y as [String: Any]):
        return x.count == y.count && x.allSatisfy { k, v in y[k].map { jsonEqual(v, $0) } ?? false }
    default: return false
    }
}

final class CodecVectorTests: XCTestCase {
    private func runFile(_ name: String) throws {
        let file = try loadVectors(name)
        XCTAssertFalse(file.cases.isEmpty)
        for c in file.cases {
            let cid = c["id"] as! String
            let nodes = c["nodes"] as! [[String: Any]]
            let pkg = packageContext(c["pkg"] as! [String: Any])

            XCTAssertEqual(try canonicalForm(nodes, schema: file.schema, pkg: pkg),
                           c["expectedCanonicalForm"] as! String, "[\(cid)] canonical form")
            XCTAssertEqual(try contentHash(nodes, schema: file.schema, pkg: pkg),
                           c["expectedHash"] as! String, "[\(cid)] hash")

            let expectedSerialize = c["expectedSerialize"] as! [[String: Any]]
            for (i, node) in nodes.enumerated() {
                let got = try serialize(node)
                XCTAssertTrue(jsonEqual(got, expectedSerialize[i]),
                              "[\(cid)] serialize[\(i)]\n  got: \(got)\n  exp: \(expectedSerialize[i])")
                // deserialize(serialize(node)) recovers the modeled + $extra split.
                let back = try deserialize(got, schema: file.schema)
                XCTAssertEqual(back["$type"] as? String, node["$type"] as? String,
                               "[\(cid)] deserialize[\(i)] $type")
            }
        }
    }

    func testCodecVectors() throws {
        try runFile("codec-vectors.json")
    }

    func testEmbeddedVectors() throws {
        try runFile("codec-vectors-embedded.json")
    }

    /// The multi-typed-subjects file. Beyond the standard checks it exercises
    /// the $types contract: expectError cases must be rejected on ALL THREE
    /// surfaces — serialize (the producer fails at emit time), deserialize
    /// (the reader rejects, never repairs), and canonicalization — and
    /// positive cases must round-trip to the same hash.
    func testTypesVectors() throws {
        let file = try loadVectors("codec-vectors-types.json")
        XCTAssertFalse(file.cases.isEmpty)
        for c in file.cases {
            let cid = c["id"] as! String
            let nodes = c["nodes"] as! [[String: Any]]
            let pkg = packageContext(c["pkg"] as! [String: Any])

            if c["expectError"] as? Bool == true {
                XCTAssertThrowsError(try canonicalForm(nodes, schema: file.schema, pkg: pkg),
                                     "[\(cid)] expected canonicalize to reject")
                XCTAssertThrowsError(try nodes.map { try serialize($0) },
                                     "[\(cid)] expected serialize to reject")
                XCTAssertThrowsError(try nodes.map { try deserialize($0, schema: file.schema) },
                                     "[\(cid)] expected deserialize to reject")
                continue
            }

            XCTAssertEqual(try canonicalForm(nodes, schema: file.schema, pkg: pkg),
                           c["expectedCanonicalForm"] as! String, "[\(cid)] canonical form")
            XCTAssertEqual(try contentHash(nodes, schema: file.schema, pkg: pkg),
                           c["expectedHash"] as! String, "[\(cid)] hash")

            let expectedSerialize = c["expectedSerialize"] as! [[String: Any]]
            let ser = try nodes.map { try serialize($0) }
            XCTAssertTrue(jsonEqual(ser, expectedSerialize), "[\(cid)] serialize")

            let roundTripped = try ser.map { try deserialize($0, schema: file.schema) }
            let rtSer = try roundTripped.map { try serialize($0) }
            XCTAssertTrue(jsonEqual(rtSer, expectedSerialize), "[\(cid)] round-trip serialize")
            XCTAssertEqual(try contentHash(roundTripped, schema: file.schema, pkg: pkg),
                           c["expectedHash"] as! String, "[\(cid)] round-trip hash")
        }
    }

    private func packageContext(_ pkg: [String: Any]) -> PackageContext {
        PackageContext(publisher: pkg["publisher"] as! String,
                       packageName: pkg["packageName"] as! String,
                       version: pkg["version"] as! String,
                       label: pkg["label"] as? String)
    }
}

// ---------------------------------------------------------------------------
// Dictionary-path helpers reproduce the golden vectors
// ---------------------------------------------------------------------------

final class DictionaryHelperTests: XCTestCase {
    private let schemaNS = "probe.example.com/schema@1.0.0"
    private let dataNS = "probe.example.com/data@1.0.0"
    private func t(_ local: String) -> String { "\(schemaNS)/\(local)" }
    private func d(_ local: String) -> String { "\(dataNS)/\(local)" }

    private func check(_ file: VectorFile, _ cid: String, _ nodes: [[String: Any]]) throws {
        let c = file.cases.first { $0["id"] as? String == cid }!
        let pkgDict = c["pkg"] as! [String: Any]
        let pkg = PackageContext(publisher: pkgDict["publisher"] as! String,
                                 packageName: pkgDict["packageName"] as! String,
                                 version: pkgDict["version"] as! String,
                                 label: pkgDict["label"] as? String)
        XCTAssertEqual(try canonicalForm(nodes, schema: file.schema, pkg: pkg),
                       c["expectedCanonicalForm"] as! String, "[helpers:\(cid)] canonical form")
        XCTAssertEqual(try contentHash(nodes, schema: file.schema, pkg: pkg),
                       c["expectedHash"] as! String, "[helpers:\(cid)] hash")
    }

    func testEmbedAndRefHelpers() throws {
        let emb = try loadVectors("codec-vectors-embedded.json")
        try check(emb, "embedded-named-in-list", [
            ["$type": t("Order"), "$id": d("o1"), "note": "A",
             "items": [embed(["sku": "X", "qty": 1], name: "first")]],
        ])
        try check(emb, "embedded-explicit-type", [
            ["$type": t("Order"), "$id": d("o1"), "note": "A",
             "items": [embed(["$type": t("LineItem"), "sku": "X", "qty": 1], name: "first")]],
        ])
        try check(emb, "empty-list-emits-nothing", [
            ["$type": t("Order"), "$id": d("o1"), "note": "A", "items": [] as [Any]],
        ])

        let basic = try loadVectors("codec-vectors.json")
        let person: [String: Any] = ["$type": t("Person"), "$id": d("p1"), "name": "Alice"]
        let account: [String: Any] = [
            "$type": t("Account"), "$id": d("a1"),
            "accountCode": "paul", "seats": 5, "rate": 1.5, "active": true,
            "owner": try ref(person), "tags": ["x", "y"],
        ]
        try check(basic, "basic-scalars-ref-list", [person, account])
    }
}

// ---------------------------------------------------------------------------
// The typed surface: generated-SDK-shaped structs reproduce the same vectors.
// These structs are the EXECUTABLE SPEC for what the platform's SwiftGenerator
// emits — a KanonakNode-carrying struct with flattening custom Codable and
// Ref<T> object properties.
// ---------------------------------------------------------------------------

private let schemaNS = "probe.example.com/schema@1.0.0"
private let dataNS = "probe.example.com/data@1.0.0"

private struct Person: Codable, KanonakResource {
    var kanonakNode = KanonakNode()
    var name: String?

    enum CodingKeys: String, CodingKey { case name }

    init(id: String? = nil, name: String? = nil) {
        kanonakNode = KanonakNode(id: id, type: "\(schemaNS)/Person")
        self.name = name
    }

    init(from decoder: Decoder) throws {
        kanonakNode = try KanonakNode(from: decoder)
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decodeIfPresent(String.self, forKey: .name)
    }

    func encode(to encoder: Encoder) throws {
        try kanonakNode.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(name, forKey: .name)
    }
}

private struct Account: Codable, KanonakResource {
    var kanonakNode = KanonakNode(type: "\(schemaNS)/Account")
    var accountCode: String?
    var seats: Int64?
    var rate: Double?
    var active: Bool?
    var owner: Ref<Person>?
    var tags: [String]?

    enum CodingKeys: String, CodingKey { case accountCode, seats, rate, active, owner, tags }

    init(from decoder: Decoder) throws {
        kanonakNode = try KanonakNode(from: decoder)
        let c = try decoder.container(keyedBy: CodingKeys.self)
        accountCode = try c.decodeIfPresent(String.self, forKey: .accountCode)
        seats = try c.decodeIfPresent(Int64.self, forKey: .seats)
        rate = try c.decodeIfPresent(Double.self, forKey: .rate)
        active = try c.decodeIfPresent(Bool.self, forKey: .active)
        owner = try c.decodeIfPresent(Ref<Person>.self, forKey: .owner)
        tags = try c.decodeIfPresent([String].self, forKey: .tags)
    }

    init(id: String) {
        kanonakNode = KanonakNode(id: id, type: "\(schemaNS)/Account")
    }

    func encode(to encoder: Encoder) throws {
        try kanonakNode.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(accountCode, forKey: .accountCode)
        try c.encodeIfPresent(seats, forKey: .seats)
        try c.encodeIfPresent(rate, forKey: .rate)
        try c.encodeIfPresent(active, forKey: .active)
        try c.encodeIfPresent(owner, forKey: .owner)
        try c.encodeIfPresent(tags, forKey: .tags)
    }
}

final class TypedSurfaceTests: XCTestCase {
    func testTypedReproducesBasicVector() throws {
        let file = try loadVectors("codec-vectors.json")
        let c = file.cases.first { $0["id"] as? String == "basic-scalars-ref-list" }!
        let pkgDict = c["pkg"] as! [String: Any]
        let pkg = PackageContext(publisher: pkgDict["publisher"] as! String,
                                 packageName: pkgDict["packageName"] as! String,
                                 version: pkgDict["version"] as! String,
                                 label: pkgDict["label"] as? String)

        let person = Person(id: "\(dataNS)/p1", name: "Alice")
        var account = Account(id: "\(dataNS)/a1")
        account.accountCode = "paul"
        account.seats = 5
        account.rate = 1.5
        account.active = true
        account.owner = try .to(person)
        account.tags = ["x", "y"]

        XCTAssertEqual(try contentHashTyped([person, account], schema: file.schema, pkg: pkg),
                       c["expectedHash"] as! String, "typed hash")
        XCTAssertEqual(try canonicalFormTyped([person, account], schema: file.schema, pkg: pkg),
                       c["expectedCanonicalForm"] as! String, "typed canonical form")
    }

    func testRefDecodeRoundTrip() throws {
        let refJSON = Data("{\"$ref\":\"\(dataNS)/p1\"}".utf8)
        let decodedRef = try JSONDecoder().decode(Ref<Person>.self, from: refJSON)
        XCTAssertEqual(decodedRef.uri, "\(dataNS)/p1")

        let embJSON = Data("{\"$name\":\"cust\",\"name\":\"Ada\"}".utf8)
        let decodedEmb = try JSONDecoder().decode(Ref<Person>.self, from: embJSON)
        XCTAssertEqual(decodedEmb.value?.name, "Ada")
        XCTAssertEqual(decodedEmb.value?.kanonakNode.name, "cust")
    }
}
