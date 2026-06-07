package canonical

import (
	"encoding/json"
	"os"
	"testing"
)

func readVectors(t *testing.T, name string) map[string]interface{} {
	data, err := os.ReadFile("../vectors/" + name)
	if err != nil {
		t.Fatalf("read %s: %v", name, err)
	}
	var doc map[string]interface{}
	if err := json.Unmarshal(data, &doc); err != nil {
		t.Fatalf("parse %s: %v", name, err)
	}
	return doc
}

func TestLexicalVectors(t *testing.T) {
	doc := readVectors(t, "lexical-vectors.json")
	for _, raw := range doc["vectors"].([]interface{}) {
		v := raw.(map[string]interface{})
		id := v["id"].(string)
		carrier := Carrier(v["carrier"].(string))
		input := v["input"].(string)
		expectError := false
		if e, ok := v["expectError"].(bool); ok {
			expectError = e
		}
		actual, err := CanonicalScalarLexical(carrier, input)
		if expectError {
			if err == nil {
				t.Errorf("[%s] expected error, got %q", id, actual)
			}
			continue
		}
		if err != nil {
			t.Errorf("[%s] unexpected error: %v", id, err)
			continue
		}
		if expected := v["expected"].(string); actual != expected {
			t.Errorf("[%s] expected %q, got %q", id, expected, actual)
		}
	}
}

func TestFullFormVectors(t *testing.T) {
	doc := readVectors(t, "full-form-vectors.json")
	for _, raw := range doc["vectors"].([]interface{}) {
		v := raw.(map[string]interface{})
		id := v["id"].(string)
		pkg := decodeSubjects(v["input"].(map[string]interface{}))
		form, err := CanonicalForm(pkg)
		if err != nil {
			t.Errorf("[%s] form error: %v", id, err)
			continue
		}
		hash, _ := CanonicalHash(pkg)
		if exp := v["expectedCanonicalForm"].(string); form != exp {
			t.Errorf("[%s] form mismatch\n  expected: %s\n  actual:   %s", id, exp, form)
		}
		if exp := v["expectedHash"].(string); hash != exp {
			t.Errorf("[%s] hash mismatch expected %s got %s", id, exp, hash)
		}
	}
}

func decodeSubjects(input map[string]interface{}) Package {
	var subjects []Subject
	for _, s := range input["subjects"].([]interface{}) {
		sm := s.(map[string]interface{})
		subjects = append(subjects, Subject{
			URI:        sm["uri"].(string),
			Statements: decodeStatements(sm),
		})
	}
	return Package{Subjects: subjects}
}

func decodeStatements(node map[string]interface{}) []Statement {
	var out []Statement
	stmts, ok := node["statements"].([]interface{})
	if !ok {
		return out
	}
	for _, st := range stmts {
		sm := st.(map[string]interface{})
		out = append(out, Statement{
			Predicate: sm["predicate"].(string),
			Value:     decodeValue(sm["value"].(map[string]interface{})),
		})
	}
	return out
}

func decodeValue(v map[string]interface{}) Value {
	if lit, ok := v["lit"].(string); ok {
		if c, found := CarrierOf(v["datatype"].(string)); found {
			return Typed{Carrier: c, Lexical: lit}
		}
		return Raw{Token: lit}
	}
	if raw, ok := v["raw"].(string); ok {
		return Raw{Token: raw}
	}
	if r, ok := v["ref"].(string); ok {
		return Ref{URI: r}
	}
	if emb, ok := v["embed"].(map[string]interface{}); ok {
		var name *string
		if n, ok := emb["name"].(string); ok {
			name = &n
		}
		return Embedded{Name: name, Statements: decodeStatements(emb)}
	}
	if list, ok := v["list"].([]interface{}); ok {
		var items []Value
		for _, item := range list {
			items = append(items, decodeValue(item.(map[string]interface{})))
		}
		return List{Items: items}
	}
	panic("decode: unknown value shape")
}
