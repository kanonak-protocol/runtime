package expression

import (
	"encoding/json"
	"math"
	"os"
	"testing"
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
		env, _ := ctx.(map[string]float64)
		v, ok := env[name]
		if !ok {
			raise("Unbound variable %q", name)
		}
		return v
	}
	raise("No resolver for leaf '%s'", node.Type())
	return 0
}

func TestExpressionVectors(t *testing.T) {
	vectors := readVectors(t, "expression-vectors.json")
	pass := 0
	total := len(vectors)
	for _, v := range vectors {
		env := v.Env
		if env == nil {
			env = map[string]float64{}
		}
		got, err := Evaluate(Node(v.Expr), env, conformanceResolve)

		if v.ExpectError {
			if err == nil {
				t.Errorf("%s: expected an error, got %v", v.ID, got)
				continue
			}
			pass++
			continue
		}
		if err != nil {
			t.Errorf("%s: threw %v", v.ID, err)
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
		pass++
	}
	t.Logf("expression-vectors: %d/%d pass", pass, total)
	if pass != total {
		t.Fatalf("expression-vectors: %d/%d pass", pass, total)
	}
}
