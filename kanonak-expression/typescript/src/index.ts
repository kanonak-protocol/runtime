/**
 * `@kanonak-protocol/expression` — the Kanonak expression RUNTIME.
 *
 * A small, deterministic tree-walker that folds a `kanonak.org/transformations`
 * (`tx`) + `kanonak.org/math` expression tree to a value. Generated SDKs
 * reference it so a typed expression can be *run*, not just *represented*.
 *
 * Three layers, exactly as the six-language proof established:
 *
 *   1. DISPATCH — derived from the ontology. An operator's shape falls out of
 *      its `tx` superclass: UnaryNumericOp -> unary `value`; BinaryArithmetic /
 *      BinaryComparison -> binary; BooleanLogic -> n-ary `operands`;
 *      ListSourcedExpression / ListAggregate -> `source`; IteratingExpression
 *      -> `source` + `loopVar` + a body; plus the structural shapes the
 *      hierarchy can't imply (`Not`'s `operand`, `Clip`'s ternary, `Contains`'
 *      haystack/needle, `Matches`' matchSource/pattern). The tables below are
 *      that derivation, frozen.
 *
 *   2. PRIMITIVES — the authored, determinism-bearing artifact. Tables map
 *      each operator URI to its fold. Determinism traps live here and are
 *      matched in every language port (Round half-away-from-zero, floored
 *      Modulo, Sign(0)=0, comparisons as 1/0, RFC 8785-style number
 *      stringification in Join, the pinned Matches regex subset).
 *
 *   3. THE FOLD — `evaluate`, a fixed shape: operators recurse + apply a
 *      primitive; literals return their value; EVERYTHING ELSE (a typed
 *      VarRef, a domain `Step`/`Time`/`Smooth`, a host graph read, any future
 *      leaf) is handed to the caller's `resolve(node, ctx, evaluate)`. The
 *      runtime is a pure operator engine; binding, graph access, and
 *      domain-leaf semantics are the caller's business. A property read is
 *      just a caller leaf that returns a list — the kernel NEVER touches a
 *      graph.
 *
 * VALUE DOMAIN (expressionRuntimeVersion "2"): `number | string | ref | list`.
 *
 *   - Booleans and comparison results remain `1`/`0` numbers (the v1 rule).
 *   - A `ref` is a canonical versionless URI identity (`{ ref: string }`),
 *     compared by URI-key equality — the same identity rule the ordered
 *     comparisons and `tx.Equals` share.
 *   - A `list` is an ordered sequence of values.
 *   - There is NO null/undefined: ABSENCE IS THE EMPTY LIST. A caller leaf
 *     for a property with no values returns `[]`; `IsSet` distinguishes.
 *
 * ERROR CONTRACT (v2 extension of the v1 fail-loud discipline):
 *
 *   - COMPUTATIONS FAIL LOUD: an arithmetic/unary-numeric/boolean-logic
 *     operator whose operand is not a number, an aggregate over a non-numeric
 *     element, `Min`/`Max`/`Average` on an empty list, a nested list in
 *     `Join`, a non-string `Matches` source, an out-of-subset `Matches`
 *     pattern — all raise `ExpressionError`, never coerce.
 *   - PREDICATES FAIL CLOSED: `Equals` and the ordering comparisons on
 *     cross-kind or non-numeric operands return `0`, never error (the SHACL
 *     and reference-engine rule: a non-comparable value is a falsified
 *     constraint, not a crash).
 *
 * v1 COMPATIBILITY: the value domain is a strict superset and no numeric
 * primitive changed — every v1 parity vector passes unchanged under v2. The
 * conformance harness runs the v1 vector file through this kernel as the
 * regression gate.
 *
 * LAMBDA BINDING (the one genuinely-new dispatch shape): the iterating
 * operators (`Filter`, `ListMap`, `ForEach`) bind their `loopVar` per element.
 * Within an iterating operator's body — and ONLY there — a `tx.VarRef` whose
 * `varName` names a lexically-enclosing `loopVar` is resolved by the kernel to
 * the bound element (innermost binder wins). Every other leaf, including a
 * `VarRef` naming anything else, still goes to the caller's `resolve`. This is
 * the scoped exception to "the kernel never privileges VarRef": VarRef is the
 * bound-variable mechanism of the kernel's own binders, nothing more. The
 * remaining iterating family (WindowedMap, PairwiseMap, Scan, DistinctBy,
 * PartitionBy) is additive within v2 under this same reserved shape.
 *
 * `EXPRESSION_RUNTIME_VERSION` freezes the determinism contract; a change to
 * any primitive, value rule, or dispatch entry requires a NEW version, never
 * an edit in place. Adding an operator, or widening the Matches subset (an
 * error becoming a defined result), is additive WITHIN a version.
 */

/** The frozen expression-runtime version (determinism contract). Not hashed. */
export const EXPRESSION_RUNTIME_VERSION = '2';

const TX = 'kanonak.org/transformations';
const MATH = 'kanonak.org/math';

/**
 * A reference value — a member's canonical versionless URI identity
 * (`publisher/package/name`). Distinct from a string so `Equals` can hold the
 * cross-kind-is-false rule: a URI is never equal to the string spelling of it.
 */
export interface RefValue {
  ref: string;
}

/**
 * The v2 value domain. Booleans are `1`/`0` numbers; absence is the empty
 * list; there is no null.
 */
export type Value = number | string | RefValue | Value[];

/** Discriminate a `RefValue` from the other object shape (arrays). */
export function isRef(v: Value): v is RefValue {
  return typeof v === 'object' && !Array.isArray(v);
}

/** A node in the expression tree. `type` is the operator/literal/leaf canonical
 * URI (versionless: `publisher/package/name`); operand keys are the frozen `tx`
 * operand property local names. Unknown fields are ignored by the kernel and are
 * available to `resolve` for domain leaves. */
export interface ExprNode {
  type: string;
  [operandOrValue: string]: unknown;
}

/** Resolve any node the kernel does not recognise as an operator or literal — a
 * binding (`tx.VarRef`, a domain's typed `refersTo` VarRef), a host graph read
 * (a property-read leaf returning a list), or a domain leaf (`Step`, `Time`,
 * `Smooth`…) — to a value. `ctx` is opaque caller state (the binding env, a
 * graph handle, a sim clock, integration state). `evaluate` is handed back so a
 * domain leaf containing sub-expressions can recurse into the kernel. */
export type Resolve<C> = (node: ExprNode, ctx: C, evaluate: (n: ExprNode, ctx: C) => Value) => Value;

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

/**
 * ListSourcedExpression / ListAggregate operators: one `source` operand,
 * evaluated as a list. A scalar source promotes to a one-element list (the
 * reference-engine rule); an empty list is absence. Each entry names the fold.
 */
type ListFold = (list: Value[], node: ExprNode) => Value;

/** IteratingExpression operators: `source` + `loopVar` + the body operand. */
const ITERATOR_BODY: Record<string, string> = {
  [`${TX}/ForEach`]: 'emit',
  [`${TX}/ListMap`]: 'mapBody',
  [`${TX}/Filter`]: 'predicate',
};

/** KindPredicate operators the kernel can answer over ITS value domain.
 * `IsBoolean` and `IsEmbedded` name kinds that exist only in a host object
 * model (booleans are 1/0 numbers here; embedded nodes never enter the
 * kernel), so they are NOT dispatch entries — they fall through to the
 * caller's `resolve`, where the host answers with full fidelity. */
const KIND_PREDICATES: Record<string, (v: Value) => boolean> = {
  [`${TX}/IsString`]: (v) => typeof v === 'string',
  [`${TX}/IsNumber`]: (v) => typeof v === 'number',
  [`${TX}/IsReference`]: (v) => typeof v === 'object' && !Array.isArray(v),
  [`${TX}/IsList`]: (v) => Array.isArray(v),
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

/** A computation operand must be a number — fail loud, never coerce. */
function requireNumber(v: Value, op: string): number {
  if (typeof v !== 'number') {
    throw new ExpressionError(`${op} requires a numeric operand, got ${kindOf(v)}`);
  }
  return v;
}

function kindOf(v: Value): string {
  if (typeof v === 'number') return 'number';
  if (typeof v === 'string') return 'string';
  return Array.isArray(v) ? 'list' : 'ref';
}

/** Unary/binary numeric primitives, keyed by operator URI. The authored,
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

const BINARY_ARITH: Record<string, (a: number, b: number) => number> = {
  [`${TX}/Add`]: (a, b) => a + b,
  [`${TX}/Subtract`]: (a, b) => a - b,
  [`${TX}/Multiply`]: (a, b) => a * b,
  [`${TX}/Divide`]: (a, b) => (requireDomain(b !== 0, 'Divide by zero'), a / b),
  [`${MATH}/Power`]: (a, b) => Math.pow(a, b),
  [`${MATH}/Modulo`]: flooredMod,
  [`${MATH}/Minimum`]: (a, b) => Math.min(a, b),
  [`${MATH}/Maximum`]: (a, b) => Math.max(a, b),
};

/**
 * Ordering comparisons are PREDICATES: on non-numeric operands they fail
 * CLOSED (`0`), matching the reference engine and the SHACL rule that a
 * non-comparable value falsifies a constraint. (Contrast the arithmetic
 * table above, which fails loud.)
 */
const BINARY_ORDER: Record<string, (a: number, b: number) => number> = {
  [`${TX}/GreaterThan`]: (a, b) => bool(a > b),
  [`${TX}/LessThan`]: (a, b) => bool(a < b),
  [`${TX}/GreaterThanOrEqual`]: (a, b) => bool(a >= b),
  [`${TX}/LessThanOrEqual`]: (a, b) => bool(a <= b),
};

/**
 * Polymorphic value equality — the full `tx.Equals` ontology contract
 * (v1 implemented its numeric half):
 *
 *   - number/number, string/string: value equality.
 *   - ref/ref: canonical versionless URI-key equality — identity-by-name,
 *     the same rule as the ordered comparisons.
 *   - lists: never equal (a list is a container, not a comparable value).
 *   - any cross-kind pair: false. Never errors.
 */
function valuesEqual(a: Value, b: Value): boolean {
  if (typeof a === 'number' && typeof b === 'number') return a === b;
  if (typeof a === 'string' && typeof b === 'string') return a === b;
  if (typeof a === 'object' && typeof b === 'object' && !Array.isArray(a) && !Array.isArray(b)) {
    return a.ref === b.ref;
  }
  return false;
}

/** Value of a literal node, or `undefined` if it is not a literal. In v2 the
 * string and URI literals are kernel-known: `StringLiteral` yields its string,
 * `UriLiteral` yields a ref value (its `refTo` IS its identity, the way an
 * IntegerLiteral's number is its value). */
function literalValue(node: ExprNode): Value | undefined {
  switch (node.type) {
    case `${TX}/IntegerLiteral`: return Number(node.integerLiteral);
    case `${TX}/DecimalLiteral`: return Number(node.decimalLiteral);
    case `${TX}/BooleanLiteral`: return bool(node.booleanLiteral === true || node.booleanLiteral === 'true');
    case `${TX}/StringLiteral`: {
      const s = node.stringLiteral;
      if (typeof s !== 'string') throw new ExpressionError('StringLiteral is missing stringLiteral');
      return s;
    }
    case `${TX}/UriLiteral`: {
      const ref = node.refTo;
      if (typeof ref !== 'string' || ref.length === 0) {
        throw new ExpressionError('UriLiteral is missing refTo');
      }
      return { ref };
    }
    default: return undefined;
  }
}

/**
 * Stringify one Join element. Numbers follow the ECMAScript number-to-string
 * rule — the same serialization RFC 8785 (JCS) pins and the canonical layer
 * already implements in every port, so Join output is deterministic across
 * languages. Refs stringify to their LOCAL name (the reference-engine rule for
 * display joins). A nested list is a computation type error — loud.
 */
function joinElement(v: Value): string {
  if (typeof v === 'string') return v;
  if (typeof v === 'number') return String(v);
  if (Array.isArray(v)) throw new ExpressionError('Join cannot stringify a nested list');
  return v.ref.slice(v.ref.lastIndexOf('/') + 1);
}

/** `IsSet`: absence is the empty list; the empty string is also unset (the
 * reference-engine rule). Everything else — including `0` — is set. */
function isSet(v: Value): boolean {
  if (typeof v === 'string') return v.length > 0;
  if (Array.isArray(v)) return v.length > 0;
  return true;
}

/** The `source`-operand fold table for the non-iterating list family. */
const LIST_FOLDS: Record<string, ListFold> = {
  [`${TX}/Count`]: (list) => list.length,
  [`${TX}/Sum`]: (list) => {
    let total = 0;
    for (const el of list) total += requireNumber(el, 'Sum');
    return total;
  },
  [`${TX}/Min`]: (list) => {
    if (list.length === 0) throw new ExpressionError('Min on an empty list is undefined; guard with IsSet');
    let best = requireNumber(list[0], 'Min');
    for (let i = 1; i < list.length; i++) {
      const n = requireNumber(list[i], 'Min');
      if (n < best) best = n;
    }
    return best;
  },
  [`${TX}/Max`]: (list) => {
    if (list.length === 0) throw new ExpressionError('Max on an empty list is undefined; guard with IsSet');
    let best = requireNumber(list[0], 'Max');
    for (let i = 1; i < list.length; i++) {
      const n = requireNumber(list[i], 'Max');
      if (n > best) best = n;
    }
    return best;
  },
  [`${TX}/Average`]: (list) => {
    if (list.length === 0) throw new ExpressionError('Average on an empty list is undefined; guard with IsSet');
    let total = 0;
    for (const el of list) total += requireNumber(el, 'Average');
    return total / list.length;
  },
  [`${TX}/Join`]: (list, node) => {
    const sep = typeof node.separator === 'string' ? node.separator : '';
    return list.map(joinElement).join(sep);
  },
  [`${TX}/Reverse`]: (list) => [...list].reverse(),
};

// ---------------------------------------------------------------------------
// Matches — the pinned RE2-compatible XSD-regex subset.
// ---------------------------------------------------------------------------

/**
 * Validate a `Matches` pattern against the pinned subset — the intersection of
 * RE2 and the host regex engines, chosen so every port compiles the same
 * pattern to the same language. RE2 is both the restrictive common denominator
 * and ReDoS-safe (no catastrophic backtracking), which is a REQUIREMENT for a
 * predicate that may gate on adversarial input. Out-of-subset constructs are a
 * LOUD error, the same discipline as Round/Modulo; WIDENING the subset later
 * (an error becoming a defined result) is additive within v2.
 *
 * Allowed: literals; `.`; anchors `^` `$`; alternation `|`; groups `(...)`,
 * `(?:...)`; a WHOLE-PATTERN flag prefix over `i`/`m`/`s` (`(?i)` at position
 * 0 only — each port translates it to its host flag mechanism); quantifiers
 * `*` `+` `?` `{m}` `{m,}` `{m,n}`; character classes with ranges and
 * negation; escapes `\d \D \w \W \s \S \b \B \n \r \t \f \v \xHH` and
 * escaped punctuation.
 *
 * Rejected (divergent or unsafe across engines): lookahead/lookbehind, named
 * groups, backreferences (`\1`…, `\k`), MID-pattern flag groups (`(?i:…)` —
 * not portable to the JS and Python engines), `\p{…}`/`\P{…}` unicode
 * property classes, POSIX classes (`[[:alpha:]]`), class intersection
 * (`&&`), atomic groups, conditionals, comments, octal/`\u`/`\x{…}` escapes.
 */
export function validateMatchesPattern(pattern: string): void {
  const fail = (what: string): never => {
    throw new ExpressionError(`Matches pattern is outside the pinned regex subset (${what}): ${pattern}`);
  };
  const ALLOWED_ESCAPES = new Set([
    'd', 'D', 'w', 'W', 's', 'S', 'b', 'B', 'n', 'r', 't', 'f', 'v',
    '.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '|', '^', '$', '\\', '/', '-', ' ',
  ]);
  let inClass = false;
  for (let i = 0; i < pattern.length; i++) {
    const c = pattern[i];
    if (c === '\\') {
      const e = pattern[i + 1];
      if (e === undefined) fail('trailing backslash');
      if (e === 'x') {
        // \xHH only — \x{…} (RE2 long form) is not portable to the JS engine.
        const hex = pattern.slice(i + 2, i + 4);
        if (!/^[0-9a-fA-F]{2}$/.test(hex)) fail('\\x escape must be \\xHH');
        if (pattern[i + 2] === '{') fail('\\x{…} escape');
        i += 3;
        continue;
      }
      if (e >= '0' && e <= '9') fail(`backreference or octal escape \\${e}`);
      if (e === 'p' || e === 'P') fail(`unicode property class \\${e}{…}`);
      if (e === 'k') fail('named backreference \\k');
      if (e === 'u') fail('\\u escape');
      if (!ALLOWED_ESCAPES.has(e!)) fail(`escape \\${e}`);
      i++;
      continue;
    }
    if (inClass) {
      if (c === ']') { inClass = false; continue; }
      if (c === '&' && pattern[i + 1] === '&') fail('character-class intersection &&');
      if (c === '[' && pattern[i + 1] === ':') fail('POSIX class [[:…:]]');
      continue;
    }
    if (c === '[') { inClass = true; continue; }
    if (c === '(' && pattern[i + 1] === '?') {
      // Only (?: survives mid-pattern; the flag prefix (?ims) is legal at
      // position 0 ONLY (validated by splitFlagPrefix before this scanner).
      if (pattern[i + 2] !== ':') fail(`group construct (?${pattern[i + 2] ?? ''}`);
      i += 2; // resume after '?:'
      continue;
    }
  }
  if (inClass) fail('unterminated character class');
}

/** Split the whole-pattern flag prefix — `(?i)`, `(?ims)` at position 0 —
 * from the body. Each port translates the flags to its host mechanism (`i`
 * case-fold, `m` multiline anchors, `s` dot-matches-newline). */
function splitFlagPrefix(pattern: string): { flags: string; body: string } {
  const m = /^\(\?([ims]+)\)/.exec(pattern);
  if (!m) return { flags: '', body: pattern };
  return { flags: m[1]!, body: pattern.slice(m[0].length) };
}

/** Compile + test under fn:matches semantics: UNANCHORED (substring) — authors
 * write `^…$` for a whole-string match. The flag prefix splits off first
 * (translated to host RegExp flags), then the body is subset-validated, so
 * the host engine only ever sees portable constructs. */
function matchesPattern(input: string, pattern: string): boolean {
  const { flags, body } = splitFlagPrefix(pattern);
  validateMatchesPattern(body);
  let re: RegExp;
  try {
    re = new RegExp(body, flags);
  } catch {
    throw new ExpressionError(`Matches pattern does not compile: ${pattern}`);
  }
  return re.test(input);
}

// ---------------------------------------------------------------------------
// Ordered comparisons (unchanged from v1).
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// The fold.
// ---------------------------------------------------------------------------

/** Lexical binding frames for the iterating operators' loopVars. */
type Frame = { name: string; value: Value };

/** Innermost-wins lookup of a lambda-bound variable. */
function boundValue(frames: readonly Frame[], name: string): Value | undefined {
  for (let i = frames.length - 1; i >= 0; i--) {
    if (frames[i]!.name === name) return frames[i]!.value;
  }
  return undefined;
}

/**
 * Evaluate an expression tree to a value. Operators fold via the frozen
 * dispatch + primitive tables; literals yield their value; any other node is
 * delegated to `resolve`. `options` carries the ordered-comparison context
 * (closures + identity-leaf resolution) and is only consulted when an
 * `IsAtLeast` / `Dominates` node is reached.
 */
export function evaluate<C = unknown>(
  node: ExprNode,
  ctx: C,
  resolve: Resolve<C>,
  options?: EvalOptions<C>,
): Value {
  return evalNode(node, ctx, resolve, options, []);
}

/** `source` operand evaluated as a list: a scalar promotes to a one-element
 * list (the reference-engine rule); an empty list is absence. */
function evalSourceList<C>(
  node: ExprNode,
  ctx: C,
  resolve: Resolve<C>,
  options: EvalOptions<C> | undefined,
  frames: readonly Frame[],
): Value[] {
  const v = evalNode(operand(node, 'source'), ctx, resolve, options, frames);
  return Array.isArray(v) ? v : [v];
}

function evalNode<C>(
  node: ExprNode,
  ctx: C,
  resolve: Resolve<C>,
  options: EvalOptions<C> | undefined,
  frames: readonly Frame[],
): Value {
  const recurse = (n: ExprNode, c: C): Value => evalNode(n, c, resolve, options, frames);
  const recurseNumber = (n: ExprNode, c: C, op: string): number => requireNumber(recurse(n, c), op);

  const arity = OPERATOR_ARITY[node.type];
  if (arity) {
    switch (arity.kind) {
      case 'unary': {
        const x = recurseNumber(operand(node, arity.operand), ctx, node.type);
        return UNARY[node.type]!(x);
      }
      case 'binary': {
        const a = recurse(operand(node, arity.left), ctx);
        const b = recurse(operand(node, arity.right), ctx);
        if (node.type === `${TX}/Equals`) return bool(valuesEqual(a, b));
        const order = BINARY_ORDER[node.type];
        if (order) {
          // Predicate: non-numeric operands fail CLOSED.
          if (typeof a !== 'number' || typeof b !== 'number') return 0;
          return order(a, b);
        }
        // Arithmetic: computation — fail LOUD on non-numbers.
        return BINARY_ARITH[node.type]!(
          requireNumber(a, node.type),
          requireNumber(b, node.type),
        );
      }
      case 'nary': {
        const items = node[arity.operands];
        if (!Array.isArray(items)) throw new ExpressionError(`${node.type} expects an '${arity.operands}' list`);
        const isAnd = node.type === `${TX}/And`;
        // Short-circuit; empty And is vacuously true, empty Or vacuously false.
        for (const item of items) {
          const v = truthy(requireNumber(recurse(item as ExprNode, ctx), node.type));
          if (isAnd && !v) return 0;
          if (!isAnd && v) return 1;
        }
        return bool(isAnd);
      }
      case 'ternary': {
        // Only Clip today: clamp clipValue into [clipLower, clipUpper].
        const v = recurseNumber(operand(node, arity.a), ctx, node.type);
        const lo = recurseNumber(operand(node, arity.b), ctx, node.type);
        const hi = recurseNumber(operand(node, arity.c), ctx, node.type);
        return Math.min(Math.max(v, lo), hi);
      }
    }
  }

  if (node.type === `${TX}/Not`) {
    return bool(!truthy(requireNumber(recurse(operand(node, 'operand'), ctx), node.type)));
  }

  if (node.type === `${TX}/IsAtLeast` || node.type === `${TX}/Dominates`) {
    return foldOrdered(node, ctx, options).value;
  }

  const listFold = LIST_FOLDS[node.type];
  if (listFold) {
    return listFold(evalSourceList(node, ctx, resolve, options, frames), node);
  }

  const body = ITERATOR_BODY[node.type];
  if (body) {
    const loopVar = node.loopVar;
    if (typeof loopVar !== 'string' || loopVar.length === 0) {
      throw new ExpressionError(`${node.type} is missing loopVar`);
    }
    const list = evalSourceList(node, ctx, resolve, options, frames);
    const bodyNode = operand(node, body);
    const out: Value[] = [];
    for (const el of list) {
      const inner = [...frames, { name: loopVar, value: el }];
      const v = evalNode(bodyNode, ctx, resolve, options, inner);
      if (node.type === `${TX}/Filter`) {
        if (truthy(requireNumber(v, 'Filter predicate'))) out.push(el);
      } else if (node.type === `${TX}/ForEach`) {
        // Flatten one level; an empty list contributes nothing — which is
        // exactly the absence rule doing the reference engine's skip.
        if (Array.isArray(v)) out.push(...v);
        else out.push(v);
      } else {
        out.push(v);
      }
    }
    return out;
  }

  if (node.type === `${TX}/Contains`) {
    const haystack = evalNode(operand(node, 'haystack'), ctx, resolve, options, frames);
    const needle = evalNode(operand(node, 'needle'), ctx, resolve, options, frames);
    const list = Array.isArray(haystack) ? haystack : [haystack];
    return bool(list.some((el) => valuesEqual(el, needle)));
  }

  if (node.type === `${TX}/IsSet`) {
    return bool(isSet(evalNode(operand(node, 'checkExpr'), ctx, resolve, options, frames)));
  }

  if (node.type === `${TX}/ListItemAt`) {
    const list = evalSourceList(node, ctx, resolve, options, frames);
    const idx = evalNode(operand(node, 'itemIndex'), ctx, resolve, options, frames);
    if (typeof idx !== 'number' || !Number.isInteger(idx) || idx < 0) {
      throw new ExpressionError('ListItemAt itemIndex must be a non-negative integer');
    }
    // Past the end is ABSENCE, not an error — the empty list; guard with IsSet.
    return idx < list.length ? list[idx]! : [];
  }

  if (node.type === `${TX}/Matches`) {
    const src = evalNode(operand(node, 'matchSource'), ctx, resolve, options, frames);
    if (typeof src !== 'string') {
      throw new ExpressionError(`Matches requires a string matchSource, got ${kindOf(src)}`);
    }
    const pattern = node.pattern;
    if (typeof pattern !== 'string') throw new ExpressionError('Matches is missing pattern');
    return bool(matchesPattern(src, pattern));
  }

  const kind = KIND_PREDICATES[node.type];
  if (kind) {
    return bool(kind(evalNode(operand(node, 'kindCheck'), ctx, resolve, options, frames)));
  }

  const lit = literalValue(node);
  if (lit !== undefined) return lit;

  // A VarRef naming a lexically-enclosing loopVar is the kernel's own bound
  // variable — the ONLY leaf the kernel answers (see the header). Any other
  // VarRef, and every other leaf, is the caller's.
  if (node.type === `${TX}/VarRef` && typeof node.varName === 'string') {
    const bound = boundValue(frames, node.varName);
    if (bound !== undefined) return bound;
  }

  // Not an operator or literal — a binding, graph read, or domain leaf. The
  // caller owns it. (Recursion from inside `resolve` re-enters WITHOUT lambda
  // frames: the caller's subtrees are the caller's scope.)
  return resolve(node, ctx, (n, c) => evalNode(n, c, resolve, options, []));
}

function operand(node: ExprNode, key: string): ExprNode {
  const v = node[key];
  if (v === null || v === undefined || typeof v !== 'object' || Array.isArray(v)) {
    throw new ExpressionError(`${node.type} is missing operand '${key}'`);
  }
  return v as ExprNode;
}

// ---------------------------------------------------------------------------
// explain — the verdict tree.
// ---------------------------------------------------------------------------

/**
 * One node of an evaluation trace — the verdict tree {@link explain} returns.
 *
 * Mirrors the expression: `type` is the node's type URI, `value` its result
 * (`1`/`0` for booleans), `children` the operand traces in evaluation order.
 * A short-circuited operand is simply ABSENT from `children` — the trace is
 * truthful about what ran. An iterating operator's children are its source
 * trace followed by one body trace per visited element. Ordered comparisons
 * carry their resolved operand identities as `leftRef`/`rightRef` instead of
 * children, since their operands are identities rather than value folds.
 *
 * This is a runtime return shape, not an ontology class: nothing authors a
 * trace, so nothing about it is modelled or serialized as vocabulary. (A host
 * assembling an AUTHORED report — e.g. a SHACL ValidationReport — builds it
 * FROM these traces; the vocabulary lives with the host's ontology.)
 */
export interface TraceNode {
  type: string;
  value: Value;
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
  // Value recursion for subtrees the caller's `resolve` re-enters: those
  // folds happen inside the caller and are invisible to the trace, exactly
  // like the caller's own computation. Only kernel-visited nodes appear.
  const recurseValue = (n: ExprNode, c: C): Value => evalNode(n, c, resolve, options, []);

  const trace = (n: ExprNode, c: C, frames: readonly Frame[]): TraceNode => {
    const leaf = (value: Value): TraceNode => ({ type: n.type, value, children: [] });

    const arity = OPERATOR_ARITY[n.type];
    if (arity) {
      switch (arity.kind) {
        case 'unary': {
          const x = trace(operand(n, arity.operand), c, frames);
          return { type: n.type, value: UNARY[n.type]!(requireNumber(x.value, n.type)), children: [x] };
        }
        case 'binary': {
          const a = trace(operand(n, arity.left), c, frames);
          const b = trace(operand(n, arity.right), c, frames);
          let value: Value;
          if (n.type === `${TX}/Equals`) {
            value = bool(valuesEqual(a.value, b.value));
          } else {
            const order = BINARY_ORDER[n.type];
            if (order) {
              value = typeof a.value !== 'number' || typeof b.value !== 'number'
                ? 0
                : order(a.value, b.value);
            } else {
              value = BINARY_ARITH[n.type]!(
                requireNumber(a.value, n.type),
                requireNumber(b.value, n.type),
              );
            }
          }
          return { type: n.type, value, children: [a, b] };
        }
        case 'nary': {
          const items = n[arity.operands];
          if (!Array.isArray(items)) throw new ExpressionError(`${n.type} expects an '${arity.operands}' list`);
          const isAnd = n.type === `${TX}/And`;
          const children: TraceNode[] = [];
          for (const item of items) {
            const child = trace(item as ExprNode, c, frames);
            children.push(child);
            const v = truthy(requireNumber(child.value, n.type));
            // Same short-circuit as `evaluate`: operands after the deciding
            // one are never evaluated and never appear in the trace.
            if (isAnd && !v) return { type: n.type, value: 0, children };
            if (!isAnd && v) return { type: n.type, value: 1, children };
          }
          return { type: n.type, value: bool(isAnd), children };
        }
        case 'ternary': {
          const v = trace(operand(n, arity.a), c, frames);
          const lo = trace(operand(n, arity.b), c, frames);
          const hi = trace(operand(n, arity.c), c, frames);
          return {
            type: n.type,
            value: Math.min(
              Math.max(requireNumber(v.value, n.type), requireNumber(lo.value, n.type)),
              requireNumber(hi.value, n.type),
            ),
            children: [v, lo, hi],
          };
        }
      }
    }

    if (n.type === `${TX}/Not`) {
      const x = trace(operand(n, 'operand'), c, frames);
      return { type: n.type, value: bool(!truthy(requireNumber(x.value, n.type))), children: [x] };
    }

    if (n.type === `${TX}/IsAtLeast` || n.type === `${TX}/Dominates`) {
      const r = foldOrdered(n, c, options);
      return { type: n.type, value: r.value, children: [], leftRef: r.left, rightRef: r.right };
    }

    const listFold = LIST_FOLDS[n.type];
    if (listFold) {
      const src = trace(operand(n, 'source'), c, frames);
      const list = Array.isArray(src.value) ? src.value : [src.value];
      return { type: n.type, value: listFold(list, n), children: [src] };
    }

    const body = ITERATOR_BODY[n.type];
    if (body) {
      const loopVar = n.loopVar;
      if (typeof loopVar !== 'string' || loopVar.length === 0) {
        throw new ExpressionError(`${n.type} is missing loopVar`);
      }
      const src = trace(operand(n, 'source'), c, frames);
      const list = Array.isArray(src.value) ? src.value : [src.value];
      const bodyNode = operand(n, body);
      const children: TraceNode[] = [src];
      const out: Value[] = [];
      for (const el of list) {
        const inner = [...frames, { name: loopVar, value: el }];
        const bodyTrace = trace(bodyNode, c, inner);
        children.push(bodyTrace);
        const v = bodyTrace.value;
        if (n.type === `${TX}/Filter`) {
          if (truthy(requireNumber(v, 'Filter predicate'))) out.push(el);
        } else if (n.type === `${TX}/ForEach`) {
          if (Array.isArray(v)) out.push(...v);
          else out.push(v);
        } else {
          out.push(v);
        }
      }
      return { type: n.type, value: out, children };
    }

    if (n.type === `${TX}/Contains`) {
      const hay = trace(operand(n, 'haystack'), c, frames);
      const needle = trace(operand(n, 'needle'), c, frames);
      const list = Array.isArray(hay.value) ? hay.value : [hay.value];
      return {
        type: n.type,
        value: bool(list.some((el) => valuesEqual(el, needle.value))),
        children: [hay, needle],
      };
    }

    if (n.type === `${TX}/IsSet`) {
      const x = trace(operand(n, 'checkExpr'), c, frames);
      return { type: n.type, value: bool(isSet(x.value)), children: [x] };
    }

    if (n.type === `${TX}/ListItemAt`) {
      const src = trace(operand(n, 'source'), c, frames);
      const idx = trace(operand(n, 'itemIndex'), c, frames);
      const list = Array.isArray(src.value) ? src.value : [src.value];
      if (typeof idx.value !== 'number' || !Number.isInteger(idx.value) || idx.value < 0) {
        throw new ExpressionError('ListItemAt itemIndex must be a non-negative integer');
      }
      const value = idx.value < list.length ? list[idx.value]! : [];
      return { type: n.type, value, children: [src, idx] };
    }

    if (n.type === `${TX}/Matches`) {
      const src = trace(operand(n, 'matchSource'), c, frames);
      if (typeof src.value !== 'string') {
        throw new ExpressionError(`Matches requires a string matchSource, got ${kindOf(src.value)}`);
      }
      const pattern = n.pattern;
      if (typeof pattern !== 'string') throw new ExpressionError('Matches is missing pattern');
      return { type: n.type, value: bool(matchesPattern(src.value, pattern)), children: [src] };
    }

    const kind = KIND_PREDICATES[n.type];
    if (kind) {
      const x = trace(operand(n, 'kindCheck'), c, frames);
      return { type: n.type, value: bool(kind(x.value)), children: [x] };
    }

    const lit = literalValue(n);
    if (lit !== undefined) return leaf(lit);

    if (n.type === `${TX}/VarRef` && typeof n.varName === 'string') {
      const bound = boundValue(frames, n.varName);
      if (bound !== undefined) return leaf(bound);
    }

    return leaf(resolve(n, c, recurseValue));
  };

  return trace(node, ctx, []);
}
