package codec

import (
	"bytes"
	"encoding/json"
	"os"
	"reflect"
	"testing"
)

// decodeNumberAware unmarshals JSON with UseNumber() so numeric lexicals survive
// as json.Number ("5", "1.5") rather than being widened to float64.
func decodeNumberAware(t *testing.T, data []byte, dst interface{}) {
	t.Helper()
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	if err := dec.Decode(dst); err != nil {
		t.Fatalf("decode: %v", err)
	}
}

// normalizeJSON round-trips a value through encoding/json so two structurally
// equal values compare equal under reflect.DeepEqual (uniform number/map types).
func normalizeJSON(t *testing.T, v interface{}) interface{} {
	t.Helper()
	data, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var out interface{}
	if err := json.Unmarshal(data, &out); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	return out
}

type codecVectors struct {
	Schema json.RawMessage `json:"schema"`
	Cases  []struct {
		ID                    string                   `json:"id"`
		Pkg                   json.RawMessage          `json:"pkg"`
		Nodes                 []map[string]interface{} `json:"nodes"`
		ExpectedCanonicalForm string                   `json:"expectedCanonicalForm"`
		ExpectedHash          string                   `json:"expectedHash"`
		ExpectedSerialize     []map[string]interface{} `json:"expectedSerialize"`
	} `json:"cases"`
}

func TestCodecVectors(t *testing.T) {
	runVectorFile(t, "../vectors/codec-vectors.json")
}

func TestCodecVectorsEmbedded(t *testing.T) {
	runVectorFile(t, "../vectors/codec-vectors-embedded.json")
}

// runVectorFile asserts every case in one golden-vector file: canonical form,
// content hash, and wire serialization.
func runVectorFile(t *testing.T, path string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}

	var doc codecVectors
	decodeNumberAware(t, data, &doc)

	var schema CodecSchema
	decodeNumberAware(t, doc.Schema, &schema)

	for _, c := range doc.Cases {
		var pkg PackageContext
		decodeNumberAware(t, c.Pkg, &pkg)

		form, err := CanonicalForm(c.Nodes, schema, pkg)
		if err != nil {
			t.Errorf("[%s] canonical form error: %v", c.ID, err)
			continue
		}
		if form != c.ExpectedCanonicalForm {
			t.Errorf("[%s] form mismatch\n  expected: %s\n  actual:   %s", c.ID, c.ExpectedCanonicalForm, form)
		}

		hash, err := ContentHash(c.Nodes, schema, pkg)
		if err != nil {
			t.Errorf("[%s] content hash error: %v", c.ID, err)
			continue
		}
		if hash != c.ExpectedHash {
			t.Errorf("[%s] hash mismatch expected %s got %s", c.ID, c.ExpectedHash, hash)
		}

		if len(c.Nodes) != len(c.ExpectedSerialize) {
			t.Errorf("[%s] node/expectedSerialize length mismatch %d vs %d", c.ID, len(c.Nodes), len(c.ExpectedSerialize))
			continue
		}
		for i, node := range c.Nodes {
			wire, err := Serialize(node)
			if err != nil {
				t.Errorf("[%s] serialize[%d] error: %v", c.ID, i, err)
				continue
			}
			got := normalizeJSON(t, wire)
			want := normalizeJSON(t, c.ExpectedSerialize[i])
			if !reflect.DeepEqual(got, want) {
				t.Errorf("[%s] serialize[%d] mismatch\n  expected: %v\n  actual:   %v", c.ID, i, want, got)
			}
		}
	}
}

type typesVectors struct {
	Schema json.RawMessage `json:"schema"`
	Cases  []struct {
		ID                    string                   `json:"id"`
		Pkg                   json.RawMessage          `json:"pkg"`
		Nodes                 []map[string]interface{} `json:"nodes"`
		ExpectError           bool                     `json:"expectError"`
		ExpectedCanonicalForm string                   `json:"expectedCanonicalForm"`
		ExpectedHash          string                   `json:"expectedHash"`
		ExpectedSerialize     []map[string]interface{} `json:"expectedSerialize"`
	} `json:"cases"`
}

// TestCodecVectorsTypes drives the 0.4.0 multi-typed-subjects file (runtime#10).
// Beyond the standard form/hash/serialize checks it exercises the $types
// contract: expectError cases must be rejected on ALL THREE surfaces —
// Serialize (the producer fails at emit time), Deserialize (the reader rejects,
// never repairs), and canonicalization — and positive cases must round-trip:
// Deserialize(Serialize(x)) preserves $types exactly and re-canonicalizes to
// the same hash.
func TestCodecVectorsTypes(t *testing.T) {
	data, err := os.ReadFile("../vectors/codec-vectors-types.json")
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}

	var doc typesVectors
	decodeNumberAware(t, data, &doc)

	var schema CodecSchema
	decodeNumberAware(t, doc.Schema, &schema)

	for _, c := range doc.Cases {
		var pkg PackageContext
		decodeNumberAware(t, c.Pkg, &pkg)

		if c.ExpectError {
			if _, err := CanonicalForm(c.Nodes, schema, pkg); err == nil {
				t.Errorf("[%s] expected canonicalize to reject, it did not", c.ID)
			}
			serializeRejected := false
			deserializeRejected := false
			for _, node := range c.Nodes {
				if _, err := Serialize(node); err != nil {
					serializeRejected = true
				}
				if _, err := Deserialize(node, schema); err != nil {
					deserializeRejected = true
				}
			}
			if !serializeRejected {
				t.Errorf("[%s] expected serialize to reject, it did not", c.ID)
			}
			if !deserializeRejected {
				t.Errorf("[%s] expected deserialize to reject, it did not", c.ID)
			}
			continue
		}

		form, err := CanonicalForm(c.Nodes, schema, pkg)
		if err != nil {
			t.Errorf("[%s] canonical form error: %v", c.ID, err)
			continue
		}
		if form != c.ExpectedCanonicalForm {
			t.Errorf("[%s] form mismatch\n  expected: %s\n  actual:   %s", c.ID, c.ExpectedCanonicalForm, form)
		}

		hash, err := ContentHash(c.Nodes, schema, pkg)
		if err != nil {
			t.Errorf("[%s] content hash error: %v", c.ID, err)
			continue
		}
		if hash != c.ExpectedHash {
			t.Errorf("[%s] hash mismatch expected %s got %s", c.ID, c.ExpectedHash, hash)
		}

		roundTripped := make([]map[string]interface{}, 0, len(c.Nodes))
		for i, node := range c.Nodes {
			wire, err := Serialize(node)
			if err != nil {
				t.Errorf("[%s] serialize[%d] error: %v", c.ID, i, err)
				continue
			}
			got := normalizeJSON(t, wire)
			want := normalizeJSON(t, c.ExpectedSerialize[i])
			if !reflect.DeepEqual(got, want) {
				t.Errorf("[%s] serialize[%d] mismatch\n  expected: %v\n  actual:   %v", c.ID, i, want, got)
			}
			back, err := Deserialize(wire, schema)
			if err != nil {
				t.Errorf("[%s] round-trip deserialize[%d] error: %v", c.ID, i, err)
				continue
			}
			rtWire, err := Serialize(back)
			if err != nil {
				t.Errorf("[%s] round-trip serialize[%d] error: %v", c.ID, i, err)
				continue
			}
			if !reflect.DeepEqual(normalizeJSON(t, rtWire), want) {
				t.Errorf("[%s] round-trip serialize[%d] mismatch", c.ID, i)
			}
			roundTripped = append(roundTripped, back)
		}
		if len(roundTripped) == len(c.Nodes) {
			rtHash, err := ContentHash(roundTripped, schema, pkg)
			if err != nil {
				t.Errorf("[%s] round-trip hash error: %v", c.ID, err)
			} else if rtHash != c.ExpectedHash {
				t.Errorf("[%s] round-trip hash expected %s got %s", c.ID, c.ExpectedHash, rtHash)
			}
		}
	}
}
