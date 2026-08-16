package codec

import (
	"os"
	"reflect"
	"regexp"
	"testing"
)

// memberURIRe pins the durable VERSIONED formation of an enum member key:
// publisher/package@version/name, byte-identical to what the wire carries as
// {"$ref": ...}. A versionless key would look right and miss every lookup.
var memberURIRe = regexp.MustCompile(`^[^/]+/[^/@]+@\d+\.\d+\.\d+/[^/]+$`)

// TestCodecVectorsEnums drives the 0.5.0 enumerations file (`enums`,
// runtime#21). Beyond the standard form/hash/serialize checks it pins the
// enumeration contract:
//
//   - the schema parses with `enums` keyed by durable VERSIONED URIs at both
//     levels;
//   - an enumeration STANDS ALONE: its class has no `classes` twin;
//   - `enums` and an enum-ranged `range` are canonicalization-INERT (the
//     positive hashes are the ones the same nodes produce without them);
//   - expectError cases are rejected at CANONICALIZATION. Unlike the $types
//     file's all-three-surfaces contract this violation is schema-DEPENDENT:
//     Serialize is schema-free and Deserialize does not recurse into embedded
//     values, so canonicalization is the only surface that can see it.
func TestCodecVectorsEnums(t *testing.T) {
	data, err := os.ReadFile("../vectors/codec-vectors-enums.json")
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}

	var doc typesVectors
	decodeNumberAware(t, data, &doc)

	var schema CodecSchema
	decodeNumberAware(t, doc.Schema, &schema)

	// --- structural assertions on the schema itself ---
	if len(schema.Enums) == 0 {
		t.Fatalf("schema carries no enums")
	}
	for key, en := range schema.Enums {
		if key != en.TypeURI {
			t.Errorf("enum key %s != typeUri %s", key, en.TypeURI)
		}
		// An enumeration stands alone. The absence of a Classes twin is
		// load-bearing: it is what makes an embedded value on an enum-ranged
		// property fail instead of being mapped as if it were a resource.
		if _, dup := schema.Classes[key]; dup {
			t.Errorf("enum %s must NOT also appear in classes", key)
		}
		if len(en.Members) == 0 {
			t.Errorf("enum %s declares no members", key)
		}
		for memberURI := range en.Members {
			if !memberURIRe.MatchString(memberURI) {
				t.Errorf("member key %s is not a versioned durable URI", memberURI)
			}
		}
	}

	// --- cases ---
	for _, c := range doc.Cases {
		var pkg PackageContext
		decodeNumberAware(t, c.Pkg, &pkg)

		if c.ExpectError {
			if _, err := CanonicalForm(c.Nodes, schema, pkg); err == nil {
				t.Errorf("[%s] expected canonicalization to reject, it did not", c.ID)
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
