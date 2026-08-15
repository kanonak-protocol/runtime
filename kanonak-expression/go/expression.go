// Package expression is the Kanonak expression RUNTIME
// (expressionRuntimeVersion "2") — a small, deterministic tree-walker that
// folds a kanonak.org/transformations (tx) + kanonak.org/math expression tree
// to a VALUE. An independent conformant port of the reference kernel, verified
// against the shared parity vectors.
//
// VALUE DOMAIN (v2): number | string | ref | list. Booleans and comparison
// results remain 1/0 float64s; a Ref is a canonical versionless URI identity
// (distinct from string so Equals holds the cross-kind-is-false rule);
// ABSENCE IS THE EMPTY LIST — there is no nil value. The caller's resolve may
// return any value: a property read is just a caller leaf that returns a
// list. The kernel NEVER touches a graph.
//
// ERROR CONTRACT: computations fail LOUD (arithmetic on a non-number, an
// aggregate over non-numeric elements, Min/Max/Average on empty, a nested
// list in Join, an out-of-subset Matches pattern); predicates fail CLOSED
// (Equals cross-kind and the ordering comparisons on non-numbers yield 0).
//
// LAMBDA BINDING: Filter/ListMap/ForEach bind their loopVar per element;
// within their bodies — and only there — a tx.VarRef naming a
// lexically-enclosing loopVar is resolved by the kernel (innermost binder
// wins). Recursion re-entered from inside resolve carries no frames.
//
// MATCHES: the pinned RE2-compatible XSD-regex subset — this engine IS the
// RE2 family, so the subset is native: code points (runes) are the counting
// unit, \b is the ASCII word boundary. The shorthand classes still expand
// textually (the dialect DEFINES them by expansion; Go's native \s lacks
// \x0B, so expansion is load-bearing here too).
//
// A change to any primitive, value rule, or dispatch entry requires a NEW
// ExpressionRuntimeVersion, never an edit in place.
package expression

import (
	"fmt"
	"math"
	"regexp"
	"strconv"
	"strings"
)

// ExpressionRuntimeVersion freezes the determinism contract. Not hashed.
const ExpressionRuntimeVersion = "2"

const (
	tx    = "kanonak.org/transformations"
	math_ = "kanonak.org/math"
)

// Node is a node in the expression tree. "type" is the operator/literal/leaf
// canonical URI; operand keys are the frozen tx operand property local names.
type Node map[string]interface{}

// Type returns the node's "type" field as a string.
func (n Node) Type() string {
	t, _ := n["type"].(string)
	return t
}

// Ref is a reference value — a member's canonical versionless URI identity.
type Ref struct{ URI string }

// Value is one of: float64, string, Ref, []Value. Booleans are 1/0 float64s;
// absence is the empty list. (Go has no sum types; the kernel type-switches.)
type Value = interface{}

// Resolve resolves any node the kernel does not recognise as an operator or
// literal — a binding (tx.VarRef), a host graph read (a property-read leaf
// returning a list), or a domain leaf — to a value. ctx is opaque caller
// state; evaluate is handed back so a domain leaf containing sub-expressions
// can recurse into the kernel (WITHOUT lambda frames).
type Resolve func(node Node, ctx interface{}, evaluate func(Node, interface{}) Value) Value

// ClosureTable is the transitive closures ordered comparisons consult.
type ClosureTable map[string]map[string][]string

// ResolveRef resolves an identity leaf inside an ordered comparison.
type ResolveRef func(node Node, ctx interface{}) string

// Options is the optional evaluation context for the ordered comparisons.
type Options struct {
	Closures   ClosureTable
	ResolveRef ResolveRef
}

// Error is the runtime error type. Raised (via panic) for domain violations
// and dispatched into Go errors at the Evaluate boundary.
type Error struct{ Msg string }

func (e *Error) Error() string { return e.Msg }

func raise(format string, args ...interface{}) {
	panic(&Error{Msg: fmt.Sprintf(format, args...)})
}

type arity struct {
	kind    string // unary | binary | nary | ternary
	a, b, c string
}

var arithArity = arity{kind: "binary", a: "arithLeft", b: "arithRight"}
var compareArity = arity{kind: "binary", a: "compareLeft", b: "compareRight"}
var valueArity = arity{kind: "unary", a: "value"}

func operatorArity(typ string) (arity, bool) {
	switch typ {
	case tx + "/Add", tx + "/Subtract", tx + "/Multiply", tx + "/Divide",
		math_ + "/Power", math_ + "/Modulo", math_ + "/Minimum", math_ + "/Maximum":
		return arithArity, true
	case tx + "/Abs", tx + "/Negate", math_ + "/Exp", math_ + "/Ln", math_ + "/Log10",
		math_ + "/Sqrt", math_ + "/Floor", math_ + "/Ceil", math_ + "/Round", math_ + "/Sign":
		return valueArity, true
	case tx + "/Equals", tx + "/GreaterThan", tx + "/LessThan",
		tx + "/GreaterThanOrEqual", tx + "/LessThanOrEqual":
		return compareArity, true
	case tx + "/And", tx + "/Or":
		return arity{kind: "nary", a: "operands"}, true
	case math_ + "/Clip":
		return arity{kind: "ternary", a: "clipValue", b: "clipLower", c: "clipUpper"}, true
	}
	return arity{}, false
}

func iteratorBody(typ string) (string, bool) {
	switch typ {
	case tx + "/ForEach":
		return "emit", true
	case tx + "/ListMap":
		return "mapBody", true
	case tx + "/Filter":
		return "predicate", true
	}
	return "", false
}

func isListFold(typ string) bool {
	switch typ {
	case tx + "/Count", tx + "/Sum", tx + "/Min", tx + "/Max",
		tx + "/Average", tx + "/Join", tx + "/Reverse":
		return true
	}
	return false
}

// --- primitives -------------------------------------------------------------

func flooredMod(a, b float64) float64 {
	if b == 0 {
		raise("Modulo by zero")
	}
	return a - b*math.Floor(a/b)
}

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
	if x > 0 {
		return 1
	}
	if x < 0 {
		return -1
	}
	return 0
}

func kindOf(v Value) string {
	switch v.(type) {
	case float64:
		return "number"
	case string:
		return "string"
	case Ref:
		return "ref"
	case []Value:
		return "list"
	}
	return "unknown"
}

func requireNum(v Value, op string) float64 {
	n, ok := v.(float64)
	if !ok {
		raise("%s requires a numeric operand, got %s", op, kindOf(v))
	}
	return n
}

func unaryPrim(typ string, x float64) float64 {
	switch typ {
	case tx + "/Abs":
		return math.Abs(x)
	case tx + "/Negate":
		return -x
	case math_ + "/Exp":
		return math.Exp(x)
	case math_ + "/Ln":
		requireDomain(x > 0, "Ln of a non-positive number")
		return math.Log(x)
	case math_ + "/Log10":
		requireDomain(x > 0, "Log10 of a non-positive number")
		return math.Log10(x)
	case math_ + "/Sqrt":
		requireDomain(x >= 0, "Sqrt of a negative number")
		return math.Sqrt(x)
	case math_ + "/Floor":
		return math.Floor(x)
	case math_ + "/Ceil":
		return math.Ceil(x)
	case math_ + "/Round":
		return roundHalfAway(x)
	case math_ + "/Sign":
		return signOf(x)
	}
	raise("No unary primitive for %s", typ)
	return 0
}

func binaryArith(typ string, a, b float64) float64 {
	switch typ {
	case tx + "/Add":
		return a + b
	case tx + "/Subtract":
		return a - b
	case tx + "/Multiply":
		return a * b
	case tx + "/Divide":
		requireDomain(b != 0, "Divide by zero")
		return a / b
	case math_ + "/Power":
		return math.Pow(a, b)
	case math_ + "/Modulo":
		return flooredMod(a, b)
	case math_ + "/Minimum":
		return math.Min(a, b)
	case math_ + "/Maximum":
		return math.Max(a, b)
	}
	raise("No arithmetic primitive for %s", typ)
	return 0
}

// binaryOrder — PREDICATES: the fold gives them numbers only; non-numeric
// operands fail CLOSED before this table is consulted.
func binaryOrder(typ string, a, b float64) (float64, bool) {
	switch typ {
	case tx + "/GreaterThan":
		return boolNum(a > b), true
	case tx + "/LessThan":
		return boolNum(a < b), true
	case tx + "/GreaterThanOrEqual":
		return boolNum(a >= b), true
	case tx + "/LessThanOrEqual":
		return boolNum(a <= b), true
	}
	return 0, false
}

// valuesEqual — the polymorphic tx.Equals contract: scalars by value, refs by
// URI identity, lists never equal, cross-kind false. Never errors.
func valuesEqual(a, b Value) bool {
	switch av := a.(type) {
	case float64:
		bv, ok := b.(float64)
		return ok && av == bv
	case string:
		bv, ok := b.(string)
		return ok && av == bv
	case Ref:
		bv, ok := b.(Ref)
		return ok && av.URI == bv.URI
	}
	return false
}

func toNumber(v interface{}) float64 {
	switch n := v.(type) {
	case float64:
		return n
	case int:
		return float64(n)
	case string:
		f, err := strconv.ParseFloat(n, 64)
		if err != nil {
			raise("not a number: %v", v)
		}
		return f
	}
	raise("not a number: %v", v)
	return 0
}

// literalValue returns a literal node's value, or ok=false if not a literal.
// StringLiteral and UriLiteral are kernel-known in v2.
func literalValue(node Node) (Value, bool) {
	switch node.Type() {
	case tx + "/IntegerLiteral":
		return toNumber(node["integerLiteral"]), true
	case tx + "/DecimalLiteral":
		return toNumber(node["decimalLiteral"]), true
	case tx + "/BooleanLiteral":
		b := node["booleanLiteral"]
		return boolNum(b == true || b == "true"), true
	case tx + "/StringLiteral":
		s, ok := node["stringLiteral"].(string)
		if !ok {
			raise("StringLiteral is missing stringLiteral")
		}
		return s, true
	case tx + "/UriLiteral":
		s, ok := node["refTo"].(string)
		if !ok || s == "" {
			raise("UriLiteral is missing refTo")
		}
		return Ref{URI: s}, true
	}
	return nil, false
}

// formatNumber — ECMAScript-style number-to-string for the always-finite
// domain: integral values render without a decimal point (the RFC 8785 rule).
func formatNumber(n float64) string {
	if n == math.Trunc(n) && math.Abs(n) < 1e21 {
		return strconv.FormatInt(int64(n), 10)
	}
	return strconv.FormatFloat(n, 'g', -1, 64)
}

func joinElement(v Value) string {
	switch e := v.(type) {
	case string:
		return e
	case float64:
		return formatNumber(e)
	case Ref:
		if i := strings.LastIndex(e.URI, "/"); i >= 0 {
			return e.URI[i+1:]
		}
		return e.URI
	}
	raise("Join cannot stringify a nested list")
	return ""
}

func isSet(v Value) bool {
	switch e := v.(type) {
	case string:
		return len(e) > 0
	case []Value:
		return len(e) > 0
	}
	return true
}

func listFold(typ string, items []Value, node Node) Value {
	switch typ {
	case tx + "/Count":
		return float64(len(items))
	case tx + "/Sum":
		total := 0.0
		for _, el := range items {
			total += requireNum(el, "Sum")
		}
		return total
	case tx + "/Min", tx + "/Max":
		name := "Min"
		if typ == tx+"/Max" {
			name = "Max"
		}
		if len(items) == 0 {
			raise("%s on an empty list is undefined; guard with IsSet", name)
		}
		best := requireNum(items[0], name)
		for _, el := range items[1:] {
			n := requireNum(el, name)
			if (name == "Min" && n < best) || (name == "Max" && n > best) {
				best = n
			}
		}
		return best
	case tx + "/Average":
		if len(items) == 0 {
			raise("Average on an empty list is undefined; guard with IsSet")
		}
		total := 0.0
		for _, el := range items {
			total += requireNum(el, "Average")
		}
		return total / float64(len(items))
	case tx + "/Join":
		sep, _ := node["separator"].(string)
		parts := make([]string, 0, len(items))
		for _, el := range items {
			parts = append(parts, joinElement(el))
		}
		return strings.Join(parts, sep)
	case tx + "/Reverse":
		out := make([]Value, len(items))
		for i, el := range items {
			out[len(items)-1-i] = el
		}
		return out
	}
	raise("No list fold for %s", typ)
	return nil
}

func kindPredicate(typ string, v Value) (float64, bool) {
	switch typ {
	case tx + "/IsString":
		_, ok := v.(string)
		return boolNum(ok), true
	case tx + "/IsNumber":
		_, ok := v.(float64)
		return boolNum(ok), true
	case tx + "/IsReference":
		_, ok := v.(Ref)
		return boolNum(ok), true
	case tx + "/IsList":
		_, ok := v.([]Value)
		return boolNum(ok), true
	}
	return 0, false
}

// --- Matches: the pinned RE2-compatible XSD-regex subset --------------------

var flagPrefixRe = regexp.MustCompile(`^\(\?([ims]+)\)`)
var quantifierRe = regexp.MustCompile(`^\{\d+(,\d*)?\}`)

// hasRepeatedFlag reports whether a prefix flag set repeats a flag.
func hasRepeatedFlag(flags string) bool {
	seen := map[rune]bool{}
	for _, f := range flags {
		if seen[f] {
			return true
		}
		seen[f] = true
	}
	return false
}

var allowedEscapes = map[rune]bool{
	'd': true, 'D': true, 'w': true, 'W': true, 's': true, 'S': true,
	'b': true, 'B': true, 'n': true, 'r': true, 't': true, 'f': true, 'v': true,
	'.': true, '*': true, '+': true, '?': true, '(': true, ')': true,
	'[': true, ']': true, '{': true, '}': true, '|': true, '^': true,
	'$': true, '\\': true, '/': true,
}

// validateMatchesPattern — check a WHOLE pattern against the pinned subset,
// flag prefix included. Thin wrapper over parseMatchesPattern so the checker
// and the evaluator can never disagree about what is valid.
func validateMatchesPattern(pattern string) {
	parseMatchesPattern(pattern)
}

// parseMatchesPattern — THE definition of a valid Matches pattern: the single
// place that decides what the pinned subset accepts AND how a whole pattern
// decomposes into flag prefix and body. Every diagnostic quotes the WHOLE
// pattern the caller passed, never the flag-stripped body.
func parseMatchesPattern(pattern string) (flags, body string) {
	fail := func(what string) {
		raise("Matches pattern is outside the pinned regex subset (%s): %s", what, pattern)
	}
	// Whole-pattern flag prefix - position 0 only, over i/m/s, EACH AT MOST
	// ONCE. The repeat rule is the subset's, not the host engine's: the JS
	// engine rejects (?ii) at compile time while the Go, Python and Rust
	// engines accept it, so the subset decides rather than the host.
	body = pattern
	if m := flagPrefixRe.FindStringSubmatch(pattern); m != nil {
		flags = m[1]
		if hasRepeatedFlag(flags) {
			fail("repeated flag in prefix (?" + flags + ")")
		}
		body = pattern[len(m[0]):]
	}
	chars := []rune(body)
	inClass := false
	for i := 0; i < len(chars); i++ {
		c := chars[i]
		if c == '\\' {
			if i+1 >= len(chars) {
				fail("trailing backslash")
			}
			e := chars[i+1]
			if e == 'x' {
				if i+2 < len(chars) && chars[i+2] == '{' {
					fail(`\x{…} escape`)
				}
				if i+3 >= len(chars) || !isHex(chars[i+2]) || !isHex(chars[i+3]) {
					fail(`\x escape must be \xHH`)
				}
				i += 3
				continue
			}
			if e >= '0' && e <= '9' {
				fail(fmt.Sprintf(`backreference or octal escape \%c`, e))
			}
			if e == 'p' || e == 'P' {
				fail(fmt.Sprintf(`unicode property class \%c{…}`, e))
			}
			if e == 'k' {
				fail(`named backreference \k`)
			}
			if e == 'u' {
				fail(`\u escape`)
			}
			if e == '-' {
				if !inClass {
					fail(`\- outside a character class`)
				}
				i++
				continue
			}
			if (e == 'b' || e == 'B' || e == 'D' || e == 'W' || e == 'S') && inClass {
				fail(fmt.Sprintf(`\%c inside a character class`, e))
			}
			if !allowedEscapes[e] {
				fail(fmt.Sprintf(`escape \%c`, e))
			}
			i++
			continue
		}
		if inClass {
			if c == ']' {
				inClass = false
			} else if c == '&' && i+1 < len(chars) && chars[i+1] == '&' {
				fail("character-class intersection &&")
			} else if c == '[' && i+1 < len(chars) && chars[i+1] == ':' {
				fail("POSIX class [[:…:]]")
			}
			continue
		}
		if c == '[' {
			inClass = true
			continue
		}
		if c == '(' && i+1 < len(chars) && chars[i+1] == '?' {
			// Only (?: survives mid-pattern; the flag prefix splits off first.
			if i+2 >= len(chars) || chars[i+2] != ':' {
				fail("group construct (?")
			}
			i += 2
			continue
		}
		if c == '{' {
			// A bare `{` must start a valid quantifier — the SCANNER enforces
			// this uniformly (a literal brace is written \{).
			if !quantifierRe.MatchString(string(chars[i:])) {
				fail(`bare '{' that is not a quantifier (write \{)`)
			}
		}
	}
	if inClass {
		fail("unterminated character class")
	}
	return flags, body
}

// expandShorthandClasses — the pinned ASCII expansions (the dialect DEFINES
// the shorthands by these). \b/\B stay as-written: Go's RE2 \b is the ASCII
// word boundary natively. Load-bearing here too: Go's native \s lacks \x0B.
func expandShorthandClasses(body string) string {
	chars := []rune(body)
	var out strings.Builder
	inClass := false
	for i := 0; i < len(chars); i++ {
		c := chars[i]
		if c == '\\' {
			e := chars[i+1] // subset-validated: never a trailing backslash
			var expansion string
			if inClass {
				switch e {
				case 'd':
					expansion = "0-9"
				case 'w':
					expansion = "0-9A-Za-z_"
				case 's':
					expansion = ` \t\n\r\f\x0B`
				}
			} else {
				switch e {
				case 'd':
					expansion = "[0-9]"
				case 'D':
					expansion = "[^0-9]"
				case 'w':
					expansion = "[0-9A-Za-z_]"
				case 'W':
					expansion = "[^0-9A-Za-z_]"
				case 's':
					expansion = `[ \t\n\r\f\x0B]`
				case 'S':
					expansion = `[^ \t\n\r\f\x0B]`
				}
			}
			if expansion != "" {
				out.WriteString(expansion)
			} else {
				out.WriteRune(c)
				out.WriteRune(e)
			}
			i++
			continue
		}
		if !inClass && c == '[' {
			inClass = true
		} else if inClass && c == ']' {
			inClass = false
		}
		out.WriteRune(c)
	}
	return out.String()
}

// matchesPattern — fn:matches semantics: UNANCHORED. This engine is the RE2
// family: runes (code points) are the native counting unit.
func matchesPattern(input, pattern string) bool {
	flags, body := parseMatchesPattern(pattern)
	expanded := expandShorthandClasses(body)
	if flags != "" {
		expanded = "(?" + flags + ")" + expanded
	}
	re, err := regexp.Compile(expanded)
	if err != nil {
		raise("Matches pattern does not compile: %s", pattern)
	}
	return re.MatchString(input)
}

func isHex(c rune) bool {
	return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')
}

// --- ordered comparisons (unchanged from v1) --------------------------------

func operand(node Node, key string) Node {
	v, ok := node[key].(map[string]interface{})
	if !ok {
		raise("%s is missing operand '%s'", node.Type(), key)
	}
	return Node(v)
}

func identityOf(node Node, ctx interface{}, opts *Options) string {
	if node.Type() == tx+"/UriLiteral" {
		s, ok := node["refTo"].(string)
		if !ok || s == "" {
			raise("UriLiteral is missing refTo")
		}
		return s
	}
	if opts == nil || opts.ResolveRef == nil {
		raise("No resolveRef supplied for identity leaf '%s'", node.Type())
	}
	return opts.ResolveRef(node, ctx)
}

func foldOrdered(node Node, ctx interface{}, opts *Options) (value float64, left, right string) {
	typ := node.Type()
	via, ok := node["viaProperty"].(string)
	if !ok || via == "" {
		raise("%s is missing viaProperty", typ)
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
		return boolNum(typ == tx+"/IsAtLeast"), left, right
	}
	for _, m := range closure[left] {
		if m == right {
			return 1, left, right
		}
	}
	return 0, left, right
}

// --- the fold ---------------------------------------------------------------

type frame struct {
	name  string
	value Value
}

func boundValue(frames []frame, name string) (Value, bool) {
	for i := len(frames) - 1; i >= 0; i-- {
		if frames[i].name == name {
			return frames[i].value, true
		}
	}
	return nil, false
}

func sourceList(node Node, ctx interface{}, resolve Resolve, opts *Options, frames []frame) []Value {
	v := evalNode(operand(node, "source"), ctx, resolve, opts, frames)
	if l, ok := v.([]Value); ok {
		return l
	}
	return []Value{v}
}

func evalNode(node Node, ctx interface{}, resolve Resolve, opts *Options, frames []frame) Value {
	typ := node.Type()
	if typ == "" {
		raise("Expression node has no 'type'")
	}

	if ar, ok := operatorArity(typ); ok {
		switch ar.kind {
		case "unary":
			x := evalNode(operand(node, ar.a), ctx, resolve, opts, frames)
			return unaryPrim(typ, requireNum(x, typ))
		case "binary":
			a := evalNode(operand(node, ar.a), ctx, resolve, opts, frames)
			b := evalNode(operand(node, ar.b), ctx, resolve, opts, frames)
			if typ == tx+"/Equals" {
				return boolNum(valuesEqual(a, b))
			}
			if _, isOrder := binaryOrder(typ, 0, 0); isOrder {
				// Predicate: non-numeric operands fail CLOSED.
				an, aok := a.(float64)
				bn, bok := b.(float64)
				if !aok || !bok {
					return float64(0)
				}
				v, _ := binaryOrder(typ, an, bn)
				return v
			}
			return binaryArith(typ, requireNum(a, typ), requireNum(b, typ))
		case "nary":
			items, ok := node[ar.a].([]interface{})
			if !ok {
				raise("%s expects an '%s' list", typ, ar.a)
			}
			isAnd := typ == tx+"/And"
			for _, item := range items {
				sub, ok := item.(map[string]interface{})
				if !ok {
					raise("%s operand is not a node", typ)
				}
				v := truthy(requireNum(evalNode(Node(sub), ctx, resolve, opts, frames), typ))
				if isAnd && !v {
					return float64(0)
				}
				if !isAnd && v {
					return float64(1)
				}
			}
			return boolNum(isAnd)
		case "ternary":
			v := requireNum(evalNode(operand(node, ar.a), ctx, resolve, opts, frames), typ)
			lo := requireNum(evalNode(operand(node, ar.b), ctx, resolve, opts, frames), typ)
			hi := requireNum(evalNode(operand(node, ar.c), ctx, resolve, opts, frames), typ)
			return math.Min(math.Max(v, lo), hi)
		}
	}

	if typ == tx+"/Not" {
		x := evalNode(operand(node, "operand"), ctx, resolve, opts, frames)
		return boolNum(!truthy(requireNum(x, typ)))
	}

	if typ == tx+"/IsAtLeast" || typ == tx+"/Dominates" {
		v, _, _ := foldOrdered(node, ctx, opts)
		return v
	}

	if isListFold(typ) {
		return listFold(typ, sourceList(node, ctx, resolve, opts, frames), node)
	}

	if bodyKey, ok := iteratorBody(typ); ok {
		loopVar, ok := node["loopVar"].(string)
		if !ok || loopVar == "" {
			raise("%s is missing loopVar", typ)
		}
		items := sourceList(node, ctx, resolve, opts, frames)
		body := operand(node, bodyKey)
		out := make([]Value, 0, len(items))
		for _, el := range items {
			inner := append(frames, frame{name: loopVar, value: el})
			v := evalNode(body, ctx, resolve, opts, inner)
			switch typ {
			case tx + "/Filter":
				if truthy(requireNum(v, "Filter predicate")) {
					out = append(out, el)
				}
			case tx + "/ForEach":
				// Flatten one level; an empty list contributes nothing.
				if l, ok := v.([]Value); ok {
					out = append(out, l...)
				} else {
					out = append(out, v)
				}
			default:
				out = append(out, v)
			}
		}
		return out
	}

	if typ == tx+"/Contains" {
		hay := evalNode(operand(node, "haystack"), ctx, resolve, opts, frames)
		needle := evalNode(operand(node, "needle"), ctx, resolve, opts, frames)
		items, ok := hay.([]Value)
		if !ok {
			items = []Value{hay}
		}
		for _, el := range items {
			if valuesEqual(el, needle) {
				return float64(1)
			}
		}
		return float64(0)
	}

	if typ == tx+"/IsSet" {
		return boolNum(isSet(evalNode(operand(node, "checkExpr"), ctx, resolve, opts, frames)))
	}

	if typ == tx+"/ListItemAt" {
		items := sourceList(node, ctx, resolve, opts, frames)
		idx := evalNode(operand(node, "itemIndex"), ctx, resolve, opts, frames)
		n, ok := idx.(float64)
		if !ok || n != math.Trunc(n) || n < 0 {
			raise("ListItemAt itemIndex must be a non-negative integer")
		}
		i := int(n)
		// Past the end is ABSENCE (the empty list); guard with IsSet.
		if i < len(items) {
			return items[i]
		}
		return []Value{}
	}

	if typ == tx+"/Matches" {
		src := evalNode(operand(node, "matchSource"), ctx, resolve, opts, frames)
		s, ok := src.(string)
		if !ok {
			raise("Matches requires a string matchSource, got %s", kindOf(src))
		}
		pattern, ok := node["pattern"].(string)
		if !ok {
			raise("Matches is missing pattern")
		}
		return boolNum(matchesPattern(s, pattern))
	}

	if v, ok := kindPredicate(typ, nil); ok {
		_ = v
		x := evalNode(operand(node, "kindCheck"), ctx, resolve, opts, frames)
		out, _ := kindPredicate(typ, x)
		return out
	}

	if lit, ok := literalValue(node); ok {
		return lit
	}

	// A VarRef naming a lexically-enclosing loopVar is the kernel's own bound
	// variable — the ONLY leaf the kernel answers. Everything else is the
	// caller's; recursion from inside resolve re-enters WITHOUT frames.
	if typ == tx+"/VarRef" {
		if name, ok := node["varName"].(string); ok {
			if v, ok := boundValue(frames, name); ok {
				return v
			}
		}
	}

	return resolve(node, ctx, func(n Node, c interface{}) Value {
		return evalNode(n, c, resolve, opts, nil)
	})
}

// Evaluate evaluates an expression tree to a value.
func Evaluate(node Node, ctx interface{}, resolve Resolve) (result Value, err error) {
	return EvaluateWithOptions(node, ctx, resolve, nil)
}

// EvaluateWithOptions evaluates with ordered-comparison context.
func EvaluateWithOptions(node Node, ctx interface{}, resolve Resolve, opts *Options) (result Value, err error) {
	defer func() {
		if r := recover(); r != nil {
			if e, ok := r.(*Error); ok {
				err = e
				return
			}
			panic(r)
		}
	}()
	return evalNode(node, ctx, resolve, opts, nil), nil
}

// --- explain ----------------------------------------------------------------

// TraceNode is one node of an evaluation trace — the verdict tree Explain
// returns. Short-circuited operands are absent from Children; an iterating
// operator's children are its source trace followed by one body trace per
// visited element. A runtime return shape, not an ontology class.
type TraceNode struct {
	Type     string
	Value    Value
	Children []*TraceNode
	LeftRef  string
	RightRef string
	HasRefs  bool
}

func leafTrace(typ string, v Value) *TraceNode {
	return &TraceNode{Type: typ, Value: v}
}

func explainPanic(node Node, ctx interface{}, resolve Resolve, opts *Options, frames []frame) *TraceNode {
	typ := node.Type()
	if typ == "" {
		raise("Expression node has no 'type'")
	}

	if ar, ok := operatorArity(typ); ok {
		switch ar.kind {
		case "unary":
			x := explainPanic(operand(node, ar.a), ctx, resolve, opts, frames)
			return &TraceNode{Type: typ, Value: unaryPrim(typ, requireNum(x.Value, typ)), Children: []*TraceNode{x}}
		case "binary":
			a := explainPanic(operand(node, ar.a), ctx, resolve, opts, frames)
			b := explainPanic(operand(node, ar.b), ctx, resolve, opts, frames)
			var value Value
			if typ == tx+"/Equals" {
				value = boolNum(valuesEqual(a.Value, b.Value))
			} else if _, isOrder := binaryOrder(typ, 0, 0); isOrder {
				an, aok := a.Value.(float64)
				bn, bok := b.Value.(float64)
				if !aok || !bok {
					value = float64(0)
				} else {
					value, _ = binaryOrder(typ, an, bn)
				}
			} else {
				value = binaryArith(typ, requireNum(a.Value, typ), requireNum(b.Value, typ))
			}
			return &TraceNode{Type: typ, Value: value, Children: []*TraceNode{a, b}}
		case "nary":
			items, ok := node[ar.a].([]interface{})
			if !ok {
				raise("%s expects an '%s' list", typ, ar.a)
			}
			isAnd := typ == tx+"/And"
			children := []*TraceNode{}
			for _, item := range items {
				sub, ok := item.(map[string]interface{})
				if !ok {
					raise("%s operand is not a node", typ)
				}
				child := explainPanic(Node(sub), ctx, resolve, opts, frames)
				children = append(children, child)
				v := truthy(requireNum(child.Value, typ))
				if isAnd && !v {
					return &TraceNode{Type: typ, Value: float64(0), Children: children}
				}
				if !isAnd && v {
					return &TraceNode{Type: typ, Value: float64(1), Children: children}
				}
			}
			return &TraceNode{Type: typ, Value: boolNum(isAnd), Children: children}
		case "ternary":
			tv := explainPanic(operand(node, ar.a), ctx, resolve, opts, frames)
			tlo := explainPanic(operand(node, ar.b), ctx, resolve, opts, frames)
			thi := explainPanic(operand(node, ar.c), ctx, resolve, opts, frames)
			v := requireNum(tv.Value, typ)
			lo := requireNum(tlo.Value, typ)
			hi := requireNum(thi.Value, typ)
			return &TraceNode{Type: typ, Value: math.Min(math.Max(v, lo), hi), Children: []*TraceNode{tv, tlo, thi}}
		}
	}

	if typ == tx+"/Not" {
		x := explainPanic(operand(node, "operand"), ctx, resolve, opts, frames)
		return &TraceNode{Type: typ, Value: boolNum(!truthy(requireNum(x.Value, typ))), Children: []*TraceNode{x}}
	}

	if typ == tx+"/IsAtLeast" || typ == tx+"/Dominates" {
		v, l, r := foldOrdered(node, ctx, opts)
		return &TraceNode{Type: typ, Value: v, LeftRef: l, RightRef: r, HasRefs: true}
	}

	if isListFold(typ) {
		src := explainPanic(operand(node, "source"), ctx, resolve, opts, frames)
		items, ok := src.Value.([]Value)
		if !ok {
			items = []Value{src.Value}
		}
		return &TraceNode{Type: typ, Value: listFold(typ, items, node), Children: []*TraceNode{src}}
	}

	if bodyKey, ok := iteratorBody(typ); ok {
		loopVar, ok := node["loopVar"].(string)
		if !ok || loopVar == "" {
			raise("%s is missing loopVar", typ)
		}
		src := explainPanic(operand(node, "source"), ctx, resolve, opts, frames)
		items, isList := src.Value.([]Value)
		if !isList {
			items = []Value{src.Value}
		}
		body := operand(node, bodyKey)
		children := []*TraceNode{src}
		out := make([]Value, 0, len(items))
		for _, el := range items {
			inner := append(frames, frame{name: loopVar, value: el})
			bt := explainPanic(body, ctx, resolve, opts, inner)
			children = append(children, bt)
			v := bt.Value
			switch typ {
			case tx + "/Filter":
				if truthy(requireNum(v, "Filter predicate")) {
					out = append(out, el)
				}
			case tx + "/ForEach":
				if l, ok := v.([]Value); ok {
					out = append(out, l...)
				} else {
					out = append(out, v)
				}
			default:
				out = append(out, v)
			}
		}
		return &TraceNode{Type: typ, Value: out, Children: children}
	}

	if typ == tx+"/Contains" {
		hay := explainPanic(operand(node, "haystack"), ctx, resolve, opts, frames)
		needle := explainPanic(operand(node, "needle"), ctx, resolve, opts, frames)
		items, ok := hay.Value.([]Value)
		if !ok {
			items = []Value{hay.Value}
		}
		v := float64(0)
		for _, el := range items {
			if valuesEqual(el, needle.Value) {
				v = 1
				break
			}
		}
		return &TraceNode{Type: typ, Value: v, Children: []*TraceNode{hay, needle}}
	}

	if typ == tx+"/IsSet" {
		x := explainPanic(operand(node, "checkExpr"), ctx, resolve, opts, frames)
		return &TraceNode{Type: typ, Value: boolNum(isSet(x.Value)), Children: []*TraceNode{x}}
	}

	if typ == tx+"/ListItemAt" {
		src := explainPanic(operand(node, "source"), ctx, resolve, opts, frames)
		idx := explainPanic(operand(node, "itemIndex"), ctx, resolve, opts, frames)
		items, ok := src.Value.([]Value)
		if !ok {
			items = []Value{src.Value}
		}
		n, isNum := idx.Value.(float64)
		if !isNum || n != math.Trunc(n) || n < 0 {
			raise("ListItemAt itemIndex must be a non-negative integer")
		}
		var value Value = []Value{}
		if int(n) < len(items) {
			value = items[int(n)]
		}
		return &TraceNode{Type: typ, Value: value, Children: []*TraceNode{src, idx}}
	}

	if typ == tx+"/Matches" {
		src := explainPanic(operand(node, "matchSource"), ctx, resolve, opts, frames)
		s, ok := src.Value.(string)
		if !ok {
			raise("Matches requires a string matchSource, got %s", kindOf(src.Value))
		}
		pattern, ok := node["pattern"].(string)
		if !ok {
			raise("Matches is missing pattern")
		}
		return &TraceNode{Type: typ, Value: boolNum(matchesPattern(s, pattern)), Children: []*TraceNode{src}}
	}

	if _, ok := kindPredicate(typ, nil); ok {
		x := explainPanic(operand(node, "kindCheck"), ctx, resolve, opts, frames)
		v, _ := kindPredicate(typ, x.Value)
		return &TraceNode{Type: typ, Value: v, Children: []*TraceNode{x}}
	}

	if lit, ok := literalValue(node); ok {
		return leafTrace(typ, lit)
	}

	if typ == tx+"/VarRef" {
		if name, ok := node["varName"].(string); ok {
			if v, ok := boundValue(frames, name); ok {
				return leafTrace(typ, v)
			}
		}
	}

	v := resolve(node, ctx, func(n Node, c interface{}) Value {
		return evalNode(n, c, resolve, opts, nil)
	})
	return leafTrace(typ, v)
}

// Explain evaluates and returns the verdict tree. The root's Value is exactly
// what Evaluate returns for the same inputs; the conformance suite runs every
// vector through both and requires agreement.
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
	return explainPanic(node, ctx, resolve, opts, nil), nil
}
