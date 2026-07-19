// Conformance: drive the Swift port with the shared golden vectors and assert
// the canonical lexicals, canonical form, and content hash all match the
// authoritative (TypeScript-generated) expected values.

import Foundation
import XCTest
@testable import KanonakCanonical

private func vectorsURL(_ name: String) -> URL {
    // .../kanonak-canonical/swift/Tests/KanonakCanonicalTests/ConformanceTests.swift
    URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // -> KanonakCanonicalTests
        .deletingLastPathComponent()  // -> Tests
        .deletingLastPathComponent()  // -> swift
        .deletingLastPathComponent()  // -> kanonak-canonical
        .appendingPathComponent("vectors")
        .appendingPathComponent(name)
}

private func loadJSON(_ name: String) throws -> [String: Any] {
    let data = try Data(contentsOf: vectorsURL(name))
    guard let doc = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        throw CanonicalError("vectors \(name): not a JSON object")
    }
    return doc
}

final class LexicalVectorTests: XCTestCase {
    func testLexicalVectors() throws {
        let doc = try loadJSON("lexical-vectors.json")
        let vectors = doc["vectors"] as! [[String: Any]]
        XCTAssertFalse(vectors.isEmpty)
        for v in vectors {
            let id = v["id"] as! String
            let carrier = Carrier(rawValue: v["carrier"] as! String)!
            let input = v["input"] as! String
            let expectError = v["expectError"] as? Bool ?? false
            if expectError {
                XCTAssertThrowsError(try canonicalScalarLexical(carrier, input), "[\(id)] expected error")
                continue
            }
            let expected = v["expected"] as! String
            XCTAssertEqual(try canonicalScalarLexical(carrier, input), expected, "[\(id)]")
        }
    }
}

final class FullFormVectorTests: XCTestCase {
    func testFullFormVectors() throws {
        let doc = try loadJSON("full-form-vectors.json")
        let vectors = doc["vectors"] as! [[String: Any]]
        XCTAssertFalse(vectors.isEmpty)
        for v in vectors {
            let id = v["id"] as! String
            let pkg = decodeSubjects(v["input"] as! [String: Any])
            let form = try canonicalForm(pkg)
            let hash = try canonicalHash(pkg)
            XCTAssertEqual(form, v["expectedCanonicalForm"] as! String, "[\(id)] form")
            XCTAssertEqual(hash, v["expectedHash"] as! String, "[\(id)] hash")
        }
    }
}

private func decodeSubjects(_ input: [String: Any]) -> Package {
    let subjects = (input["subjects"] as! [[String: Any]]).map { sm in
        Subject(uri: sm["uri"] as! String, statements: decodeStatements(sm))
    }
    return Package(subjects: subjects)
}

private func decodeStatements(_ node: [String: Any]) -> [Statement] {
    guard let stmts = node["statements"] as? [[String: Any]] else { return [] }
    return stmts.map { sm in
        Statement(predicate: sm["predicate"] as! String,
                  value: decodeValue(sm["value"] as! [String: Any]))
    }
}

private func decodeValue(_ v: [String: Any]) -> Value {
    if let lit = v["lit"] as? String {
        if let c = carrierOf(v["datatype"] as! String) {
            return .typed(carrier: c, lexical: lit)
        }
        return .raw(lit)
    }
    if let raw = v["raw"] as? String { return .raw(raw) }
    if let r = v["ref"] as? String { return .ref(r) }
    if let emb = v["embed"] as? [String: Any] {
        return .embedded(name: emb["name"] as? String, statements: decodeStatements(emb))
    }
    if let list = v["list"] as? [[String: Any]] {
        return .list(list.map(decodeValue))
    }
    fatalError("decode: unknown value shape \(v)")
}
