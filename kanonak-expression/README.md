# kanonak-expression

The Kanonak **expression runtime** — `expressionRuntimeVersion: "2"`.

A small, deterministic tree-walker that folds a `kanonak.org/transformations`
(`tx`) + `kanonak.org/math` expression to a **value**. The codec family's
third member: `canonical` content-addresses, `codec` serializes, **`expression`
evaluates**. Generated SDKs reference it so a typed expression can be *run*, not
just *represented* — without each language hand-writing (and diverging on) an
evaluator.

Every language port (Python, JavaScript/TypeScript, Go, Rust, Java, C#, Swift)
produces identical values for the shared parity vectors in
[`vectors/`](./vectors), including the determinism traps that expose language
differences.

## Ports

Version 2 lands in the TypeScript reference kernel first; the remaining ports
follow under the conformance-vector discipline. Every port ALWAYS passes the
v1 vector file (the numeric core is unchanged); a port is v2-complete when it
also passes `expression-vectors-2.json`.

| Language | Path | v1 vectors | v2 vectors | Conformance command |
|---|---|---|---|---|
| TypeScript | [`typescript/`](./typescript) | ✅ | ✅ | `npm run conformance` |
| Rust | [`rust/`](./rust) | ✅ | ⏳ | `cargo test` |
| Python | [`python/`](./python) | ✅ | ⏳ | `python conformance.py ../vectors` |
| Go | [`go/`](./go) | ✅ | ⏳ | `go test ./...` |
| Java | [`java/`](./java) | ✅ | ⏳ | `javac -d out src/main/java/org/kanonak/expression/*.java conformance/Conformance.java && java -cp out Conformance ../vectors` |
| C# | [`csharp/`](./csharp) | ✅ | ⏳ | `cd csharp && dotnet run --project test/Kanonak.Expression.Conformance -- ../vectors` |
| Swift | [`swift/`](./swift) | ✅ | ⏳ | `swift test --filter ExpressionVectorTests` (at the repo root) |

## The three layers

1. **Dispatch — derived from the ontology.** An operator's shape falls out of
   its `tx` superclass: `UnaryNumericOp` → unary `value`; `BinaryArithmetic` /
   `BinaryComparison` → binary; `BooleanLogic` → n-ary `operands`;
   `ListSourcedExpression` / `ListAggregate` → `source`; `IteratingExpression`
   → `source` + `loopVar` + a body; plus the structural shapes the hierarchy
   can't imply (`Not`'s `operand`, `Clip`'s ternary, `Contains`'
   haystack/needle, `Matches`' matchSource/pattern).
2. **Primitives — the one authored, determinism-bearing artifact.** Tables from
   operator URI to its fold, matched per language. This is where determinism is
   pinned and frozen.
3. **The fold.** Operators recurse + apply a primitive; literals yield their
   value; **everything else is the caller's**.

## The runtime is a pure operator engine

It knows operators and literals. **Every other node** — a `tx.VarRef`, a domain's
typed `refersTo` VarRef, a host graph read, a `Step` / `Time` / `Smooth` /
`Delay`, any future leaf — is handed to a caller-supplied hook:

```
resolve(node, ctx, evaluate) -> Value
```

`ctx` is opaque caller state (the binding env, a graph handle, a sim clock,
integration state); `evaluate` is passed back so a domain leaf containing
sub-expressions can recurse into the kernel. The runtime never privileges
`tx.VarRef` as a *binding* mechanism — it is just one leaf a domain may resolve.
**A property read is a caller leaf that returns a list**: the kernel NEVER
touches a graph, in any language, ever. Operators (shared, derived) on one
side; variable binding, graph access, and domain-leaf semantics (the caller's
`resolve`) on the other.

## Value domain (new in version 2)

`number | string | ref | list`.

- **Booleans and comparison results are `1` / `0` numbers** (the v1 rule,
  unchanged), so every language keeps one numeric path for logic.
- A **ref** is a canonical versionless URI identity
  (`publisher/package/name`), compared by URI-key equality — the same identity
  rule the ordered comparisons and `tx.Equals` share. In JSON (vectors, tree
  interchange) a ref is `{"ref": "publisher/package/name"}`.
- A **list** is an ordered sequence of values.
- **There is no null: absence is the empty list.** A caller leaf for a
  property with no values returns `[]`; `IsSet` distinguishes; `Count` of
  absence is `0`. A scalar `source` promotes to a one-element list.
- Kinds that exist only in a host object model — a *boolean-typed* statement
  (booleans are numbers here) or an *embedded* node — are answered by the
  host: `IsBoolean` / `IsEmbedded` are NOT kernel operators and fall through
  to `resolve`, where the host has full fidelity.

## Error contract (frozen by `expressionRuntimeVersion`)

The v1 traps, unchanged: **Round** half away from zero; **Modulo** floored,
errors on zero; **Sign(0) = 0**; **Divide by zero**, **Ln/Log10 of ≤ 0**,
**Sqrt of < 0** — explicit runtime errors, never `NaN`/`Inf`.

The v2 extension splits by role:

- **Computations fail LOUD.** Arithmetic / unary-numeric / boolean-logic
  operators on a non-number, an aggregate over a non-numeric element,
  `Min`/`Max`/`Average` on an empty list, a nested list in `Join`, a
  non-string `Matches` source, an out-of-subset `Matches` pattern, a
  non-integer or negative `ListItemAt` index — all raise, never coerce.
- **Predicates fail CLOSED.** `Equals` on any cross-kind pair (a URI is never
  equal to the string spelling of it; lists are containers, never equal) and
  the ordering comparisons (`GreaterThan` et al.) on non-numeric operands
  return `0`, never error — the rule the reference engine and SHACL share: a
  non-comparable value falsifies a constraint, it doesn't crash the gate.

A change to any primitive, value rule, or dispatch entry requires a **new**
`expressionRuntimeVersion`, never an edit in place. Adding an operator, or
widening the `Matches` subset (an error becoming a defined result), is
additive **within** a version. **Every v1 vector passes unchanged under v2**
— the conformance suite runs the v1 file through the v2 kernel as the
regression gate.

**Always-finite invariant.** Because every numeric domain violation raises
rather than producing `NaN`/`Inf`, a number reaching a numeric primitive is
always finite. Primitives therefore need not define behavior on `NaN`/`Inf` —
those values are unreachable by construction.

## Operators

| Group (dispatch) | Operands | Operators |
|---|---|---|
| `BinaryArithmetic` | `arithLeft`, `arithRight` | Add, Subtract, Multiply, Divide, Power, Modulo, Minimum, Maximum |
| `UnaryNumericOp` | `value` | Abs, Negate, Exp, Ln, Log10, Sqrt, Floor, Ceil, Round, Sign |
| `BinaryComparison` | `compareLeft`, `compareRight` | Equals (polymorphic), GreaterThan, LessThan, GreaterThanOrEqual, LessThanOrEqual |
| `OrderedComparison` | `compareLeft`, `compareRight`, `viaProperty` | IsAtLeast, Dominates |
| `BooleanLogic` | `operands` (list) | And, Or |
| (direct) | `operand` | Not |
| (direct, ternary) | `clipValue`, `clipLower`, `clipUpper` | Clip |
| `ListAggregate` | `source` | Count, Sum, Min, Max, Average |
| `ListSourced` | `source` (+ data) | Join (`separator`), Reverse |
| (direct) | `source`, `itemIndex` | ListItemAt |
| (direct) | `haystack`, `needle` | Contains |
| (direct) | `checkExpr` | IsSet |
| `IteratingExpression` | `source`, `loopVar`, body | Filter (`predicate`), ListMap (`mapBody`), ForEach (`emit`) |
| `KindPredicate` | `kindCheck` | IsString, IsNumber, IsReference, IsList |
| (direct) | `matchSource`, `pattern` (data) | Matches |

Literals: `IntegerLiteral`, `DecimalLiteral`, `BooleanLiteral` (→ `1`/`0`),
`StringLiteral`, `UriLiteral` (→ a ref value; doubles as the identity leaf
inside ordered comparisons).

List-family semantics are the SDK reference engine's, frozen as vectors:
`Sum` on empty = 0; `Min`/`Max`/`Average` on empty = loud error; `Count`
type-agnostic; `Join` stringifies numbers per the ECMAScript rule RFC 8785
pins (the same serialization the canonical layer implements in every port)
and refs to their local name; `ForEach` flattens one level (an empty body
list contributes nothing — absence doing the skip).

The remaining iterating family (`WindowedMap`, `PairwiseMap`, `Scan`,
`DistinctBy`, `PartitionBy`) is additive within v2 under the reserved lambda
shape below.

## Lambda binding (the one genuinely-new dispatch shape)

The iterating operators bind their `loopVar` per element. Within an iterating
operator's body — and ONLY there — a `tx.VarRef` whose `varName` names a
lexically-enclosing `loopVar` is resolved by the kernel to the bound element
(innermost binder wins; shadowing is lexical). Every other leaf, including a
`VarRef` naming anything else, still goes to the caller's `resolve`. This is
the scoped exception to "the kernel never privileges VarRef": VarRef is the
bound-variable mechanism of the kernel's own binders, nothing more. Recursion
re-entered from inside `resolve` carries NO frames — the caller's subtrees
are the caller's scope.

## Matches — the pinned regex subset

`Matches(matchSource, pattern)` follows XPath `fn:matches` semantics:
**unanchored** (substring) — authors write `^…$` for a whole-string match.

The pattern language is the **RE2-compatible XSD-regex subset** — the
intersection of RE2 and the host engines, chosen so every port compiles the
same pattern to the same language, and ReDoS-safe (no catastrophic
backtracking — a requirement for a predicate that may gate on adversarial
input, not a nicety).

**Counting unit — pinned: code points.** `.` and quantifiers count Unicode
code points in every port: an astral-plane character (a surrogate pair in
UTF-16 hosts) is ONE `.`, and `.{3}` matches exactly three code points. This
matches RE2's rune model, Python, and what SHACL/SPARQL string length means —
and it is why the `minLength`/`maxLength` lowering (`(?s)^.{m,}$`) is safe to
express as regex cardinality. UTF-16 engines opt in explicitly (the JS port
compiles with the `u` flag; Java/C# ports normalize equivalently); the astral
vectors gate it, so a port that counts code units fails conformance, not the
tenant.

**Shorthand classes — pinned: ASCII, by textual expansion.** The native
shorthands diverge across exactly our targets (Rust/Python `\d` matches
Arabic-Indic digits; JS `\s` matches NBSP; `\b` is Unicode in Rust/Python,
ASCII in JS/RE2). The dialect therefore DEFINES them by expansion — `\d` ≡
`[0-9]`, `\w` ≡ `[0-9A-Za-z_]`, `\s` ≡ ASCII whitespace (` \t\n\r\f\x0B`),
negations likewise outside classes — and every port applies the expansion
textually before compiling (including the JS port, whose native `\s` is
wider than the pin). `\b`/`\B` are ASCII word boundaries (native in JS/RE2;
Rust uses `(?-u:\b)`, Python compiles with `re.ASCII`). Negated shorthands
inside a class (`[\D]`) are rejected — inexpressible as fragments. The
Arabic-digit / NBSP / `\bé` vectors gate all of this per port.

- **Allowed**: literals; `.`; `^` `$`; `|`; `(...)`, `(?:...)`; a
  whole-pattern flag prefix `(?i)` / `(?m)` / `(?s)` (position 0 only; each
  port translates to its host flag mechanism); `*` `+` `?` `{m}` `{m,}`
  `{m,n}`; character classes with ranges and negation; escapes
  `\d \D \w \W \s \S \b \B \n \r \t \f \v \xHH` (per the pinned ASCII
  semantics above), escaped syntax punctuation, and `\-` inside character
  classes.
- **Rejected, loud error**: lookahead/lookbehind, named groups,
  backreferences, mid-pattern flag groups (`(?i:…)` — not portable to the JS
  and Python engines), `\p{…}`/`\P{…}`, POSIX classes (`[[:alpha:]]`), class
  intersection (`&&`), atomic groups, conditionals, comments,
  octal/`\u`/`\x{…}` escapes, escaped space, `\-` outside a class,
  `\b`/`\B`/`\D`/`\W`/`\S` inside a class, and a bare unescaped `{` that is
  not a quantifier (engines disagree on the lenient literal reading — the
  subset requires `\{`).

Out-of-subset patterns error at evaluation — the same fail-closed discipline
as `Round`/`Modulo` — and the adversarial vectors probe exactly the
divergence points. Widening the subset later is additive within v2.

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
`leftRef`/`rightRef` instead of children; an iterating operator's children are
its source trace followed by one body trace per visited element).
Short-circuited operands are absent — the trace is truthful about what ran.
`evaluate` stays a bare value and is untouched by tracing; every parity vector
runs through both entry points and their values must agree, so they cannot
drift. The trace is a runtime return shape, not an ontology class — nothing
authors one. (A host assembling an AUTHORED report — e.g. a SHACL
ValidationReport — builds it FROM these traces; the report vocabulary lives
with the host's ontology, not here.)

## Totality

Evaluation always terminates: the operator set has no loops, no recursion
beyond tree depth, and no unbounded iteration — an expression over a finite
tree performs a bounded fold (iterating operators are bounded by their finite
source lists), ordered comparisons are set-membership lookups in supplied
data, and `Matches` runs on a ReDoS-safe subset (linear-time in RE2; no
catastrophic backtracking reachable in host engines within the subset). This
is a stated property of the runtime, not an accident: predicates evaluated
over third-party-authored rules (access decisions, policy checks, conformance
gates) may rely on it.

## Vectors

Two files, both required for a v2-complete port:

- [`vectors/expression-vectors.json`](./vectors/expression-vectors.json) —
  the v1 parity gate (69 vectors, unchanged). **Every vector passes unchanged
  under a v2 kernel**; this file is the proof that the numeric core did not
  move.
- [`vectors/expression-vectors-2.json`](./vectors/expression-vectors-2.json)
  — the v2 surface: the value domain, the list/aggregate/membership family,
  lambda binding, polymorphic `Equals`, kind predicates, and the `Matches`
  subset with its adversarial divergence probes.

`expr` is the tree; `env` binds `tx.VarRef` names for the conformance
`resolve` hook (values are Values: numbers, strings, arrays, `{"ref": …}`
objects); `refEnv` binds identity leaves for `resolveRef`; `expected` is the
exact result (`tolerance` for the few transcendentals where libm differs by
an ULP); `expectError` marks required errors; `trace` structurally asserts
the `explain` verdict tree. Every language port runs both files;
emission/build must fail if any ontology operator lacks a primitive in a
language.
