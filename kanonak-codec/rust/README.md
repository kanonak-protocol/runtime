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
use kanonak_codec::{build_package, canonical_form, content_hash, serialize, deserialize, Node};

// nodes: &[Node], schema/pkg: &serde_json::Value
let hash = content_hash(&nodes, &schema, &pkg)?;   // "sha256:..."
let form = canonical_form(&nodes, &schema, &pkg)?; // {"subjects":[...]}
let wire = serialize(&node);                        // normalized-JSON Node
let node = deserialize(&wire, &schema)?;            // typed Node (unmodeled -> $extra)
```

- `build_package` — the canonical input model (subject per node + synthesized
  package-wrapper subject). The seam every other entrypoint shares.
- `content_hash` / `canonical_form` — delegate to `kanonak-canonical`.
- `serialize` — modeled fields (nulls dropped) then `$extra` spread as siblings
  (modeled wins a collision; no `$extra` key on the wire).
- `deserialize` — `$`-keys and `$type`-modeled fields stay top-level; everything
  else collects into `$extra` for lossless round-trip.

Embedded object values are not yet supported — pass a reference (`{"$ref": ...}`).
Malformed input fails loudly (`CodecError`); no fallbacks.

## Test

```sh
cargo test
```

Runs the shared golden vectors in `../vectors/codec-vectors.json`.
