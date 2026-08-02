// `KanonakExpression` — the Kanonak expression RUNTIME, Swift port.
//
// A small, deterministic tree-walker that folds a `kanonak.org/transformations`
// (`tx`) + `kanonak.org/math` expression tree to a single number. Generated SDKs
// reference it so a typed expression can be *run*, not just *represented*.
//
// Three layers, matching every other port:
//
//   1. DISPATCH — derived from the ontology. An operator's arity falls out of its
//      `tx` superclass: UnaryNumericOp -> unary `value`; BinaryArithmetic /
//      BinaryComparison -> binary; BooleanLogic -> n-ary `operands`; plus the two
//      structural shapes the hierarchy can't imply (`Not`'s `operand`, `Clip`'s
//      ternary). `operatorArity` below is that derivation, frozen.
//
//   2. PRIMITIVES — the one authored, determinism-bearing artifact. `unaryOps` /
//      `binaryOps` map each operator URI to its fold. The determinism traps live
//      here and are matched in every language port (Round half away from zero,
//      floored Modulo, Sign(0)=0, comparisons as 1/0).
//
//   3. THE FOLD — `evaluate`, a fixed shape: operators recurse + apply a
//      primitive; literals return their numeric value; EVERYTHING ELSE (a typed
//      VarRef, a domain `Step`/`Time`/`Smooth`, any future leaf) is handed to the
//      caller's `resolve(node, ctx, evaluate)`. The runtime is a pure operator
//      engine; binding and domain-leaf semantics are the caller's business. It
//      never privileges `tx.VarRef` — that is just one leaf a domain may resolve.
//
// Value domain: uniform numeric (`Double`). Booleans and comparison results are
// `1`/`0`, so every language stays on one numeric path.
// `expressionRuntimeVersion` freezes the determinism contract; a change to any
// primitive, value rule, or dispatch requires a NEW version, never an edit in
// place.
//
// Node representation is `[String: Any]` — the same dictionary node contract the
// codec port uses, so a node decoded with `JSONSerialization` feeds straight in.

import Foundation

/// The frozen expression-runtime version (determinism contract). Not hashed.
public let expressionRuntimeVersion = "1"

public struct ExpressionError: Error, CustomStringConvertible {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var description: String { message }
}

private let tx = "kanonak.org/transformations"
private let math = "kanonak.org/math"

/// A node in the expression tree: `type` is the operator/literal/leaf canonical
/// URI (versionless: `publisher/package/name`); operand keys are the frozen `tx`
/// operand property local names. Unknown fields are ignored by the kernel and are
/// available to `resolve` for domain leaves.
public typealias ExprNode = [String: Any]

/// Recurse back into the kernel — handed to `resolve` so a domain leaf holding
/// sub-expressions can evaluate them.
public typealias Evaluator<C> = (ExprNode, C) throws -> Double

/// Resolve any node the kernel does not recognise as an operator or literal — a
/// binding (`tx.VarRef`, a domain's typed `refersTo` VarRef) or a domain leaf
/// (`Step`, `Time`, `Smooth`…) — to a number. `ctx` is opaque caller state (the
/// binding env, a sim clock, integration state).
public typealias Resolve<C> = (ExprNode, C, Evaluator<C>) throws -> Double

// ---------------------------------------------------------------------------
// Layer 1: dispatch
// ---------------------------------------------------------------------------

/// Operand shape per operator, derived from the `tx` superclass hierarchy.
private enum Arity {
    case unary(String)
    case binary(String, String)
    case nary(String)
    case ternary(String, String, String)
}

private let arith = Arity.binary("arithLeft", "arithRight")
private let compare = Arity.binary("compareLeft", "compareRight")
private let value = Arity.unary("value")

private let operatorArity: [String: Arity] = [
    "\(tx)/Add": arith,
    "\(tx)/Subtract": arith,
    "\(tx)/Multiply": arith,
    "\(tx)/Divide": arith,
    "\(math)/Power": arith,
    "\(math)/Modulo": arith,
    "\(math)/Minimum": arith,
    "\(math)/Maximum": arith,

    "\(tx)/Abs": value,
    "\(tx)/Negate": value,
    "\(math)/Exp": value,
    "\(math)/Ln": value,
    "\(math)/Log10": value,
    "\(math)/Sqrt": value,
    "\(math)/Floor": value,
    "\(math)/Ceil": value,
    "\(math)/Round": value,
    "\(math)/Sign": value,

    "\(tx)/Equals": compare,
    "\(tx)/GreaterThan": compare,
    "\(tx)/LessThan": compare,
    "\(tx)/GreaterThanOrEqual": compare,
    "\(tx)/LessThanOrEqual": compare,

    "\(tx)/And": .nary("operands"),
    "\(tx)/Or": .nary("operands"),
    // `Not` is a direct Expression subclass with boolean (not numeric-unary)
    // semantics — handled explicitly in `evaluate`, not via the numeric tables.

    "\(math)/Clip": .ternary("clipValue", "clipLower", "clipUpper"),
]

// ---------------------------------------------------------------------------
// Layer 2: primitives (the determinism-bearing table)
// ---------------------------------------------------------------------------

private func requireDomain(_ ok: Bool, _ message: String) throws {
    if !ok { throw ExpressionError(message) }
}

/// Floored modulo (the host `%`/`truncatingRemainder` truncates toward zero):
/// Modulo(-7, 3) = 2.
private func flooredMod(_ a: Double, _ b: Double) throws -> Double {
    try requireDomain(b != 0, "Modulo by zero")
    return a - b * (a / b).rounded(.down)
}

private func boolValue(_ b: Bool) -> Double { b ? 1 : 0 }
private func truthy(_ n: Double) -> Bool { n != 0 }

private let unaryOps: [String: (Double) throws -> Double] = [
    "\(tx)/Abs": { abs($0) },
    "\(tx)/Negate": { -$0 },
    "\(math)/Exp": { exp($0) },
    "\(math)/Ln": { try requireDomain($0 > 0, "Ln of a non-positive number"); return log($0) },
    "\(math)/Log10": { try requireDomain($0 > 0, "Log10 of a non-positive number"); return log10($0) },
    "\(math)/Sqrt": { try requireDomain($0 >= 0, "Sqrt of a negative number"); return sqrt($0) },
    "\(math)/Floor": { $0.rounded(.down) },
    "\(math)/Ceil": { $0.rounded(.up) },
    // Round half AWAY FROM ZERO: Round(-2.5) = -3, Round(2.5) = 3.
    "\(math)/Round": { $0.rounded(.toNearestOrAwayFromZero) },
    // Sign(0) = 0, Sign(neg) = -1, Sign(pos) = 1.
    "\(math)/Sign": { $0 > 0 ? 1 : ($0 < 0 ? -1 : 0) },
]

private let binaryOps: [String: (Double, Double) throws -> Double] = [
    "\(tx)/Add": { $0 + $1 },
    "\(tx)/Subtract": { $0 - $1 },
    "\(tx)/Multiply": { $0 * $1 },
    "\(tx)/Divide": { try requireDomain($1 != 0, "Divide by zero"); return $0 / $1 },
    "\(math)/Power": { pow($0, $1) },
    "\(math)/Modulo": flooredMod,
    "\(math)/Minimum": { Swift.min($0, $1) },
    "\(math)/Maximum": { Swift.max($0, $1) },
    "\(tx)/Equals": { boolValue($0 == $1) },
    "\(tx)/GreaterThan": { boolValue($0 > $1) },
    "\(tx)/LessThan": { boolValue($0 < $1) },
    "\(tx)/GreaterThanOrEqual": { boolValue($0 >= $1) },
    "\(tx)/LessThanOrEqual": { boolValue($0 <= $1) },
]

// ---------------------------------------------------------------------------
// Layer 3: the fold
// ---------------------------------------------------------------------------

/// The numeric value of a JSON-decoded scalar. `NSNumber` (what
/// `JSONSerialization` yields on both Darwin and corelibs), the native numeric
/// types, and a numeric string — matching the reference port's `Number(...)`
/// coercion. Returns nil when the value carries no number.
private func numericValue(_ raw: Any?) -> Double? {
    guard let raw = raw else { return nil }
    if let d = raw as? Double { return d }
    if let i = raw as? Int { return Double(i) }
    if let n = raw as? NSNumber { return n.doubleValue }
    if let s = raw as? String { return Double(s) }
    return nil
}

/// The numeric value of a literal node, or nil if it is not a literal.
private func literalValue(_ node: ExprNode, _ type: String) -> Double? {
    switch type {
    case "\(tx)/IntegerLiteral":
        return numericValue(node["integerLiteral"])
    case "\(tx)/DecimalLiteral":
        return numericValue(node["decimalLiteral"])
    case "\(tx)/BooleanLiteral":
        // `true`/`false` (or the strings) are the contract. A `booleanLiteral`
        // carrying anything else is outside it and folds to 0, as in the
        // reference port — parity beats a local opinion here.
        if let s = node["booleanLiteral"] as? String { return s == "true" ? 1 : 0 }
        if let b = node["booleanLiteral"] as? Bool { return boolValue(b) }
        return 0
    default:
        return nil
    }
}

private func operand(_ node: ExprNode, _ key: String, _ type: String) throws -> ExprNode {
    guard let child = node[key] as? ExprNode else {
        throw ExpressionError("\(type) is missing operand '\(key)'")
    }
    return child
}

/// Evaluate an expression tree to a number. Operators fold via the frozen
/// dispatch + primitive tables; literals yield their numeric value; any other
/// node is delegated to `resolve`.
public func evaluate<C>(_ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>) throws -> Double {
    let recurse: Evaluator<C> = { n, c in try evaluate(n, c, resolve) }

    // A node with no `type` matches no operator and no literal, and falls
    // through to the caller's resolve — same as every other port.
    let type = node["type"] as? String ?? ""

    if let arity = operatorArity[type] {
        switch arity {
        case .unary(let key):
            let x = try recurse(try operand(node, key, type), ctx)
            guard let op = unaryOps[type] else {
                throw ExpressionError("no primitive for unary operator '\(type)'")
            }
            return try op(x)

        case .binary(let leftKey, let rightKey):
            let a = try recurse(try operand(node, leftKey, type), ctx)
            let b = try recurse(try operand(node, rightKey, type), ctx)
            guard let op = binaryOps[type] else {
                throw ExpressionError("no primitive for binary operator '\(type)'")
            }
            return try op(a, b)

        case .nary(let key):
            guard let items = node[key] as? [Any] else {
                throw ExpressionError("\(type) expects an '\(key)' list")
            }
            let isAnd = type == "\(tx)/And"
            // Short-circuit; empty And is vacuously true, empty Or vacuously false.
            for item in items {
                guard let child = item as? ExprNode else {
                    throw ExpressionError("\(type): '\(key)' list holds a non-node item")
                }
                let v = truthy(try recurse(child, ctx))
                if isAnd && !v { return 0 }
                if !isAnd && v { return 1 }
            }
            return boolValue(isAnd)

        case .ternary(let aKey, let bKey, let cKey):
            // Only Clip today: clamp clipValue into [clipLower, clipUpper].
            let v = try recurse(try operand(node, aKey, type), ctx)
            let lo = try recurse(try operand(node, bKey, type), ctx)
            let hi = try recurse(try operand(node, cKey, type), ctx)
            return Swift.min(Swift.max(v, lo), hi)
        }
    }

    if type == "\(tx)/Not" {
        return boolValue(!truthy(try recurse(try operand(node, "operand", type), ctx)))
    }

    if let lit = literalValue(node, type) { return lit }

    // Not an operator or literal — a binding or domain leaf. The caller owns it.
    return try resolve(node, ctx, recurse)
}
