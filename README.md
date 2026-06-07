# Kanonak runtime

The public, open-source runtime stack that every generated Kanonak SDK depends
on — so the runtime is referenced from a public registry, **never inlined** into
each generated SDK.

Two libraries, six languages each, conformance-verified to produce a
**byte-identical content address** across all of them:

- **`kanonak-canonical`** — the canonical form + content hash; the reference
  implementation of the open spec [`kanonak.org/canonical-form`](https://kanonak.org)
  (`canonicalFormVersion "1"`).
- **`kanonak-codec`** — the generic, ontology-independent codec runtime: builds
  the canonical input model from typed nodes + an embedded schema,
  content-addresses it via `kanonak-canonical`, and (de)serializes the
  normalized-JSON wire form. **Depends on `kanonak-canonical`.**

These are infrastructure, in the spirit of `serde` / `pydantic` / `jackson`:
small, dependency-light, and fully determined by the public spec + the public
conformance vectors in this repo. (The Kanonak **code generator** and platform
are a separate, commercial concern and are not part of this repo.)

## Layout

```
kanonak-canonical/{python,rust,go,java,csharp,typescript}/   + vectors/
kanonak-codec/{python,rust,go,java,csharp,typescript}/ + vectors/
```

Each `vectors/` directory is the shared conformance contract: every language
implementation is tested against the same `(input → canonical form, content
hash, serialization)` golden vectors.

## Published packages

| Language | Registry | `canonical` | `codec` |
|---|---|---|---|
| Python | PyPI | `kanonak-canonical` | `kanonak-codec` |
| Rust | crates.io | `kanonak-canonical` | `kanonak-codec` |
| TypeScript | npm | `@kanonak-protocol/canonical` | `@kanonak-protocol/codec` |
| C# | NuGet | `Kanonak.Canonical` | `Kanonak.Codec` |
| Java | Maven Central | `org.kanonak:kanonak-canonical` | `org.kanonak:kanonak-codec` |
| Go | proxy | `github.com/kanonak-protocol/runtime/kanonak-canonical/go` | `.../kanonak-codec/go` |

## Releasing

See **[`PUBLISHING.md`](./PUBLISHING.md)** for the strategy and
**[`release-targets.yml`](./release-targets.yml)** for the single source of truth
on every registry connection, account, and secret. Publish order is always
`canonical` → `codec`, gated on the conformance vectors.

## License

[Apache-2.0](./LICENSE).
