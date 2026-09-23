// Drives the shared parity vectors through the Go port — BOTH files:
// expression-vectors.json (v1 — passes UNCHANGED under the v2 kernel; the
// numeric-regression gate) and expression-vectors-2.json (the value-domain
// extension). Every vector runs through Evaluate AND Explain and their values
// must agree; env bindings and expected are Values (numbers, strings, arrays,
// {"ref": …} objects); vectors with a trace assert the verdict tree
// structurally.
package expression

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

type vectorFile struct {
	Vectors []map[string]interface{} `json:"vectors"`
}

// valueOf converts the vector-file JSON encoding into a kernel Value.
func valueOf(v interface{}) Value {
	switch e := v.(type) {
	case bool:
		return boolNum(e)
	case float64:
		return e
	case string:
		return e
	case []interface{}:
		out := make([]Value, len(e))
		for i, x := range e {
			out[i] = valueOf(x)
		}
		return out
	case map[string]interface{}:
		if r, ok := e["ref"].(string); ok {
			return Ref{URI: r}
		}
	}
	panic("unrepresentable vector value")
}

func valuesDeepEqual(a, b Value) bool {
	al, aok := a.([]Value)
	bl, bok := b.([]Value)
	if aok && bok {
		if len(al) != len(bl) {
			return false
		}
		for i := range al {
			if !valuesDeepEqual(al[i], bl[i]) {
				return false
			}
		}
		return true
	}
	if aok || bok {
		return false
	}
	return valuesEqual(a, b)
}

func makeResolve(env map[string]Value, graph map[string]map[string]Value) Resolve {
	return func(node Node, ctx interface{}, evaluate func(Node, interface{}) Value) Value {
		if node.Type() == tx+"/VarRef" {
			name, _ := node["varName"].(string)
			if v, ok := env[name]; ok {
				return v
			}
			raise("Unbound variable %q", name)
		}
		// tx.PropertyRead: a host graph read through the handed-back evaluate
		// (so an enclosing loopVar is visible — runtime#25); absent → empty
		// list, several → list, one → itself.
		if node.Type() == tx+"/PropertyRead" {
			src, _ := node["readSource"].(map[string]interface{})
			source := evaluate(Node(src), ctx)
			r, ok := source.(Ref)
			if !ok {
				raise("PropertyRead over a non-ref: %v", source)
			}
			prop, _ := node["readProp"].(string)
			if props, ok := graph[r.URI]; ok {
				if values, ok := props[prop]; ok {
					return values
				}
			}
			return []Value{}
		}
		raise("No resolver for leaf '%s'", node.Type())
		return nil
	}
}

func makeResolveRef(refEnv map[string]string) ResolveRef {
	return func(node Node, _ interface{}) string {
		if node.Type() == tx+"/VarRef" {
			name, _ := node["varName"].(string)
			if v, ok := refEnv[name]; ok {
				return v
			}
			raise("Unbound reference %q", name)
		}
		raise("No reference resolver for leaf '%s'", node.Type())
		return ""
	}
}

func traceMatches(got *TraceNode, want map[string]interface{}) bool {
	if typ, _ := want["type"].(string); typ != got.Type {
		return false
	}
	wantValue, ok := want["value"]
	if !ok || !valuesDeepEqual(got.Value, valueOf(wantValue)) {
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

func runFile(t *testing.T, name string) {
	raw, err := os.ReadFile(filepath.Join("..", "vectors", name))
	if err != nil {
		t.Fatalf("read %s: %v", name, err)
	}
	var doc vectorFile
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatalf("parse %s: %v", name, err)
	}

	pass := 0
	for _, v := range doc.Vectors {
		id, _ := v["id"].(string)
		exprRaw, _ := v["expr"].(map[string]interface{})
		expr := Node(exprRaw)

		env := map[string]Value{}
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
		graph := map[string]map[string]Value{}
		if g, ok := v["graph"].(map[string]interface{}); ok {
			for uri, props := range g {
				m := map[string]Value{}
				if pm, ok := props.(map[string]interface{}); ok {
					for prop, x := range pm {
						m[prop] = valueOf(x)
					}
				}
				graph[uri] = m
			}
		}
		var closures ClosureTable
		if c, ok := v["closures"].(map[string]interface{}); ok {
			closures = ClosureTable{}
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
		opts := &Options{Closures: closures, ResolveRef: makeResolveRef(refEnv)}
		resolve := makeResolve(env, graph)

		expectError, _ := v["expectError"].(bool)
		got, evalErr := EvaluateWithOptions(expr, nil, resolve, opts)
		trace, explainErr := Explain(expr, nil, resolve, opts)

		if expectError {
			if evalErr != nil && explainErr != nil {
				pass++
			} else {
				t.Errorf("%s/%s: expected an error from Evaluate AND Explain", name, id)
			}
			continue
		}
		if evalErr != nil {
			t.Errorf("%s/%s: raised %v", name, id, evalErr)
			continue
		}
		if explainErr != nil {
			t.Errorf("%s/%s: explain raised %v", name, id, explainErr)
			continue
		}

		expected := valueOf(v["expected"])
		ok := false
		if tol, hasTol := v["tolerance"].(float64); hasTol {
			gn, gok := got.(float64)
			en, eok := expected.(float64)
			ok = gok && eok && abs(gn-en) <= tol
		} else {
			ok = valuesDeepEqual(got, expected)
		}
		if !ok {
			t.Errorf("%s/%s: expected %v got %v", name, id, expected, got)
			continue
		}
		if !valuesDeepEqual(trace.Value, got) {
			t.Errorf("%s/%s: explain value %v != evaluate value %v", name, id, trace.Value, got)
			continue
		}
		if w, ok := v["trace"].(map[string]interface{}); ok {
			if !traceMatches(trace, w) {
				t.Errorf("%s/%s: trace mismatch", name, id)
				continue
			}
		}
		pass++
	}
	t.Logf("%s: %d/%d pass", name, pass, len(doc.Vectors))
}

func abs(x float64) float64 {
	if x < 0 {
		return -x
	}
	return x
}

func TestExpressionVectors(t *testing.T) {
	// v1 vectors are the regression gate: every one passes unchanged under v2.
	runFile(t, "expression-vectors.json")
	runFile(t, "expression-vectors-2.json")
	runAlignFile(t, "expression-alignment-vectors.json")
}

// alignedMatches is structural equality of an aligned tree against the
// vector's expected JSON tree.
func alignedMatches(got *AlignedNode, want map[string]interface{}) bool {
	typ, _ := want["type"].(string)
	if got.Expr.Type() != typ || got.Trace.Type != typ {
		return false
	}
	wantOperand, _ := want["operand"].(string)
	if got.Operand != wantOperand {
		return false
	}
	if wi, ok := want["index"].(float64); ok {
		if !got.HasIndex || float64(got.Index) != wi {
			return false
		}
	} else if got.HasIndex {
		return false
	}
	if wel, ok := want["element"].(map[string]interface{}); ok {
		lv, _ := wel["loopVar"].(string)
		if got.Element == nil || got.Element.LoopVar != lv || !valuesDeepEqual(got.Element.Value, valueOf(wel["value"])) {
			return false
		}
	} else if got.Element != nil {
		return false
	}
	wv, ok := want["value"]
	if !ok || !valuesDeepEqual(got.Trace.Value, valueOf(wv)) {
		return false
	}
	wantChildren, _ := want["children"].([]interface{})
	if len(wantChildren) != len(got.Children) {
		return false
	}
	for i, c := range got.Children {
		w, _ := wantChildren[i].(map[string]interface{})
		if !alignedMatches(c, w) {
			return false
		}
	}
	return true
}

// runAlignFile drives the alignment vectors: explain expr, then Align — the
// pairing must be exactly the expected tree, or an error where one is expected.
func runAlignFile(t *testing.T, name string) {
	raw, err := os.ReadFile(filepath.Join("..", "vectors", name))
	if err != nil {
		t.Fatalf("read %s: %v", name, err)
	}
	var doc vectorFile
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatalf("parse %s: %v", name, err)
	}
	pass := 0
	for _, v := range doc.Vectors {
		id, _ := v["id"].(string)
		exprRaw, _ := v["expr"].(map[string]interface{})
		expr := Node(exprRaw)
		env := map[string]Value{}
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
		graph := map[string]map[string]Value{}
		if g, ok := v["graph"].(map[string]interface{}); ok {
			for uri, props := range g {
				m := map[string]Value{}
				if pm, ok := props.(map[string]interface{}); ok {
					for prop, x := range pm {
						m[prop] = valueOf(x)
					}
				}
				graph[uri] = m
			}
		}
		var closures ClosureTable
		if c, ok := v["closures"].(map[string]interface{}); ok {
			closures = ClosureTable{}
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
		opts := &Options{Closures: closures, ResolveRef: makeResolveRef(refEnv)}
		trace, err := Explain(expr, nil, makeResolve(env, graph), opts)
		if err != nil {
			t.Errorf("[%s/%s] explain: %v", name, id, err)
			continue
		}
		if expectError, _ := v["expectError"].(bool); expectError {
			target := expr
			if ae, ok := v["alignExpr"].(map[string]interface{}); ok {
				target = Node(ae)
			}
			if _, err := Align(target, trace); err == nil {
				t.Errorf("[%s/%s] expected align to reject the trace", name, id)
				continue
			}
			pass++
			continue
		}
		got, err := Align(expr, trace)
		if err != nil {
			t.Errorf("[%s/%s] align: %v", name, id, err)
			continue
		}
		want, _ := v["expected"].(map[string]interface{})
		if !alignedMatches(got, want) {
			t.Errorf("[%s/%s] alignment mismatch", name, id)
			continue
		}
		pass++
	}
	t.Logf("%s: %d/%d pass", name, pass, len(doc.Vectors))
}
