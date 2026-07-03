# @kanonak-protocol/wire

The TypeScript port of the Kanonak wire kernel (`kanonak.org/wire-form`,
`wireFormatVersion "1"`), verified against the shared golden vectors.

A minimal, allocation-conscious binary reader/writer for hot-path wire
protocols: bounds-checked cursor reads/writes, big-endian integers, strict
UTF-8, lowercase-canonical UUIDs, and a rich fail-loud error taxonomy.
`bytes(n)`/`rest()` return zero-copy `Uint8Array` subviews.

Source & issues: https://github.com/kanonak-protocol/runtime
