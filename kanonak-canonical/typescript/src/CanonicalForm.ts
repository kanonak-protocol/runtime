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
  return `sha256:${sha256Hex(UTF8.encode(canonicalForm(input)))}`;
}

// --- SHA-256 (FIPS 180-4), pure JS, synchronous. ----------------------------
//
// Self-contained so the canonical form runs in ANY JavaScript environment —
// browser bundlers (Vite/Rollup) and edge workers included — with no `node:`
// built-in. The digest is byte-for-byte identical to a native SHA-256 (it is
// the same algorithm); the golden full-form vectors (`expectedHash`) are the
// guard. Web Crypto's `subtle.digest` is async, which would change this sync
// API, so a sync implementation is used deliberately.

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

function rotr(x: number, n: number): number {
  return (x >>> n) | (x << (32 - n));
}

function sha256Hex(bytes: Uint8Array): string {
  const h = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);

  const l = bytes.length;
  const blocks = ((l + 8) >> 6) + 1; // 512-bit blocks after 0x80 + 64-bit length
  const padded = new Uint8Array(blocks * 64);
  padded.set(bytes);
  padded[l] = 0x80;
  const dv = new DataView(padded.buffer);
  const bitLen = l * 8;
  dv.setUint32(padded.length - 8, Math.floor(bitLen / 0x100000000));
  dv.setUint32(padded.length - 4, bitLen >>> 0);

  const w = new Uint32Array(64);
  for (let i = 0; i < padded.length; i += 64) {
    for (let t = 0; t < 16; t++) w[t] = dv.getUint32(i + t * 4);
    for (let t = 16; t < 64; t++) {
      const s0 = rotr(w[t - 15], 7) ^ rotr(w[t - 15], 18) ^ (w[t - 15] >>> 3);
      const s1 = rotr(w[t - 2], 17) ^ rotr(w[t - 2], 19) ^ (w[t - 2] >>> 10);
      w[t] = (w[t - 16] + s0 + w[t - 7] + s1) >>> 0;
    }
    let a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5], g = h[6], hh = h[7];
    for (let t = 0; t < 64; t++) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + S1 + ch + K[t] + w[t]) >>> 0;
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + b) >>> 0; h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0; h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0;
  }

  let hex = '';
  for (let i = 0; i < 8; i++) hex += h[i].toString(16).padStart(8, '0');
  return hex;
}
