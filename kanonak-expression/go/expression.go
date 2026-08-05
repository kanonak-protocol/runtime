// Package expression is the Kanonak expression RUNTIME
// (expressionRuntimeVersion "1") — a small, deterministic tree-walker that folds
// a kanonak.org/transformations (tx) + kanonak.org/math expression tree to a
// single float64. An independent conformant port of the reference kernel,
// verified against the shared parity vectors.
//
// Three layers, exactly as the reference kernel:
//
//  1. DISPATCH — an operator's arity falls out of its tx superclass:
//     UnaryNumericOp -> unary `value`; BinaryArithmetic/BinaryComparison ->
//     binary; BooleanLogic -> n-ary `operands`; plus the two structural shapes
//     the hierarchy can't imply (Not's `operand`, Clip's ternary). The
//     operatorArity table is that derivation, frozen.
//  2. PRIMITIVES — the one authored, determinism-bearing artifact. unaryPrim /
//     binaryPrim map each operator URI to its fold. Determinism traps live here
//     (Round half-away-from-zero, floored Modulo, Sign(0)=0, comparisons as
//     1/0) and match every language port.
//  3. THE FOLD — Evaluate, a fixed shape: operators recurse + apply a primitive;
//     literals return their numeric value; EVERYTHING ELSE (a typed VarRef, a
//     domain Step/Time/Smooth, any future leaf) is handed to the caller's
//     resolve(node, ctx, evaluate). The runtime is a pure operator engine;
//     binding and domain-leaf semantics are the caller's business. It never
//     privileges tx.VarRef — that is just one leaf a domain may resolve.
//
// A change to any primitive, value rule, or dispatch entry requires a NEW
// ExpressionRuntimeVersion, never an edit in place.
package expression

import (
	"fmt"
	"math"
)

// ExpressionRuntimeVersion freezes the determinism contract. Not hashed.
const ExpressionRuntimeVersion = "1"

const (
	tx   = "kanonak.org/transformations"
	math_ = "kanonak.org/math"
)

// Node is a node in the expression tree. "type" is the operator/literal/leaf
// canonical URI (versionless: publisher/package/name); operand keys are the
// frozen tx operand property local names. Unknown fields are ignored by the
// kernel and are available to resolve for domain leaves.
type Node map[string]interface{}

// Type returns the node's "type" field as a string.
func (n Node) Type() string {
	t, _ := n["type"].(string)
	return t
}

// Resolve resolves any node the kernel does not recognise as an operator or
// literal — a binding (tx.VarRef, a domain's typed refersTo VarRef) or a domain
// leaf (Step, Time, Smooth…) — to a float64. ctx is opaque caller state.
// evaluate is handed back so a domain leaf containing sub-expressions can
// recurse into the kernel.
type Resolve func(node Node, ctx interface{}, evaluate func(Node, interface{}) float64) float64

// ClosureTable is the transitive closures ordered comparisons consult, keyed by
// the ordering property's canonical URI, then by member: Closures[property][from]
// is the set of members `from` reaches. Flat, already-closed data — typically
// the SDK reasoner's prp-trp saturation emitted at code-generation time. The
// kernel does set membership only; it never computes a closure, resolves a
// package, or reasons.
type ClosureTable map[string]map[string][]string

// ResolveRef resolves an identity leaf inside an ordered comparison — any
// operand node that is not a tx.UriLiteral — to a member's canonical versionless
// URI (publisher/package/name). The identity-domain mirror of Resolve: the
// kernel owns the constant leaf, the caller owns bindings.
type ResolveRef func(node Node, ctx interface{}) string

// Options is the optional evaluation context for the ordered comparisons
// (IsAtLeast, Dominates). Absent (or missing a needed entry), an ordered
// comparison fails loudly — never a silent false from a missing table.
type Options struct {
	Closures   ClosureTable
	ResolveRef ResolveRef
}

// Error is the runtime error type. Raised (via panic) for domain violations and
// dispatched into Go errors at the Evaluate boundary.
type Error struct{ Msg string }

func (e *Error) Error() string { return e.Msg }

func raise(format string, args ...interface{}) {
	panic(&Error{Msg: fmt.Sprintf(format, args...)})
}

// arity describes the operand shape per operator, derived from the tx hierarchy.
type arity struct {
	kind  string // "unary" | "binary" | "nary" | "ternary"
	left  string
	right string
	op    string
	a, b, c string
}

var (
	arithA  = arity{kind: "binary", left: "arithLeft", right: "arithRight"}
	compare = arity{kind: "binary", left: "compareLeft", right: "compareRight"}
	value   = arity{kind: "unary", op: "value"}
)

var operatorArity = map[string]arity{
	tx + "/Add":      arithA,
	tx + "/Subtract": arithA,
	tx + "/Multiply": arithA,
	tx + "/Divide":   arithA,
	math_ + "/Power":   arithA,
	math_ + "/Modulo":  arithA,
	math_ + "/Minimum": arithA,
	math_ + "/Maximum": arithA,

	tx + "/Abs":    value,
	tx + "/Negate": value,
	math_ + "/Exp":   value,
	math_ + "/Ln":    value,
	math_ + "/Log10": value,
	math_ + "/Sqrt":  value,
	math_ + "/Floor": value,
	math_ + "/Ceil":  value,
	math_ + "/Round": value,
	math_ + "/Sign":  value,

	tx + "/Equals":             compare,
	tx + "/GreaterThan":        compare,
	tx + "/LessThan":           compare,
	tx + "/GreaterThanOrEqual": compare,
	tx + "/LessThanOrEqual":    compare,

	tx + "/And": {kind: "nary", op: "operands"},
	tx + "/Or":  {kind: "nary", op: "operands"},
	// Not is a direct Expression subclass with boolean (not numeric-unary)
	// semantics — handled explicitly in Evaluate, not via the numeric tables.

	math_ + "/Clip": {kind: "ternary", a: "clipValue", b: "clipLower", c: "clipUpper"},
}

// flooredMod is the floored modulo (Go's math.Mod truncates toward zero):
// Modulo(-7,3) = 2.
func flooredMod(a, b float64) float64 {
	if b == 0 {
		raise("Modulo by zero")
	}
	return a - b*math.Floor(a/b)
}

// roundHalfAway rounds half away from zero: Round(-2.5) = -3, Round(2.5) = 3.
func roundHalfAway(a float64) float64 {
	if a < 0 {
		return -math.Floor(-a + 0.5)
	}
	return math.Floor(a + 0.5)
}

func truthy(n float64) bool { return n != 0 }

func boolNum(b bool) float64 {
	if b {
		return 1
	}
	return 0
}

func requireDomain(ok bool, msg string) {
	if !ok {
		raise("%s", msg)
	}
}

func signOf(x float64) float64 {
	switch {
	case x > 0:
		return 1
	case x < 0:
		return -1
	default:
		return 0
	}
}

// unaryPrim/binaryPrim are the authored, determinism-bearing tables, keyed by
// operator URI — matched per language.
var unaryPrim = map[string]func(float64) float64{
	tx + "/Abs":    math.Abs,
	tx + "/Negate": func(x float64) float64 { return -x },
	math_ + "/Exp":   math.Exp,
	math_ + "/Ln":    func(x float64) float64 { requireDomain(x > 0, "Ln of a non-positive number"); return math.Log(x) },
	math_ + "/Log10": func(x float64) float64 { requireDomain(x > 0, "Log10 of a non-positive number"); return math.Log10(x) },
	math_ + "/Sqrt":  func(x float64) float64 { requireDomain(x >= 0, "Sqrt of a negative number"); return math.Sqrt(x) },
	math_ + "/Floor": math.Floor,
	math_ + "/Ceil":  math.Ceil,
	math_ + "/Round": roundHalfAway,
	math_ + "/Sign":  signOf,
}

var binaryPrim = map[string]func(a, b float64) float64{
	tx + "/Add":      func(a, b float64) float64 { return a + b },
	tx + "/Subtract": func(a, b float64) float64 { return a - b },
	tx + "/Multiply": func(a, b float64) float64 { return a * b },
	tx + "/Divide":   func(a, b float64) float64 { requireDomain(b != 0, "Divide by zero"); return a / b },
	math_ + "/Power":   math.Pow,
	math_ + "/Modulo":  flooredMod,
	math_ + "/Minimum": math.Min,
	math_ + "/Maximum": math.Max,
	tx + "/Equals":             func(a, b float64) float64 { return boolNum(a == b) },
	tx + "/GreaterThan":        func(a, b float64) float64 { return boolNum(a > b) },
	tx + "/LessThan":           func(a, b float64) float64 { return boolNum(a < b) },
	tx + "/GreaterThanOrEqual": func(a, b float64) float64 { return boolNum(a >= b) },
	tx + "/LessThanOrEqual":    func(a, b float64) float64 { return boolNum(a <= b) },
}

// toNumber coerces a literal's payload (which arrives as float64, int, bool, or
// a string) to a float64.
func toNumber(v interface{}) float64 {
	switch x := v.(type) {
	case float64:
		return x
	case float32:
		return float64(x)
	case int:
		return float64(x)
	case int64:
		return float64(x)
	case bool:
		return boolNum(x)
	case string:
		var f float64
		if _, err := fmt.Sscanf(x, "%g", &f); err != nil {
			raise("invalid numeric literal %q", x)
		}
		return f
	default:
		raise("invalid numeric literal of type %T", v)
		return 0
	}
}

// literalValue returns the numeric value of a literal node and ok=true, or
// ok=false if the node is not a literal.
func literalValue(node Node) (float64, bool) {
	switch node.Type() {
	case tx + "/IntegerLiteral":
		return toNumber(node["integerLiteral"]), true
	case tx + "/DecimalLiteral":
		return toNumber(node["decimalLiteral"]), true
	case tx + "/BooleanLiteral":
		b := node["booleanLiteral"]
		return boolNum(b == true || b == "true"), true
	default:
		return 0, false
	}
}

func operand(node Node, key string) Node {
	v, ok := node[key]
	if !ok || v == nil {
		raise("%s is missing operand '%s'", node.Type(), key)
	}
	m, ok := v.(map[string]interface{})
	if !ok {
		if n, ok2 := v.(Node); ok2 {
			return n
		}
		raise("%s is missing operand '%s'", node.Type(), key)
	}
	return Node(m)
}

// identityOf resolves an ordered-comparison operand to a member's canonical
// versionless URI. tx.UriLiteral is the kernel-known constant leaf (its refTo
// IS the identity, the way a literal's value is its number); every other node
// is the caller's, through Options.ResolveRef.
func identityOf(node Node, ctx interface{}, opts *Options) string {
	if node.Type() == tx+"/UriLiteral" {
		ref, _ := node["refTo"].(string)
		if ref == "" {
			raise("UriLiteral is missing refTo")
		}
		return ref
	}
	if opts == nil || opts.ResolveRef == nil {
		raise("No resolveRef supplied for identity leaf '%s'", node.Type())
	}
	return opts.ResolveRef(node, ctx)
}

// foldOrdered folds IsAtLeast / Dominates to 1/0 plus the resolved operand
// identities. The ordering is the supplied closure for the node's viaProperty —
// membership in already-closed data, nothing more. Identity is canonical
// versionless URI string equality, matching tx.Equals' identity rule.
// IsAtLeast folds reflexivity into the operator (same member → 1); Dominates
// is strict (same member → 0). Two members with no path yield 0 — fail-closed
// — but a MISSING closure table is a configuration failure and errors loudly.
func foldOrdered(node Node, ctx interface{}, opts *Options) (value float64, left, right string) {
	via, _ := node["viaProperty"].(string)
	if via == "" {
		raise("%s is missing viaProperty", node.Type())
	}
	left = identityOf(operand(node, "compareLeft"), ctx, opts)
	right = identityOf(operand(node, "compareRight"), ctx, opts)
	var closure map[string][]string
	if opts != nil && opts.Closures != nil {
		closure = opts.Closures[via]
	}
	if closure == nil {
		raise("No closure supplied for ordering property '%s'", via)
	}
	if left == right {
		return boolNum(node.Type() == tx+"/IsAtLeast"), left, right
	}
	for _, m := range closure[left] {
		if m == right {
			return 1, left, right
		}
	}
	return 0, left, right
}

// evaluatePanic is the inner fold; it panics with *Error on domain violations.
func evaluatePanic(node Node, ctx interface{}, resolve Resolve, opts *Options) float64 {
	recurse := func(n Node, c interface{}) float64 { return evaluatePanic(n, c, resolve, opts) }

	if ar, ok := operatorArity[node.Type()]; ok {
		switch ar.kind {
		case "unary":
			x := recurse(operand(node, ar.op), ctx)
			return unaryPrim[node.Type()](x)
		case "binary":
			a := recurse(operand(node, ar.left), ctx)
			b := recurse(operand(node, ar.right), ctx)
			return binaryPrim[node.Type()](a, b)
		case "nary":
			items, ok := node[ar.op].([]interface{})
			if !ok {
				raise("%s expects an '%s' list", node.Type(), ar.op)
			}
			isAnd := node.Type() == tx+"/And"
			for _, item := range items {
				m, ok := item.(map[string]interface{})
				if !ok {
					if n, ok2 := item.(Node); ok2 {
						m = map[string]interface{}(n)
					} else {
						raise("%s operand is not a node", node.Type())
					}
				}
				v := truthy(recurse(Node(m), ctx))
				if isAnd && !v {
					return 0
				}
				if !isAnd && v {
					return 1
				}
			}
			return boolNum(isAnd)
		case "ternary":
			v := recurse(operand(node, ar.a), ctx)
			lo := recurse(operand(node, ar.b), ctx)
			hi := recurse(operand(node, ar.c), ctx)
			return math.Min(math.Max(v, lo), hi)
		}
	}

	if node.Type() == tx+"/Not" {
		return boolNum(!truthy(recurse(operand(node, "operand"), ctx)))
	}

	if node.Type() == tx+"/IsAtLeast" || node.Type() == tx+"/Dominates" {
		v, _, _ := foldOrdered(node, ctx, opts)
		return v
	}

	if lit, ok := literalValue(node); ok {
		return lit
	}

	// Not an operator or literal — a binding or domain leaf. The caller owns it.
	return resolve(node, ctx, func(n Node, c interface{}) float64 { return evaluatePanic(n, c, resolve, opts) })
}

// Evaluate folds an expression tree to a float64. Operators fold via the frozen
// dispatch + primitive tables; literals yield their numeric value; any other
// node is delegated to resolve. A domain violation (divide/modulo by zero,
// Ln/Log10 of ≤0, Sqrt of <0, a malformed node, an unresolvable leaf) is
// returned as an error.
func Evaluate(node Node, ctx interface{}, resolve Resolve) (result float64, err error) {
	return EvaluateWithOptions(node, ctx, resolve, nil)
}

// EvaluateWithOptions is Evaluate with the ordered-comparison evaluation
// context (closures + identity-leaf resolution). opts is only consulted when an
// IsAtLeast / Dominates node is reached; nil is valid for trees without them.
func EvaluateWithOptions(node Node, ctx interface{}, resolve Resolve, opts *Options) (result float64, err error) {
	defer func() {
		if r := recover(); r != nil {
			if e, ok := r.(*Error); ok {
				err = e
				return
			}
			panic(r)
		}
	}()
	return evaluatePanic(node, ctx, resolve, opts), nil
}

// TraceNode is one node of an evaluation trace — the verdict tree Explain
// returns. It mirrors the expression: Type is the node's type URI, Value its
// result (1/0 for booleans), Children the operand traces in evaluation order.
// A short-circuited operand is simply ABSENT from Children — the trace is
// truthful about what ran. Ordered comparisons carry their resolved operand
// identities as LeftRef/RightRef instead of children. This is a runtime return
// shape, not an ontology class.
type TraceNode struct {
	Type     string
	Value    float64
	Children []*TraceNode
	LeftRef  string
	RightRef string
}

// explainPanic is the traced fold; it panics with *Error on domain violations.
func explainPanic(node Node, ctx interface{}, resolve Resolve, opts *Options) *TraceNode {
	// Numeric recursion for subtrees the caller's resolve re-enters: those folds
	// happen inside the caller and are invisible to the trace. Only
	// kernel-visited nodes appear.
	recurseValue := func(n Node, c interface{}) float64 { return evaluatePanic(n, c, resolve, opts) }

	if ar, ok := operatorArity[node.Type()]; ok {
		switch ar.kind {
		case "unary":
			x := explainPanic(operand(node, ar.op), ctx, resolve, opts)
			return &TraceNode{Type: node.Type(), Value: unaryPrim[node.Type()](x.Value), Children: []*TraceNode{x}}
		case "binary":
			a := explainPanic(operand(node, ar.left), ctx, resolve, opts)
			b := explainPanic(operand(node, ar.right), ctx, resolve, opts)
			return &TraceNode{Type: node.Type(), Value: binaryPrim[node.Type()](a.Value, b.Value), Children: []*TraceNode{a, b}}
		case "nary":
			items, ok := node[ar.op].([]interface{})
			if !ok {
				raise("%s expects an '%s' list", node.Type(), ar.op)
			}
			isAnd := node.Type() == tx+"/And"
			children := []*TraceNode{}
			for _, item := range items {
				m, ok := item.(map[string]interface{})
				if !ok {
					if n, ok2 := item.(Node); ok2 {
						m = map[string]interface{}(n)
					} else {
						raise("%s operand is not a node", node.Type())
					}
				}
				child := explainPanic(Node(m), ctx, resolve, opts)
				children = append(children, child)
				v := truthy(child.Value)
				// Same short-circuit as Evaluate: operands after the deciding one
				// are never evaluated and never appear in the trace.
				if isAnd && !v {
					return &TraceNode{Type: node.Type(), Value: 0, Children: children}
				}
				if !isAnd && v {
					return &TraceNode{Type: node.Type(), Value: 1, Children: children}
				}
			}
			return &TraceNode{Type: node.Type(), Value: boolNum(isAnd), Children: children}
		case "ternary":
			v := explainPanic(operand(node, ar.a), ctx, resolve, opts)
			lo := explainPanic(operand(node, ar.b), ctx, resolve, opts)
			hi := explainPanic(operand(node, ar.c), ctx, resolve, opts)
			return &TraceNode{
				Type:     node.Type(),
				Value:    math.Min(math.Max(v.Value, lo.Value), hi.Value),
				Children: []*TraceNode{v, lo, hi},
			}
		}
	}

	if node.Type() == tx+"/Not" {
		x := explainPanic(operand(node, "operand"), ctx, resolve, opts)
		return &TraceNode{Type: node.Type(), Value: boolNum(!truthy(x.Value)), Children: []*TraceNode{x}}
	}

	if node.Type() == tx+"/IsAtLeast" || node.Type() == tx+"/Dominates" {
		v, left, right := foldOrdered(node, ctx, opts)
		return &TraceNode{Type: node.Type(), Value: v, Children: []*TraceNode{}, LeftRef: left, RightRef: right}
	}

	if lit, ok := literalValue(node); ok {
		return &TraceNode{Type: node.Type(), Value: lit, Children: []*TraceNode{}}
	}

	v := resolve(node, ctx, recurseValue)
	return &TraceNode{Type: node.Type(), Value: v, Children: []*TraceNode{}}
}

// Explain evaluates an expression tree and returns the verdict tree — every
// evaluated node, its own result, and (for ordered comparisons) the identities
// it compared. The root's Value is exactly what Evaluate returns for the same
// inputs; the conformance suite runs every vector through both and requires
// agreement, so the two entry points cannot drift. Kept separate from Evaluate
// so the hot path never pays for trace allocation. Errors propagate exactly as
// in Evaluate — a failed evaluation yields an error, not a partial trace.
func Explain(node Node, ctx interface{}, resolve Resolve, opts *Options) (trace *TraceNode, err error) {
	defer func() {
		if r := recover(); r != nil {
			if e, ok := r.(*Error); ok {
				err = e
				return
			}
			panic(r)
		}
	}()
	return explainPanic(node, ctx, resolve, opts), nil
}
