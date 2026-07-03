# kanonak-wire

The Rust port of the Kanonak wire kernel (`kanonak.org/wire-form`,
`wireFormatVersion "1"`), verified against the shared golden vectors.

A minimal, allocation-conscious binary reader/writer for hot-path wire
protocols: bounds-checked cursor reads/writes, big-endian integers, strict
UTF-8, lowercase-canonical UUIDs, and a rich fail-loud error taxonomy.
`bytes(n)`/`rest()`/`utf8(n)` return zero-copy borrows of the source buffer;
writer numeric parameters use exact-width types — the type is the validation.

Source & issues: https://github.com/kanonak-protocol/runtime
