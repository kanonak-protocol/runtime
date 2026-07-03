# kanonak-wire

The Kanonak **wire kernel** — `wireFormatVersion: "1"`.

A minimal, allocation-conscious binary reader/writer for hot-path wire protocols.
The codec family's fourth member: `canonical` content-addresses, `codec`
serializes JSON wire forms, `expression` evaluates, **`wire` moves bytes**. Where
`expression` is *declaration → interpretation* (a tree-walker at decision
points), wire is *declaration → generation*: generated protocol codecs call this
kernel, and the declaration compiles away. The kernel contains only what is
invariant across ALL protocols — bounds-checked cursor reads/writes, endianness,
strict text validation, and a rich error taxonomy. It knows nothing about any
particular protocol.

Every language port (Python, TypeScript, Go, Rust, Java, C#) produces identical
values and identical errors for the shared golden vectors in
[`vectors/`](./vectors), including the determinism traps that expose language
differences (unsigned values above int32, strict UTF-8 rejection, the UTF-16
lone-surrogate encoder trap).

A change to any rule below requires a **new** `wireFormatVersion`, never an edit
in place.

## Vectors

- [`vectors/wire-vectors.json`](./vectors/wire-vectors.json) — the parity gate.
  `readVectors` are op-scripts: a hex byte buffer plus an ordered list of reader
  calls, each asserting an expected value or a required error `{kind, offset}`
  (offset = absolute byte offset where the failing read started). `writeVectors`
  run writer ops and assert the exact output bytes, or a required error `{kind}`.
  Cases whose trap is unrepresentable in a language's type system carry a
  `requires` capability and are skipped WITH a reported skip count elsewhere:
  `wide-numeric-params` (writer numeric params can exceed the wire type's
  range — TS, Python, Java; Rust/Go/C# take exact-width param types, where the
  type IS the validation), `dynamic-numeric` (non-integer values — TS, Python),
  `utf16-strings` (strings can hold unpaired surrogates — TS, Java, C#, Python).

## Ports

| Language | Path | Status | Conformance command |
|---|---|---|---|
| TypeScript | [`typescript/`](./typescript) | ✅ | `npm run conformance` |
| Rust | [`rust/`](./rust) | ✅ | `cargo test` |
| Python | [`python/`](./python) | ✅ | `python conformance.py ../vectors` |
| Go | [`go/`](./go) | ✅ | `go test ./...` |
| Java | [`java/`](./java) | ✅ | `javac -d out src/main/java/org/kanonak/wire/*.java conformance/Conformance.java && java -cp out Conformance ../vectors` |
| C# | [`csharp/`](./csharp) | ✅ | `cd csharp && dotnet run --project test/Kanonak.Wire.Conformance -- ../vectors` |

## What a port implements

Two types plus an error: `WireReader` (an immutable-buffer cursor), `WireWriter`
(an append-only buffer builder), and `WireError` carrying `{kind, offset?}`.
Same logical API in every language, cased per idiom (`u16be` / `u16_be` /
`U16BE` / `U16Be`).

### Reader primitives

| Primitive | Contract |
|---|---|
| `u8` | one byte, 0..255 |
| `u16be` | two bytes, big-endian, 0..65535 |
| `u32be` | four bytes, big-endian, 0..2³²−1 — languages without a fitting unsigned type widen (Java: `long`) |
| `bytes(n)` | exactly n bytes as a **zero-copy view** wherever the language has a natural byte-view type (Rust slice, Go subslice, TS `Uint8Array` subarray, Python `memoryview`/slice, C# `ReadOnlyMemory`, Java `ByteBuffer` view) |
| `uuid` | exactly 16 bytes → **lowercase** hyphenated 8-4-4-4-12 string; NO version/variant validation — any 16 bytes are legal |
| `utf8(n)` | n bytes decoded as **strict** UTF-8 — invalid sequences, overlong encodings, and surrogate-range encodings are `InvalidUtf8`; never lossy, never U+FFFD. Bounds are checked before validity: n beyond remaining is `Truncated` |
| `lenPrefixedBytes16` | u16be length L, then exactly L bytes; L > remaining is `LengthOverrun` (NOT `Truncated` — the length field itself is suspect) |
| `rest` | all remaining bytes (possibly empty), zero-copy; never errors; advances to end |
| `remaining` | count of unread bytes |
| `expectEnd` | errors `TrailingBytes` if any bytes remain |

The reader never copies and never reads past the requested extent (a multibyte
character split by the requested `n` is `InvalidUtf8`, not a peek-ahead).

### Writer primitives

`u8(v)`, `u16be(v)`, `u32be(v)`, `bytes(b)`, `uuid(s)`, `utf8(s)`,
`lenPrefixedBytes16(b)`, and a terminal `toBytes()` (per-idiom name). Rules:

- **Range validation.** Languages whose numeric types don't enforce the range
  (TS `number`, Python `int`, Java `int`) MUST validate: out-of-range or
  non-integer values are `ValueOutOfRange` — never a silent truncation.
- **The lone-surrogate trap.** Languages whose strings can hold unpaired
  surrogates (JS, Java, C#, Python) MUST reject them with `InvalidUtf8` —
  never emit replacement characters. (JS `TextEncoder` silently emits U+FFFD;
  ports must pre-validate.)
- **UUID input.** Accept hyphenated 8-4-4-4-12 hex, case-insensitively; emit
  the 16 bytes. Anything else (un-hyphenated, wrong length, non-hex) is
  `InvalidUuid`. Canonical string output stays lowercase.

### Error taxonomy

| Kind | When | Carries |
|---|---|---|
| `Truncated` | a read needs more bytes than remain | needed, remaining, offset |
| `LengthOverrun` | a declared length exceeds remaining | declared, remaining, offset |
| `TrailingBytes` | `expectEnd` with bytes remaining | count, offset |
| `InvalidUtf8` | strict decode/encode failure | offset (reads) |
| `InvalidUuid` | writer UUID parse failure | — |
| `ValueOutOfRange` | writer numeric range/integrality failure | value |
| `UnknownTag` | constructor for generated union dispatch (not exercised by kernel vectors) | tag, context |

Every error message states what was expected, what was found, and where. There
are no silent fallbacks: no `null` returns, no partial values, no lossy decodes.

### Zero-copy / MOST-FASTEST contract

`bytes(n)` and `rest()` return views into the source buffer, never copies —
copying is always an explicit caller decision. Writers may preallocate
(`withCapacity`-style constructors) so generated encoders can compute exact
sizes and write once. Decoding a 3-field frame is three bounds checks and one
subview: zero allocations in languages with byte views.

Spec: `kanonak.org/wire-form`. Source & issues:
https://github.com/kanonak-protocol/runtime
