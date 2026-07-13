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

	canonical "github.com/kanonak-protocol/runtime/kanonak-canonical/go"
)

// envelopeKeys are the $-envelope keys excluded from statement/field emission.
// $name (0.2.0) carries an embedded value's authored dict-key — hash-relevant.
// $types (0.4.0, runtime#10) carries a multi-typed node's FULL type set.
var envelopeKeys = map[string]bool{
	"$type": true, "$types": true, "$id": true, "$name": true, "$contentHash": true, "$version": true, "$extra": true,
}

// validatedTypes validates a node-or-embedded's $types envelope (0.4.0,
// runtime#10) and returns the validated set, or nil when the node is
// single-typed. Invariants: sorted by UTF-8 bytes (Go string comparison IS
// byte-wise lexicographic), at least two members, no duplicates, and $type
// (the dispatch key, chosen by the schema layer's primary rule) is a member.
// Enforced wherever the envelope is touched — Serialize, Deserialize, and
// canonicalization — so a producer fails at emit time and a reader never masks
// a nondeterministic emitter by silently repairing the set.
func validatedTypes(m map[string]interface{}, where string) ([]string, error) {
	raw, has := m["$types"]
	if !has || raw == nil {
		return nil, nil
	}
	list, ok := raw.([]interface{})
	if !ok {
		return nil, fmt.Errorf("codec: %s: $types must be a list of non-empty type URIs", where)
	}
	types := make([]string, 0, len(list))
	for _, item := range list {
		s, isStr := item.(string)
		if !isStr || s == "" {
			return nil, fmt.Errorf("codec: %s: $types must be a list of non-empty type URIs", where)
		}
		types = append(types, s)
	}
	if len(types) < 2 {
		return nil, fmt.Errorf(
			"codec: %s: $types with %d member(s) is forbidden — a single-typed node carries "+
				"only $type (a second encoding of the same content would be hash-ambiguous)",
			where, len(types))
	}
	for i := 1; i < len(types); i++ {
		if types[i-1] == types[i] {
			return nil, fmt.Errorf("codec: %s: $types carries duplicate member %s", where, types[i])
		}
		if types[i-1] > types[i] {
			return nil, fmt.Errorf(
				"codec: %s: $types is not sorted by UTF-8 bytes (%s sorts after %s) — "+
					"ordering is the producer's job, never the reader's",
				where, types[i-1], types[i])
		}
	}
	primary, _ := m["$type"].(string)
	for _, t := range types {
		if primary != "" && t == primary {
			return types, nil
		}
	}
	return nil, fmt.Errorf(
		"codec: %s: $type (%q) must be present and a member of $types", where, primary)
}

// assertTypesEnvelopes recursively validates every $types envelope in a wire
// value (the node itself and any embedded node at any depth). Shared by
// Serialize (the producer fails at emit time) and Deserialize (the strict
// reader rejects rather than repairs).
func assertTypesEnvelopes(value interface{}, where string) error {
	switch v := value.(type) {
	case []interface{}:
		for i, item := range v {
			if err := assertTypesEnvelopes(item, fmt.Sprintf("%s[%d]", where, i)); err != nil {
				return err
			}
		}
	case map[string]interface{}:
		if _, has := v["$types"]; has {
			if _, err := validatedTypes(v, where); err != nil {
				return err
			}
		}
		for key, item := range v {
			if key == "$types" {
				continue
			}
			if err := assertTypesEnvelopes(item, where+"."+key); err != nil {
				return err
			}
		}
	}
	return nil
}

// Prop describes a single modeled property of a class. Range (0.2.0) is the
// range class's URI — present for object props; maps an embedded value's fields
// when the embedded carries no explicit $type (inference only, never
// materialized as a statement).
type Prop struct {
	Predicate string `json:"predicate"`
	Kind      string `json:"kind"` // "datatype" | "object"
	Datatype  string `json:"datatype,omitempty"`
	Range     string `json:"range,omitempty"`
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
func value(prop Prop, raw interface{}, schema CodecSchema) (canonical.Value, error) {
	if prop.Kind == "object" {
		// A node: a reference ({"$ref"}) or an embedded resource.
		m, ok := raw.(map[string]interface{})
		if !ok {
			return nil, fmt.Errorf(
				"codec: object property %s expects a reference ({\"$ref\": ...}) or an "+
					"embedded node (a map), got %T", prop.Predicate, raw)
		}
		if ref, has := m["$ref"]; has {
			uri, isStr := ref.(string)
			if !isStr {
				return nil, fmt.Errorf("codec: object $ref must be a string, got %T", ref)
			}
			return canonical.Ref{URI: uri}, nil
		}
		return embeddedValue(prop, m, schema)
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

// embeddedValue canonicalizes an embedded value (0.2.0): a map with no $id, an
// optional $name (the authored dict-key — hash-relevant), an optional $type,
// and schema-mapped fields. An explicit $type emits a type statement inside the
// embedded (hash-relevant even when it equals the range-derived type); without
// it, fields map via the containing property's Range and NO type statement is
// emitted — range-derived typing is inference only.
func embeddedValue(prop Prop, m map[string]interface{}, schema CodecSchema) (canonical.Value, error) {
	if _, has := m["$id"]; has {
		return nil, fmt.Errorf(
			"codec: an embedded value under %s must not carry $id — to point at a "+
				"named resource, pass a reference ({\"$ref\": ...})", prop.Predicate)
	}
	types, err := validatedTypes(m, fmt.Sprintf("embedded value under %s", prop.Predicate))
	if err != nil {
		return nil, err
	}
	explicitType, _ := m["$type"].(string)
	clsURI := explicitType
	if clsURI == "" {
		clsURI = prop.Range
	}
	if clsURI == "" {
		return nil, fmt.Errorf(
			"codec: cannot map embedded value under %s: it carries no $type and the "+
				"property declares no range", prop.Predicate)
	}
	cls, ok := schema.Classes[clsURI]
	if !ok {
		return nil, fmt.Errorf("codec: no schema for embedded type %s", clsURI)
	}

	stmts, err := fieldStatements(m, cls, schema)
	if err != nil {
		return nil, err
	}
	if types != nil {
		// A multi-typed embedded ($types implies an explicit $type): one type
		// statement per member, in $types (UTF-8 sorted) order — all hash-relevant.
		for _, member := range types {
			stmts = append(stmts, canonical.Statement{
				Predicate: schema.TypePredicate,
				Value:     canonical.Ref{URI: member},
			})
		}
	} else if explicitType != "" {
		stmts = append(stmts, canonical.Statement{
			Predicate: schema.TypePredicate,
			Value:     canonical.Ref{URI: explicitType},
		})
	}
	var name *string
	if n, isStr := m["$name"].(string); isStr && n != "" {
		name = &n
	}
	return canonical.Embedded{Name: name, Statements: stmts}, nil
}

// fieldStatements builds the canonical statements for one node-or-embedded's
// modeled fields plus its $extra — everything except the type triple (subjects
// always carry one; embeddeds only when explicitly typed).
func fieldStatements(source map[string]interface{}, cls Class, schema CodecSchema) ([]canonical.Statement, error) {
	var out []canonical.Statement

	for key, raw := range source {
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
			// An empty list contributes NO statement — absent and empty are
			// identical at the canonical layer (the wire serialize still
			// preserves the empty list).
			if len(list) == 0 {
				continue
			}
			items := make([]canonical.Value, 0, len(list))
			for _, x := range list {
				v, err := value(prop, x, schema)
				if err != nil {
					return nil, err
				}
				items = append(items, v)
			}
			out = append(out, canonical.Statement{Predicate: prop.Predicate, Value: canonical.List{Items: items}})
		} else {
			v, err := value(prop, raw, schema)
			if err != nil {
				return nil, err
			}
			out = append(out, canonical.Statement{Predicate: prop.Predicate, Value: v})
		}
	}

	if extra, ok := source["$extra"].(map[string]interface{}); ok {
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

// statements builds the canonical statements for one subject node: the rdf:type
// triple(s), then its fields.
func statements(node map[string]interface{}, schema CodecSchema) ([]canonical.Statement, error) {
	id, _ := node["$id"].(string)
	if id == "" {
		id = "(no $id)"
	}
	types, err := validatedTypes(node, fmt.Sprintf("node %s", id))
	if err != nil {
		return nil, err
	}
	typeURI, _ := node["$type"].(string)
	if typeURI == "" {
		return nil, fmt.Errorf("codec: node is missing $type")
	}
	cls, ok := schema.Classes[typeURI]
	if !ok {
		return nil, fmt.Errorf("codec: no schema for type %s", typeURI)
	}

	// The rdf:type triple(s) every subject carries: one per $types member for a
	// multi-typed node (in $types' UTF-8 sorted order), else the single $type.
	members := types
	if members == nil {
		members = []string{typeURI}
	}
	out := make([]canonical.Statement, 0, len(members))
	for _, member := range members {
		out = append(out, canonical.Statement{
			Predicate: schema.TypePredicate, Value: canonical.Ref{URI: member},
		})
	}
	fields, err := fieldStatements(node, cls, schema)
	if err != nil {
		return nil, err
	}
	return append(out, fields...), nil
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
// Fallible since 0.4.0: an invalid $types envelope (at any depth) is a producer
// bug and fails at emit time.
func Serialize(node map[string]interface{}) (map[string]interface{}, error) {
	where, _ := node["$id"].(string)
	if where == "" {
		where, _ = node["$type"].(string)
	}
	if where == "" {
		where = "(node)"
	}
	if err := assertTypesEnvelopes(node, "serialize "+where); err != nil {
		return nil, err
	}
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
	return out, nil
}

// Deserialize parses normalized JSON into a typed node. $-envelope keys and
// fields modeled on the node's $type stay top-level; every other key is
// collected into $extra so a strongly-typed consumer round-trips it losslessly.
func Deserialize(jsonObj map[string]interface{}, schema CodecSchema) (map[string]interface{}, error) {
	typeURI, ok := jsonObj["$type"].(string)
	if !ok {
		return nil, fmt.Errorf("codec: cannot deserialize: missing string $type")
	}
	// Reader-side $types validation, at every depth: an unsorted / singleton /
	// duplicate / non-member set is REJECTED, never silently repaired —
	// determinism belongs to the producer, and a lenient reader would mask a
	// nondeterministic emitter.
	where, _ := jsonObj["$id"].(string)
	if where == "" {
		where = typeURI
	}
	if err := assertTypesEnvelopes(jsonObj, "deserialize "+where); err != nil {
		return nil, err
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
