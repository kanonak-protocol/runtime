/**
 * The language-neutral canonical INPUT model — the one cross-language contract a
 * codec builds to content-address a typed object, carrying datatypes explicitly
 * so canonicalization needs no parser. It is exactly the shape
 * `full-form-vectors.json` is defined over and `kanonak hash -v` emits.
 */
export interface CanonicalInput {
  subjects: CanonicalInputSubject[];
}

export interface CanonicalInputSubject {
  /** The subject's canonical URI, e.g. `publisher/package@1.0.0/Name`. */
  uri: string;
  statements: CanonicalInputStatement[];
}

export interface CanonicalInputStatement {
  /** The predicate's canonical URI. */
  predicate: string;
  value: CanonicalInputValue;
}

/**
 * A value: a typed scalar (`lit` + its `datatype` URI), an untyped/open-world
 * scalar (`raw` token, no carrier), a reference, an embedded node, or a list.
 */
export type CanonicalInputValue =
  | { lit: string; datatype: string }
  | { raw: string }
  | { ref: string }
  | { embed: { name?: string; statements: CanonicalInputStatement[] } }
  | { list: CanonicalInputValue[] };
