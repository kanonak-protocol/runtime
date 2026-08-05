/**
 * Drives the shared expression parity vectors through this port. Each vector's
 * `expr` is evaluated with a `resolve` hook that binds `tx.VarRef` names from the
 * vector's `env` — the demonstration that variable binding lives in the caller,
 * not the runtime. Ordered-comparison vectors additionally supply `closures`
 * (the ClosureTable) and `refEnv` (identity bindings for the `resolveRef` hook).
 * Every vector also runs through `explain` and its root value must agree with
 * `evaluate` — the guarantee that the two entry points cannot drift; vectors
 * with a `trace` assert the verdict tree structurally. Run: `npm run conformance`.
 */
import { readFileSync } from 'node:fs';
import {
  evaluate,
  explain,
  ExpressionError,
  type ClosureTable,
  type EvalOptions,
  type ExprNode,
  type TraceNode,
} from './index.js';

const VARREF = 'kanonak.org/transformations/VarRef';

interface Vector {
  id: string;
  expr: ExprNode;
  env?: Record<string, number>;
  refEnv?: Record<string, string>;
  closures?: ClosureTable;
  expected?: number;
  tolerance?: number;
  expectError?: boolean;
  trace?: TraceNode;
}

interface Ctx {
  env: Record<string, number>;
  refEnv: Record<string, string>;
}

const vfile = new URL('../../vectors/expression-vectors.json', import.meta.url);
const data = JSON.parse(readFileSync(vfile, 'utf8')) as { vectors: Vector[] };

// The caller's resolve: tx.VarRef -> env binding; any other leaf is unbound here.
const resolve = (node: ExprNode, ctx: Ctx): number => {
  if (node.type === VARREF) {
    const name = node.varName as string;
    if (!(name in ctx.env)) throw new ExpressionError(`Unbound variable "${name}"`);
    return ctx.env[name];
  }
  throw new ExpressionError(`No resolver for leaf '${node.type}'`);
};

// The caller's resolveRef: tx.VarRef -> refEnv member URI. Same division as
// resolve — the kernel owns UriLiteral, the caller owns bindings.
const resolveRef = (node: ExprNode, ctx: Ctx): string => {
  if (node.type === VARREF) {
    const name = node.varName as string;
    if (!(name in ctx.refEnv)) throw new ExpressionError(`Unbound reference "${name}"`);
    return ctx.refEnv[name];
  }
  throw new ExpressionError(`No reference resolver for leaf '${node.type}'`);
};

/** Structural equality of verdict trees, including absent-vs-present refs. */
function traceEqual(a: TraceNode, b: TraceNode): boolean {
  if (a.type !== b.type || a.value !== b.value) return false;
  if ((a.leftRef ?? null) !== (b.leftRef ?? null)) return false;
  if ((a.rightRef ?? null) !== (b.rightRef ?? null)) return false;
  if (a.children.length !== b.children.length) return false;
  return a.children.every((c, i) => traceEqual(c, b.children[i]));
}

let pass = 0;
let fail = 0;
for (const v of data.vectors) {
  const ctx: Ctx = { env: v.env ?? {}, refEnv: v.refEnv ?? {} };
  const options: EvalOptions<Ctx> = { closures: v.closures, resolveRef };
  if (v.expectError) {
    let evalThrew = false;
    let explainThrew = false;
    try { evaluate(v.expr, ctx, resolve, options); } catch { evalThrew = true; }
    try { explain(v.expr, ctx, resolve, options); } catch { explainThrew = true; }
    if (evalThrew && explainThrew) pass++;
    else { fail++; console.error(`${v.id}: expected an error from evaluate AND explain`); }
    continue;
  }
  let got: number;
  let trace: TraceNode;
  try {
    got = evaluate(v.expr, ctx, resolve, options);
    trace = explain(v.expr, ctx, resolve, options);
  } catch (e) {
    fail++; console.error(`${v.id}: threw ${(e as Error).message}`); continue;
  }
  const ok = v.tolerance !== undefined
    ? Math.abs(got - (v.expected as number)) <= v.tolerance
    : got === v.expected;
  if (!ok) {
    fail++; console.error(`${v.id}: expected ${v.expected} got ${got}`); continue;
  }
  if (trace.value !== got) {
    fail++; console.error(`${v.id}: explain value ${trace.value} != evaluate value ${got}`); continue;
  }
  if (v.trace && !traceEqual(trace, v.trace)) {
    fail++; console.error(`${v.id}: trace mismatch\n  expected ${JSON.stringify(v.trace)}\n  got      ${JSON.stringify(trace)}`); continue;
  }
  pass++;
}

console.log(`expression-vectors: ${pass}/${data.vectors.length} pass`);
if (fail > 0) { console.error(`\n${fail} FAILURES`); process.exit(1); }
console.log('ALL VECTORS PASS');
