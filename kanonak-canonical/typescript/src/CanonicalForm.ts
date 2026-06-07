/**
 * Canonical structural form + content hash for the language-neutral
 * `CanonicalInput` model. Produces a deterministic, representation-independent
 * JSON serialization; `canonicalHash` is SHA-256 over that form, prefixed
 * `sha256:`. Invariant under subject/statement ordering; lists keep source order.
 *
 * Wire form: compact JSON, RFC 8785 escaping, a FIXED per-blob field order
 * (predicate, type, then carrier/name, then value/statements/items). Ordering of
 * subjects (by URI) and statements (by predicate) is by UTF-8 byte sequence.
 * Frozen to `canonicalFormVersion` "1".
 */
import { createHash } from 'node:crypto';
import { Carrier, carrierOf, canonicalScalarLexical } from './Datatypes.js';
import type { CanonicalInput, CanonicalInputStatement, CanonicalInputValue } from './InputModel.js';

/** The frozen canonical-form version. NOT part of the hashed bytes. */
export const CANONICAL_FORM_VERSION = '1';

const UTF8 = new TextEncoder();

/** Lexicographic comparison by UTF-8 byte sequence (== Unicode code-point order). */
function compareUtf8(a: string, b: string): number {
  const ab = UTF8.encode(a);
  const bb = UTF8.encode(b);
  const n = Math.min(ab.length, bb.length);
  for (let i = 0; i < n; i++) {
    if (ab[i] !== bb[i]) return ab[i] - bb[i];
  }
  return ab.length - bb.length;
}

type ValueBlob =
  | { type: 'string'; value: string }
  | { type: 'typed'; carrier: string; value: string }
  | { type: 'ref'; value: string }
  | { type: 'embedded'; name?: string; statements: StatementBlob[] }
  | { type: 'list'; items: ValueBlob[] };

interface StatementBlob {
  predicate: string;
  type: ValueBlob['type'];
  value?: string;
  carrier?: string;
  name?: string;
  statements?: StatementBlob[];
  items?: ValueBlob[];
}

/** Canonicalize one value into its wire blob (no predicate). */
function valueBlob(v: CanonicalInputValue): ValueBlob {
  if ('lit' in v) {
    const carrier = carrierOf(v.datatype);
    if (carrier) {
      return { type: 'typed', carrier, value: canonicalScalarLexical(carrier as Carrier, v.lit) };
    }
    // Out-of-set datatype: a byte-preserved raw token (the untyped tier).
    return { type: 'string', value: v.lit };
  }
  if ('raw' in v) return { type: 'string', value: v.raw };
  if ('ref' in v) return { type: 'ref', value: v.ref };
  if ('embed' in v) {
    const statements = canonicalStatements(v.embed.statements ?? []);
    return v.embed.name && v.embed.name.length > 0
      ? { type: 'embedded', name: v.embed.name, statements }
      : { type: 'embedded', statements };
  }
  if ('list' in v) return { type: 'list', items: v.list.map(valueBlob) };
  throw new Error(`canonical input: unknown value shape ${JSON.stringify(v)}`);
}

/** Flatten a statement + its value into one record, with the fixed key order. */
function statementBlob(st: CanonicalInputStatement): StatementBlob {
  const v = valueBlob(st.value);
  switch (v.type) {
    case 'string':  return { predicate: st.predicate, type: 'string', value: v.value };
    case 'typed':   return { predicate: st.predicate, type: 'typed', carrier: v.carrier, value: v.value };
    case 'ref':     return { predicate: st.predicate, type: 'ref', value: v.value };
    case 'embedded':
      return v.name !== undefined
        ? { predicate: st.predicate, type: 'embedded', name: v.name, statements: v.statements }
        : { predicate: st.predicate, type: 'embedded', statements: v.statements };
    case 'list':    return { predicate: st.predicate, type: 'list', items: v.items };
  }
}

function canonicalStatements(stmts: CanonicalInputStatement[]): StatementBlob[] {
  return stmts.map(statementBlob).sort((a, b) => compareUtf8(a.predicate, b.predicate));
}

/** The canonical form (the `{subjects:[…]}` JSON) of a `CanonicalInput`. */
export function canonicalForm(input: CanonicalInput): string {
  const subjects = input.subjects.map((s) => ({
    uri: s.uri,
    statements: canonicalStatements(s.statements ?? []),
  }));
  subjects.sort((a, b) => compareUtf8(a.uri, b.uri));
  return JSON.stringify({ subjects });
}

/** SHA-256 of the canonical form, prefixed `sha256:`. */
export function canonicalHash(input: CanonicalInput): string {
  const hex = createHash('sha256').update(canonicalForm(input), 'utf8').digest('hex');
  return `sha256:${hex}`;
}
