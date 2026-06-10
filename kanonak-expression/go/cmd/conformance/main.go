// Command conformance drives the shared expression parity vectors through this
// Go port and prints "expression-vectors: P/50 pass", exiting non-zero on any
// failure. The resolve hook binds tx.VarRef names from each vector's env — the
// demonstration that variable binding lives in the caller, not the runtime.
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

type vector struct {
	ID          string                 `json:"id"`
	Expr        map[string]interface{} `json:"expr"`
	Env         map[string]float64     `json:"env"`
	Expected    *float64               `json:"expected"`
	Tolerance   *float64               `json:"tolerance"`
	ExpectError bool                   `json:"expectError"`
}

func resolve(node expr.Node, ctx interface{}, _ func(expr.Node, interface{}) float64) float64 {
	if node.Type() == varRef {
		name, _ := node["varName"].(string)
		env, _ := ctx.(map[string]float64)
		v, ok := env[name]
		if !ok {
			panic(&expr.Error{Msg: fmt.Sprintf("Unbound variable %q", name)})
		}
		return v
	}
	panic(&expr.Error{Msg: fmt.Sprintf("No resolver for leaf '%s'", node.Type())})
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
		env := v.Env
		if env == nil {
			env = map[string]float64{}
		}
		got, evalErr := expr.Evaluate(expr.Node(v.Expr), env, resolve)

		if v.ExpectError {
			if evalErr != nil {
				pass++
			} else {
				fail++
				fmt.Fprintf(os.Stderr, "%s: expected an error, got %v\n", v.ID, got)
			}
			continue
		}
		if evalErr != nil {
			fail++
			fmt.Fprintf(os.Stderr, "%s: threw %v\n", v.ID, evalErr)
			continue
		}
		exp := *v.Expected
		ok := got == exp
		if v.Tolerance != nil {
			ok = math.Abs(got-exp) <= *v.Tolerance
		}
		if ok {
			pass++
		} else {
			fail++
			fmt.Fprintf(os.Stderr, "%s: expected %v got %v\n", v.ID, exp, got)
		}
	}

	fmt.Printf("expression-vectors: %d/%d pass\n", pass, len(doc.Vectors))
	if fail > 0 {
		fmt.Fprintf(os.Stderr, "\n%d FAILURES\n", fail)
		os.Exit(1)
	}
}
