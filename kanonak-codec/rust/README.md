# kanonak-codec (Rust)

The generic, ontology-independent Kanonak codec runtime — the Rust port of
`@kanonak-protocol/codec`. Given a `CodecSchema` (the per-package metadata a generated
typed SDK embeds) and a set of typed nodes, it builds the canonical input model
and content-addresses it via [`kanonak-canonical`](../../kanonak-canonical/rust),
producing the exact content hash the `kanonak hash` CLI emits. It also
(de)serializes the normalized-JSON wire form.

A **node** is a plain JSON object (`serde_json::Map<String, serde_json::Value>`):
the `$`-envelope (`$type`, `$id`, optional `$extra`, …) plus alias-collapsed
local-name fields. A generated typed model serializes to one.

## API

```rust
use kanonak_codec::{
    build_package, canonical_form, content_hash, deserialize_exact, enum_member, serialize_exact,
    type_matches, Node,
};

// nodes: &[Node], schema/pkg: &serde_json::Value
let hash = content_hash(&nodes, &schema, &pkg)?;   // "sha256:..."
let form = canonical_form(&nodes, &schema, &pkg)?; // {"subjects":[...]}
let wire = serialize_exact(&node, &schema)?;        // normalized-JSON Node, numbers in their JSON forms
let node = deserialize_exact(&wire, &schema)?;      // typed Node (unmodeled -> $extra)
let is_app = type_matches(&node, "example.com/vocab@5.1.0/Application", &schema)?; // bool
let member = enum_member(&schema, "example.com/vocab@5.0.0/north");                // Option<EnumMemberMatch>
```

- `build_package` — the canonical input model (subject per node + synthesized
  package-wrapper subject). The seam every other entrypoint shares.
- `content_hash` / `canonical_form` — delegate to `kanonak-canonical`.
- `serialize_exact` / `deserialize_exact` (2.2.0) — `serialize` and `deserialize`, with each integer
  and decimal value in its JSON form (kanonak-protocol 4.11.0's `numbers-in-json`, runtime#54), through
  lists and embedded values. In the node, `integer`, its four sign-restricted forms and `decimal` are a
  string of their canonical digits, since a `serde_json::Value` holds no integer past 64 bits; `long`,
  `unsignedLong` and `int` and narrower are numbers. `deserialize_exact` judges the `Value`s it's given:
  without serde_json's `arbitrary_precision`, a number such as `1.0000000000000000001` is already the `f64`
  `1.0`, so a caller that must refuse it parses with that feature. A JSON number past ±(2^53 − 1) for a wide integer, a number for a decimal, a string for a narrow integer, or a value outside its datatype's range is refused, ending `[json-number-form]`, as `codec-vectors-json-numbers.json` pins in every port. The two below are deprecated for them.
- The serde binding (2.2.0), for a generated struct's fields, so serde_json reads and writes them as
  `serialize_exact` and `deserialize_exact` do:
  - `Integer<D>`, an `integer` or one of its sign-restricted forms held as its canonical digits, where `D` is
    `XsdInteger`, `NonNegativeInteger`, `PositiveInteger`, `NonPositiveInteger` or `NegativeInteger`, whose range
    it checks;
  - `Long` and `UnsignedLong`, over `i64` and `u64`, written as strings;
  - `Decimal`, held as its canonical digits and read from a string only.

  `int` and narrower take `i32` and the rest, which serde_json already refuses a string, a fraction or overflow
  for. Each refusal of these types ends `[json-number-form]`.
- `serialize` — modeled fields (nulls dropped) then `$extra` spread as siblings
  (modeled wins a collision; no `$extra` key on the wire).
- `deserialize` — `$`-keys and `$type`-modeled fields stay top-level; everything
  else collects into `$extra` for lossless round-trip.
- `type_matches` — whether a node (any member of `$types`, else its `$type`) is
  an instance of a class: the class itself, or a subclass whose schema entry
  lists it in the optional `ancestors` field (every superclass, transitively,
  by versioned URI; canonicalization-inert). Classes compare by versionless
  identity; a type the schema cannot read never matches. A typed instance asks
  the same through `KanonakNode::type_matches` on its envelope.
- `enum_member` — the enumeration member a `{"$ref": ...}` names, as an
  `EnumMemberMatch { enum_type, uri, member }`, or `None` ("not this schema's").

**Reading earlier compatible versions (runtime#28).** A node typed at an
earlier compatible version of the schema's own package still reads: the exact
versioned class first, else the same class at the highest version for which
`kanonak_canonical::is_readable_by(written, schema_version)` holds.
`deserialize`, `type_matches` and `enum_member` share that one lookup, and the
decoded node keeps the `$type` it was written with. Rejections end in a
bracketed kind — `[unknown-type]`, `[newer-version]`, `[other-major]`,
`[other-minor-line]`. Hashing stays EXACT-VERSION only: a content hash is
computed over the producer's versioned URIs, so a node (or embedded value)
whose class the schema has only at a compatible later version fails with
`[hash-needs-exact-version]` rather than producing a different hash.
Hashing also refuses a statement the schema doesn't declare, an `$extra` entry or a key the class doesn't model, with `[hash-undeclared-statement]`: a codec holds one package's schema and can't type another package's predicate, so such a node is hashed with the SDK, over the graph. Serializing and deserializing still carry it (codec 2.0.0, runtime#37).

**Reference values (runtime#6).** A `{"$ref": uri}` must address a named
resource (`publisher/package[@version]/name`); a `$ref` containing a URI
fragment (`#`) is rejected at any depth by canonicalization, `serialize` and
`deserialize` alike, with an error ending in `[fragment-reference]`.

An object property carries a reference (`{"$ref": ...}`) or an embedded node,
gated by `codec-vectors-embedded.json`. Malformed input fails loudly
(`CodecError`); no fallbacks.

## Test

```sh
cargo test
```

Runs the shared golden vectors in `../vectors/` (`codec-vectors*.json`,
including the compatibility file `codec-vectors-compat.json`).
