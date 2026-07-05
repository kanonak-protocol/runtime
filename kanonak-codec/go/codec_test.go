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
			got := normalizeJSON(t, Serialize(node))
			want := normalizeJSON(t, c.ExpectedSerialize[i])
			if !reflect.DeepEqual(got, want) {
				t.Errorf("[%s] serialize[%d] mismatch\n  expected: %v\n  actual:   %v", c.ID, i, want, got)
			}
		}
	}
}
