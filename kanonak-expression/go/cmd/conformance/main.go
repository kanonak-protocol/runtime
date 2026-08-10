// Command conformance drives the shared expression parity vectors through this
// Go port — BOTH files: expression-vectors.json (v1 — the numeric-regression
// gate, unchanged under v2) and expression-vectors-2.json (the value-domain
// extension) — and prints per-file pass counts, exiting non-zero on any
// failure. The resolve hook binds tx.VarRef names from each vector's env
// (values are Values: numbers, strings, arrays, {"ref": …} objects), and the
// resolveRef hook binds identity leaves from refEnv. Every vector runs through
// Evaluate AND Explain and their values must agree; vectors with a trace
// assert the verdict tree structurally.
//
// Run: go run ./cmd/conformance
package main

import (
	"encoding/json"
	"fmt"
	"os"

	expr "github.com/kanonak-protocol/runtime/kanonak-expression/go"
)

const varRef = "kanonak.org/transformations/VarRef"

type vectorFile struct {
	Vectors []map[string]interface{} `json:"vectors"`
}

func valueOf(v interface{}) expr.Value {
	switch e := v.(type) {
	case bool:
		if e {
			return float64(1)
		}
		return float64(0)
	case float64:
		return e
	case string:
		return e
	case []interface{}:
		out := make([]expr.Value, len(e))
		for i, x := range e {
			out[i] = valueOf(x)
		}
		return out
	case map[string]interface{}:
		if r, ok := e["ref"].(string); ok {
			return expr.Ref{URI: r}
		}
	}
	panic("unrepresentable vector value")
}

func deepEqual(a, b expr.Value) bool {
	al, aok := a.([]expr.Value)
	bl, bok := b.([]expr.Value)
	if aok && bok {
		if len(al) != len(bl) {
			return false
		}
		for i := range al {
			if !deepEqual(al[i], bl[i]) {
				return false
			}
		}
		return true
	}
	if aok || bok {
		return false
	}
	switch av := a.(type) {
	case float64:
		bv, ok := b.(float64)
		return ok && av == bv
	case string:
		bv, ok := b.(string)
		return ok && av == bv
	case expr.Ref:
		bv, ok := b.(expr.Ref)
		return ok && av.URI == bv.URI
	}
	return false
}

func traceMatches(got *expr.TraceNode, want map[string]interface{}) bool {
	if typ, _ := want["type"].(string); typ != got.Type {
		return false
	}
	wantValue, ok := want["value"]
	if !ok || !deepEqual(got.Value, valueOf(wantValue)) {
		return false
	}
	wantLeft, _ := want["leftRef"].(string)
	wantRight, _ := want["rightRef"].(string)
	if wantLeft != got.LeftRef || wantRight != got.RightRef {
		return false
	}
	wantChildren, _ := want["children"].([]interface{})
	if len(wantChildren) != len(got.Children) {
		return false
	}
	for i, wc := range wantChildren {
		w, _ := wc.(map[string]interface{})
		if !traceMatches(got.Children[i], w) {
			return false
		}
	}
	return true
}

func runFile(name string) (pass, total int) {
	raw, err := os.ReadFile("../vectors/" + name)
	if err != nil {
		fmt.Fprintf(os.Stderr, "read %s: %v\n", name, err)
		os.Exit(1)
	}
	var doc vectorFile
	if err := json.Unmarshal(raw, &doc); err != nil {
		fmt.Fprintf(os.Stderr, "parse %s: %v\n", name, err)
		os.Exit(1)
	}

	for _, v := range doc.Vectors {
		total++
		id, _ := v["id"].(string)
		exprNode := expr.Node(v["expr"].(map[string]interface{}))

		env := map[string]expr.Value{}
		if e, ok := v["env"].(map[string]interface{}); ok {
			for k, x := range e {
				env[k] = valueOf(x)
			}
		}
		refEnv := map[string]string{}
		if e, ok := v["refEnv"].(map[string]interface{}); ok {
			for k, x := range e {
				if s, ok := x.(string); ok {
					refEnv[k] = s
				}
			}
		}
		var closures expr.ClosureTable
		if c, ok := v["closures"].(map[string]interface{}); ok {
			closures = expr.ClosureTable{}
			for prop, members := range c {
				inner := map[string][]string{}
				if m, ok := members.(map[string]interface{}); ok {
					for from, reach := range m {
						var set []string
						if arr, ok := reach.([]interface{}); ok {
							for _, x := range arr {
								if s, ok := x.(string); ok {
									set = append(set, s)
								}
							}
						}
						inner[from] = set
					}
				}
				closures[prop] = inner
			}
		}

		resolve := func(node expr.Node, _ interface{}, _ func(expr.Node, interface{}) expr.Value) expr.Value {
			if node.Type() == varRef {
				name, _ := node["varName"].(string)
				if v, ok := env[name]; ok {
					return v
				}
				panic(&expr.Error{Msg: fmt.Sprintf("Unbound variable %q", name)})
			}
			panic(&expr.Error{Msg: fmt.Sprintf("No resolver for leaf '%s'", node.Type())})
		}
		resolveRef := func(node expr.Node, _ interface{}) string {
			if node.Type() == varRef {
				name, _ := node["varName"].(string)
				if v, ok := refEnv[name]; ok {
					return v
				}
				panic(&expr.Error{Msg: fmt.Sprintf("Unbound reference %q", name)})
			}
			panic(&expr.Error{Msg: fmt.Sprintf("No reference resolver for leaf '%s'", node.Type())})
		}
		opts := &expr.Options{Closures: closures, ResolveRef: resolveRef}

		expectError, _ := v["expectError"].(bool)
		got, evalErr := expr.EvaluateWithOptions(exprNode, nil, resolve, opts)
		trace, explainErr := expr.Explain(exprNode, nil, resolve, opts)

		if expectError {
			if evalErr != nil && explainErr != nil {
				pass++
			} else {
				fmt.Fprintf(os.Stderr, "%s/%s: expected an error from Evaluate AND Explain\n", name, id)
			}
			continue
		}
		if evalErr != nil {
			fmt.Fprintf(os.Stderr, "%s/%s: raised %v\n", name, id, evalErr)
			continue
		}
		if explainErr != nil {
			fmt.Fprintf(os.Stderr, "%s/%s: explain raised %v\n", name, id, explainErr)
			continue
		}

		expected := valueOf(v["expected"])
		ok := false
		if tol, hasTol := v["tolerance"].(float64); hasTol {
			gn, gok := got.(float64)
			en, eok := expected.(float64)
			d := gn - en
			if d < 0 {
				d = -d
			}
			ok = gok && eok && d <= tol
		} else {
			ok = deepEqual(got, expected)
		}
		if !ok {
			fmt.Fprintf(os.Stderr, "%s/%s: expected %v got %v\n", name, id, expected, got)
			continue
		}
		if !deepEqual(trace.Value, got) {
			fmt.Fprintf(os.Stderr, "%s/%s: explain value %v != evaluate value %v\n", name, id, trace.Value, got)
			continue
		}
		if w, ok := v["trace"].(map[string]interface{}); ok {
			if !traceMatches(trace, w) {
				fmt.Fprintf(os.Stderr, "%s/%s: trace mismatch\n", name, id)
				continue
			}
		}
		pass++
	}
	fmt.Printf("%s: %d/%d pass\n", name, pass, total)
	return pass, total
}

func main() {
	p1, t1 := runFile("expression-vectors.json")
	p2, t2 := runFile("expression-vectors-2.json")
	if p1 != t1 || p2 != t2 {
		fmt.Fprintf(os.Stderr, "\n%d FAILURES\n", (t1-p1)+(t2-p2))
		os.Exit(1)
	}
	fmt.Println("ALL VECTORS PASS")
}
