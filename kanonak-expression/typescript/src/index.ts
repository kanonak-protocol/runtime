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
 * Evaluate an expression tree to a number. Operators fold via the frozen
 * dispatch + primitive tables; literals yield their numeric value; any other
 * node is delegated to `resolve`.
 */
export function evaluate<C = unknown>(node: ExprNode, ctx: C, resolve: Resolve<C>): number {
  const recurse = (n: ExprNode, c: C): number => evaluate(n, c, resolve);

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
