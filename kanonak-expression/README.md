# kanonak-expression

The Kanonak **expression runtime** — `expressionRuntimeVersion: "1"`.

A small, deterministic tree-walker that folds a `kanonak.org/transformations`
(`tx`) + `kanonak.org/math` expression to a single number. The codec family's
third member: `canonical` content-addresses, `codec` serializes, **`expression`
evaluates**. Generated SDKs reference it so a typed expression can be *run*, not
just *represented* — without each language hand-writing (and diverging on) an
evaluator.

Every language port (Python, JavaScript/TypeScript, Go, Rust, Java, C#) produces
identical values for the shared parity vectors in [`vectors/`](./vectors),
including the determinism traps that expose language differences.

## The three layers

1. **Dispatch — derived from the ontology.** An operator's arity falls out of its
   `tx` superclass: `UnaryNumericOp` → unary `value`; `BinaryArithmetic` /
   `BinaryComparison` → binary; `BooleanLogic` → n-ary `operands`; plus the two
   shapes the hierarchy can't imply (`Not`'s `operand`, `Clip`'s ternary).
2. **Primitives — the one authored, determinism-bearing artifact.** A table from
   operator URI to its fold, matched per language. This is where determinism is
   pinned and frozen.
3. **The fold.** Operators recurse + apply a primitive; literals yield their
   numeric value; **everything else is the caller's**.

## The runtime is a pure operator engine

It knows operators and literals. **Every other node** — a `tx.VarRef`, a domain's
typed `refersTo` VarRef, a `Step` / `Time` / `Smooth` / `Delay`, any future leaf —
is handed to a caller-supplied hook:

```
resolve(node, ctx, evaluate) -> number
```

`ctx` is opaque caller state (the binding env, a sim clock, integration state);
`evaluate` is passed back so a domain leaf containing sub-expressions can recurse
into the kernel. The runtime never privileges `tx.VarRef` — it is just one leaf a
domain may resolve. Operators (shared, derived) on one side; variable binding and
domain-leaf semantics (the caller's `resolve`) on the other.

## Value domain

Uniform numeric. Booleans and comparison results are `1` / `0`, so every language
stays on one numeric path.

## Determinism contract (frozen by `expressionRuntimeVersion`)

- **Round** — half away from zero: `Round(-2.5) = -3`, `Round(2.5) = 3`.
- **Modulo** — floored (`a - b·floor(a/b)`): `Modulo(-7, 3) = 2`. Errors on zero.
- **Sign** — `Sign(0) = 0`, `Sign(neg) = -1`, `Sign(pos) = 1`.
- **Comparisons / booleans** — `1` / `0`.
- **Divide by zero**, **Modulo by zero**, **Ln/Log10 of ≤ 0**, **Sqrt of < 0** —
  explicit runtime errors, never `NaN`/`Inf`.

A change to any primitive, value rule, or dispatch entry requires a **new**
`expressionRuntimeVersion`, never an edit in place.

**Always-finite invariant.** Because every domain violation raises rather than
producing `NaN`/`Inf`, an operand reaching a primitive is always a finite number.
Primitives therefore need not define behavior on `NaN`/`Inf` — those values are
unreachable by construction — so a few host-specific edge differences (e.g.
`Sign(NaN)`, or a type-less node) cannot occur in a valid evaluation. A new
operator that could yield a non-finite value must guard it explicitly, the same
way `Divide`/`Ln`/`Sqrt` do.

## Operators

| Group (dispatch) | Operands | Operators |
|---|---|---|
| `BinaryArithmetic` | `arithLeft`, `arithRight` | Add, Subtract, Multiply, Divide, Power, Modulo, Minimum, Maximum |
| `UnaryNumericOp` | `value` | Abs, Negate, Exp, Ln, Log10, Sqrt, Floor, Ceil, Round, Sign |
| `BinaryComparison` | `compareLeft`, `compareRight` | Equals, GreaterThan, LessThan, GreaterThanOrEqual, LessThanOrEqual |
| `BooleanLogic` | `operands` (list) | And, Or |
| (direct) | `operand` | Not |
| (direct, ternary) | `clipValue`, `clipLower`, `clipUpper` | Clip |

Literals: `IntegerLiteral`, `DecimalLiteral`, `BooleanLiteral` (→ `1`/`0`).

## Vectors

[`vectors/expression-vectors.json`](./vectors/expression-vectors.json) is the
parity gate: each `expr` is a tree, `env` binds `tx.VarRef` names for the
conformance `resolve`, `expected` is the exact result (`tolerance` for the few
transcendentals where libm differs by an ULP), and `expectError` marks required
errors. Every language port runs it; emission/build must fail if any ontology
operator lacks a primitive in a language.
