/**
 * Drives the shared codec vectors through this port: typed nodes + the embedded
 * schema -> canonical form + content hash + normalized-JSON serialize. Runs the
 * 0.1.0 contract file (codec-vectors.json) and the 0.2.0 embedded-values file
 * (codec-vectors-embedded.json). Run: `npm run conformance`.
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

let totalFails = 0;

function runFile(relative: string): void {
  const vfile = new URL(`../../vectors/${relative}`, import.meta.url);
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
  console.log(`${relative}: ${pass}/${d.cases.length} pass`);
  totalFails += fails;
}

runFile('codec-vectors.json');
runFile('codec-vectors-embedded.json');

if (totalFails > 0) { console.error(`\n${totalFails} FAILURES`); process.exit(1); }
console.log('\nALL VECTORS PASS');
