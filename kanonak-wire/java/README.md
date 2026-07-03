# kanonak-wire

The Java port of the Kanonak wire kernel (`kanonak.org/wire-form`,
`wireFormatVersion "1"`), verified against the shared golden vectors.

A minimal, allocation-conscious binary reader/writer for hot-path wire
protocols: bounds-checked cursor reads/writes, big-endian integers, strict
UTF-8, lowercase-canonical UUIDs, and a rich fail-loud error taxonomy.
`bytes(n)`/`rest()` return zero-copy read-only `ByteBuffer` VIEWS over the
backing array; `u32be` widens to `long` (Java has no unsigned 32-bit type);
UTF-8 decode/encode uses `CharsetDecoder`/`CharsetEncoder` with
`CodingErrorAction.REPORT` — never a lossy U+FFFD, and an unpaired UTF-16
surrogate on the write side is `InvalidUtf8`. Writer numeric parameters
(`int`/`long`) can exceed the wire range, so every numeric write validates:
out-of-range values are `ValueOutOfRange`, never a silent truncation.

Source & issues: https://github.com/kanonak-protocol/runtime
