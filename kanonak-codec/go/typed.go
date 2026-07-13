// The typed SDK-facing surface (0.3.0): the $-envelope as data, the explicit
// reference-or-embedded union, and the encoding/json bridge from a typed model
// to the node contract. A generated struct EMBEDS KanonakNode (struct embedding
// promotes the envelope fields through encoding/json), types its object
// properties as Ref[T], and binds through ToNode — native serde, no
// reflection, one contract with the dictionary path.

package codec

import (
	"bytes"
	"encoding/json"
	"fmt"
)

// KanonakNode is the $-envelope as data — the struct a generated typed model
// EMBEDS so an instance carries its own identity and serializes straight to
// the normalized-JSON wire form via encoding/json. Envelope keys are reserved
// (never ontology statements).
//
// LIMITATION (Go typed surface v1): encoding/json has no extension-data
// mechanism (no [JsonExtensionData] / #[serde(flatten)] map equivalent), so
// open-world extras outside the type-model are NOT carried by the typed
// surface — they remain available via the dictionary node path ($extra on the
// map contract, see Serialize/Deserialize).
type KanonakNode struct {
	// Id is the resource's canonical URI. Required to form a subject.
	Id string `json:"$id,omitempty"`

	// Type is the durable class URI — the value of the synthesized type triple.
	Type string `json:"$type,omitempty"`

	// Types is a multi-typed node's FULL type set (0.4.0, runtime#10) — present
	// only when the node carries more than one type statement. Sorted by UTF-8
	// bytes, at least two members, no duplicates, Type a member; each member
	// emits one type statement in canonical form. Exposed ONLY as the $types
	// envelope — deliberately no unprefixed accessor, because an ontology can
	// model a property literally named "types"; the $ prefix exists to avoid
	// exactly that collision.
	Types []string `json:"$types,omitempty"`

	// Name is an embedded value's authored dict-key — HASH-RELEVANT
	// (serialized into the canonical form). Only meaningful when the instance
	// is used as an embedded value (via EmbedNamed); empty for subjects.
	Name string `json:"$name,omitempty"`

	// PackageContentHash is package provenance on read; ignored if echoed
	// back on write.
	PackageContentHash string `json:"$contentHash,omitempty"`

	// PackageVersion is package provenance on read; ignored if echoed back
	// on write.
	PackageVersion string `json:"$version,omitempty"`
}

// KanonakNodeRef returns the envelope itself — because a generated struct
// embeds KanonakNode, method promotion makes every generated struct's pointer
// satisfy KanonakResource for free.
func (n *KanonakNode) KanonakNodeRef() *KanonakNode { return n }

// KanonakResource is implemented by generated typed structs (over their
// embedded KanonakNode) so the runtime can read/write an instance's envelope —
// what lets RefToResource resolve identity and EmbedNamed carry the authored
// dict-key.
type KanonakResource interface {
	KanonakNodeRef() *KanonakNode
}

// Ref is an object property's value: EXACTLY ONE of a reference to a named
// resource (its canonical URI) or an embedded node (the value itself, carried
// inline — derived identity, no $id). The typed twin of the wire form's
// {"$ref": uri} vs embedded-node distinction; the choice between the arms is
// authorial and hash-relevant, so it is explicit here, never inferred. An
// embedded value's fields map via the containing property's declared range
// when it carries no explicit $type — that range-derived typing is inference
// only, never materialized as a statement.
type Ref[T any] struct {
	uri   string
	value *T
}

// RefTo is a reference to a named resource by its canonical URI. An empty uri
// is reported at marshal time (constructors stay error-free).
func RefTo[T any](uri string) Ref[T] {
	return Ref[T]{uri: uri}
}

// RefToResource is a reference to a named resource by the instance itself —
// resolved through the target's envelope Id. The target must already carry
// its identity; an embedded (id-less) value cannot be referenced.
func RefToResource[T any](target KanonakResource) (Ref[T], error) {
	if target == nil || target.KanonakNodeRef().Id == "" {
		return Ref[T]{}, fmt.Errorf(
			"codec: RefToResource requires a resource with a non-empty envelope $id — " +
				"to carry the value inline instead, use Embed")
	}
	return Ref[T]{uri: target.KanonakNodeRef().Id}, nil
}

// Embed is an embedded value, carried inline (derived identity, no $id).
func Embed[T any](value T) Ref[T] {
	return Ref[T]{value: &value}
}

// EmbedNamed is an embedded value with its authored dict-key name —
// HASH-RELEVANT (rides $name). The PT constraint binds T's pointer type so a
// value-typed generated struct (whose pointer satisfies KanonakResource via
// method promotion) still infers cleanly: EmbedNamed(LineItem{...}, "first")
// yields a Ref[LineItem].
func EmbedNamed[T any, PT interface {
	*T
	KanonakResource
}](value T, name string) Ref[T] {
	PT(&value).KanonakNodeRef().Name = name
	return Ref[T]{value: &value}
}

// IsReference reports whether this is the reference arm.
func (r Ref[T]) IsReference() bool { return r.uri != "" }

// URI is the referenced resource's canonical URI — the reference arm (else "").
func (r Ref[T]) URI() string { return r.uri }

// Value is the embedded value — the embedded arm (else nil).
func (r Ref[T]) Value() *T { return r.value }

// MarshalJSON writes the reference arm as {"$ref": uri} and the embedded arm
// as the value's own wire form. A Ref with neither arm set (the zero value,
// or RefTo("")) fails loudly.
func (r Ref[T]) MarshalJSON() ([]byte, error) {
	if r.uri != "" {
		return json.Marshal(struct {
			Ref string `json:"$ref"`
		}{r.uri})
	}
	if r.value != nil {
		return json.Marshal(r.value)
	}
	return nil, fmt.Errorf(
		"codec: a Ref carries neither a reference URI nor an embedded value — " +
			"construct it via RefTo/RefToResource/Embed/EmbedNamed")
}

// UnmarshalJSON reads an object carrying "$ref" as the reference arm and
// anything else as the embedded arm (unmarshaled into T).
func (r *Ref[T]) UnmarshalJSON(data []byte) error {
	var probe map[string]interface{}
	if err := json.Unmarshal(data, &probe); err == nil {
		if raw, has := probe["$ref"]; has {
			uri, isStr := raw.(string)
			if !isStr {
				return fmt.Errorf("codec: object $ref must be a string, got %T", raw)
			}
			*r = Ref[T]{uri: uri}
			return nil
		}
	}
	var value T
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	if err := dec.Decode(&value); err != nil {
		return err
	}
	*r = Ref[T]{value: &value}
	return nil
}

// ToNode is a typed instance's codec node (the dictionary contract). The
// bridge is native serde: the instance serializes to its normalized-JSON wire
// form (envelope-as-data + Ref values), and the wire form maps onto the node
// contract through the SAME split Deserialize defines — so the typed path and
// the dictionary path are one contract, not two.
func ToNode(typed interface{}, schema CodecSchema) (map[string]interface{}, error) {
	wire, err := json.Marshal(typed)
	if err != nil {
		return nil, fmt.Errorf("codec: typed value failed to serialize: %v", err)
	}
	dec := json.NewDecoder(bytes.NewReader(wire))
	dec.UseNumber()
	var obj map[string]interface{}
	if err := dec.Decode(&obj); err != nil {
		return nil, fmt.Errorf(
			"codec: a typed instance must serialize to a JSON object (the wire node form): %v", err)
	}
	return Deserialize(obj, schema)
}
