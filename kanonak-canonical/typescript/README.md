# @kanonak-protocol/canonical

The canonical form + content hash for the [Kanonak Protocol](https://kanonak.org) —
the TypeScript port of `kanonak.org/canonical-form` (`canonicalFormVersion "1"`),
verified byte-for-byte against the shared golden vectors. The identity primitive
every generated SDK content-addresses through.

```ts
import { canonicalForm, canonicalHash } from '@kanonak-protocol/canonical';
import type { CanonicalInput } from '@kanonak-protocol/canonical';

const input: CanonicalInput = {
  subjects: [
    {
      uri: 'example.com/data@1.0.0/a1',
      statements: [
        { predicate: 'example.com/schema@1.0.0/qty', value: { lit: '0100', datatype: 'kanonak.org/core-xsd/integer' } },
      ],
    },
  ],
};

canonicalForm(input); // deterministic, representation-independent JSON
canonicalHash(input); // sha256:… — matches `kanonak hash`
```

`canonicalFormVersion: "1"` — the content-address rules are frozen and evolve
only by minting a new version.

Source & issues: https://github.com/kanonak-protocol/runtime
