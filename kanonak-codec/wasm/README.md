# kanonak-codec (WebAssembly component)

The generic, ontology-independent Kanonak codec runtime — as a WebAssembly
component (the 7th port). This is the [Rust reference](../rust) compiled to
`wasm32-wasip2` behind the `kanonak:codec` WIT interface, statically bundling
[`kanonak-canonical`](../../kanonak-canonical/rust) — **not a
reimplementation**. A Wasm host or component holding Kanonak-typed data gets
the exact content-identity contract of the six native ports: byte-identical
canonical form, the same `sha256:` content hash as the `kanonak hash` CLI, and
identical (de)serialization.

## Interface

The surface is JSON text in / JSON text out ([`wit/codec.wit`](./wit/codec.wit)):

```wit
interface codec {
    content-hash:   func(nodes: string, schema: string, pkg: string) -> result<string, string>;
    canonical-form: func(nodes: string, schema: string, pkg: string) -> result<string, string>;
    serialize:      func(node: string, schema: string) -> result<string, string>;
    deserialize:    func(node: string, schema: string) -> result<string, string>;
}
```

`nodes` is a JSON array of typed nodes (the `$`-envelope plus alias-collapsed
fields), `schema` the embedded `CodecSchema`, `pkg` the package context
(`{publisher, packageName, version, label?}`). The schema is just an input
string — the component stays agnostic to how a caller obtained it. Errors are
the codec's fail-loud messages; there are no fallbacks. `serialize` accepts a
`schema` argument for symmetry with `deserialize`; it is validated as JSON but
not consulted.

## Build

```sh
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
```

Produces the component at
`target/wasm32-wasip2/release/kanonak_codec_wasm.wasm`
(`wasm-tools component wit` shows the exported `kanonak:codec/codec` surface;
the `wasi:*@0.2` imports come from Rust's std and any component host provides
them).

## Conformance

```sh
cd conformance
npm install
npm run conformance
```

Transpiles the component with [`@bytecodealliance/jco`](https://github.com/bytecodealliance/jco)
and drives the SAME shared golden vectors as the six native ports —
`../vectors/codec-vectors.json`, `codec-vectors-embedded.json`, and
`codec-vectors-types.json` (including the `$ref` object-reference and `$extra`
open-world cases, and the `$types` reject-on-all-three-surfaces + round-trip
contract) — through the component's WIT surface.
