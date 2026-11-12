/**
 * `@kanonak-protocol/canonical` — the canonical form + content hash for the
 * Kanonak Protocol (the TypeScript port of `kanonak.org/canonical-form`,
 * `canonicalFormVersion "1"`), verified byte-for-byte against the shared golden
 * vectors. The identity primitive every generated SDK content-addresses through.
 *
 * The per-carrier lexical functions and the `canonicalScalarLexical` dispatch
 * are INTERNAL implementation detail (the spec + golden vectors are the contract
 * a re-implementer follows); they stay exported from `./Datatypes.js` for tests.
 * The integer and decimal ones are public, as every other port's are, since a
 * codec reading a value from JSON checks its lexical form with them (runtime#54).
 * So are the hexBinary and base64Binary ones, since a program reading a binary
 * value's bytes canonicalizes it first, so its decoder only ever sees the one
 * form every port writes (runtime#83).
 */
export { canonicalForm, canonicalHash, CANONICAL_FORM_VERSION } from './CanonicalForm.js';
export {
  Carrier,
  carrierOf,
  canonicalInteger,
  canonicalDecimal,
  canonicalHexBinary,
  canonicalBase64,
} from './Datatypes.js';
export {
  parseCoordinate,
  parsePackageCoordinate,
  versionlessKey,
  localName,
  displayName,
  lenientVersionlessKey,
  isReadableBy,
} from './Coordinate.js';
export type { Coordinate, CoordinateVersion, PackageCoordinate } from './Coordinate.js';
export type {
  CanonicalInput,
  CanonicalInputSubject,
  CanonicalInputStatement,
  CanonicalInputValue,
} from './InputModel.js';
