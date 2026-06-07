/**
 * `@kanonak-protocol/canonical` — the canonical form + content hash for the
 * Kanonak Protocol (the TypeScript port of `kanonak.org/canonical-form`,
 * `canonicalFormVersion "1"`), verified byte-for-byte against the shared golden
 * vectors. The identity primitive every generated SDK content-addresses through.
 *
 * The per-carrier lexical functions and the `canonicalScalarLexical` dispatch
 * are INTERNAL implementation detail (the spec + golden vectors are the contract
 * a re-implementer follows); they stay exported from `./Datatypes.js` for tests.
 */
export { canonicalForm, canonicalHash, CANONICAL_FORM_VERSION } from './CanonicalForm.js';
export { Carrier, carrierOf } from './Datatypes.js';
export type {
  CanonicalInput,
  CanonicalInputSubject,
  CanonicalInputStatement,
  CanonicalInputValue,
} from './InputModel.js';
