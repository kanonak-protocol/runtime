# Kanonak.Wire

The C# port of the Kanonak wire kernel (`kanonak.org/wire-form`,
`wireFormatVersion "1"`), verified against the shared golden vectors.
Targets `netstandard2.0` + `net8.0`.

A minimal, allocation-conscious binary reader/writer for hot-path wire
protocols: bounds-checked cursor reads/writes, big-endian integers, strict
UTF-8, lowercase-canonical UUIDs, and a rich fail-loud error taxonomy.
`Bytes(n)`/`Rest()` return zero-copy `ReadOnlyMemory<byte>` views of the
source buffer; writer numeric parameters use exact-width types
(`byte`/`ushort`/`uint`) — the type is the validation. C# strings are UTF-16,
so `Utf8(string)` rejects unpaired surrogates with `InvalidUtf8` — never a
lossy replacement character.

Conformance (from this directory):

```
dotnet run --project test/Kanonak.Wire.Conformance -- ../vectors
```

Source & issues: https://github.com/kanonak-protocol/runtime
