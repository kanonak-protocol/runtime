/**
 * `@kanonak-protocol/expression` — the Kanonak expression RUNTIME.
 *
 * A small, deterministic tree-walker that folds a `kanonak.org/transformations`
 * (`tx`) + `kanonak.org/math` expression tree to a single number. Generated SDKs
 * reference it so a typed expression can be *run*, not just *represented*.
 *
 * Three layers, exactly as the six-language proof established:
 *
 *   1. DISPATCH — derived from the ontology. An operator's arity falls out of its
 *      `tx` superclass: UnaryNumericOp -> unary `value`; BinaryArithmetic /
 *      BinaryComparison -> binary; BooleanLogic -> n-ary `operands`; plus the two
 *      structural shapes the hierarchy can't imply (`Not`'s `operand`, `Clip`'s
 *      ternary). The OPERATOR_ARITY table below is that derivation, frozen.
 *
 *   2. PRIMITIVES — the one authored, determinism-bearing artifact. PRIMITIVES
 *      maps each operator URI to its fold. Determinism traps live here and are
 *      matched in every language port (Round half-away-from-zero, floored Modulo,
 *      Sign(0)=0, comparisons as 1/0).
 *
 *   3. THE FOLD — `evaluate`, a fixed shape: operators recurse + apply a
 *      primitive; literals return their numeric value; EVERYTHING ELSE (a typed
 *      VarRef, a domain `Step`/`Time`/`Smooth`, any future leaf) is handed to the
 *      caller's `resolve(node, ctx, evaluate)`. The runtime is a pure operator
 *      engine; binding and domain-leaf semantics are the caller's business. It
 *      never privileges `tx.VarRef` — that is just one leaf a domain may resolve.
 *
 * Value domain: uniform numeric. Booleans and comparison results are `1`/`0`, so
 * every language stays on one numeric path. `EXPRESSION_RUNTIME_VERSION` freezes
 * the determinism contract; a change to any primitive, value rule, or dispatch
 * requires a NEW version, never an edit in place.
 */

/** The frozen expression-runtime version (determinism contract). Not hashed. */
export const EXPRESSION_RUNTIME_VERSION = '1';

const TX = 'kanonak.org/transformations';
const MATH = 'kanonak.org/math';

/** A node in the expression tree. `type` is the operator/literal/leaf canonical
 * URI (versionless: `publisher/package/name`); operand keys are the frozen `tx`
 * operand property local names. Unknown fields are ignored by the kernel and are
 * available to `resolve` for domain leaves. */
export interface ExprNode {
  type: string;
  [operandOrValue: string]: unknown;
}

/** Resolve any node the kernel does not recognise as an operator or literal — a
 * binding (`tx.VarRef`, a domain's typed `refersTo` VarRef) or a domain leaf
 * (`Step`, `Time`, `Smooth`…) — to a number. `ctx` is opaque caller state (the
 * binding env, a sim clock, integration state). `evaluate` is handed back so a
 * domain leaf containing sub-expressions can recurse into the kernel. */
export type Resolve<C> = (node: ExprNode, ctx: C, evaluate: (n: ExprNode, ctx: C) => number) => number;

/**
 * The transitive closures ordered comparisons consult, keyed by the ordering
 * property's canonical URI, then by member: `closures[property][from]` is the
 * set of members `from` reaches. Flat, already-closed data — typically the
 * SDK reasoner's `prp-trp` saturation emitted at code-generation time. The
 * kernel does set membership only; it NEVER computes a closure, resolves a
 * package, or reasons. Supplying the closure is the caller's business, the
 * same division that keeps variable binding out of the kernel.
 */
export type ClosureTable = Record<string, Record<string, readonly string[]>>;

/** Resolve an identity leaf inside an ordered comparison — any operand node
 * that is not a `tx.UriLiteral` — to a member's canonical versionless URI
 * (`publisher/package/name`). The identity-domain mirror of {@link Resolve}:
 * the kernel owns the constant leaf, the caller owns bindings. */
export type ResolveRef<C> = (node: ExprNode, ctx: C) => string;

/** Optional evaluation context for the ordered comparisons (`IsAtLeast`,
 * `Dominates`). Absent (or missing a needed entry), an ordered comparison
 * fails loudly — never a silent false from a missing table. */
export interface EvalOptions<C> {
  closures?: ClosureTable;
  resolveRef?: ResolveRef<C>;
}

export class ExpressionError extends Error {}

/** Operand shape per operator, derived from the `tx` superclass hierarchy. */
type Arity =
  | { kind: 'unary'; operand: string }
  | { kind: 'binary'; left: string; right: string }
  | { kind: 'nary'; operands: string }
  | { kind: 'ternary'; a: string; b: string; c: string };

const UN = (operand: string): Arity => ({ kind: 'unary', operand });
const BIN = (left: string, right: string): Arity => ({ kind: 'binary', left, right });

// UnaryNumericOp + Not -> `value` / `operand`; BinaryArithmetic -> arithLeft/Right;
// BinaryComparison -> compareLeft/Right; BooleanLogic -> operands list; Clip ternary.
const ARITH = BIN('arithLeft', 'arithRight');
const COMPARE = BIN('compareLeft', 'compareRight');
const VALUE = UN('value');

const OPERATOR_ARITY: Record<string, Arity> = {
  [`${TX}/Add`]: ARITH,
  [`${TX}/Subtract`]: ARITH,
  [`${TX}/Multiply`]: ARITH,
  [`${TX}/Divide`]: ARITH,
  [`${MATH}/Power`]: ARITH,
  [`${MATH}/Modulo`]: ARITH,
  [`${MATH}/Minimum`]: ARITH,
  [`${MATH}/Maximum`]: ARITH,

  [`${TX}/Abs`]: VALUE,
  [`${TX}/Negate`]: VALUE,
  [`${MATH}/Exp`]: VALUE,
  [`${MATH}/Ln`]: VALUE,
  [`${MATH}/Log10`]: VALUE,
  [`${MATH}/Sqrt`]: VALUE,
  [`${MATH}/Floor`]: VALUE,
  [`${MATH}/Ceil`]: VALUE,
  [`${MATH}/Round`]: VALUE,
  [`${MATH}/Sign`]: VALUE,

  [`${TX}/Equals`]: COMPARE,
  [`${TX}/GreaterThan`]: COMPARE,
  [`${TX}/LessThan`]: COMPARE,
  [`${TX}/GreaterThanOrEqual`]: COMPARE,
  [`${TX}/LessThanOrEqual`]: COMPARE,

  [`${TX}/And`]: { kind: 'nary', operands: 'operands' },
  [`${TX}/Or`]: { kind: 'nary', operands: 'operands' },
  // `Not` is a direct Expression subclass with boolean (not numeric-unary)
  // semantics — handled explicitly in `evaluate`, not via the numeric tables.

  [`${MATH}/Clip`]: { kind: 'ternary', a: 'clipValue', b: 'clipLower', c: 'clipUpper' },
};

/** Floored modulo (the host `%` truncates toward zero): Modulo(-7,3) = 2. */
function flooredMod(a: number, b: number): number {
  if (b === 0) throw new ExpressionError('Modulo by zero');
  return a - b * Math.floor(a / b);
}

/** Round half away from zero: Round(-2.5) = -3, Round(2.5) = 3. */
function roundHalfAway(a: number): number {
  return a < 0 ? -Math.round(-a) : Math.round(a);
}

const truthy = (n: number): boolean => n !== 0;
const bool = (b: boolean): number => (b ? 1 : 0);

function requireDomain(ok: boolean, msg: string): void {
  if (!ok) throw new ExpressionError(msg);
}

/** Unary/binary/ternary primitives, keyed by operator URI. The authored,
 * determinism-bearing table — matched per language. */
const UNARY: Record<string, (x: number) => number> = {
  [`${TX}/Abs`]: (x) => Math.abs(x),
  [`${TX}/Negate`]: (x) => -x,
  [`${MATH}/Exp`]: (x) => Math.exp(x),
  [`${MATH}/Ln`]: (x) => (requireDomain(x > 0, 'Ln of a non-positive number'), Math.log(x)),
  [`${MATH}/Log10`]: (x) => (requireDomain(x > 0, 'Log10 of a non-positive number'), Math.log10(x)),
  [`${MATH}/Sqrt`]: (x) => (requireDomain(x >= 0, 'Sqrt of a negative number'), Math.sqrt(x)),
  [`${MATH}/Floor`]: (x) => Math.floor(x),
  [`${MATH}/Ceil`]: (x) => Math.ceil(x),
  [`${MATH}/Round`]: roundHalfAway,
  [`${MATH}/Sign`]: (x) => Math.sign(x),
};

const BINARY: Record<string, (a: number, b: number) => number> = {
  [`${TX}/Add`]: (a, b) => a + b,
  [`${TX}/Subtract`]: (a, b) => a - b,
  [`${TX}/Multiply`]: (a, b) => a * b,
  [`${TX}/Divide`]: (a, b) => (requireDomain(b !== 0, 'Divide by zero'), a / b),
  [`${MATH}/Power`]: (a, b) => Math.pow(a, b),
  [`${MATH}/Modulo`]: flooredMod,
  [`${MATH}/Minimum`]: (a, b) => Math.min(a, b),
  [`${MATH}/Maximum`]: (a, b) => Math.max(a, b),
  [`${TX}/Equals`]: (a, b) => bool(a === b),
  [`${TX}/GreaterThan`]: (a, b) => bool(a > b),
  [`${TX}/LessThan`]: (a, b) => bool(a < b),
  [`${TX}/GreaterThanOrEqual`]: (a, b) => bool(a >= b),
  [`${TX}/LessThanOrEqual`]: (a, b) => bool(a <= b),
};

/** Numeric value of a literal node, or `undefined` if it is not a literal. */
function literalValue(node: ExprNode): number | undefined {
  switch (node.type) {
    case `${TX}/IntegerLiteral`: return Number(node.integerLiteral);
    case `${TX}/DecimalLiteral`: return Number(node.decimalLiteral);
    case `${TX}/BooleanLiteral`: return bool(node.booleanLiteral === true || node.booleanLiteral === 'true');
    default: return undefined;
  }
}

/**
 * The identity an ordered comparison compares — a member's canonical
 * versionless URI. `tx.UriLiteral` is the kernel-known constant leaf (its
 * `refTo` IS the identity, the way a literal's value is its number); every
 * other node is the caller's, through `resolveRef` — the same division as the
 * numeric domain's literals-vs-`resolve`.
 */
function identityOf<C>(node: ExprNode, ctx: C, options: EvalOptions<C> | undefined): string {
  if (node.type === `${TX}/UriLiteral`) {
    const ref = node.refTo;
    if (typeof ref !== 'string' || ref.length === 0) {
      throw new ExpressionError('UriLiteral is missing refTo');
    }
    return ref;
  }
  if (!options?.resolveRef) {
    throw new ExpressionError(`No resolveRef supplied for identity leaf '${node.type}'`);
  }
  return options.resolveRef(node, ctx);
}

/**
 * Fold an ordered comparison (`IsAtLeast` / `Dominates`) to `1`/`0`.
 *
 * The ordering is the supplied closure for the node's `viaProperty` —
 * membership in already-closed data, nothing more. Identity is canonical
 * versionless URI string equality, matching `tx.Equals`' identity rule.
 * `IsAtLeast` folds reflexivity into the operator (same member → 1) so a
 * strictly-ordered vocabulary need not declare itself reflexive; `Dominates`
 * is strict (same member → 0). Two members with no path yield 0 — the
 * fail-closed direction for a predicate — but a MISSING closure table is a
 * configuration failure and errors loudly.
 */
function foldOrdered<C>(
  node: ExprNode,
  ctx: C,
  options: EvalOptions<C> | undefined,
): { value: number; left: string; right: string } {
  const via = node.viaProperty;
  if (typeof via !== 'string' || via.length === 0) {
    throw new ExpressionError(`${node.type} is missing viaProperty`);
  }
  const left = identityOf(operand(node, 'compareLeft'), ctx, options);
  const right = identityOf(operand(node, 'compareRight'), ctx, options);
  const closure = options?.closures?.[via];
  if (!closure) {
    throw new ExpressionError(`No closure supplied for ordering property '${via}'`);
  }
  const value = left === right
    ? bool(node.type === `${TX}/IsAtLeast`)
    : bool((closure[left] ?? []).includes(right));
  return { value, left, right };
}

/**
 * Evaluate an expression tree to a number. Operators fold via the frozen
 * dispatch + primitive tables; literals yield their numeric value; any other
 * node is delegated to `resolve`. `options` carries the ordered-comparison
 * context (closures + identity-leaf resolution) and is only consulted when an
 * `IsAtLeast` / `Dominates` node is reached.
 */
export function evaluate<C = unknown>(
  node: ExprNode,
  ctx: C,
  resolve: Resolve<C>,
  options?: EvalOptions<C>,
): number {
  const recurse = (n: ExprNode, c: C): number => evaluate(n, c, resolve, options);

  const arity = OPERATOR_ARITY[node.type];
  if (arity) {
    switch (arity.kind) {
      case 'unary': {
        const x = recurse(operand(node, arity.operand), ctx);
        return UNARY[node.type](x);
      }
      case 'binary': {
        const a = recurse(operand(node, arity.left), ctx);
        const b = recurse(operand(node, arity.right), ctx);
        return BINARY[node.type](a, b);
      }
      case 'nary': {
        const items = node[arity.operands];
        if (!Array.isArray(items)) throw new ExpressionError(`${node.type} expects an '${arity.operands}' list`);
        const isAnd = node.type === `${TX}/And`;
        // Short-circuit; empty And is vacuously true, empty Or vacuously false.
        for (const item of items) {
          const v = truthy(recurse(item as ExprNode, ctx));
          if (isAnd && !v) return 0;
          if (!isAnd && v) return 1;
        }
        return bool(isAnd);
      }
      case 'ternary': {
        // Only Clip today: clamp clipValue into [clipLower, clipUpper].
        const v = recurse(operand(node, arity.a), ctx);
        const lo = recurse(operand(node, arity.b), ctx);
        const hi = recurse(operand(node, arity.c), ctx);
        return Math.min(Math.max(v, lo), hi);
      }
    }
  }

  if (node.type === `${TX}/Not`) {
    return bool(!truthy(recurse(operand(node, 'operand'), ctx)));
  }

  if (node.type === `${TX}/IsAtLeast` || node.type === `${TX}/Dominates`) {
    return foldOrdered(node, ctx, options).value;
  }

  const lit = literalValue(node);
  if (lit !== undefined) return lit;

  // Not an operator or literal — a binding or domain leaf. The caller owns it.
  return resolve(node, ctx, recurse);
}

function operand(node: ExprNode, key: string): ExprNode {
  const v = node[key];
  if (v === null || v === undefined || typeof v !== 'object') {
    throw new ExpressionError(`${node.type} is missing operand '${key}'`);
  }
  return v as ExprNode;
}

/**
 * One node of an evaluation trace — the verdict tree {@link explain} returns.
 *
 * Mirrors the expression: `type` is the node's type URI, `value` its result
 * (`1`/`0` for booleans), `children` the operand traces in evaluation order.
 * A short-circuited operand is simply ABSENT from `children` — the trace is
 * truthful about what ran. Ordered comparisons carry their resolved operand
 * identities as `leftRef`/`rightRef` instead of children, since their
 * operands are identities rather than numeric folds.
 *
 * This is a runtime return shape, not an ontology class: nothing authors a
 * trace, so nothing about it is modelled or serialized as vocabulary.
 */
export interface TraceNode {
  type: string;
  value: number;
  children: TraceNode[];
  leftRef?: string;
  rightRef?: string;
}

/**
 * Evaluate an expression tree and return the verdict tree — the regex-debugger
 * view: every evaluated node, its own result, and (for ordered comparisons)
 * the identities it compared. The root's `value` is exactly what
 * {@link evaluate} returns for the same inputs; the conformance suite runs
 * every vector through both and requires agreement, so the two entry points
 * cannot drift. Kept separate from `evaluate` so the hot path never pays for
 * trace allocation.
 *
 * Errors propagate exactly as in `evaluate` — a failed evaluation yields an
 * error, not a partial trace.
 */
export function explain<C = unknown>(
  node: ExprNode,
  ctx: C,
  resolve: Resolve<C>,
  options?: EvalOptions<C>,
): TraceNode {
  // Numeric recursion for subtrees the caller's `resolve` re-enters: those
  // folds happen inside the caller and are invisible to the trace, exactly
  // like the caller's own computation. Only kernel-visited nodes appear.
  const recurseValue = (n: ExprNode, c: C): number => evaluate(n, c, resolve, options);

  const trace = (n: ExprNode, c: C): TraceNode => {
    const arity = OPERATOR_ARITY[n.type];
    if (arity) {
      switch (arity.kind) {
        case 'unary': {
          const x = trace(operand(n, arity.operand), c);
          return { type: n.type, value: UNARY[n.type](x.value), children: [x] };
        }
        case 'binary': {
          const a = trace(operand(n, arity.left), c);
          const b = trace(operand(n, arity.right), c);
          return { type: n.type, value: BINARY[n.type](a.value, b.value), children: [a, b] };
        }
        case 'nary': {
          const items = n[arity.operands];
          if (!Array.isArray(items)) throw new ExpressionError(`${n.type} expects an '${arity.operands}' list`);
          const isAnd = n.type === `${TX}/And`;
          const children: TraceNode[] = [];
          for (const item of items) {
            const child = trace(item as ExprNode, c);
            children.push(child);
            const v = truthy(child.value);
            // Same short-circuit as `evaluate`: operands after the deciding
            // one are never evaluated and never appear in the trace.
            if (isAnd && !v) return { type: n.type, value: 0, children };
            if (!isAnd && v) return { type: n.type, value: 1, children };
          }
          return { type: n.type, value: bool(isAnd), children };
        }
        case 'ternary': {
          const v = trace(operand(n, arity.a), c);
          const lo = trace(operand(n, arity.b), c);
          const hi = trace(operand(n, arity.c), c);
          return {
            type: n.type,
            value: Math.min(Math.max(v.value, lo.value), hi.value),
            children: [v, lo, hi],
          };
        }
      }
    }

    if (n.type === `${TX}/Not`) {
      const x = trace(operand(n, 'operand'), c);
      return { type: n.type, value: bool(!truthy(x.value)), children: [x] };
    }

    if (n.type === `${TX}/IsAtLeast` || n.type === `${TX}/Dominates`) {
      const r = foldOrdered(n, c, options);
      return { type: n.type, value: r.value, children: [], leftRef: r.left, rightRef: r.right };
    }

    const lit = literalValue(n);
    if (lit !== undefined) return { type: n.type, value: lit, children: [] };

    return { type: n.type, value: resolve(n, c, recurseValue), children: [] };
  };

  return trace(node, ctx);
}
