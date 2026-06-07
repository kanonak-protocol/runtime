# kanonak-canonical

Per-language libraries that compute the **Kanonak canonical form** and its
**content hash** — the permanent content address of a Package / EphemeralPackage
(`sha256:…`, `q-<hex16>` name, `kanonak.lock` integrity).

The cross-language contract is the spec [`kanonak.org/canonical-form`](https://kanonak.org/canonical-form) plus
the **golden conformance vectors** in [`vectors/`](./vectors). Each port is an
independent conformant implementation: it passes 100% of the vectors and is
therefore guaranteed to agree on content addresses with the TypeScript SDK, the
CLI (`kanonak hash`), and every other port. These libraries are referenced as a
dependency by the generated typed SDKs (not re-inlined per package).

`canonicalFormVersion: "1"` — frozen. A change to any carrier, lexical form,
ordering, or wire layout requires a NEW version, never an edit in place.

## Vectors

- `vectors/lexical-vectors.json` — per-carrier `(carrier, raw token) → canonical
  lexical` (or `expectError`).
- `vectors/full-form-vectors.json` — `input → exact canonical-form bytes → hash`,
  over the language-neutral typed-value input model.

## Ports

All six ports pass 100% of the golden vectors (64 lexical + 9 full-form).

| Language | Path | Status | Conformance command |
|---|---|---|---|
| C# | [`csharp/`](./csharp) | ✅ | `dotnet run --project csharp/test/Kanonak.Canonical.Conformance -- ../vectors` |
| Rust | [`rust/`](./rust) | ✅ | `cargo test` (in `rust/`) |
| Go | [`go/`](./go) | ✅ | `go test ./...` (in `go/`) |
| Java | [`java/`](./java) | ✅ | `javac -d out src/main/java/org/kanonak/canonical/*.java conformance/Conformance.java && java -cp out Conformance ../vectors` (in `java/`) |
| Python | [`python/`](./python) | ✅ | `python conformance.py ../vectors` (in `python/`) |
| TypeScript | [`typescript/`](./typescript) | ✅ | `npm install && npm run conformance` (in `typescript/`) |

The TypeScript port is published as `@kanonak-protocol/canonical`; the SDK and
the codec consume it (it is no longer bundled inside the SDK).

## What a port implements

1. **Carrier routing** — `carrierOf(datatypeUri)` over the closed v1 carrier set
   (the whole xsd integer tree → one `integer` carrier, etc.).
2. **Per-carrier canonical lexical forms** — integer/decimal (arbitrary
   precision, never IEEE), double/float (shortest round-trip + `INF`/`-INF`/`NaN`),
   boolean, string/anyURI (NFC), langString (NFC + BCP 47 tag case), hexBinary,
   base64Binary, dateTime (UTC `Z` shift) / date / time (lexical, no shift).
3. **Wire form** — subjects ordered by UTF-8 bytes of the URI, statements by the
   predicate URI; compact JSON with RFC 8785 escaping and a fixed per-blob field
   order; the typed scalar blob `{type,carrier,value}`. SHA-256 of the UTF-8
   bytes, prefixed `sha256:`.

The `ephemeral` namespace-neutralization for EphemeralPackage body hashes is a
*caller* concern (the producer/codec), not part of `canonicalForm` itself.
