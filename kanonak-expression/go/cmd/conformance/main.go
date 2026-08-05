// Command conformance drives the shared expression parity vectors through this
// Go port and prints "expression-vectors: P/N pass", exiting non-zero on any
// failure. The resolve hook binds tx.VarRef names from each vector's env, and
// the resolveRef hook binds identity leaves from refEnv — the demonstration
// that binding (numeric and identity alike) lives in the caller, not the
// runtime. Every vector runs through Evaluate AND Explain and their values
// must agree; vectors with a trace assert the verdict tree structurally.
//
// Run: go run ./cmd/conformance
package main

import (
	"encoding/json"
	"fmt"
	"math"
	"os"

	expr "github.com/kanonak-protocol/runtime/kanonak-expression/go"
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
	Closures    expr.ClosureTable      `json:"closures"`
	Expected    *float64               `json:"expected"`
	Tolerance   *float64               `json:"tolerance"`
	ExpectError bool                   `json:"expectError"`
	Trace       *traceExpect           `json:"trace"`
}

type ctx struct {
	env    map[string]float64
	refEnv map[string]string
}

func resolve(node expr.Node, c interface{}, _ func(expr.Node, interface{}) float64) float64 {
	if node.Type() == varRef {
		name, _ := node["varName"].(string)
		cc, _ := c.(*ctx)
		v, ok := cc.env[name]
		if !ok {
			panic(&expr.Error{Msg: fmt.Sprintf("Unbound variable %q", name)})
		}
		return v
	}
	panic(&expr.Error{Msg: fmt.Sprintf("No resolver for leaf '%s'", node.Type())})
}

func resolveRef(node expr.Node, c interface{}) string {
	if node.Type() == varRef {
		name, _ := node["varName"].(string)
		cc, _ := c.(*ctx)
		v, ok := cc.refEnv[name]
		if !ok {
			panic(&expr.Error{Msg: fmt.Sprintf("Unbound reference %q", name)})
		}
		return v
	}
	panic(&expr.Error{Msg: fmt.Sprintf("No reference resolver for leaf '%s'", node.Type())})
}

func traceMatches(got *expr.TraceNode, want *traceExpect) bool {
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

func main() {
	data, err := os.ReadFile("../vectors/expression-vectors.json")
	if err != nil {
		fmt.Fprintln(os.Stderr, "read vectors:", err)
		os.Exit(1)
	}
	var doc struct {
		Vectors []vector `json:"vectors"`
	}
	if err := json.Unmarshal(data, &doc); err != nil {
		fmt.Fprintln(os.Stderr, "parse vectors:", err)
		os.Exit(1)
	}

	pass, fail := 0, 0
	for _, v := range doc.Vectors {
		c := &ctx{env: v.Env, refEnv: v.RefEnv}
		if c.env == nil {
			c.env = map[string]float64{}
		}
		if c.refEnv == nil {
			c.refEnv = map[string]string{}
		}
		opts := &expr.Options{Closures: v.Closures, ResolveRef: resolveRef}
		got, evalErr := expr.EvaluateWithOptions(expr.Node(v.Expr), c, resolve, opts)
		trace, traceErr := expr.Explain(expr.Node(v.Expr), c, resolve, opts)

		if v.ExpectError {
			if evalErr != nil && traceErr != nil {
				pass++
			} else {
				fail++
				fmt.Fprintf(os.Stderr, "%s: expected an error from Evaluate AND Explain\n", v.ID)
			}
			continue
		}
		if evalErr != nil {
			fail++
			fmt.Fprintf(os.Stderr, "%s: threw %v\n", v.ID, evalErr)
			continue
		}
		if traceErr != nil {
			fail++
			fmt.Fprintf(os.Stderr, "%s: Explain threw %v\n", v.ID, traceErr)
			continue
		}
		exp := *v.Expected
		ok := got == exp
		if v.Tolerance != nil {
			ok = math.Abs(got-exp) <= *v.Tolerance
		}
		if !ok {
			fail++
			fmt.Fprintf(os.Stderr, "%s: expected %v got %v\n", v.ID, exp, got)
			continue
		}
		if trace.Value != got {
			fail++
			fmt.Fprintf(os.Stderr, "%s: Explain value %v != Evaluate value %v\n", v.ID, trace.Value, got)
			continue
		}
		if v.Trace != nil && !traceMatches(trace, v.Trace) {
			fail++
			fmt.Fprintf(os.Stderr, "%s: trace mismatch\n", v.ID)
			continue
		}
		pass++
	}

	fmt.Printf("expression-vectors: %d/%d pass\n", pass, len(doc.Vectors))
	if fail > 0 {
		fmt.Fprintf(os.Stderr, "\n%d FAILURES\n", fail)
		os.Exit(1)
	}
}
