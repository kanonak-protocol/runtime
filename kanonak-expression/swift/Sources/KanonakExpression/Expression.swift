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

/// Resolve an identity leaf inside an ordered comparison — any operand node that
/// is not a `tx.UriLiteral` — to a member's canonical versionless URI
/// (`publisher/package/name`). The identity-domain mirror of `Resolve`: the
/// kernel owns the constant leaf, the caller owns bindings.
public typealias ResolveRef<C> = (ExprNode, C) throws -> String

/// The transitive closures ordered comparisons consult, keyed by the ordering
/// property's canonical URI, then by member: `closures[property][from]` is the
/// set of members `from` reaches. Flat, already-closed data — typically the SDK
/// reasoner's `prp-trp` saturation emitted at code-generation time. The kernel
/// does set membership only; it never computes a closure, resolves a package, or
/// reasons.
public typealias ClosureTable = [String: [String: [String]]]

/// Optional evaluation context for the ordered comparisons (`IsAtLeast`,
/// `Dominates`). Absent (or missing a needed entry), an ordered comparison fails
/// loudly — never a silent false from a missing table.
public struct EvalOptions<C> {
    public let closures: ClosureTable?
    public let resolveRef: ResolveRef<C>?
    public init(closures: ClosureTable? = nil, resolveRef: ResolveRef<C>? = nil) {
        self.closures = closures
        self.resolveRef = resolveRef
    }
}

/// One node of an evaluation trace — the verdict tree `explain` returns. Mirrors
/// the expression: `type` is the node's type URI, `value` its result (`1`/`0`
/// for booleans), `children` the operand traces in evaluation order. A
/// short-circuited operand is simply ABSENT from `children` — the trace is
/// truthful about what ran. Ordered comparisons carry their resolved operand
/// identities as `leftRef`/`rightRef` instead of children. A runtime return
/// shape, not an ontology class.
public struct TraceNode {
    public let type: String
    public let value: Double
    public let children: [TraceNode]
    public let leftRef: String?
    public let rightRef: String?
    init(_ type: String, _ value: Double, _ children: [TraceNode] = [],
         leftRef: String? = nil, rightRef: String? = nil) {
        self.type = type
        self.value = value
        self.children = children
        self.leftRef = leftRef
        self.rightRef = rightRef
    }
}

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

/// The identity an ordered comparison compares — a member's canonical
/// versionless URI. `tx.UriLiteral` is the kernel-known constant leaf (its
/// `refTo` IS the identity, the way a literal's value is its number); every
/// other node is the caller's, through `options.resolveRef`.
private func identityOf<C>(_ node: ExprNode, _ ctx: C, _ options: EvalOptions<C>?) throws -> String {
    let type = node["type"] as? String ?? ""
    if type == "\(tx)/UriLiteral" {
        guard let ref = node["refTo"] as? String, !ref.isEmpty else {
            throw ExpressionError("UriLiteral is missing refTo")
        }
        return ref
    }
    guard let resolveRef = options?.resolveRef else {
        throw ExpressionError("No resolveRef supplied for identity leaf '\(type)'")
    }
    return try resolveRef(node, ctx)
}

/// Fold an ordered comparison (`IsAtLeast` / `Dominates`) to `1`/`0` plus the
/// resolved operand identities. The ordering is the supplied closure for the
/// node's `viaProperty` — membership in already-closed data, nothing more.
/// Identity is canonical versionless URI string equality, matching `tx.Equals`'
/// identity rule. `IsAtLeast` folds reflexivity into the operator (same member
/// → 1); `Dominates` is strict (same member → 0). Two members with no path
/// yield 0 — fail-closed — but a MISSING closure table is a configuration
/// failure and errors loudly.
private func foldOrdered<C>(
    _ node: ExprNode, _ type: String, _ ctx: C, _ options: EvalOptions<C>?
) throws -> (value: Double, left: String, right: String) {
    guard let via = node["viaProperty"] as? String, !via.isEmpty else {
        throw ExpressionError("\(type) is missing viaProperty")
    }
    let left = try identityOf(try operand(node, "compareLeft", type), ctx, options)
    let right = try identityOf(try operand(node, "compareRight", type), ctx, options)
    guard let closure = options?.closures?[via] else {
        throw ExpressionError("No closure supplied for ordering property '\(via)'")
    }
    if left == right {
        return (boolValue(type == "\(tx)/IsAtLeast"), left, right)
    }
    return (boolValue(closure[left]?.contains(right) ?? false), left, right)
}

/// Evaluate an expression tree to a number. Operators fold via the frozen
/// dispatch + primitive tables; literals yield their numeric value; any other
/// node is delegated to `resolve`.
public func evaluate<C>(_ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>) throws -> Double {
    try evaluate(node, ctx, resolve, nil)
}

/// `evaluate` with the ordered-comparison evaluation context (closures +
/// identity-leaf resolution). `options` is only consulted when an `IsAtLeast` /
/// `Dominates` node is reached; `nil` is valid for trees without them.
public func evaluate<C>(
    _ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>, _ options: EvalOptions<C>?
) throws -> Double {
    let recurse: Evaluator<C> = { n, c in try evaluate(n, c, resolve, options) }

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

    if type == "\(tx)/IsAtLeast" || type == "\(tx)/Dominates" {
        return try foldOrdered(node, type, ctx, options).value
    }

    if let lit = literalValue(node, type) { return lit }

    // Not an operator or literal — a binding or domain leaf. The caller owns it.
    return try resolve(node, ctx, recurse)
}

/// Evaluate an expression tree and return the verdict tree — the regex-debugger
/// view: every evaluated node, its own result, and (for ordered comparisons)
/// the identities it compared. The root's `value` is exactly what `evaluate`
/// returns for the same inputs; the conformance suite runs every vector through
/// both and requires agreement, so the two entry points cannot drift. Kept
/// separate from `evaluate` so the hot path never pays for trace allocation.
/// Errors propagate exactly as in `evaluate` — a failed evaluation throws,
/// never a partial trace.
public func explain<C>(
    _ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>, _ options: EvalOptions<C>? = nil
) throws -> TraceNode {
    // Numeric recursion for subtrees the caller's `resolve` re-enters: those
    // folds happen inside the caller and are invisible to the trace. Only
    // kernel-visited nodes appear.
    let recurseValue: Evaluator<C> = { n, c in try evaluate(n, c, resolve, options) }

    let type = node["type"] as? String ?? ""

    if let arity = operatorArity[type] {
        switch arity {
        case .unary(let key):
            let x = try explain(try operand(node, key, type), ctx, resolve, options)
            guard let op = unaryOps[type] else {
                throw ExpressionError("no primitive for unary operator '\(type)'")
            }
            return TraceNode(type, try op(x.value), [x])

        case .binary(let leftKey, let rightKey):
            let a = try explain(try operand(node, leftKey, type), ctx, resolve, options)
            let b = try explain(try operand(node, rightKey, type), ctx, resolve, options)
            guard let op = binaryOps[type] else {
                throw ExpressionError("no primitive for binary operator '\(type)'")
            }
            return TraceNode(type, try op(a.value, b.value), [a, b])

        case .nary(let key):
            guard let items = node[key] as? [Any] else {
                throw ExpressionError("\(type) expects an '\(key)' list")
            }
            let isAnd = type == "\(tx)/And"
            var children: [TraceNode] = []
            for item in items {
                guard let child = item as? ExprNode else {
                    throw ExpressionError("\(type): '\(key)' list holds a non-node item")
                }
                let childTrace = try explain(child, ctx, resolve, options)
                children.append(childTrace)
                let v = truthy(childTrace.value)
                // Same short-circuit as `evaluate`: operands after the deciding
                // one are never evaluated and never appear in the trace.
                if isAnd && !v { return TraceNode(type, 0, children) }
                if !isAnd && v { return TraceNode(type, 1, children) }
            }
            return TraceNode(type, boolValue(isAnd), children)

        case .ternary(let aKey, let bKey, let cKey):
            let v = try explain(try operand(node, aKey, type), ctx, resolve, options)
            let lo = try explain(try operand(node, bKey, type), ctx, resolve, options)
            let hi = try explain(try operand(node, cKey, type), ctx, resolve, options)
            return TraceNode(type, Swift.min(Swift.max(v.value, lo.value), hi.value), [v, lo, hi])
        }
    }

    if type == "\(tx)/Not" {
        let x = try explain(try operand(node, "operand", type), ctx, resolve, options)
        return TraceNode(type, boolValue(!truthy(x.value)), [x])
    }

    if type == "\(tx)/IsAtLeast" || type == "\(tx)/Dominates" {
        let r = try foldOrdered(node, type, ctx, options)
        return TraceNode(type, r.value, [], leftRef: r.left, rightRef: r.right)
    }

    if let lit = literalValue(node, type) { return TraceNode(type, lit) }

    return TraceNode(type, try resolve(node, ctx, recurseValue))
}
