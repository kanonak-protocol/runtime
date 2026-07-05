# @kanonak-protocol/codec

The generic, ontology-independent **codec runtime** referenced by Kanonak's
generated typed SDKs. It turns typed objects into Kanonak content addresses and
the normalized-JSON wire form, given the per-package `CodecSchema` that a
generated SDK embeds.

It is the TypeScript port of the codec runtime. The C#/Rust/Go/Java/Python ports
reference their `kanonak-canonical` library for the same canonical-hash entry;
TypeScript references **`@kanonak-protocol/canonical`** — its only dependency.

## What it does

- **`packageContentHash(nodes, schema, pkg)` → `sha256:…`** — the permanent
  content address of the package those nodes form. Builds the language-neutral
  canonical input model (`kanonak.org/canonical-form`) and hashes it via
  `@kanonak-protocol/canonical`. Byte-identical to `kanonak hash` of the
  equivalent authored `.kan.yml`, including the synthesized `rdf:type` triples
  and the package-wrapper subject.
- **`packageCanonicalForm(nodes, schema, pkg)`** — the canonical form itself
  (the `{subjects:[…]}` JSON), for inspection/debugging.
- **`serialize(node)` / `deserialize(json, schema)`** — the normalized-JSON wire
  form. Open-world assertions outside the type-model round-trip losslessly
  through `$extra` (top-level wire fields, collected under `$extra` on the typed
  node — `[JsonExtensionData]` semantics).

## Self-contained by design

Carriers come from the schema's datatype URIs, and the resolved foundation URIs
(`type`/`label` predicates, the `Package` class) are embedded by the generator —
so hashing needs **no runtime ontology resolution** and no registry access. A
generated SDK plus this library (and its one `@kanonak-protocol/canonical`
dependency) is enough to content-address typed objects in a serverless/edge
runtime.

## Shape

```ts
import { packageContentHash, serialize, deserialize } from '@kanonak-protocol/codec';
import type { CodecSchema, CodecNode, PackageContext } from '@kanonak-protocol/codec';

// `Schema` is the constant a generated SDK embeds for its own classes.
const hash = packageContentHash(nodes, Schema, {
  publisher: 'example.com', packageName: 'my-data', version: '1.0.0',
});
```

`canonicalFormVersion: "1"` — the content-address rules are frozen and evolve
only by minting a new version (via the underlying canonical-form spec).

## Embedded values (0.2.0)

An object property's value is a **reference** (`{ $ref: uri }`) or an
**embedded node**: a map with no `$id`, an optional `$name` (the authored
dict-key — hash-relevant), an optional `$type`, and schema-mapped fields.
An explicit `$type` emits a type statement inside the embedded (hash-relevant
even when it equals the range-derived type); without one, fields are mapped via
the containing property's `range` and no type statement is emitted. Embedded
collections are ordered lists — list order is semantic and hashed. An empty
list contributes no statement (absent and empty are identical at the canonical
layer). Gated by `vectors/codec-vectors-embedded.json`.

Source & issues: https://github.com/kanonak-protocol/runtime
