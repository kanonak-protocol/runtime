/**
 * Drives the shared codec vectors through this port: typed nodes + the embedded
 * schema -> canonical form + content hash + normalized-JSON serialize. Run:
 * `npm run conformance`.
 */
import { readFileSync } from 'node:fs';
import {
  packageCanonicalForm,
  packageContentHash,
  serialize,
  type CodecSchema,
  type CodecNode,
  type PackageContext,
} from './index.js';

const vfile = new URL('../../vectors/codec-vectors.json', import.meta.url);
const d: any = JSON.parse(readFileSync(vfile, 'utf8'));
const schema: CodecSchema = d.schema;

let fails = 0;
let pass = 0;
for (const c of d.cases) {
  const nodes: CodecNode[] = c.nodes;
  const pkg: PackageContext = c.pkg;
  const form = packageCanonicalForm(nodes, schema, pkg);
  const hash = packageContentHash(nodes, schema, pkg);
  const ser = nodes.map((n) => serialize(n));

  const formOk = form === c.expectedCanonicalForm;
  const hashOk = hash === c.expectedHash;
  const serOk = JSON.stringify(ser) === JSON.stringify(c.expectedSerialize);

  if (formOk && hashOk && serOk) {
    pass++;
  } else {
    fails++;
    if (!formOk) console.error(`${c.id}: canonical form mismatch\n  expected ${c.expectedCanonicalForm}\n  got      ${form}`);
    if (!hashOk) console.error(`${c.id}: hash expected ${c.expectedHash} got ${hash}`);
    if (!serOk) console.error(`${c.id}: serialize mismatch\n  expected ${JSON.stringify(c.expectedSerialize)}\n  got      ${JSON.stringify(ser)}`);
  }
}
console.log(`codec-vectors: ${pass}/${d.cases.length} pass`);

if (fails > 0) { console.error(`\n${fails} FAILURES`); process.exit(1); }
console.log('\nALL VECTORS PASS');
