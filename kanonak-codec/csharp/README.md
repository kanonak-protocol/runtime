# Kanonak.Codec (C#)

The generic, ontology-independent **codec runtime** referenced by Kanonak's
generated typed SDKs — the C# port of `@kanonak-protocol/codec`. It turns typed nodes
into Kanonak content addresses and the normalized-JSON wire form, given the
per-package `CodecSchema` that a generated SDK embeds.

It depends on the sibling **`Kanonak.Canonical`** port (a local `ProjectReference`)
for the same canonical form + content hash the `kanonak hash` CLI produces.

## What it does

- **`Codec.ContentHash(nodes, schema, pkg)` → `sha256:…`** — the permanent
  content address of the package those nodes form. Builds the language-neutral
  canonical input model and hashes it via `Kanonak.Canonical`. Byte-identical to
  `kanonak hash` of the equivalent authored `.kan.yml`, including the synthesized
  `rdf:type` triples and the package-wrapper subject.
- **`Codec.CanonicalForm(nodes, schema, pkg)`** — the canonical form itself
  (the `{subjects:[…]}` JSON), for inspection/debugging.
- **`Codec.SerializeExact(node, schema)` / `Codec.DeserializeExact(json, schema)`**
  (2.2.0) — the normalized-JSON wire form, with each integer and decimal value in
  its JSON form (kanonak-protocol 4.11.0's `numbers-in-json`, runtime#54), through
  lists and embedded values. `DeserializeExact` gives `integer` and its four
  sign-restricted forms as a `BigInteger`, `long` as a `long`, `unsignedLong` as a
  `ulong`, `decimal` as a string of its canonical digits, and `int` and narrower as
  a `long`. A JSON number past ±(2^53 − 1) for a wide integer, a number for a decimal, a string for a narrow integer, or a value outside its datatype's range is refused, ending `[json-number-form]`, as `codec-vectors-json-numbers.json` pins in every port.
- **`Codec.Serialize(node)` / `Codec.Deserialize(json, schema)`**, obsolete for the two above — the
  normalized-JSON wire form. Open-world assertions outside the type-model
  round-trip losslessly through `$extra` (top-level wire fields, collected under
  `$extra` on the typed node — `[JsonExtensionData]` semantics). Hashing refuses a statement the schema doesn't declare, an `$extra` entry or a key the class doesn't model, with `[hash-undeclared-statement]`: a codec holds one package's schema and can't type another package's predicate, so such a node is hashed with the SDK, over the graph. Serializing and deserializing still carry it (codec 2.0.0, runtime#37).
- **`Codec.TypeMatches(node, classUri, schema)`** — what a generated type guard
  asks: is this node (a map or a `KanonakNode`) an instance of `classUri`?
- **`Codec.EnumMember(schema, reference)` → `EnumMemberMatch`** (`EnumType`,
  `Uri`, `Member`) or `null` — the enumeration member a `{"$ref": …}` names.

### Reading an earlier compatible version (0.6.1)

A node is typed with the class version its producer resolved; a codec built
from a later COMPATIBLE version of the same package still reads it.
`Deserialize` resolves `$type` by the exact key first, then by the same
publisher/package/name at the highest schema version for which
`Coordinate.IsReadableBy(written, schemaVersion)` (`Kanonak.Canonical`) holds,
and the node keeps its WRITTEN `$type`. A rejection message ends in a
bracketed kind: `[unknown-type]`, `[newer-version]`, `[other-major]` or
`[other-minor-line]`. `TypeMatches` resolves each of the node's types
(`$types`, else `$type`) the same way and matches the class itself or any
entry of the optional `CodecClass.Ancestors` (every superclass, by versioned
URI; canonicalization never reads it, and a schema without it still parses);
unreadable types never match. `EnumMember` resolves a reference exactly, then
to the same member at a version that can read it. Hashing stays EXACT-version
only: a node or embedded value whose `$type` is only compatibly present is
rejected with `[hash-needs-exact-version]`, since its predicates would carry
the schema's version instead of the producer's.

The YAML / Package wire (`toPackage`/`fromPackage`) is out of scope for this port.

## Node shape

A node is a plain `IReadOnlyDictionary<string, object>`: the `$`-envelope
(`$type`, `$id`, optional `$extra`) plus alias-collapsed local-name fields.
Field values are CLR primitives (`string`, `bool`, numeric), an
`IReadOnlyList` of those, or a reference map (`{ "$ref": uri }`). `$extra` is a
map keyed by predicate URI. A reference must address a named resource
(`publisher/package[@version]/name`): a `$ref` containing a URI fragment (`#`)
is rejected, at any depth, by canonicalization, `Serialize` and `Deserialize`,
with a message ending in `[fragment-reference]` (runtime#6).

## Project layout

- `src/Kanonak.Codec/` — the library (`netstandard2.0`), public package
  `Kanonak.Codec`. References `../../kanonak-canonical/csharp/src/Kanonak.Canonical`.
- `test/Kanonak.Codec.Conformance/` — the conformance runner (`net10.0`).

## Conformance

The runner drives the shared codec vectors (every
`kanonak-codec/vectors/codec-vectors*.json` file: base, embedded, types,
references, enums, compat) and asserts the canonical form, content hash,
(structurally compared) `Serialize`/`Deserialize` output, and the compatibility
contract all match the authoritative expected values.

```bash
cd test/Kanonak.Codec.Conformance
dotnet run
```

All vectors must pass. The basic-case hash is
`sha256:6ed4e664dbaf7d3331d71af297f48da23994af34d081a86f555cb34706de2913`.
