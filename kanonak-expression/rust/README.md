# kanonak-expression

The expression runtime for the [Kanonak Protocol](https://kanonak.org) — the Rust
port of the `kanonak.org/transformations` + `kanonak.org/math` evaluator
(`expressionRuntimeVersion "1"`), verified against the shared parity vectors.

A small, deterministic tree-walker that folds a typed expression to a single
number: 27 operators with ontology-derived dispatch, a frozen determinism
contract (Round half-away-from-zero, floored Modulo, Sign(0)=0, comparisons as
1/0, domain errors never `NaN`/`Inf`), and one `resolve(node, ctx, evaluate)`
hook for variable bindings and domain leaves. Byte-identical results across all
six language ports.

The behavior every generated Kanonak SDK evaluates through — the third member of
the codec family (`canonical` content-addresses, `codec` serializes,
`expression` evaluates).

Source & issues: https://github.com/kanonak-protocol/runtime
