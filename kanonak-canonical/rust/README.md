# kanonak-canonical

The canonical form + content hash for the [Kanonak Protocol](https://kanonak.org) —
the Rust port of `kanonak.org/canonical-form` (`canonicalFormVersion "1"`), verified
byte-for-byte against the shared golden vectors.

The identity primitive every generated Kanonak SDK content-addresses through:
turn typed values into a deterministic, representation-independent canonical form
and its `sha256:` content hash — byte-identical across all six language ports.

Source & issues: https://github.com/kanonak-protocol/runtime
