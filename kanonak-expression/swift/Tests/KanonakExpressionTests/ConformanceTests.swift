// Conformance: drive the Swift port with the shared expression parity vectors.
// Each vector's `expr` is evaluated with a `resolve` hook that binds `tx.VarRef`
// names from the vector's `env` — the demonstration that variable binding lives
// in the caller, not the runtime. Ordered-comparison vectors additionally supply
// `closures` (the ClosureTable) and `refEnv` (identity bindings for the
// resolveRef hook). Every vector also runs through `explain` and its root value
// must agree with `evaluate`; vectors with a `trace` assert the verdict tree
// structurally.

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

/// The conformance context: numeric bindings (env) plus identity bindings (refEnv).
private struct Ctx {
    let env: [String: Double]
    let refEnv: [String: String]
}

/// The caller's resolve: tx.VarRef -> env binding; any other leaf is unbound here.
private let resolve: Resolve<Ctx> = { node, ctx, _ in
    guard node["type"] as? String == varRef else {
        throw ExpressionError("No resolver for leaf '\(node["type"] as? String ?? "")'")
    }
    guard let name = node["varName"] as? String else {
        throw ExpressionError("VarRef without a varName")
    }
    guard let bound = ctx.env[name] else {
        throw ExpressionError("Unbound variable \"\(name)\"")
    }
    return bound
}

/// The identity-domain mirror: tx.VarRef -> refEnv member URI. Same division as
/// `resolve` — the kernel owns UriLiteral, the caller owns bindings.
private let resolveRef: ResolveRef<Ctx> = { node, ctx in
    guard node["type"] as? String == varRef else {
        throw ExpressionError("No reference resolver for leaf '\(node["type"] as? String ?? "")'")
    }
    guard let name = node["varName"] as? String else {
        throw ExpressionError("VarRef without a varName")
    }
    guard let bound = ctx.refEnv[name] else {
        throw ExpressionError("Unbound reference \"\(name)\"")
    }
    return bound
}

/// Structural equality of a produced verdict tree against the vector's expected
/// JSON tree, including absent-vs-present refs.
private func traceMatches(_ got: TraceNode, _ want: [String: Any]) -> Bool {
    guard got.type == want["type"] as? String else { return false }
    guard got.value == (want["value"] as? NSNumber)?.doubleValue else { return false }
    guard got.leftRef == want["leftRef"] as? String else { return false }
    guard got.rightRef == want["rightRef"] as? String else { return false }
    let wantChildren = want["children"] as? [[String: Any]] ?? []
    guard got.children.count == wantChildren.count else { return false }
    return zip(got.children, wantChildren).allSatisfy { pair in traceMatches(pair.0, pair.1) }
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
            let refEnv = v["refEnv"] as? [String: String] ?? [:]
            let ctx = Ctx(env: env, refEnv: refEnv)

            var closures: ClosureTable? = nil
            if let raw = v["closures"] as? [String: Any] {
                var table: ClosureTable = [:]
                for (prop, members) in raw {
                    var inner: [String: [String]] = [:]
                    for (from, reachable) in (members as? [String: Any] ?? [:]) {
                        inner[from] = (reachable as? [Any])?.compactMap { $0 as? String } ?? []
                    }
                    table[prop] = inner
                }
                closures = table
            }
            let options = EvalOptions<Ctx>(closures: closures, resolveRef: resolveRef)

            if v["expectError"] as? Bool ?? false {
                XCTAssertThrowsError(try evaluate(expr, ctx, resolve, options), "[\(id)] expected an error from evaluate")
                XCTAssertThrowsError(try explain(expr, ctx, resolve, options), "[\(id)] expected an error from explain")
                evaluated += 1
                continue
            }

            let expected = (v["expected"] as! NSNumber).doubleValue
            let got = try evaluate(expr, ctx, resolve, options)
            let trace = try explain(expr, ctx, resolve, options)
            if let tolerance = (v["tolerance"] as? NSNumber)?.doubleValue {
                XCTAssertEqual(got, expected, accuracy: tolerance, "[\(id)]")
            } else {
                XCTAssertEqual(got, expected, "[\(id)]")
            }
            XCTAssertEqual(trace.value, got, "[\(id)] explain value must equal evaluate value")
            if let wantTrace = v["trace"] as? [String: Any] {
                XCTAssertTrue(traceMatches(trace, wantTrace), "[\(id)] trace mismatch")
            }
            evaluated += 1
        }

        XCTAssertEqual(evaluated, vectors.count, "every vector must be evaluated")
        print("expression-vectors: \(evaluated)/\(vectors.count) evaluated")
    }
}
