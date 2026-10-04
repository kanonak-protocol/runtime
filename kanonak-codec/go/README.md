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

wire := codec.Serialize(node)                 // typed node -> normalized JSON
node, err := codec.Deserialize(wire, schema)  // normalized JSON -> typed node

ok, err := codec.TypeMatches(node, classURI, schema) // generated type guard
ok, err = typed.KanonakNode.TypeMatches(classURI, schema) // same, on a typed instance
m, found := codec.LookupEnumMember(schema, ref)      // {"$ref": ...} -> enum member
```

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
`[other-minor-line]`, `[hash-needs-exact-version]`.

Embedded object values are not yet supported — pass a reference
(`{"$ref": "..."}`) instead. Malformed input fails loudly; there are no silent
fallbacks.

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
