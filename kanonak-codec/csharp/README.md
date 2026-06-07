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
- **`Codec.Serialize(node)` / `Codec.Deserialize(json, schema)`** — the
  normalized-JSON wire form. Open-world assertions outside the type-model
  round-trip losslessly through `$extra` (top-level wire fields, collected under
  `$extra` on the typed node — `[JsonExtensionData]` semantics).

The YAML / Package wire (`toPackage`/`fromPackage`) is out of scope for this port.

## Node shape

A node is a plain `IReadOnlyDictionary<string, object>`: the `$`-envelope
(`$type`, `$id`, optional `$extra`) plus alias-collapsed local-name fields.
Field values are CLR primitives (`string`, `bool`, numeric), an
`IReadOnlyList` of those, or a reference map (`{ "$ref": uri }`). `$extra` is a
map keyed by predicate URI.

## Project layout

- `src/Kanonak.Codec/` — the library (`netstandard2.0`), public package
  `Kanonak.Codec`. References `../../kanonak-canonical/csharp/src/Kanonak.Canonical`.
- `test/Kanonak.Codec.Conformance/` — the conformance runner (`net10.0`).

## Conformance

The runner drives the shared codec vectors
(`kanonak-codec/vectors/codec-vectors.json`) and asserts the
canonical form, content hash, and (structurally compared) `Serialize` output all
match the authoritative expected values.

```bash
cd test/Kanonak.Codec.Conformance
dotnet run
```

All vectors must pass. The basic-case hash is
`sha256:6ed4e664dbaf7d3331d71af297f48da23994af34d081a86f555cb34706de2913`.
