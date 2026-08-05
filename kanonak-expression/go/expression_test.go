package expression

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

const varRef = "kanonak.org/transformations/VarRef"

type traceExpect struct {
	Type     string        `json:"type"`
	Value    float64       `json:"value"`
	Children []traceExpect `json:"children"`
	LeftRef  string        `json:"leftRef"`
	RightRef string        `json:"rightRef"`
}

type vector struct {
	ID          string                 `json:"id"`
	Expr        map[string]interface{} `json:"expr"`
	Env         map[string]float64     `json:"env"`
	RefEnv      map[string]string      `json:"refEnv"`
	Closures    ClosureTable           `json:"closures"`
	Expected    *float64               `json:"expected"`
	Tolerance   *float64               `json:"tolerance"`
	ExpectError bool                   `json:"expectError"`
	Trace       *traceExpect           `json:"trace"`
}

type conformanceCtx struct {
	env    map[string]float64
	refEnv map[string]string
}

func readVectors(t *testing.T, name string) []vector {
	data, err := os.ReadFile("../vectors/" + name)
	if err != nil {
		t.Fatalf("read %s: %v", name, err)
	}
	var doc struct {
		Vectors []vector `json:"vectors"`
	}
	if err := json.Unmarshal(data, &doc); err != nil {
		t.Fatalf("parse %s: %v", name, err)
	}
	return doc.Vectors
}

// resolve is the conformance hook: tx.VarRef -> env binding; any other leaf is
// unbound here and raises. This is the demonstration that variable binding lives
// in the caller, not the runtime.
func conformanceResolve(node Node, ctx interface{}, _ func(Node, interface{}) float64) float64 {
	if node.Type() == varRef {
		name, _ := node["varName"].(string)
		c, _ := ctx.(*conformanceCtx)
		v, ok := c.env[name]
		if !ok {
			raise("Unbound variable %q", name)
		}
		return v
	}
	raise("No resolver for leaf '%s'", node.Type())
	return 0
}

// conformanceResolveRef is the identity-domain mirror: tx.VarRef -> refEnv
// member URI. Same division as resolve — the kernel owns UriLiteral, the
// caller owns bindings.
func conformanceResolveRef(node Node, ctx interface{}) string {
	if node.Type() == varRef {
		name, _ := node["varName"].(string)
		c, _ := ctx.(*conformanceCtx)
		v, ok := c.refEnv[name]
		if !ok {
			raise("Unbound reference %q", name)
		}
		return v
	}
	raise("No reference resolver for leaf '%s'", node.Type())
	return ""
}

// traceMatches compares a produced verdict tree against the vector's expected
// tree structurally, including absent-vs-present refs.
func traceMatches(got *TraceNode, want *traceExpect) bool {
	if got.Type != want.Type || got.Value != want.Value {
		return false
	}
	if got.LeftRef != want.LeftRef || got.RightRef != want.RightRef {
		return false
	}
	if len(got.Children) != len(want.Children) {
		return false
	}
	for i, c := range got.Children {
		if !traceMatches(c, &want.Children[i]) {
			return false
		}
	}
	return true
}

func TestExpressionVectors(t *testing.T) {
	vectors := readVectors(t, "expression-vectors.json")
	pass := 0
	total := len(vectors)
	for _, v := range vectors {
		ctx := &conformanceCtx{env: v.Env, refEnv: v.RefEnv}
		if ctx.env == nil {
			ctx.env = map[string]float64{}
		}
		if ctx.refEnv == nil {
			ctx.refEnv = map[string]string{}
		}
		opts := &Options{Closures: v.Closures, ResolveRef: conformanceResolveRef}
		got, err := EvaluateWithOptions(Node(v.Expr), ctx, conformanceResolve, opts)
		trace, terr := Explain(Node(v.Expr), ctx, conformanceResolve, opts)

		if v.ExpectError {
			if err == nil || terr == nil {
				t.Errorf("%s: expected an error from Evaluate AND Explain", v.ID)
				continue
			}
			pass++
			continue
		}
		if err != nil {
			t.Errorf("%s: threw %v", v.ID, err)
			continue
		}
		if terr != nil {
			t.Errorf("%s: Explain threw %v", v.ID, terr)
			continue
		}
		exp := *v.Expected
		ok := got == exp
		if v.Tolerance != nil {
			ok = math.Abs(got-exp) <= *v.Tolerance
		}
		if !ok {
			t.Errorf("%s: expected %v got %v", v.ID, exp, got)
			continue
		}
		if trace.Value != got {
			t.Errorf("%s: Explain value %v != Evaluate value %v", v.ID, trace.Value, got)
			continue
		}
		if v.Trace != nil && !traceMatches(trace, v.Trace) {
			t.Errorf("%s: trace mismatch", v.ID)
			continue
		}
		pass++
	}
	t.Logf("expression-vectors: %d/%d pass", pass, total)
	if pass != total {
		t.Fatalf("expression-vectors: %d/%d pass", pass, total)
	}
}
