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
    build_package, canonical_form, content_hash, deserialize, enum_member, serialize,
    type_matches, Node,
};

// nodes: &[Node], schema/pkg: &serde_json::Value
let hash = content_hash(&nodes, &schema, &pkg)?;   // "sha256:..."
let form = canonical_form(&nodes, &schema, &pkg)?; // {"subjects":[...]}
let wire = serialize(&node);                        // normalized-JSON Node
let node = deserialize(&wire, &schema)?;            // typed Node (unmodeled -> $extra)
let is_app = type_matches(&node, "example.com/vocab@5.1.0/Application", &schema)?; // bool
let member = enum_member(&schema, "example.com/vocab@5.0.0/north");                // Option<EnumMemberMatch>
```

- `build_package` — the canonical input model (subject per node + synthesized
  package-wrapper subject). The seam every other entrypoint shares.
- `content_hash` / `canonical_form` — delegate to `kanonak-canonical`.
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

**Reference values (runtime#6).** A `{"$ref": uri}` must address a named
resource (`publisher/package[@version]/name`); a `$ref` containing a URI
fragment (`#`) is rejected at any depth by canonicalization, `serialize` and
`deserialize` alike, with an error ending in `[fragment-reference]`.

Embedded object values are not yet supported — pass a reference (`{"$ref": ...}`).
Malformed input fails loudly (`CodecError`); no fallbacks.

## Test

```sh
cargo test
```

Runs the shared golden vectors in `../vectors/` (`codec-vectors*.json`,
including the compatibility file `codec-vectors-compat.json`).
