// Conformance: drive the Swift port with the shared expression parity vectors.
// Each vector's `expr` is evaluated with a `resolve` hook that binds `tx.VarRef`
// names from the vector's `env` — the demonstration that variable binding lives
// in the caller, not the runtime.

import Foundation
import XCTest
@testable import KanonakExpression

private let varRef = "kanonak.org/transformations/VarRef"

private func vectorsURL(_ name: String) -> URL {
    // .../kanonak-expression/swift/Tests/KanonakExpressionTests/ConformanceTests.swift
    URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // -> KanonakExpressionTests
        .deletingLastPathComponent()  // -> Tests
        .deletingLastPathComponent()  // -> swift
        .deletingLastPathComponent()  // -> kanonak-expression
        .appendingPathComponent("vectors")
        .appendingPathComponent(name)
}

private func loadJSON(_ name: String) throws -> [String: Any] {
    let data = try Data(contentsOf: vectorsURL(name))
    guard let doc = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        throw ExpressionError("vectors \(name): not a JSON object")
    }
    return doc
}

/// The caller's resolve: tx.VarRef -> env binding; any other leaf is unbound here.
private let resolve: Resolve<[String: Double]> = { node, env, _ in
    guard node["type"] as? String == varRef else {
        throw ExpressionError("No resolver for leaf '\(node["type"] as? String ?? "")'")
    }
    guard let name = node["varName"] as? String else {
        throw ExpressionError("VarRef without a varName")
    }
    guard let bound = env[name] else {
        throw ExpressionError("Unbound variable \"\(name)\"")
    }
    return bound
}

final class ExpressionVectorTests: XCTestCase {
    func testExpressionVectors() throws {
        let doc = try loadJSON("expression-vectors.json")
        XCTAssertEqual(doc["expressionRuntimeVersion"] as? String, expressionRuntimeVersion)

        let vectors = doc["vectors"] as! [[String: Any]]
        XCTAssertFalse(vectors.isEmpty)

        // Every vector must be REACHED, not merely not-failed: an empty or
        // short-circuited loop would otherwise pass silently.
        var evaluated = 0

        for v in vectors {
            let id = v["id"] as! String
            let expr = v["expr"] as! ExprNode

            // `env` values arrive as JSON numbers; the resolve hook binds Doubles.
            var env: [String: Double] = [:]
            for (name, raw) in (v["env"] as? [String: Any] ?? [:]) {
                env[name] = (raw as? NSNumber)?.doubleValue
            }

            if v["expectError"] as? Bool ?? false {
                XCTAssertThrowsError(try evaluate(expr, env, resolve), "[\(id)] expected an error")
                evaluated += 1
                continue
            }

            let expected = (v["expected"] as! NSNumber).doubleValue
            let got = try evaluate(expr, env, resolve)
            if let tolerance = (v["tolerance"] as? NSNumber)?.doubleValue {
                XCTAssertEqual(got, expected, accuracy: tolerance, "[\(id)]")
            } else {
                XCTAssertEqual(got, expected, "[\(id)]")
            }
            evaluated += 1
        }

        XCTAssertEqual(evaluated, vectors.count, "every vector must be evaluated")
        print("expression-vectors: \(evaluated)/\(vectors.count) evaluated")
    }
}
