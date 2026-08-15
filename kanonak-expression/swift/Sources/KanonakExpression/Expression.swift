// `KanonakExpression` — the Kanonak expression RUNTIME, Swift port
// (expressionRuntimeVersion "2").
//
// A small, deterministic tree-walker that folds a `kanonak.org/transformations`
// (`tx`) + `kanonak.org/math` expression tree to a VALUE. Generated SDKs
// reference it so a typed expression can be *run*, not just *represented*.
//
// VALUE DOMAIN (v2): `number | string | ref | list` (`EvalValue`). Booleans and
// comparison results remain `1`/`0` numbers; a ref is a canonical versionless
// URI identity (distinct from string so Equals holds the cross-kind-is-false
// rule); ABSENCE IS THE EMPTY LIST — there is no nil value. The caller's
// `resolve` may return any value: a property read is just a caller leaf that
// returns a list. The kernel NEVER touches a graph.
//
// ERROR CONTRACT: computations fail LOUD (arithmetic on a non-number, an
// aggregate over non-numeric elements, Min/Max/Average on empty, a nested list
// in Join, an out-of-subset Matches pattern); predicates fail CLOSED (`Equals`
// cross-kind and the ordering comparisons on non-numbers yield `0`).
//
// LAMBDA BINDING: Filter/ListMap/ForEach bind their `loopVar` per element;
// within their bodies — and only there — a `tx.VarRef` naming a
// lexically-enclosing loopVar is resolved by the kernel (innermost binder
// wins). Recursion re-entered from inside `resolve` carries no frames.
//
// MATCHES: the pinned RE2-compatible XSD-regex subset via NSRegularExpression
// (ICU). ICU counts code points natively; the dialect translations THIS engine
// owes: `\v` (a vertical-whitespace CLASS in ICU) becomes `\x0B`; non-dotAll
// `.` (ICU excludes more line terminators than the pinned `[^\n]`) translates
// to `[^\n]`; `\b`/`\B` (Unicode in ICU) become explicit ASCII-word-class
// lookarounds. The shorthand expansions are load-bearing (ICU's native
// `\d`/`\w`/`\s` are Unicode); `(?i)` is ICU's Unicode case folding, matching
// the pin natively.
//
// Node representation is `[String: Any]` — the same dictionary node contract
// the codec port uses, so a node decoded with `JSONSerialization` feeds
// straight in.

import Foundation

/// The frozen expression-runtime version (determinism contract). Not hashed.
public let expressionRuntimeVersion = "2"

public struct ExpressionError: Error, CustomStringConvertible {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var description: String { message }
}

private let tx = "kanonak.org/transformations"
private let math = "kanonak.org/math"

/// A node in the expression tree: `type` is the operator/literal/leaf canonical
/// URI; operand keys are the frozen `tx` operand property local names.
public typealias ExprNode = [String: Any]

/// The v2 value domain. Booleans are `1`/`0` numbers; absence is the empty list.
public indirect enum EvalValue: Equatable {
    case num(Double)
    case str(String)
    case ref(String)
    case list([EvalValue])

    var kind: String {
        switch self {
        case .num: return "number"
        case .str: return "string"
        case .ref: return "ref"
        case .list: return "list"
        }
    }
}

/// Recurse back into the kernel — handed to `resolve` so a domain leaf holding
/// sub-expressions can evaluate them (WITHOUT lambda frames).
public typealias Evaluator<C> = (ExprNode, C) throws -> EvalValue

/// Resolve any node the kernel does not recognise as an operator or literal — a
/// binding (`tx.VarRef`), a host graph read (a property-read leaf returning a
/// list), or a domain leaf — to a value.
public typealias Resolve<C> = (ExprNode, C, Evaluator<C>) throws -> EvalValue

/// Resolve an identity leaf inside an ordered comparison.
public typealias ResolveRef<C> = (ExprNode, C) throws -> String

/// The transitive closures ordered comparisons consult.
public typealias ClosureTable = [String: [String: [String]]]

/// Optional evaluation context for the ordered comparisons.
public struct EvalOptions<C> {
    public let closures: ClosureTable?
    public let resolveRef: ResolveRef<C>?
    public init(closures: ClosureTable? = nil, resolveRef: ResolveRef<C>? = nil) {
        self.closures = closures
        self.resolveRef = resolveRef
    }
}

/// One node of an evaluation trace — the verdict tree `explain` returns.
/// Short-circuited operands are ABSENT from `children`; an iterating operator's
/// children are its source trace followed by one body trace per visited
/// element. A runtime return shape, not an ontology class.
public struct TraceNode {
    public let type: String
    public let value: EvalValue
    public let children: [TraceNode]
    public let leftRef: String?
    public let rightRef: String?
    init(_ type: String, _ value: EvalValue, _ children: [TraceNode] = [],
         leftRef: String? = nil, rightRef: String? = nil) {
        self.type = type
        self.value = value
        self.children = children
        self.leftRef = leftRef
        self.rightRef = rightRef
    }
}

// MARK: - Dispatch

private enum Arity {
    case unary(String)
    case binary(String, String)
    case nary(String)
    case ternary(String, String, String)
}

private func operatorArity(_ typ: String) -> Arity? {
    switch typ {
    case "\(tx)/Add", "\(tx)/Subtract", "\(tx)/Multiply", "\(tx)/Divide",
         "\(math)/Power", "\(math)/Modulo", "\(math)/Minimum", "\(math)/Maximum":
        return .binary("arithLeft", "arithRight")
    case "\(tx)/Abs", "\(tx)/Negate", "\(math)/Exp", "\(math)/Ln", "\(math)/Log10",
         "\(math)/Sqrt", "\(math)/Floor", "\(math)/Ceil", "\(math)/Round", "\(math)/Sign":
        return .unary("value")
    case "\(tx)/Equals", "\(tx)/GreaterThan", "\(tx)/LessThan",
         "\(tx)/GreaterThanOrEqual", "\(tx)/LessThanOrEqual":
        return .binary("compareLeft", "compareRight")
    case "\(tx)/And", "\(tx)/Or":
        return .nary("operands")
    case "\(math)/Clip":
        return .ternary("clipValue", "clipLower", "clipUpper")
    default:
        return nil
    }
}

private func iteratorBody(_ typ: String) -> String? {
    switch typ {
    case "\(tx)/ForEach": return "emit"
    case "\(tx)/ListMap": return "mapBody"
    case "\(tx)/Filter": return "predicate"
    default: return nil
    }
}

private func isListFold(_ typ: String) -> Bool {
    switch typ {
    case "\(tx)/Count", "\(tx)/Sum", "\(tx)/Min", "\(tx)/Max",
         "\(tx)/Average", "\(tx)/Join", "\(tx)/Reverse":
        return true
    default:
        return false
    }
}

// MARK: - Primitives

private func flooredMod(_ a: Double, _ b: Double) throws -> Double {
    guard b != 0 else { throw ExpressionError("Modulo by zero") }
    return a - b * (a / b).rounded(.down)
}

/// Round half away from zero: Round(-2.5) = -3, Round(2.5) = 3.
private func roundHalfAway(_ a: Double) -> Double {
    a < 0 ? -((-a + 0.5).rounded(.down)) : (a + 0.5).rounded(.down)
}

private func truthy(_ n: Double) -> Bool { n != 0 }
private func boolNum(_ b: Bool) -> Double { b ? 1 : 0 }

private func requireDomain(_ ok: Bool, _ msg: String) throws {
    if !ok { throw ExpressionError(msg) }
}

private func requireNum(_ v: EvalValue, _ op: String) throws -> Double {
    guard case let .num(n) = v else {
        throw ExpressionError("\(op) requires a numeric operand, got \(v.kind)")
    }
    return n
}

private func unaryPrim(_ typ: String, _ x: Double) throws -> Double {
    switch typ {
    case "\(tx)/Abs": return abs(x)
    case "\(tx)/Negate": return -x
    case "\(math)/Exp": return Foundation.exp(x)
    case "\(math)/Ln":
        try requireDomain(x > 0, "Ln of a non-positive number")
        return Foundation.log(x)
    case "\(math)/Log10":
        try requireDomain(x > 0, "Log10 of a non-positive number")
        return Foundation.log10(x)
    case "\(math)/Sqrt":
        try requireDomain(x >= 0, "Sqrt of a negative number")
        return Foundation.sqrt(x)
    case "\(math)/Floor": return x.rounded(.down)
    case "\(math)/Ceil": return x.rounded(.up)
    case "\(math)/Round": return roundHalfAway(x)
    case "\(math)/Sign": return x > 0 ? 1 : (x < 0 ? -1 : 0)
    default: throw ExpressionError("No unary primitive for \(typ)")
    }
}

private func binaryArith(_ typ: String, _ a: Double, _ b: Double) throws -> Double {
    switch typ {
    case "\(tx)/Add": return a + b
    case "\(tx)/Subtract": return a - b
    case "\(tx)/Multiply": return a * b
    case "\(tx)/Divide":
        try requireDomain(b != 0, "Divide by zero")
        return a / b
    case "\(math)/Power": return Foundation.pow(a, b)
    case "\(math)/Modulo": return try flooredMod(a, b)
    case "\(math)/Minimum": return Swift.min(a, b)
    case "\(math)/Maximum": return Swift.max(a, b)
    default: throw ExpressionError("No arithmetic primitive for \(typ)")
    }
}

private func isOrderComparison(_ typ: String) -> Bool {
    typ == "\(tx)/GreaterThan" || typ == "\(tx)/LessThan"
        || typ == "\(tx)/GreaterThanOrEqual" || typ == "\(tx)/LessThanOrEqual"
}

private func binaryOrder(_ typ: String, _ a: Double, _ b: Double) -> Double {
    switch typ {
    case "\(tx)/GreaterThan": return boolNum(a > b)
    case "\(tx)/LessThan": return boolNum(a < b)
    case "\(tx)/GreaterThanOrEqual": return boolNum(a >= b)
    default: return boolNum(a <= b)
    }
}

/// Polymorphic tx.Equals: scalars by value, refs by URI identity, lists never
/// equal, cross-kind false. Never errors.
private func valuesEqual(_ a: EvalValue, _ b: EvalValue) -> Bool {
    switch (a, b) {
    case let (.num(x), .num(y)): return x == y
    case let (.str(x), .str(y)): return x == y
    case let (.ref(x), .ref(y)): return x == y
    default: return false
    }
}

private func nodeType(_ node: ExprNode) throws -> String {
    guard let t = node["type"] as? String else {
        throw ExpressionError("Expression node has no 'type'")
    }
    return t
}

private func operand(_ node: ExprNode, _ typ: String, _ key: String) throws -> ExprNode {
    guard let v = node[key] as? ExprNode else {
        throw ExpressionError("\(typ) is missing operand '\(key)'")
    }
    return v
}

private func toNumber(_ v: Any?) throws -> Double {
    if let n = v as? Double { return n }
    if let n = v as? Int { return Double(n) }
    if let n = v as? NSNumber { return n.doubleValue }
    if let s = v as? String, let n = Double(s) { return n }
    throw ExpressionError("not a number: \(String(describing: v))")
}

/// A literal node's value, or nil when not a literal.
private func literalValue(_ node: ExprNode, _ typ: String) throws -> EvalValue? {
    switch typ {
    case "\(tx)/IntegerLiteral": return .num(try toNumber(node["integerLiteral"]))
    case "\(tx)/DecimalLiteral": return .num(try toNumber(node["decimalLiteral"]))
    case "\(tx)/BooleanLiteral":
        let b = node["booleanLiteral"]
        return .num(boolNum((b as? Bool) == true || (b as? String) == "true"))
    case "\(tx)/StringLiteral":
        guard let s = node["stringLiteral"] as? String else {
            throw ExpressionError("StringLiteral is missing stringLiteral")
        }
        return .str(s)
    case "\(tx)/UriLiteral":
        guard let s = node["refTo"] as? String, !s.isEmpty else {
            throw ExpressionError("UriLiteral is missing refTo")
        }
        return .ref(s)
    default:
        return nil
    }
}

/// ECMAScript-style number-to-string (the RFC 8785 rule): integral without a
/// decimal point; else Swift's shortest round-trip description.
private func formatNumber(_ n: Double) -> String {
    if n == n.rounded(.down) && abs(n) < 1e21 {
        return String(Int64(n))
    }
    return "\(n)"
}

private func joinElement(_ v: EvalValue) throws -> String {
    switch v {
    case let .str(s): return s
    case let .num(n): return formatNumber(n)
    case let .ref(r):
        if let i = r.lastIndex(of: "/") {
            return String(r[r.index(after: i)...])
        }
        return r
    case .list:
        throw ExpressionError("Join cannot stringify a nested list")
    }
}

private func isSetValue(_ v: EvalValue) -> Bool {
    switch v {
    case let .str(s): return !s.isEmpty
    case let .list(l): return !l.isEmpty
    default: return true
    }
}

private func listFold(_ typ: String, _ items: [EvalValue], _ node: ExprNode) throws -> EvalValue {
    switch typ {
    case "\(tx)/Count":
        return .num(Double(items.count))
    case "\(tx)/Sum":
        var total = 0.0
        for el in items { total += try requireNum(el, "Sum") }
        return .num(total)
    case "\(tx)/Min", "\(tx)/Max":
        let name = typ.hasSuffix("Min") ? "Min" : "Max"
        guard !items.isEmpty else {
            throw ExpressionError("\(name) on an empty list is undefined; guard with IsSet")
        }
        var best = try requireNum(items[0], name)
        for el in items.dropFirst() {
            let n = try requireNum(el, name)
            if (name == "Min" && n < best) || (name == "Max" && n > best) { best = n }
        }
        return .num(best)
    case "\(tx)/Average":
        guard !items.isEmpty else {
            throw ExpressionError("Average on an empty list is undefined; guard with IsSet")
        }
        var total = 0.0
        for el in items { total += try requireNum(el, "Average") }
        return .num(total / Double(items.count))
    case "\(tx)/Join":
        let sep = node["separator"] as? String ?? ""
        var parts: [String] = []
        parts.reserveCapacity(items.count)
        for el in items { parts.append(try joinElement(el)) }
        return .str(parts.joined(separator: sep))
    case "\(tx)/Reverse":
        return .list(items.reversed())
    default:
        throw ExpressionError("No list fold for \(typ)")
    }
}

private func kindPredicate(_ typ: String, _ v: EvalValue) -> Double? {
    switch typ {
    case "\(tx)/IsString": if case .str = v { return 1 } else { return 0 }
    case "\(tx)/IsNumber": if case .num = v { return 1 } else { return 0 }
    case "\(tx)/IsReference": if case .ref = v { return 1 } else { return 0 }
    case "\(tx)/IsList": if case .list = v { return 1 } else { return 0 }
    default: return nil
    }
}

// MARK: - Matches: the pinned RE2-compatible XSD-regex subset

private let allowedEscapes = Set("dDwWsSbBnrtfv.*+?()[]{}|^$\\/")

// The ASCII word-boundary lookarounds — ICU's native \b is Unicode.
private let wordClass = "[0-9A-Za-z_]"
private let bBoundary = "(?:(?<=\(wordClass))(?!\(wordClass))|(?<!\(wordClass))(?=\(wordClass)))"
private let bNonBoundary = "(?:(?<=\(wordClass))(?=\(wordClass))|(?<!\(wordClass))(?!\(wordClass)))"

private func isHex(_ c: Character) -> Bool {
    c.isHexDigit && c.isASCII
}

private func quantifierAt(_ chars: [Character], _ start: Int) -> Bool {
    var j = start + 1
    var sawDigit = false
    while j < chars.count, chars[j].isNumber { j += 1; sawDigit = true }
    guard sawDigit else { return false }
    if j < chars.count, chars[j] == "," {
        j += 1
        while j < chars.count, chars[j].isNumber { j += 1 }
    }
    return j < chars.count && chars[j] == "}"
}

/// Check a WHOLE pattern against the pinned subset, flag prefix included. Thin
/// wrapper over `parseMatchesPattern` so this checker and the evaluator can
/// never disagree about what is a valid pattern.
func validateMatchesPattern(_ pattern: String) throws {
    _ = try parseMatchesPattern(pattern)
}

/// THE definition of a valid `Matches` pattern: the single place that decides
/// what the pinned subset accepts AND how a whole pattern decomposes into its
/// flag prefix and body. Every diagnostic quotes the WHOLE pattern the caller
/// passed, never the flag-stripped body.
private func parseMatchesPattern(_ pattern: String) throws -> (flags: String, body: String) {
    func fail(_ what: String) throws -> Never {
        throw ExpressionError("Matches pattern is outside the pinned regex subset (\(what)): \(pattern)")
    }

    // Whole-pattern flag prefix - position 0 only, over i/m/s, EACH AT MOST
    // ONCE. The repeat rule is the subset's, not the host engine's: the JS
    // engine rejects (?ii) at compile time while the Go, Python and Rust
    // engines accept it, so the subset decides rather than the host.
    var flags = ""
    var body = pattern
    let whole = Array(pattern)
    if whole.count >= 4, whole[0] == "(", whole[1] == "?" {
        var j = 2
        while j < whole.count, whole[j] == "i" || whole[j] == "m" || whole[j] == "s" { j += 1 }
        if j > 2, j < whole.count, whole[j] == ")" {
            flags = String(whole[2..<j])
            var seen = Set<Character>()
            for f in flags {
                if !seen.insert(f).inserted {
                    try fail("repeated flag in prefix (?\(flags))")
                }
            }
            body = String(whole[(j + 1)...])
        }
    }

    let chars = Array(body)
    var inClass = false
    var i = 0
    while i < chars.count {
        let c = chars[i]
        if c == "\\" {
            guard i + 1 < chars.count else { try fail("trailing backslash") }
            let e = chars[i + 1]
            if e == "x" {
                if i + 2 < chars.count, chars[i + 2] == "{" { try fail("\\x{…} escape") }
                guard i + 3 < chars.count, isHex(chars[i + 2]), isHex(chars[i + 3]) else {
                    try fail("\\x escape must be \\xHH")
                }
                i += 4
                continue
            }
            if e.isNumber { try fail("backreference or octal escape \\\(e)") }
            if e == "p" || e == "P" { try fail("unicode property class \\\(e){…}") }
            if e == "k" { try fail("named backreference \\k") }
            if e == "u" { try fail("\\u escape") }
            if e == "-" {
                if !inClass { try fail("\\- outside a character class") }
                i += 2
                continue
            }
            if (e == "b" || e == "B" || e == "D" || e == "W" || e == "S") && inClass {
                try fail("\\\(e) inside a character class")
            }
            if !allowedEscapes.contains(e) { try fail("escape \\\(e)") }
            i += 2
            continue
        }
        if inClass {
            if c == "]" { inClass = false }
            else if c == "&", i + 1 < chars.count, chars[i + 1] == "&" { try fail("character-class intersection &&") }
            else if c == "[", i + 1 < chars.count, chars[i + 1] == ":" { try fail("POSIX class [[:…:]]") }
            i += 1
            continue
        }
        if c == "[" { inClass = true; i += 1; continue }
        if c == "(", i + 1 < chars.count, chars[i + 1] == "?" {
            // Only (?: survives mid-pattern; the flag prefix splits off first.
            guard i + 2 < chars.count, chars[i + 2] == ":" else { try fail("group construct (?") }
            i += 3
            continue
        }
        if c == "{" {
            // A bare `{` must start a valid quantifier — the SCANNER enforces
            // this uniformly (a literal brace is written \{).
            if !quantifierAt(chars, i) { try fail("bare '{' that is not a quantifier (write \\{)") }
        }
        i += 1
    }
    if inClass { try fail("unterminated character class") }
    return (flags, body)
}

/// The pinned ASCII expansions plus THIS engine's owed translations:
/// `\v` → `\x0B` (ICU's \v is a class), non-dotAll `.` → `[^\n]` (ICU excludes
/// more line terminators than the pin), and `\b`/`\B` → the explicit
/// ASCII-boundary lookarounds (ICU's \b is Unicode).
private func expandShorthandClasses(_ body: String, dotAll: Bool) -> String {
    let chars = Array(body)
    var out = ""
    out.reserveCapacity(body.count + 32)
    var inClass = false
    var i = 0
    while i < chars.count {
        let c = chars[i]
        if c == "\\" {
            let e = chars[i + 1] // subset-validated: never a trailing backslash
            var expansion: String?
            if inClass {
                switch e {
                case "d": expansion = "0-9"
                case "w": expansion = "0-9A-Za-z_"
                case "s": expansion = " \\t\\n\\r\\f\\x0B"
                case "v": expansion = "\\x0B"
                default: expansion = nil
                }
            } else {
                switch e {
                case "d": expansion = "[0-9]"
                case "D": expansion = "[^0-9]"
                case "w": expansion = "[0-9A-Za-z_]"
                case "W": expansion = "[^0-9A-Za-z_]"
                case "s": expansion = "[ \\t\\n\\r\\f\\x0B]"
                case "S": expansion = "[^ \\t\\n\\r\\f\\x0B]"
                case "b": expansion = bBoundary
                case "B": expansion = bNonBoundary
                case "v": expansion = "\\x0B"
                default: expansion = nil
                }
            }
            if let x = expansion { out += x } else { out.append(c); out.append(e) }
            i += 2
            continue
        }
        if !inClass && c == "." && !dotAll {
            out += "[^\\n]"
            i += 1
            continue
        }
        if !inClass && c == "[" { inClass = true }
        else if inClass && c == "]" { inClass = false }
        out.append(c)
        i += 1
    }
    return out
}

/// fn:matches semantics: UNANCHORED. ICU counts code points natively; `(?i)`
/// is ICU's Unicode case folding, matching the pin.
private func matchesPattern(_ input: String, _ pattern: String) throws -> Bool {
    let (flags, body) = try parseMatchesPattern(pattern)
    let dotAll = flags.contains("s")
    var options: NSRegularExpression.Options = []
    if flags.contains("i") { options.insert(.caseInsensitive) }
    if flags.contains("m") { options.insert(.anchorsMatchLines) }
    if dotAll { options.insert(.dotMatchesLineSeparators) }
    let expanded = expandShorthandClasses(body, dotAll: dotAll)
    let re: NSRegularExpression
    do {
        re = try NSRegularExpression(pattern: expanded, options: options)
    } catch {
        throw ExpressionError("Matches pattern does not compile: \(pattern)")
    }
    let range = NSRange(input.startIndex..., in: input)
    return re.firstMatch(in: input, options: [], range: range) != nil
}

// MARK: - Ordered comparisons (unchanged from v1)

private func identityOf<C>(_ node: ExprNode, _ ctx: C, _ options: EvalOptions<C>?) throws -> String {
    let typ = try nodeType(node)
    if typ == "\(tx)/UriLiteral" {
        guard let s = node["refTo"] as? String, !s.isEmpty else {
            throw ExpressionError("UriLiteral is missing refTo")
        }
        return s
    }
    guard let resolveRef = options?.resolveRef else {
        throw ExpressionError("No resolveRef supplied for identity leaf '\(typ)'")
    }
    return try resolveRef(node, ctx)
}

private func foldOrdered<C>(
    _ node: ExprNode, _ typ: String, _ ctx: C, _ options: EvalOptions<C>?
) throws -> (value: Double, left: String, right: String) {
    guard let via = node["viaProperty"] as? String, !via.isEmpty else {
        throw ExpressionError("\(typ) is missing viaProperty")
    }
    let left = try identityOf(try operand(node, typ, "compareLeft"), ctx, options)
    let right = try identityOf(try operand(node, typ, "compareRight"), ctx, options)
    guard let closure = options?.closures?[via] else {
        throw ExpressionError("No closure supplied for ordering property '\(via)'")
    }
    let value: Double
    if left == right {
        value = boolNum(typ == "\(tx)/IsAtLeast")
    } else {
        value = boolNum(closure[left]?.contains(right) ?? false)
    }
    return (value, left, right)
}

// MARK: - The fold

private typealias Frames = [(name: String, value: EvalValue)]

private func boundValue(_ frames: Frames, _ name: String) -> EvalValue? {
    for (n, v) in frames.reversed() where n == name { return v }
    return nil
}

/// Evaluate an expression tree to a value.
public func evaluate<C>(
    _ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>,
    options: EvalOptions<C>? = nil
) throws -> EvalValue {
    var frames: Frames = []
    return try go(node, ctx, resolve, options, &frames)
}

private func sourceList<C>(
    _ node: ExprNode, _ typ: String, _ ctx: C, _ resolve: @escaping Resolve<C>,
    _ options: EvalOptions<C>?, _ frames: inout Frames
) throws -> [EvalValue] {
    let v = try go(try operand(node, typ, "source"), ctx, resolve, options, &frames)
    if case let .list(l) = v { return l }
    return [v]
}

private func go<C>(
    _ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>,
    _ options: EvalOptions<C>?, _ frames: inout Frames
) throws -> EvalValue {
    let typ = try nodeType(node)

    if let arity = operatorArity(typ) {
        switch arity {
        case let .unary(key):
            let x = try go(try operand(node, typ, key), ctx, resolve, options, &frames)
            return .num(try unaryPrim(typ, try requireNum(x, typ)))
        case let .binary(leftKey, rightKey):
            let a = try go(try operand(node, typ, leftKey), ctx, resolve, options, &frames)
            let b = try go(try operand(node, typ, rightKey), ctx, resolve, options, &frames)
            if typ == "\(tx)/Equals" { return .num(boolNum(valuesEqual(a, b))) }
            if isOrderComparison(typ) {
                // Predicate: non-numeric operands fail CLOSED.
                guard case let .num(x) = a, case let .num(y) = b else { return .num(0) }
                return .num(binaryOrder(typ, x, y))
            }
            return .num(try binaryArith(typ, try requireNum(a, typ), try requireNum(b, typ)))
        case let .nary(key):
            guard let items = node[key] as? [Any] else {
                throw ExpressionError("\(typ) expects an '\(key)' list")
            }
            let isAnd = typ == "\(tx)/And"
            for item in items {
                guard let sub = item as? ExprNode else {
                    throw ExpressionError("\(typ) operand is not a node")
                }
                let v = truthy(try requireNum(try go(sub, ctx, resolve, options, &frames), typ))
                if isAnd && !v { return .num(0) }
                if !isAnd && v { return .num(1) }
            }
            return .num(boolNum(isAnd))
        case let .ternary(aKey, bKey, cKey):
            // Only Clip today: clamp clipValue into [clipLower, clipUpper].
            let v = try requireNum(try go(try operand(node, typ, aKey), ctx, resolve, options, &frames), typ)
            let lo = try requireNum(try go(try operand(node, typ, bKey), ctx, resolve, options, &frames), typ)
            let hi = try requireNum(try go(try operand(node, typ, cKey), ctx, resolve, options, &frames), typ)
            return .num(Swift.min(Swift.max(v, lo), hi))
        }
    }

    if typ == "\(tx)/Not" {
        let x = try go(try operand(node, typ, "operand"), ctx, resolve, options, &frames)
        return .num(boolNum(!truthy(try requireNum(x, typ))))
    }

    if typ == "\(tx)/IsAtLeast" || typ == "\(tx)/Dominates" {
        return .num(try foldOrdered(node, typ, ctx, options).value)
    }

    if isListFold(typ) {
        let items = try sourceList(node, typ, ctx, resolve, options, &frames)
        return try listFold(typ, items, node)
    }

    if let bodyKey = iteratorBody(typ) {
        guard let loopVar = node["loopVar"] as? String, !loopVar.isEmpty else {
            throw ExpressionError("\(typ) is missing loopVar")
        }
        let items = try sourceList(node, typ, ctx, resolve, options, &frames)
        let body = try operand(node, typ, bodyKey)
        var out: [EvalValue] = []
        for el in items {
            frames.append((loopVar, el))
            defer { frames.removeLast() }
            let v = try go(body, ctx, resolve, options, &frames)
            switch typ {
            case "\(tx)/Filter":
                if truthy(try requireNum(v, "Filter predicate")) { out.append(el) }
            case "\(tx)/ForEach":
                // Flatten one level; an empty list contributes nothing.
                if case let .list(items) = v { out.append(contentsOf: items) }
                else { out.append(v) }
            default:
                out.append(v)
            }
        }
        return .list(out)
    }

    if typ == "\(tx)/Contains" {
        let hay = try go(try operand(node, typ, "haystack"), ctx, resolve, options, &frames)
        let needle = try go(try operand(node, typ, "needle"), ctx, resolve, options, &frames)
        let items: [EvalValue]
        if case let .list(l) = hay { items = l } else { items = [hay] }
        return .num(boolNum(items.contains { valuesEqual($0, needle) }))
    }

    if typ == "\(tx)/IsSet" {
        let v = try go(try operand(node, typ, "checkExpr"), ctx, resolve, options, &frames)
        return .num(boolNum(isSetValue(v)))
    }

    if typ == "\(tx)/ListItemAt" {
        let items = try sourceList(node, typ, ctx, resolve, options, &frames)
        let idx = try go(try operand(node, typ, "itemIndex"), ctx, resolve, options, &frames)
        guard case let .num(n) = idx, n == n.rounded(.down), n >= 0 else {
            throw ExpressionError("ListItemAt itemIndex must be a non-negative integer")
        }
        let i = Int(n)
        // Past the end is ABSENCE (the empty list); guard with IsSet.
        return i < items.count ? items[i] : .list([])
    }

    if typ == "\(tx)/Matches" {
        let src = try go(try operand(node, typ, "matchSource"), ctx, resolve, options, &frames)
        guard case let .str(s) = src else {
            throw ExpressionError("Matches requires a string matchSource, got \(src.kind)")
        }
        guard let pattern = node["pattern"] as? String else {
            throw ExpressionError("Matches is missing pattern")
        }
        return .num(boolNum(try matchesPattern(s, pattern)))
    }

    if let v = try kindPredicateValue(typ, node, ctx, resolve, options, &frames) {
        return v
    }

    if let lit = try literalValue(node, typ) {
        return lit
    }

    // A VarRef naming a lexically-enclosing loopVar is the kernel's own bound
    // variable — the ONLY leaf the kernel answers. Everything else is the
    // caller's; recursion from inside `resolve` re-enters WITHOUT frames.
    if typ == "\(tx)/VarRef", let name = node["varName"] as? String,
       let bound = boundValue(frames, name) {
        return bound
    }

    return try resolve(node, ctx) { n, c in
        var fresh: Frames = []
        return try go(n, c, resolve, options, &fresh)
    }
}

private func kindPredicateValue<C>(
    _ typ: String, _ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>,
    _ options: EvalOptions<C>?, _ frames: inout Frames
) throws -> EvalValue? {
    guard kindPredicate(typ, .num(0)) != nil else { return nil }
    let x = try go(try operand(node, typ, "kindCheck"), ctx, resolve, options, &frames)
    return .num(kindPredicate(typ, x)!)
}

// MARK: - explain

/// Evaluate and return the verdict tree. The root's `value` is exactly what
/// `evaluate` returns for the same inputs; the conformance suite runs every
/// vector through both and requires agreement.
public func explain<C>(
    _ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>,
    options: EvalOptions<C>? = nil
) throws -> TraceNode {
    var frames: Frames = []
    return try trace(node, ctx, resolve, options, &frames)
}

private func trace<C>(
    _ node: ExprNode, _ ctx: C, _ resolve: @escaping Resolve<C>,
    _ options: EvalOptions<C>?, _ frames: inout Frames
) throws -> TraceNode {
    let typ = try nodeType(node)

    if let arity = operatorArity(typ) {
        switch arity {
        case let .unary(key):
            let x = try trace(try operand(node, typ, key), ctx, resolve, options, &frames)
            return TraceNode(typ, .num(try unaryPrim(typ, try requireNum(x.value, typ))), [x])
        case let .binary(leftKey, rightKey):
            let a = try trace(try operand(node, typ, leftKey), ctx, resolve, options, &frames)
            let b = try trace(try operand(node, typ, rightKey), ctx, resolve, options, &frames)
            let value: EvalValue
            if typ == "\(tx)/Equals" {
                value = .num(boolNum(valuesEqual(a.value, b.value)))
            } else if isOrderComparison(typ) {
                if case let .num(x) = a.value, case let .num(y) = b.value {
                    value = .num(binaryOrder(typ, x, y))
                } else {
                    value = .num(0)
                }
            } else {
                value = .num(try binaryArith(typ, try requireNum(a.value, typ), try requireNum(b.value, typ)))
            }
            return TraceNode(typ, value, [a, b])
        case let .nary(key):
            guard let items = node[key] as? [Any] else {
                throw ExpressionError("\(typ) expects an '\(key)' list")
            }
            let isAnd = typ == "\(tx)/And"
            var children: [TraceNode] = []
            for item in items {
                guard let sub = item as? ExprNode else {
                    throw ExpressionError("\(typ) operand is not a node")
                }
                let child = try trace(sub, ctx, resolve, options, &frames)
                children.append(child)
                let v = truthy(try requireNum(child.value, typ))
                // Same short-circuit as `evaluate`: later operands never run
                // and never appear in the trace.
                if isAnd && !v { return TraceNode(typ, .num(0), children) }
                if !isAnd && v { return TraceNode(typ, .num(1), children) }
            }
            return TraceNode(typ, .num(boolNum(isAnd)), children)
        case let .ternary(aKey, bKey, cKey):
            let tv = try trace(try operand(node, typ, aKey), ctx, resolve, options, &frames)
            let tlo = try trace(try operand(node, typ, bKey), ctx, resolve, options, &frames)
            let thi = try trace(try operand(node, typ, cKey), ctx, resolve, options, &frames)
            let v = try requireNum(tv.value, typ)
            let lo = try requireNum(tlo.value, typ)
            let hi = try requireNum(thi.value, typ)
            return TraceNode(typ, .num(Swift.min(Swift.max(v, lo), hi)), [tv, tlo, thi])
        }
    }

    if typ == "\(tx)/Not" {
        let x = try trace(try operand(node, typ, "operand"), ctx, resolve, options, &frames)
        return TraceNode(typ, .num(boolNum(!truthy(try requireNum(x.value, typ)))), [x])
    }

    if typ == "\(tx)/IsAtLeast" || typ == "\(tx)/Dominates" {
        let r = try foldOrdered(node, typ, ctx, options)
        return TraceNode(typ, .num(r.value), [], leftRef: r.left, rightRef: r.right)
    }

    if isListFold(typ) {
        let src = try trace(try operand(node, typ, "source"), ctx, resolve, options, &frames)
        let items: [EvalValue]
        if case let .list(l) = src.value { items = l } else { items = [src.value] }
        return TraceNode(typ, try listFold(typ, items, node), [src])
    }

    if let bodyKey = iteratorBody(typ) {
        guard let loopVar = node["loopVar"] as? String, !loopVar.isEmpty else {
            throw ExpressionError("\(typ) is missing loopVar")
        }
        let src = try trace(try operand(node, typ, "source"), ctx, resolve, options, &frames)
        let items: [EvalValue]
        if case let .list(l) = src.value { items = l } else { items = [src.value] }
        let body = try operand(node, typ, bodyKey)
        var children: [TraceNode] = [src]
        var out: [EvalValue] = []
        for el in items {
            frames.append((loopVar, el))
            defer { frames.removeLast() }
            let bt = try trace(body, ctx, resolve, options, &frames)
            children.append(bt)
            let v = bt.value
            switch typ {
            case "\(tx)/Filter":
                if truthy(try requireNum(v, "Filter predicate")) { out.append(el) }
            case "\(tx)/ForEach":
                if case let .list(items) = v { out.append(contentsOf: items) }
                else { out.append(v) }
            default:
                out.append(v)
            }
        }
        return TraceNode(typ, .list(out), children)
    }

    if typ == "\(tx)/Contains" {
        let hay = try trace(try operand(node, typ, "haystack"), ctx, resolve, options, &frames)
        let needle = try trace(try operand(node, typ, "needle"), ctx, resolve, options, &frames)
        let items: [EvalValue]
        if case let .list(l) = hay.value { items = l } else { items = [hay.value] }
        let v = boolNum(items.contains { valuesEqual($0, needle.value) })
        return TraceNode(typ, .num(v), [hay, needle])
    }

    if typ == "\(tx)/IsSet" {
        let x = try trace(try operand(node, typ, "checkExpr"), ctx, resolve, options, &frames)
        return TraceNode(typ, .num(boolNum(isSetValue(x.value))), [x])
    }

    if typ == "\(tx)/ListItemAt" {
        let src = try trace(try operand(node, typ, "source"), ctx, resolve, options, &frames)
        let idx = try trace(try operand(node, typ, "itemIndex"), ctx, resolve, options, &frames)
        let items: [EvalValue]
        if case let .list(l) = src.value { items = l } else { items = [src.value] }
        guard case let .num(n) = idx.value, n == n.rounded(.down), n >= 0 else {
            throw ExpressionError("ListItemAt itemIndex must be a non-negative integer")
        }
        let i = Int(n)
        let value: EvalValue = i < items.count ? items[i] : .list([])
        return TraceNode(typ, value, [src, idx])
    }

    if typ == "\(tx)/Matches" {
        let src = try trace(try operand(node, typ, "matchSource"), ctx, resolve, options, &frames)
        guard case let .str(s) = src.value else {
            throw ExpressionError("Matches requires a string matchSource, got \(src.value.kind)")
        }
        guard let pattern = node["pattern"] as? String else {
            throw ExpressionError("Matches is missing pattern")
        }
        return TraceNode(typ, .num(boolNum(try matchesPattern(s, pattern))), [src])
    }

    if kindPredicate(typ, .num(0)) != nil {
        let x = try trace(try operand(node, typ, "kindCheck"), ctx, resolve, options, &frames)
        return TraceNode(typ, .num(kindPredicate(typ, x.value)!), [x])
    }

    if let lit = try literalValue(node, typ) {
        return TraceNode(typ, lit)
    }

    if typ == "\(tx)/VarRef", let name = node["varName"] as? String,
       let bound = boundValue(frames, name) {
        return TraceNode(typ, bound)
    }

    let v = try resolve(node, ctx) { n, c in
        var fresh: Frames = []
        return try go(n, c, resolve, options, &fresh)
    }
    return TraceNode(typ, v)
}
