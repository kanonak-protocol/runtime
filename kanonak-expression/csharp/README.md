# Kanonak.Expression

The expression runtime for the [Kanonak Protocol](https://kanonak.org) — the C#
port of the `kanonak.org/transformations` + `kanonak.org/math` evaluator
(`expressionRuntimeVersion "1"`), verified against the shared parity vectors.
Targets `netstandard2.0` + `net8.0`, no third-party dependencies.

A small, deterministic tree-walker that folds a typed expression tree to a single
number — the behavior every generated Kanonak SDK evaluates through, identical
across all six language ports including the determinism traps (Round half-away,
floored Modulo, Sign(0)=0, comparisons as 1/0).

Operators and literals are the runtime's; every other node — a `tx.VarRef`, a
domain `Step` / `Time` / `Smooth` — is handed to a caller-supplied
`resolve(node, ctx, evaluate)` hook, so variable binding and domain-leaf
semantics live in the caller, not the engine.

Source & issues: https://github.com/kanonak-protocol/runtime
