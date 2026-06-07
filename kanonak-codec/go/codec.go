// Package codec is the generic, ontology-independent Kanonak codec runtime
// (Go port). Given a CodecSchema (the per-package metadata a generated SDK
// embeds) and a set of typed nodes, it builds the canonical input model and
// content-addresses it via kanonak.org/canonical — the same content-form the
// TypeScript/Python references and the `kanonak hash` CLI produce. It also
// (de)serializes the normalized-JSON wire form.
//
// A node is a plain map[string]interface{} (the $-envelope plus alias-collapsed
// local-name fields). Decode it with a json.Decoder configured via UseNumber()
// so numeric lexicals survive as json.Number (e.g. "5", "1.5") rather than
// being widened to float64.
package codec

import (
	"fmt"

	"kanonak.org/canonical"
)

// envelopeKeys are the $-envelope keys excluded from statement/field emission.
var envelopeKeys = map[string]bool{
	"$type": true, "$id": true, "$contentHash": true, "$version": true, "$extra": true,
}

// Prop describes a single modeled property of a class.
type Prop struct {
	Predicate string `json:"predicate"`
	Kind      string `json:"kind"` // "datatype" | "object"
	Datatype  string `json:"datatype,omitempty"`
}

// Class describes a modeled type: its URI and its properties keyed by local name.
type Class struct {
	TypeURI string          `json:"typeUri"`
	Props   map[string]Prop `json:"props"`
}

// CodecSchema is the per-package metadata a generated SDK embeds.
type CodecSchema struct {
	TypePredicate  string           `json:"typePredicate"`
	LabelPredicate string           `json:"labelPredicate"`
	PackageTypeURI string           `json:"packageTypeUri"`
	Classes        map[string]Class `json:"classes"`
}

// PackageContext identifies the package being built/hashed.
type PackageContext struct {
	Publisher   string  `json:"publisher"`
	PackageName string  `json:"packageName"`
	Version     string  `json:"version"`
	Label       *string `json:"label,omitempty"`
}

// lexical returns the raw lexical token of a scalar — the input the canonical
// form normalizes. bool -> "true"/"false", string -> as-is, json.Number ->
// its original token. Anything else fails loudly.
func lexical(value interface{}) (string, error) {
	switch v := value.(type) {
	case bool:
		if v {
			return "true", nil
		}
		return "false", nil
	case string:
		return v, nil
	case interface{ String() string }: // json.Number
		return v.String(), nil
	default:
		return "", fmt.Errorf("codec: unsupported scalar lexical %T (%v)", value, value)
	}
}

// value builds a canonical Value for a single (non-list) raw field value.
func value(prop Prop, raw interface{}) (canonical.Value, error) {
	if prop.Kind == "object" {
		m, ok := raw.(map[string]interface{})
		if ok {
			if ref, has := m["$ref"]; has {
				uri, isStr := ref.(string)
				if !isStr {
					return nil, fmt.Errorf("codec: object $ref must be a string, got %T", ref)
				}
				return canonical.Ref{URI: uri}, nil
			}
		}
		return nil, fmt.Errorf(
			"codec: embedded object values are not yet supported by the codec runtime; " +
				"pass a reference ({\"$ref\": ...})")
	}
	lex, err := lexical(raw)
	if err != nil {
		return nil, err
	}
	carrier, ok := canonical.CarrierOf(prop.Datatype)
	if !ok {
		return canonical.Raw{Token: lex}, nil
	}
	return canonical.Typed{Carrier: carrier, Lexical: lex}, nil
}

// statements builds the canonical statements for one node.
func statements(node map[string]interface{}, schema CodecSchema) ([]canonical.Statement, error) {
	typeURI, _ := node["$type"].(string)
	if typeURI == "" {
		return nil, fmt.Errorf("codec: node is missing $type")
	}
	cls, ok := schema.Classes[typeURI]
	if !ok {
		return nil, fmt.Errorf("codec: no schema for type %s", typeURI)
	}

	out := []canonical.Statement{
		{Predicate: schema.TypePredicate, Value: canonical.Ref{URI: typeURI}},
	}

	for key, raw := range node {
		if envelopeKeys[key] || raw == nil {
			continue
		}
		prop, has := cls.Props[key]
		if !has {
			lex, err := lexical(raw)
			if err != nil {
				return nil, err
			}
			out = append(out, canonical.Statement{Predicate: key, Value: canonical.Raw{Token: lex}})
			continue
		}
		if list, isList := raw.([]interface{}); isList {
			items := make([]canonical.Value, 0, len(list))
			for _, x := range list {
				v, err := value(prop, x)
				if err != nil {
					return nil, err
				}
				items = append(items, v)
			}
			out = append(out, canonical.Statement{Predicate: prop.Predicate, Value: canonical.List{Items: items}})
		} else {
			v, err := value(prop, raw)
			if err != nil {
				return nil, err
			}
			out = append(out, canonical.Statement{Predicate: prop.Predicate, Value: v})
		}
	}

	if extra, ok := node["$extra"].(map[string]interface{}); ok {
		for predicate, raw := range extra {
			if raw == nil {
				continue
			}
			lex, err := lexical(raw)
			if err != nil {
				return nil, err
			}
			out = append(out, canonical.Statement{Predicate: predicate, Value: canonical.Raw{Token: lex}})
		}
	}
	return out, nil
}

// BuildPackage builds the canonical input model: a subject per node plus the
// synthesized package-wrapper subject (raw label + Package type), exactly the
// subject set `kanonak hash` produces for the equivalent authored package.
func BuildPackage(nodes []map[string]interface{}, schema CodecSchema, pkg PackageContext) (canonical.Package, error) {
	subjects := make([]canonical.Subject, 0, len(nodes)+1)
	for _, node := range nodes {
		id, _ := node["$id"].(string)
		if id == "" {
			return canonical.Package{}, fmt.Errorf("codec: node is missing $id")
		}
		stmts, err := statements(node, schema)
		if err != nil {
			return canonical.Package{}, err
		}
		subjects = append(subjects, canonical.Subject{URI: id, Statements: stmts})
	}

	pkgURI := fmt.Sprintf("%s/%s@%s/%s", pkg.Publisher, pkg.PackageName, pkg.Version, pkg.PackageName)
	var pkgStmts []canonical.Statement
	if pkg.Label != nil {
		pkgStmts = append(pkgStmts, canonical.Statement{
			Predicate: schema.LabelPredicate,
			Value:     canonical.Raw{Token: *pkg.Label},
		})
	}
	pkgStmts = append(pkgStmts, canonical.Statement{
		Predicate: schema.TypePredicate,
		Value:     canonical.Ref{URI: schema.PackageTypeURI},
	})
	subjects = append(subjects, canonical.Subject{URI: pkgURI, Statements: pkgStmts})

	return canonical.Package{Subjects: subjects}, nil
}

// CanonicalForm returns the canonical form (the {subjects:[...]} JSON) of a
// package built from nodes.
func CanonicalForm(nodes []map[string]interface{}, schema CodecSchema, pkg PackageContext) (string, error) {
	p, err := BuildPackage(nodes, schema, pkg)
	if err != nil {
		return "", err
	}
	return canonical.CanonicalForm(p)
}

// ContentHash returns the sha256: content hash of a package built from nodes —
// matches `kanonak hash`.
func ContentHash(nodes []map[string]interface{}, schema CodecSchema, pkg PackageContext) (string, error) {
	p, err := BuildPackage(nodes, schema, pkg)
	if err != nil {
		return "", err
	}
	return canonical.CanonicalHash(p)
}

// Serialize renders a typed node to its normalized-JSON wire form. $extra
// entries ride as sibling fields after the modeled ones; a modeled field wins a
// name collision ([JsonExtensionData] semantics). nil values are dropped.
func Serialize(node map[string]interface{}) map[string]interface{} {
	out := make(map[string]interface{})
	for key, val := range node {
		if key == "$extra" || val == nil {
			continue
		}
		out[key] = val
	}
	if extra, ok := node["$extra"].(map[string]interface{}); ok {
		for key, val := range extra {
			if val == nil {
				continue
			}
			if _, exists := out[key]; !exists {
				out[key] = val
			}
		}
	}
	return out
}

// Deserialize parses normalized JSON into a typed node. $-envelope keys and
// fields modeled on the node's $type stay top-level; every other key is
// collected into $extra so a strongly-typed consumer round-trips it losslessly.
func Deserialize(jsonObj map[string]interface{}, schema CodecSchema) (map[string]interface{}, error) {
	typeURI, ok := jsonObj["$type"].(string)
	if !ok {
		return nil, fmt.Errorf("codec: cannot deserialize: missing string $type")
	}
	cls, ok := schema.Classes[typeURI]
	if !ok {
		return nil, fmt.Errorf("codec: cannot deserialize: no schema for type %s", typeURI)
	}

	node := map[string]interface{}{"$type": typeURI}
	extra := make(map[string]interface{})
	for key, val := range jsonObj {
		if key == "$type" {
			continue
		}
		_, modeled := cls.Props[key]
		if len(key) > 0 && key[0] == '$' || modeled {
			node[key] = val
		} else {
			extra[key] = val
		}
	}
	if len(extra) > 0 {
		node["$extra"] = extra
	}
	return node, nil
}
