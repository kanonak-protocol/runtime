// Conformance: drive the Swift port with the shared expression parity vectors —
// BOTH files: expression-vectors.json (v1 — passes UNCHANGED under the v2
// kernel; the numeric-regression gate) and expression-vectors-2.json (the
// value-domain extension). env bindings and expected are Values (numbers,
// strings, arrays, {"ref": …} objects). Every vector runs through `evaluate`
// AND `explain` and their values must agree; vectors with a `trace` assert the
// verdict tree structurally.

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

/// JSON -> Value, the vector-file encoding. Booleans normalize to 1/0 — and
/// JSONSerialization surfaces them as NSNumber, whose doubleValue is already
/// exactly that normalization, so no boolean special-case is needed (and the
/// CoreFoundation type-id probe it would take is not portable to
/// swift-corelibs-foundation on Linux).
private func valueOf(_ v: Any) throws -> EvalValue {
    if let n = v as? NSNumber { return .num(n.doubleValue) }
    if let s = v as? String { return .str(s) }
    if let arr = v as? [Any] { return .list(try arr.map(valueOf)) }
    if let obj = v as? [String: Any], let r = obj["ref"] as? String { return .ref(r) }
    throw ExpressionError("unrepresentable vector value: \(v)")
}

/// Deep Value equality — the vector-comparison rule (lists structural; EvalValue
/// is Equatable with exactly these semantics).
private func deepEqual(_ a: EvalValue, _ b: EvalValue) -> Bool { a == b }

/// The conformance context: value bindings (env) plus identity bindings (refEnv).
private struct Ctx {
    let env: [String: EvalValue]
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

/// The identity-domain mirror: tx.VarRef -> refEnv member URI.
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

private func closuresOf(_ v: [String: Any]) -> ClosureTable? {
    guard let raw = v["closures"] as? [String: Any] else { return nil }
    var table: ClosureTable = [:]
    for (prop, members) in raw {
        var inner: [String: [String]] = [:]
        if let m = members as? [String: Any] {
            for (from, reach) in m {
                inner[from] = (reach as? [Any])?.compactMap { $0 as? String } ?? []
            }
        }
        table[prop] = inner
    }
    return table
}

/// Structural equality of a verdict tree against the vector's expected JSON tree.
private func traceMatches(_ got: TraceNode, _ want: [String: Any]) throws -> Bool {
    guard want["type"] as? String == got.type else { return false }
    guard let wantValue = want["value"], deepEqual(got.value, try valueOf(wantValue)) else { return false }
    if want["leftRef"] as? String != got.leftRef { return false }
    if want["rightRef"] as? String != got.rightRef { return false }
    let wantChildren = want["children"] as? [[String: Any]] ?? []
    guard wantChildren.count == got.children.count else { return false }
    for (g, w) in zip(got.children, wantChildren) {
        if !(try traceMatches(g, w)) { return false }
    }
    return true
}

final class ExpressionVectorTests: XCTestCase {
    private func runFile(_ name: String) throws {
        let doc = try loadJSON(name)
        guard let vectors = doc["vectors"] as? [[String: Any]], !vectors.isEmpty else {
            XCTFail("\(name): no vectors loaded — refusing to report a passing gate")
            return
        }

        var pass = 0
        for v in vectors {
            let id = v["id"] as? String ?? "(no id)"
            guard let expr = v["expr"] as? [String: Any] else {
                XCTFail("[\(name)/\(id)] missing expr")
                continue
            }
            var env: [String: EvalValue] = [:]
            for (k, x) in (v["env"] as? [String: Any] ?? [:]) {
                env[k] = try valueOf(x)
            }
            let refEnv = (v["refEnv"] as? [String: String]) ?? [:]
            let ctx = Ctx(env: env, refEnv: refEnv)
            let options = EvalOptions<Ctx>(closures: closuresOf(v), resolveRef: resolveRef)

            let expectError = v["expectError"] as? Bool ?? false
            if expectError {
                var evalThrew = false
                var explainThrew = false
                do { _ = try evaluate(expr, ctx, resolve, options: options) } catch { evalThrew = true }
                do { _ = try explain(expr, ctx, resolve, options: options) } catch { explainThrew = true }
                if evalThrew && explainThrew { pass += 1 }
                else { XCTFail("[\(name)/\(id)] expected an error from evaluate AND explain") }
                continue
            }

            let got: EvalValue
            let traced: TraceNode
            do {
                got = try evaluate(expr, ctx, resolve, options: options)
                traced = try explain(expr, ctx, resolve, options: options)
            } catch {
                XCTFail("[\(name)/\(id)] threw: \(error)")
                continue
            }

            let expected = try valueOf(v["expected"] ?? NSNull())
            let ok: Bool
            if let tol = (v["tolerance"] as? NSNumber)?.doubleValue {
                if case let .num(g) = got, case let .num(e) = expected {
                    ok = abs(g - e) <= tol
                } else {
                    ok = false
                }
            } else {
                ok = deepEqual(got, expected)
            }
            guard ok else {
                XCTFail("[\(name)/\(id)] expected \(expected), got \(got)")
                continue
            }
            guard deepEqual(traced.value, got) else {
                XCTFail("[\(name)/\(id)] explain value \(traced.value) != evaluate value \(got)")
                continue
            }
            if let wantTrace = v["trace"] as? [String: Any] {
                guard try traceMatches(traced, wantTrace) else {
                    XCTFail("[\(name)/\(id)] trace mismatch")
                    continue
                }
            }
            pass += 1
        }
        print("\(name): \(pass)/\(vectors.count) pass")
        XCTAssertEqual(pass, vectors.count, "\(name): \(vectors.count - pass) vector(s) failed")
    }

    func testExpressionVectors() throws {
        // v1 vectors are the regression gate: every one passes unchanged under v2.
        try runFile("expression-vectors.json")
        try runFile("expression-vectors-2.json")
    }
}
