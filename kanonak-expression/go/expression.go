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

// evaluatePanic is the inner fold; it panics with *Error on domain violations.
func evaluatePanic(node Node, ctx interface{}, resolve Resolve) float64 {
	recurse := func(n Node, c interface{}) float64 { return evaluatePanic(n, c, resolve) }

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

	if lit, ok := literalValue(node); ok {
		return lit
	}

	// Not an operator or literal — a binding or domain leaf. The caller owns it.
	return resolve(node, ctx, func(n Node, c interface{}) float64 { return evaluatePanic(n, c, resolve) })
}

// Evaluate folds an expression tree to a float64. Operators fold via the frozen
// dispatch + primitive tables; literals yield their numeric value; any other
// node is delegated to resolve. A domain violation (divide/modulo by zero,
// Ln/Log10 of ≤0, Sqrt of <0, a malformed node, an unresolvable leaf) is
// returned as an error.
func Evaluate(node Node, ctx interface{}, resolve Resolve) (result float64, err error) {
	defer func() {
		if r := recover(); r != nil {
			if e, ok := r.(*Error); ok {
				err = e
				return
			}
			panic(r)
		}
	}()
	return evaluatePanic(node, ctx, resolve), nil
}
