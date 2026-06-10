/**
 * Drives the shared expression parity vectors through this port. Each vector's
 * `expr` is evaluated with a `resolve` hook that binds `tx.VarRef` names from the
 * vector's `env` — the demonstration that variable binding lives in the caller,
 * not the runtime. Run: `npm run conformance`.
 */
import { readFileSync } from 'node:fs';
import { evaluate, ExpressionError, type ExprNode } from './index.js';

const VARREF = 'kanonak.org/transformations/VarRef';

interface Vector {
  id: string;
  expr: ExprNode;
  env?: Record<string, number>;
  expected?: number;
  tolerance?: number;
  expectError?: boolean;
}

const vfile = new URL('../../vectors/expression-vectors.json', import.meta.url);
const data = JSON.parse(readFileSync(vfile, 'utf8')) as { vectors: Vector[] };

// The caller's resolve: tx.VarRef -> env binding; any other leaf is unbound here.
const resolve = (node: ExprNode, env: Record<string, number>): number => {
  if (node.type === VARREF) {
    const name = node.varName as string;
    if (!(name in env)) throw new ExpressionError(`Unbound variable "${name}"`);
    return env[name];
  }
  throw new ExpressionError(`No resolver for leaf '${node.type}'`);
};

let pass = 0;
let fail = 0;
for (const v of data.vectors) {
  const env = v.env ?? {};
  if (v.expectError) {
    try {
      evaluate(v.expr, env, resolve);
      fail++; console.error(`${v.id}: expected an error, got a value`);
    } catch {
      pass++;
    }
    continue;
  }
  let got: number;
  try {
    got = evaluate(v.expr, env, resolve);
  } catch (e) {
    fail++; console.error(`${v.id}: threw ${(e as Error).message}`); continue;
  }
  const ok = v.tolerance !== undefined
    ? Math.abs(got - (v.expected as number)) <= v.tolerance
    : got === v.expected;
  if (ok) pass++;
  else { fail++; console.error(`${v.id}: expected ${v.expected} got ${got}`); }
}

console.log(`expression-vectors: ${pass}/${data.vectors.length} pass`);
if (fail > 0) { console.error(`\n${fail} FAILURES`); process.exit(1); }
console.log('ALL VECTORS PASS');
