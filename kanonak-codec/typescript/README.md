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
- **`serializeExact(node, schema)` / `deserializeExact(json, schema)`** (2.2.0) —
  the normalized-JSON wire form, with each integer and decimal value in its JSON
  form (kanonak-protocol 4.11.0's `numbers-in-json`, runtime#54), through lists
  and embedded values. `deserializeExact` gives a wide integer (`integer`, its
  four sign-restricted forms, `long`, `unsignedLong`) as a `bigint`, a `decimal`
  as a string of its canonical digits, and `int` and narrower as a `number`;
  `serializeExact` writes a wide integer and a decimal as a string of their
  canonical digits, and a narrow integer as a number. `deserializeExact` judges
  the values it's given: `JSON.parse` has already read a number such as
  `1.0000000000000000001` as `1`, so a caller that must refuse it judges the
  number's source, as a reviver's `context.source` gives it (Node 22). A JSON number past ±(2^53 − 1) for a wide integer, a number for a decimal, a string for a narrow integer, or a value outside its datatype's range is refused, ending `[json-number-form]`, as `codec-vectors-json-numbers.json` pins in every port.
- **`serialize(node)` / `deserialize(json, schema)`**, deprecated for the two above — the normalized-JSON wire
  form. Open-world assertions outside the type-model round-trip losslessly
  through `$extra` (top-level wire fields, collected under `$extra` on the typed
  node — `[JsonExtensionData]` semantics).

Hashing also refuses a statement the schema doesn't declare, an `$extra` entry or a key the class doesn't model, with `[hash-undeclared-statement]`: a codec holds one package's schema and can't type another package's predicate, so such a node is hashed with the SDK, over the graph. Serializing and deserializing still carry it (codec 2.0.0, runtime#37).

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

A reference value addresses a **named resource**: `publisher/package[@version]/name`.
A `$ref` containing a URI fragment (`#…`) is rejected by canonicalization,
`serialize` and `deserialize` alike, at any depth, with an error ending
`[fragment-reference]`. Fragments address embedded resources for navigation;
they are not part of the reference graph. Gated by
`vectors/codec-vectors-references.json`.

## Reading earlier compatible versions (0.6.1)

A node is typed with the version of the class its producer's import resolved
to. A codec generated from a later **compatible** version of the same package
still reads it: `deserialize` takes the exact versioned class first, then the
same class at the highest schema version that `isReadableBy` (from
`@kanonak-protocol/canonical`) says can read the written one. The node keeps
the `$type` it was written with.

- **`typeMatches(node, classUri, schema)`** — what a generated type guard asks.
  Each of the node's types (`$types`, else `$type`) goes through the same
  compatible lookup and matches when the class is `classUri` or has it among
  its `ancestors`, the optional transitive superclass list a `CodecClass` may
  carry. Classes compare by versionless identity.
- **`enumMember(schema, ref)`** — the enumeration member a `{"$ref"}` names,
  exact first, then the same member at a compatible later version.
- **Hashing stays exact-version.** A content hash covers the producer's
  predicate and type URIs, versions included, so a schema at a later version
  cannot reproduce it. Hashing a node whose exact class is missing fails with
  `[hash-needs-exact-version]` when only a compatible class exists.

Errors end in a bracketed kind: `[unknown-type]`, `[newer-version]` (the data
may use terms this schema lacks), `[other-major]`, `[other-minor-line]` (below
1.0.0 the minor is the incompatible line) and `[hash-needs-exact-version]`.
Gated by `vectors/codec-vectors-compat.json`.

Source & issues: https://github.com/kanonak-protocol/runtime
