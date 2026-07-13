package codec

// Typed-surface conformance: hand-written GENERATED-STYLE structs for the
// embedded-vectors probe schema, driven through KanonakNode / Ref[T] / ToNode,
// asserted against the SAME golden vectors the node contract is gated by.
// Also the executable spec for what the Go SDK generator must emit: structs
// EMBED KanonakNode, object properties are []Ref[T] (or Ref[T] for single),
// wire names ride json tags, optional scalars are pointers with omitempty.
// Slices carry omitzero, NOT omitempty: omitempty drops an EMPTY non-nil
// slice too, while omitzero (Go 1.24+) omits only the nil slice — so an
// authored empty list survives to the wire as [] (empty-list-emits-nothing).

import (
	"os"
	"reflect"
	"testing"
)

// -- Generated-style model for probe.example.com/schema@1.0.0 ----------------

type Order struct {
	KanonakNode
	Note     *string         `json:"note,omitempty"`
	Items    []Ref[LineItem] `json:"items,omitzero"`
	Customer []Ref[Customer] `json:"customer,omitzero"`
}

// OrderSingleCustomer is the same $type with a single-valued customer — the
// wire/hash contract is carried by $type + json tags, not the Go struct name;
// exercises the bare (non-list) embedded form.
type OrderSingleCustomer struct {
	KanonakNode
	Note     *string       `json:"note,omitempty"`
	Customer Ref[Customer] `json:"customer,omitzero"`
}

type LineItem struct {
	KanonakNode
	Sku *string `json:"sku,omitempty"`
	Qty *int64  `json:"qty,omitempty"`
}

type Customer struct {
	KanonakNode
	Name    *string        `json:"name,omitempty"`
	Address []Ref[Address] `json:"address,omitzero"`
}

type Address struct {
	KanonakNode
	City *string `json:"city,omitempty"`
}

type Person struct {
	KanonakNode
	Name *string `json:"name,omitempty"`
}

type Account struct {
	KanonakNode
	AccountCode *string      `json:"accountCode,omitempty"`
	Seats       *int64       `json:"seats,omitempty"`
	Rate        *float64     `json:"rate,omitempty"`
	Active      *bool        `json:"active,omitempty"`
	Owner       *Ref[Person] `json:"owner,omitempty"`
	Tags        []string     `json:"tags,omitzero"`
}

// -- Harness ------------------------------------------------------------------

const (
	typedSchemaNS = "probe.example.com/schema@1.0.0"
	typedDataNS   = "probe.example.com/data@1.0.0"

	embeddedVectorsPath = "../vectors/codec-vectors-embedded.json"
	basicVectorsPath    = "../vectors/codec-vectors.json"
)

func envelope(id, typeLocal string) KanonakNode {
	return KanonakNode{Id: typedDataNS + "/" + id, Type: typedSchemaNS + "/" + typeLocal}
}

func strPtr(s string) *string   { return &s }
func i64Ptr(i int64) *int64     { return &i }
func f64Ptr(f float64) *float64 { return &f }
func boolPtr(b bool) *bool      { return &b }

// loadVectorDoc reads one golden-vector file and its embedded schema.
func loadVectorDoc(t *testing.T, path string) (codecVectors, CodecSchema) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var doc codecVectors
	decodeNumberAware(t, data, &doc)
	var schema CodecSchema
	decodeNumberAware(t, doc.Schema, &schema)
	return doc, schema
}

// runTypedCase asserts that typed fixtures, bound through ToNode, reproduce
// the golden expected canonical form AND content hash of the named vector
// case — the typed path and the node path are one contract.
func runTypedCase(t *testing.T, path, caseID string, typed []interface{}) {
	t.Helper()
	doc, schema := loadVectorDoc(t, path)

	for _, c := range doc.Cases {
		if c.ID != caseID {
			continue
		}
		var pkg PackageContext
		decodeNumberAware(t, c.Pkg, &pkg)

		nodes := make([]map[string]interface{}, 0, len(typed))
		for i, tv := range typed {
			node, err := ToNode(tv, schema)
			if err != nil {
				t.Fatalf("[%s] ToNode[%d]: %v", caseID, i, err)
			}
			nodes = append(nodes, node)
		}

		form, err := CanonicalForm(nodes, schema, pkg)
		if err != nil {
			t.Fatalf("[%s] canonical form error: %v", caseID, err)
		}
		if form != c.ExpectedCanonicalForm {
			t.Errorf("[%s] form mismatch\n  expected: %s\n  actual:   %s", caseID, c.ExpectedCanonicalForm, form)
		}

		hash, err := ContentHash(nodes, schema, pkg)
		if err != nil {
			t.Fatalf("[%s] content hash error: %v", caseID, err)
		}
		if hash != c.ExpectedHash {
			t.Errorf("[%s] hash mismatch expected %s got %s", caseID, c.ExpectedHash, hash)
		}
		return
	}
	t.Fatalf("[%s] vector case not found in %s", caseID, path)
}

// -- Cases ----------------------------------------------------------------

func TestTypedEmbeddedVectors(t *testing.T) {
	runTypedCase(t, embeddedVectorsPath, "embedded-named-in-list", []interface{}{
		Order{
			KanonakNode: envelope("o1", "Order"),
			Note:        strPtr("A"),
			Items: []Ref[LineItem]{
				EmbedNamed(LineItem{Sku: strPtr("X"), Qty: i64Ptr(1)}, "first"),
			},
		},
	})

	runTypedCase(t, embeddedVectorsPath, "embedded-unnamed-positional", []interface{}{
		Order{
			KanonakNode: envelope("o1", "Order"),
			Note:        strPtr("A"),
			Items: []Ref[LineItem]{
				Embed(LineItem{Sku: strPtr("X"), Qty: i64Ptr(1)}),
			},
		},
	})

	// An explicit $type on the embedded emits a type statement inside it —
	// hash-relevant even when it equals the range-derived type.
	runTypedCase(t, embeddedVectorsPath, "embedded-explicit-type", []interface{}{
		Order{
			KanonakNode: envelope("o1", "Order"),
			Note:        strPtr("A"),
			Items: []Ref[LineItem]{
				EmbedNamed(LineItem{
					KanonakNode: KanonakNode{Type: typedSchemaNS + "/LineItem"},
					Sku:         strPtr("X"),
					Qty:         i64Ptr(1),
				}, "first"),
			},
		},
	})

	runTypedCase(t, embeddedVectorsPath, "embedded-list-order", []interface{}{
		Order{
			KanonakNode: envelope("o1", "Order"),
			Items: []Ref[LineItem]{
				EmbedNamed(LineItem{Sku: strPtr("X"), Qty: i64Ptr(1)}, "a"),
				EmbedNamed(LineItem{Sku: strPtr("Y"), Qty: i64Ptr(2)}, "b"),
			},
		},
	})

	runTypedCase(t, embeddedVectorsPath, "embedded-nested", []interface{}{
		Order{
			KanonakNode: envelope("o1", "Order"),
			Customer: []Ref[Customer]{
				EmbedNamed(Customer{
					Name: strPtr("Ada"),
					Address: []Ref[Address]{
						EmbedNamed(Address{City: strPtr("Austin")}, "home"),
					},
				}, "cust"),
			},
		},
	})

	runTypedCase(t, embeddedVectorsPath, "single-embedded-bare", []interface{}{
		OrderSingleCustomer{
			KanonakNode: envelope("o1", "Order"),
			Customer:    EmbedNamed(Customer{Name: strPtr("Ada")}, "cust"),
		},
	})

	// A non-nil EMPTY list — omitzero keeps the [] on the wire; the canonical
	// layer then emits no statement for it.
	runTypedCase(t, embeddedVectorsPath, "empty-list-emits-nothing", []interface{}{
		Order{
			KanonakNode: envelope("o1", "Order"),
			Note:        strPtr("A"),
			Items:       []Ref[LineItem]{},
		},
	})
}

// TestTypedEmptyListSurvivesWire pins the slice-tag behavior the
// empty-list-emits-nothing case rides on: a non-nil empty []Ref marshals as
// [] (omitzero omits only the nil slice), so the authored empty list reaches
// the node contract instead of silently reading as absent.
func TestTypedEmptyListSurvivesWire(t *testing.T) {
	_, schema := loadVectorDoc(t, embeddedVectorsPath)
	node, err := ToNode(Order{
		KanonakNode: envelope("o1", "Order"),
		Items:       []Ref[LineItem]{},
	}, schema)
	if err != nil {
		t.Fatalf("ToNode: %v", err)
	}
	items, has := node["items"]
	if !has {
		t.Fatal("empty items list was dropped from the wire form — expected []")
	}
	if !reflect.DeepEqual(items, []interface{}{}) {
		t.Fatalf("items expected empty list, got %#v", items)
	}
}

func TestTypedBasicVectors(t *testing.T) {
	account := func(owner Ref[Person]) Account {
		return Account{
			KanonakNode: envelope("a1", "Account"),
			AccountCode: strPtr("paul"),
			Seats:       i64Ptr(5),
			Rate:        f64Ptr(1.5),
			Active:      boolPtr(true),
			Owner:       &owner,
			Tags:        []string{"x", "y"},
		}
	}
	alice := Person{KanonakNode: envelope("p1", "Person"), Name: strPtr("Alice")}

	// Reference by canonical URI.
	runTypedCase(t, basicVectorsPath, "basic-scalars-ref-list", []interface{}{
		alice,
		account(RefTo[Person](typedDataNS + "/p1")),
	})

	// Reference by instance: resolved through the target's envelope Id.
	owner, err := RefToResource[Person](&alice)
	if err != nil {
		t.Fatalf("RefToResource: %v", err)
	}
	runTypedCase(t, basicVectorsPath, "basic-scalars-ref-list", []interface{}{
		alice,
		account(owner),
	})
}

// -- Typed-surface $types cases (0.4.0, runtime#10) ---------------------------
//
// The multi-typed set rides the generated model as the $types envelope only
// (deliberately no unprefixed accessor — an ontology can model a property
// literally named "types") and reproduces the same golden vectors.

type DefResource struct {
	KanonakNode
	Note *string `json:"note,omitempty"`
}

type Bundle struct {
	KanonakNode
	Parts []Ref[PartDef] `json:"parts,omitzero"`
}

// BundleSinglePart is the same $type with a single-valued parts — exercises
// the bare (non-list) embedded form.
type BundleSinglePart struct {
	KanonakNode
	Parts Ref[PartDef] `json:"parts,omitzero"`
}

type PartDef struct {
	KanonakNode
	Size *int64 `json:"size,omitempty"`
}

const typesVectorsPath = "../vectors/codec-vectors-types.json"

func TestTypedTypesVectors(t *testing.T) {
	runTypedCase(t, typesVectorsPath, "covered-redundant-set", []interface{}{
		DefResource{
			KanonakNode: KanonakNode{
				Id:   typedDataNS + "/w1",
				Type: typedSchemaNS + "/ClassDef",
				Types: []string{
					typedSchemaNS + "/AnnotatedDef",
					typedSchemaNS + "/ClassDef",
				},
			},
			Note: strPtr("A"),
		},
	})

	runTypedCase(t, typesVectorsPath, "embedded-multi-typed-named", []interface{}{
		BundleSinglePart{
			KanonakNode: envelope("b1", "Bundle"),
			Parts: EmbedNamed(PartDef{
				KanonakNode: KanonakNode{
					Type: typedSchemaNS + "/PartDef",
					Types: []string{
						typedSchemaNS + "/PartDef",
						typedSchemaNS + "/SealedDef",
					},
				},
				Size: i64Ptr(2),
			}, "first"),
		},
	})

	runTypedCase(t, typesVectorsPath, "types-in-list-items", []interface{}{
		Bundle{
			KanonakNode: envelope("b1", "Bundle"),
			Parts: []Ref[PartDef]{
				EmbedNamed(PartDef{
					KanonakNode: KanonakNode{
						Type: typedSchemaNS + "/PartDef",
						Types: []string{
							typedSchemaNS + "/PartDef",
							typedSchemaNS + "/SealedDef",
						},
					},
					Size: i64Ptr(1),
				}, "a"),
				EmbedNamed(PartDef{
					KanonakNode: KanonakNode{Type: typedSchemaNS + "/PartDef"},
					Size:        i64Ptr(2),
				}, "b"),
			},
		},
	})
}
