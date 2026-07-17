# Kanonak runtime

The public, open-source runtime stack that every generated Kanonak SDK depends
on — so the runtime is referenced from a public registry, **never inlined** into
each generated SDK.

Four libraries, six languages each (plus a WebAssembly-component build of the
codec), conformance-verified to produce **byte-identical results** across all
of them:

- **`kanonak-canonical`** — the canonical form + content hash; the reference
  implementation of the open spec [`kanonak.org/canonical-form`](https://kanonak.org/canonical-form)
  (`canonicalFormVersion "1"`).
- **`kanonak-codec`** — the generic, ontology-independent codec runtime: builds
  the canonical input model from typed nodes + an embedded schema,
  content-addresses it via `kanonak-canonical`, and (de)serializes the
  normalized-JSON wire form. **Depends on `kanonak-canonical`.**
- **`kanonak-expression`** — the deterministic expression runtime
  (`expressionRuntimeVersion "1"`): a tree-walker that folds a
  `kanonak.org/transformations` + `kanonak.org/math` expression to a number,
  identically in every language.
- **`kanonak-wire`** — the binary wire kernel (`wireFormatVersion "1"`): a
  minimal, allocation-conscious reader/writer for hot-path wire protocols.
  Where `expression` is declaration → interpretation, `wire` is what generated
  protocol codecs call into — the declaration compiles away.

These are infrastructure, in the spirit of `serde` / `pydantic` / `jackson`:
small, dependency-light, and fully determined by the public spec + the public
conformance vectors in this repo. (The Kanonak **code generator** and platform
are a separate, commercial concern and are not part of this repo.)

## Layout

```
kanonak-canonical/{python,rust,go,java,csharp,typescript}/   + vectors/
kanonak-codec/{python,rust,go,java,csharp,typescript,wasm}/ + vectors/
kanonak-expression/{python,rust,go,java,csharp,typescript}/ + vectors/
kanonak-wire/{python,rust,go,java,csharp,typescript}/ + vectors/
```

`kanonak-codec/wasm/` is the 7th codec port: a WebAssembly component
(`wasm32-wasip2`, exporting the `kanonak:codec` WIT interface) built from the
Rust reference — not a reimplementation — and conformance-gated by the same
vectors. See [`kanonak-codec/wasm/README.md`](./kanonak-codec/wasm/README.md).

Each `vectors/` directory is the shared conformance contract: every language
implementation is tested against the same `(input → canonical form, content
hash, serialization)` golden vectors.

## Published packages

| Language | Registry | `canonical` | `codec` | `expression` | `wire` |
|---|---|---|---|---|---|
| Python | PyPI | `kanonak-canonical` | `kanonak-codec` | `kanonak-expression` | `kanonak-wire` |
| Rust | crates.io | `kanonak-canonical` | `kanonak-codec` | `kanonak-expression` | `kanonak-wire` |
| TypeScript | npm | `@kanonak-protocol/canonical` | `@kanonak-protocol/codec` | `@kanonak-protocol/expression` | `@kanonak-protocol/wire` |
| C# | NuGet | `Kanonak.Canonical` | `Kanonak.Codec` | `Kanonak.Expression` | `Kanonak.Wire` |
| Java | Maven Central | `org.kanonak:kanonak-canonical` | `org.kanonak:kanonak-codec` | `org.kanonak:kanonak-expression` | `org.kanonak:kanonak-wire` |
| Go | proxy | `github.com/kanonak-protocol/runtime/kanonak-canonical/go` | `.../kanonak-codec/go` | `.../kanonak-expression/go` | `.../kanonak-wire/go` |
| Wasm component | GHCR (OCI) | — | `ghcr.io/kanonak-protocol/codec` | — | — |

The `kanonak-codec` Wasm component ships as a wkg-format OCI artifact, tagged
with the codec version:

```sh
wkg oci pull ghcr.io/kanonak-protocol/codec:0.4.0 -o codec.wasm
```

## Releasing

See **[`PUBLISHING.md`](./PUBLISHING.md)** for the strategy and
**[`release-targets.yml`](./release-targets.yml)** for the single source of truth
on every registry connection, account, and secret. Publish order is always
`canonical` → `codec`, gated on the conformance vectors.

## License

[Apache-2.0](./LICENSE).
