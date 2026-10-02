package codec

import (
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"
)

type compatVectors struct {
	Pkg              PackageContext         `json:"pkg"`
	Schemas          map[string]CodecSchema `json:"schemas"`
	DeserializeCases []struct {
		ID          string                 `json:"id"`
		Schema      string                 `json:"schema"`
		Input       map[string]interface{} `json:"input"`
		Expected    map[string]interface{} `json:"expected"`
		ExpectError string                 `json:"expectError"`
	} `json:"deserializeCases"`
	TypeMatchesCases []struct {
		ID       string                 `json:"id"`
		Schema   string                 `json:"schema"`
		Node     map[string]interface{} `json:"node"`
		ClassURI string                 `json:"classUri"`
		Expected bool                   `json:"expected"`
	} `json:"typeMatchesCases"`
	EnumMemberCases []struct {
		ID       string         `json:"id"`
		Schema   string         `json:"schema"`
		Ref      string         `json:"ref"`
		Expected *compatMatched `json:"expected"`
	} `json:"enumMemberCases"`
	HashCases []struct {
		ID           string                   `json:"id"`
		Schema       string                   `json:"schema"`
		Nodes        []map[string]interface{} `json:"nodes"`
		ExpectedHash string                   `json:"expectedHash"`
		ExpectError  string                   `json:"expectError"`
	} `json:"hashCases"`
}

// compatMatched is an enumMember case's expected value; Label may be null.
type compatMatched struct {
	EnumType string  `json:"enumType"`
	URI      string  `json:"uri"`
	Label    *string `json:"label"`
}

// TestCodecVectorsCompat drives the 0.6.1 compatibility file (runtime#28):
// Deserialize / TypeMatches / LookupEnumMember / hashing over a node typed at
// an earlier compatible version of the schema's package. A rejection must end
// in the bracketed kind the vector names - the one part of an error message
// every port reproduces.
func TestCodecVectorsCompat(t *testing.T) {
	data, err := os.ReadFile("../vectors/codec-vectors-compat.json")
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var doc compatVectors
	decodeNumberAware(t, data, &doc)

	total := 0
	schemaOf := func(id, name string) CodecSchema {
		s, ok := doc.Schemas[name]
		if !ok {
			t.Fatalf("[%s] unknown schema %q", id, name)
		}
		return s
	}
	rejected := func(id, kind string, err error) {
		if err == nil {
			t.Errorf("[%s] expected a [%s] rejection, got a value", id, kind)
		} else if !strings.HasSuffix(err.Error(), "["+kind+"]") {
			t.Errorf("[%s] expected [%s], got: %v", id, kind, err)
		}
	}

	for _, c := range doc.DeserializeCases {
		total++
		got, err := Deserialize(c.Input, schemaOf(c.ID, c.Schema))
		if c.ExpectError != "" {
			rejected(c.ID, c.ExpectError, err)
			continue
		}
		if err != nil {
			t.Errorf("[%s] deserialize error: %v", c.ID, err)
			continue
		}
		if g, w := normalizeJSON(t, got), normalizeJSON(t, c.Expected); !reflect.DeepEqual(g, w) {
			t.Errorf("[%s] deserialize mismatch\n  expected: %v\n  actual:   %v", c.ID, w, g)
		}
	}

	for _, c := range doc.TypeMatchesCases {
		total++
		got, err := TypeMatches(c.Node, c.ClassURI, schemaOf(c.ID, c.Schema))
		if err != nil {
			t.Errorf("[%s] TypeMatches error: %v", c.ID, err)
		} else if got != c.Expected {
			t.Errorf("[%s] TypeMatches expected %v got %v", c.ID, c.Expected, got)
		}
		raw, _ := json.Marshal(c.Node)
		var typed KanonakNode
		if err := json.Unmarshal(raw, &typed); err != nil {
			t.Fatalf("[%s] typed node: %v", c.ID, err)
		}
		if got, err := typed.TypeMatches(c.ClassURI, schemaOf(c.ID, c.Schema)); err != nil || got != c.Expected {
			t.Errorf("[%s] KanonakNode.TypeMatches expected %v got %v (err %v)", c.ID, c.Expected, got, err)
		}
	}

	for _, c := range doc.EnumMemberCases {
		total++
		var got *compatMatched
		if m, ok := LookupEnumMember(schemaOf(c.ID, c.Schema), c.Ref); ok {
			got = &compatMatched{EnumType: m.EnumType, URI: m.URI}
			if m.Member.Label != "" {
				label := m.Member.Label
				got.Label = &label
			}
		}
		if !reflect.DeepEqual(got, c.Expected) {
			t.Errorf("[%s] LookupEnumMember expected %+v got %+v", c.ID, c.Expected, got)
		}
	}

	for _, c := range doc.HashCases {
		total++
		got, err := ContentHash(c.Nodes, schemaOf(c.ID, c.Schema), doc.Pkg)
		if c.ExpectError != "" {
			rejected(c.ID, c.ExpectError, err)
			continue
		}
		if err != nil {
			t.Errorf("[%s] content hash error: %v", c.ID, err)
		} else if got != c.ExpectedHash {
			t.Errorf("[%s] hash expected %s got %s", c.ID, c.ExpectedHash, got)
		}
	}

	if total == 0 {
		t.Fatalf("compat vectors carry no cases")
	}
	t.Logf("codec-vectors-compat.json: %d cases", total)
}

// TestTypeMatchesRejectsNonCoordinate pins that a malformed class URI (a bug
// in the caller, never data) fails visibly instead of answering false.
func TestTypeMatchesRejectsNonCoordinate(t *testing.T) {
	if _, err := TypeMatches(map[string]interface{}{"$type": "a/b@1.0.0/C"}, "not a coordinate", CodecSchema{}); err == nil {
		t.Fatalf("expected an error for a non-coordinate class URI")
	}
}
