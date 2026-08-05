# kanonak-expression

The Kanonak **expression runtime** — `expressionRuntimeVersion: "1"`.

A small, deterministic tree-walker that folds a `kanonak.org/transformations`
(`tx`) + `kanonak.org/math` expression to a single number. The codec family's
third member: `canonical` content-addresses, `codec` serializes, **`expression`
evaluates**. Generated SDKs reference it so a typed expression can be *run*, not
just *represented* — without each language hand-writing (and diverging on) an
evaluator.

Every language port (Python, JavaScript/TypeScript, Go, Rust, Java, C#, Swift)
produces identical values for the shared parity vectors in
[`vectors/`](./vectors), including the determinism traps that expose language
differences.

## Ports

All seven ports pass 100% of the 69 parity vectors.

| Language | Path | Status | Conformance command |
|---|---|---|---|
| TypeScript | [`typescript/`](./typescript) | ✅ | `npm run conformance` |
| Rust | [`rust/`](./rust) | ✅ | `cargo test` |
| Python | [`python/`](./python) | ✅ | `python conformance.py ../vectors` |
| Go | [`go/`](./go) | ✅ | `go test ./...` |
| Java | [`java/`](./java) | ✅ | `javac -d out src/main/java/org/kanonak/expression/*.java conformance/Conformance.java && java -cp out Conformance ../vectors` |
| C# | [`csharp/`](./csharp) | ✅ | `cd csharp && dotnet run --project test/Kanonak.Expression.Conformance -- ../vectors` |
| Swift | [`swift/`](./swift) | ✅ | `swift test --filter ExpressionVectorTests` (at the repo root) |

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
| `OrderedComparison` | `compareLeft`, `compareRight`, `viaProperty` | IsAtLeast, Dominates |
| `BooleanLogic` | `operands` (list) | And, Or |
| (direct) | `operand` | Not |
| (direct, ternary) | `clipValue`, `clipLower`, `clipUpper` | Clip |

Literals: `IntegerLiteral`, `DecimalLiteral`, `BooleanLiteral` (→ `1`/`0`).
Identity leaf (inside ordered comparisons): `UriLiteral` (`refTo` = a member's
canonical versionless URI).

## Ordered comparisons

`IsAtLeast` / `Dominates` compare **ordered vocabulary members** — members of a
closed set ranked by an `owl:TransitiveProperty` — by consulting that property's
transitive closure. Identity is canonical versionless URI equality
(`publisher/package/name`), the same rule as `tx.Equals`; the ordering is never
a projected ordinal, so inserting a member into a scale cannot silently shift
the meaning of persisted rules.

- **The closure is caller-supplied evaluation context** (`EvalOptions.closures`:
  `property → member → reachable members`). Flat, already-closed data —
  typically the SDK reasoner's `prp-trp` saturation emitted at code-generation
  time. The kernel does set membership only; it never resolves a package or
  computes a closure, the same division that keeps variable binding out of the
  kernel. A missing closure table is a loud error, never a silent false.
- **`IsAtLeast` is reflexive at the operator** (same member → true) so a
  strictly-ordered vocabulary need not declare itself reflexive to be
  comparable; **`Dominates` is strict** (same member → false).
- **Unrelated members yield false, not an error** — the fail-closed direction
  for a predicate.
- Operand leaves: `UriLiteral` is kernel-known (its `refTo` IS the identity,
  as a literal's value is its number); any other leaf goes to the caller's
  `resolveRef(node, ctx)` — the identity-domain mirror of `resolve`.

## explain — the verdict tree

`explain(node, ctx, resolve, options)` evaluates and returns a trace mirroring
the expression — the regex-debugger view: each evaluated node with its own
verdict (`type`, `value`, `children`; ordered comparisons carry the resolved
`leftRef`/`rightRef` instead of children). Short-circuited operands are absent
— the trace is truthful about what ran. `evaluate` stays a bare number and is
untouched by tracing; every parity vector runs through both entry points and
their values must agree, so they cannot drift. The trace is a runtime return
shape, not an ontology class — nothing authors one.

## Totality

Evaluation always terminates: the operator set has no loops, no recursion
beyond tree depth, and no unbounded iteration — an expression over a finite
tree performs a bounded fold, and ordered comparisons are set-membership
lookups in supplied data. This is a stated property of the runtime, not an
accident: predicates evaluated over third-party-authored rules (access
decisions, policy checks) may rely on it. Combined with operands restricted to
ordered vocabulary members, a predicate's domain is finite — equivalence,
subsumption, satisfiability, and monotonicity of rules are checkable by
exhaustive enumeration.

## Vectors

[`vectors/expression-vectors.json`](./vectors/expression-vectors.json) is the
parity gate: each `expr` is a tree, `env` binds `tx.VarRef` names for the
conformance `resolve`, `expected` is the exact result (`tolerance` for the few
transcendentals where libm differs by an ULP), and `expectError` marks required
errors. Every language port runs it; emission/build must fail if any ontology
operator lacks a primitive in a language.
