/**
 * Drives the shared golden vectors through this port: the lexical vectors
 * (carrier + raw -> canonical lexical) and the full-form vectors (CanonicalInput
 * -> canonical form + content hash). Run: `npm run conformance`.
 */
import { readFileSync } from 'node:fs';
import { canonicalForm, canonicalHash } from './index.js';
import { canonicalScalarLexical, type Carrier } from './Datatypes.js';
import {
  parseCoordinate,
  versionlessKey,
  localName,
  lenientVersionlessKey,
} from './Coordinate.js';

const vdir = new URL('../../vectors/', import.meta.url);
const read = (f: string): any => JSON.parse(readFileSync(new URL(f, vdir), 'utf8'));

let fails = 0;

const lx = read('lexical-vectors.json');
let lpass = 0;
for (const v of lx.vectors) {
  if (v.expectError) {
    // Negative case: a valid implementation must reject the malformed lexical.
    try {
      const got = canonicalScalarLexical(v.carrier as Carrier, v.input);
      fails++; console.error(`lexical ${v.id}: expected an error, got ${JSON.stringify(got)}`);
    } catch { lpass++; }
    continue;
  }
  let got: string;
  try {
    got = canonicalScalarLexical(v.carrier as Carrier, v.input);
  } catch (e) {
    fails++; console.error(`lexical ${v.id}: threw ${(e as Error).message}`); continue;
  }
  if (got === v.expected) lpass++;
  else { fails++; console.error(`lexical ${v.id}: expected ${JSON.stringify(v.expected)} got ${JSON.stringify(got)}`); }
}
console.log(`lexical-vectors: ${lpass}/${lx.vectors.length} pass`);

const ff = read('full-form-vectors.json');
let fpass = 0;
for (const v of ff.vectors) {
  const form = canonicalForm(v.input);
  const hash = canonicalHash(v.input);
  const ok = form === v.expectedCanonicalForm && hash === v.expectedHash;
  if (ok) fpass++;
  else {
    fails++;
    if (form !== v.expectedCanonicalForm) {
      console.error(`full-form ${v.id}: canonical form mismatch\n  expected ${v.expectedCanonicalForm}\n  got      ${form}`);
    }
    if (hash !== v.expectedHash) console.error(`full-form ${v.id}: hash expected ${v.expectedHash} got ${hash}`);
  }
}
console.log(`full-form-vectors: ${fpass}/${ff.vectors.length} pass`);

const co = read('coordinate-vectors.json');
let cpass = 0;
for (const v of co.parseVectors) {
  if (v.expectError) {
    try {
      const got = parseCoordinate(v.input);
      fails++; console.error(`coordinate ${v.id}: expected an error, got ${JSON.stringify(got)}`);
    } catch { cpass++; }
    continue;
  }
  try {
    const got = parseCoordinate(v.input);
    const e = v.expected;
    const versionOk = e.version === null
      ? got.version === undefined
      : got.version !== undefined
        && got.version.major === e.version.major
        && got.version.minor === e.version.minor
        && got.version.patch === e.version.patch;
    const expectedKey = `${e.publisher}/${e.package}/${e.name}`;
    const ok = got.publisher === e.publisher
      && got.package_ === e.package
      && got.name === e.name
      && versionOk
      && versionlessKey(v.input) === expectedKey
      && localName(v.input) === e.name;
    if (ok) cpass++;
    else { fails++; console.error(`coordinate ${v.id}: expected ${JSON.stringify(e)} got ${JSON.stringify(got)}`); }
  } catch (e) {
    fails++; console.error(`coordinate ${v.id}: threw ${(e as Error).message}`);
  }
}
for (const v of co.lenientKeyVectors) {
  const got = lenientVersionlessKey(v.input) ?? null;
  if (got === v.expected) cpass++;
  else { fails++; console.error(`coordinate ${v.id}: expected ${JSON.stringify(v.expected)} got ${JSON.stringify(got)}`); }
}
console.log(`coordinate-vectors: ${cpass}/${co.parseVectors.length + co.lenientKeyVectors.length} pass`);

if (fails > 0) { console.error(`\n${fails} FAILURES`); process.exit(1); }
console.log('\nALL VECTORS PASS');
