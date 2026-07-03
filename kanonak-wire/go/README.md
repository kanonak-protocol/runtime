# kanonak-wire

The Go port of the Kanonak wire kernel (`kanonak.org/wire-form`,
`wireFormatVersion "1"`), verified against the shared golden vectors.

A minimal, allocation-conscious binary reader/writer for hot-path wire
protocols: bounds-checked cursor reads/writes, big-endian integers, strict
UTF-8, lowercase-canonical UUIDs, and a rich fail-loud error taxonomy.
`Bytes(n)`/`Rest()` return zero-copy subslices of the source buffer; writer
numeric parameters use exact-width types (`byte`/`uint16`/`uint32`) — the
type is the validation. Writer `UTF8` validates (Go strings can hold
arbitrary bytes) — ill-formed input is `InvalidUtf8`, never lossy.

Conformance: `go test ./...`

Source & issues: https://github.com/kanonak-protocol/runtime
