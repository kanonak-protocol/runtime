# kanonak-codec (Go)

The generic, ontology-independent Kanonak codec runtime — Go port.

Given a `CodecSchema` (the per-package metadata a generated SDK embeds) and a
set of typed nodes, it builds the canonical input model and content-addresses it
via [`kanonak.org/canonical`](../../kanonak-canonical/go) — producing the same
content-form as the TypeScript/Python references and the `kanonak hash` CLI. It
also (de)serializes the normalized-JSON wire form.

## Node representation

A node is a plain `map[string]interface{}`: the `$`-envelope (`$type`, `$id`,
optional `$extra`) plus alias-collapsed local-name fields. Decode JSON with a
`json.Decoder` configured via `UseNumber()` so numeric lexicals survive as
`json.Number` (e.g. `"5"`, `"1.5"`) rather than being widened to `float64`:

```go
dec := json.NewDecoder(r)
dec.UseNumber()
dec.Decode(&node)
```

## API

```go
schema := codec.CodecSchema{ /* typePredicate, labelPredicate, packageTypeUri, classes */ }
pkg    := codec.PackageContext{Publisher: "...", PackageName: "...", Version: "..."}

form, err := codec.CanonicalForm(nodes, schema, pkg) // {"subjects":[...]} JSON
hash, err := codec.ContentHash(nodes, schema, pkg)   // "sha256:..."

wire, err := codec.Serialize(node, schema)   // typed node -> normalized JSON
node, err := codec.Deserialize(wire, schema) // normalized JSON -> typed node

ok, err := codec.TypeMatches(node, classURI, schema) // generated type guard
ok, err = typed.KanonakNode.TypeMatches(classURI, schema) // same, on a typed instance
m, found := codec.LookupEnumMember(schema, ref)      // {"$ref": ...} -> enum member
```

`Serialize` and `Deserialize` carry the normalized-JSON wire form. Open-world
assertions outside the type-model round-trip losslessly through `$extra`
(top-level wire fields, collected under `$extra` on the node —
`[JsonExtensionData]` semantics). From 3.0.0, each integer and decimal value
travels in its JSON form (kanonak-protocol 4.11.0's `numbers-in-json`,
runtime#54), through lists and embedded values. `Deserialize` gives `integer`
and its four sign-restricted forms as a `*big.Int`, `long` as an `int64`,
`unsignedLong` as a `uint64`, `decimal` as a `string` of its canonical digits,
and `int`, `short`, `byte`, `unsignedInt`, `unsignedShort` and `unsignedByte`
as an `int64`, from a `json.Number` or a `float64` alike. `Serialize` writes a
wide integer and a decimal as a string of their canonical digits, and a narrow
integer as an `int64`. An embedded value whose class the schema doesn't hold
is carried as given. A JSON number past ±(2^53 − 1) for a wide integer, a
number for a decimal, a string for a narrow integer, or a value outside its
datatype's range is refused with an error ending in `[json-number-form]`, gated
by `codec-vectors-json-numbers.json`.

A reference value (`{"$ref": uri}`) must address a named resource
(`publisher/package[@version]/name`): a `$ref` carrying a URI fragment (`#`), at
any depth, is rejected by canonicalization, `Serialize` and `Deserialize` alike,
with an error ending in `[fragment-reference]`.

### Compatible versions

A node is typed with the class version its producer resolved; a codec generated
from a later COMPATIBLE version of the same package still reads it.
`Deserialize` resolves `$type` through the exact versioned class first, then the
same publisher/package/name at the highest schema version that can read the
written one (`canonical.IsReadableBy`), and keeps the `$type` it was written
with. `TypeMatches` answers a generated type guard the same way, subclass-aware
through a class's optional `Ancestors` (`"ancestors"`: every superclass's
versioned URI; canonicalization never reads it); it errors only when `classURI`
is not a class coordinate. `LookupEnumMember` resolves a member reference the
same way (`EnumMemberMatch{EnumType, URI, Member}`, or `found == false` for
"not this schema's"). Hashing stays exact-version only. Every rejection message
ends in a bracketed kind: `[unknown-type]`, `[newer-version]`, `[other-major]`,
`[other-minor-line]`, `[hash-needs-exact-version]`, `[hash-undeclared-statement]`.
Hashing also refuses a statement the schema doesn't declare, an `$extra` entry or a key the class doesn't model, with `[hash-undeclared-statement]`: a codec holds one package's schema and can't type another package's predicate, so such a node is hashed with the SDK, over the graph. Serializing and deserializing still carry it (codec 2.0.0, runtime#37). From 2.0.0 the module path is
`github.com/kanonak-protocol/runtime/kanonak-codec/go/v3`.

An object property carries a reference (`{"$ref": "..."}`) or an embedded
node, gated by `codec-vectors-embedded.json`. Malformed input fails loudly;
there are no silent fallbacks.

## Dependency

`go.mod` resolves `kanonak.org/canonical` via a local `replace` directive
pointing at `../../kanonak-canonical/go`.

## Test

```sh
go test ./...
```

Conformance is driven by the shared vectors at
`../vectors/codec-vectors*.json` (the same vectors every `kanonak-codec` port
runs; `codec-vectors-compat.json` pins the compatible-version contract). The basic case hash is
`sha256:6ed4e664dbaf7d3331d71af297f48da23994af34d081a86f555cb34706de2913`.
