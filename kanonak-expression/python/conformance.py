"""Drives the shared parity vectors through the Python port — BOTH files:
``expression-vectors.json`` (v1 — passes UNCHANGED under the v2 kernel; the
numeric-regression gate) and ``expression-vectors-2.json`` (the value-domain
extension). Every vector runs through ``evaluate`` AND ``explain`` and their
values must agree; ``env`` bindings and ``expected`` are Values (numbers,
strings, arrays, ``{"ref": …}`` objects); vectors with a ``trace`` assert the
verdict tree structurally.

Run:  python conformance.py ../vectors
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

from kanonak_expression import (
    EvalOptions,
    ExpressionError,
    Ref,
    TraceNode,
    evaluate,
    explain,
)

VARREF = "kanonak.org/transformations/VarRef"


def value_of(v: Any):
    """JSON → Value, the vector-file encoding. Booleans normalize to 1/0."""
    if isinstance(v, bool):
        return 1.0 if v else 0.0
    if isinstance(v, (int, float)):
        return float(v)
    if isinstance(v, str):
        return v
    if isinstance(v, list):
        return [value_of(x) for x in v]
    if isinstance(v, dict) and isinstance(v.get("ref"), str):
        return Ref(v["ref"])
    raise ValueError(f"unrepresentable vector value: {v!r}")


def values_deep_equal(a, b) -> bool:
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(values_deep_equal(x, y) for x, y in zip(a, b))
    if isinstance(a, Ref) and isinstance(b, Ref):
        return a.ref == b.ref
    if isinstance(a, str) and isinstance(b, str):
        return a == b
    if isinstance(a, (int, float)) and isinstance(b, (int, float)) \
            and not isinstance(a, bool) and not isinstance(b, bool):
        return float(a) == float(b)
    return False


def make_resolve(env):
    def resolve(node, ctx, _evaluate):
        if node.get("type") == VARREF:
            name = node.get("varName")
            if name not in env:
                raise ExpressionError(f'Unbound variable "{name}"')
            return env[name]
        raise ExpressionError(f"No resolver for leaf '{node.get('type')}'")
    return resolve


def make_resolve_ref(ref_env):
    def resolve_ref(node, ctx):
        if node.get("type") == VARREF:
            name = node.get("varName")
            if name not in ref_env:
                raise ExpressionError(f'Unbound reference "{name}"')
            return ref_env[name]
        raise ExpressionError(f"No reference resolver for leaf '{node.get('type')}'")
    return resolve_ref


def trace_matches(got: TraceNode, want: dict) -> bool:
    if want.get("type") != got.type:
        return False
    if "value" not in want or not values_deep_equal(got.value, value_of(want["value"])):
        return False
    if want.get("leftRef") != got.left_ref:
        return False
    if want.get("rightRef") != got.right_ref:
        return False
    want_children = want.get("children", [])
    if len(want_children) != len(got.children):
        return False
    return all(trace_matches(g, w) for g, w in zip(got.children, want_children))


def run_file(vectors_dir: Path, name: str) -> tuple[int, int]:
    data = json.loads((vectors_dir / name).read_text(encoding="utf-8"))
    vectors = data["vectors"]
    passed = 0
    failed = 0
    for v in vectors:
        vid = v["id"]
        env = {k: value_of(x) for k, x in (v.get("env") or {}).items()}
        ref_env = dict(v.get("refEnv") or {})
        options = EvalOptions(closures=v.get("closures"), resolve_ref=make_resolve_ref(ref_env))
        resolve = make_resolve(env)

        if v.get("expectError"):
            eval_threw = False
            explain_threw = False
            try:
                evaluate(v["expr"], None, resolve, options)
            except ExpressionError:
                eval_threw = True
            try:
                explain(v["expr"], None, resolve, options)
            except ExpressionError:
                explain_threw = True
            if eval_threw and explain_threw:
                passed += 1
            else:
                failed += 1
                print(f"{name}/{vid}: expected an error from evaluate AND explain")
            continue

        try:
            got = evaluate(v["expr"], None, resolve, options)
            trace = explain(v["expr"], None, resolve, options)
        except ExpressionError as exc:
            failed += 1
            print(f"{name}/{vid}: raised {exc}")
            continue

        expected = value_of(v["expected"])
        if "tolerance" in v:
            ok = isinstance(got, float) and abs(got - expected) <= v["tolerance"]
        else:
            ok = values_deep_equal(got, expected)
        if not ok:
            failed += 1
            print(f"{name}/{vid}: expected {expected!r} got {got!r}")
            continue
        if not values_deep_equal(trace.value, got):
            failed += 1
            print(f"{name}/{vid}: explain value {trace.value!r} != evaluate value {got!r}")
            continue
        if "trace" in v and not trace_matches(trace, v["trace"]):
            failed += 1
            print(f"{name}/{vid}: trace mismatch")
            continue
        passed += 1

    print(f"{name}: {passed}/{len(vectors)} pass")
    return passed, len(vectors)


def main() -> int:
    vectors_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("../vectors")
    p1, t1 = run_file(vectors_dir, "expression-vectors.json")
    p2, t2 = run_file(vectors_dir, "expression-vectors-2.json")
    if p1 != t1 or p2 != t2:
        print(f"\n{(t1 - p1) + (t2 - p2)} FAILURES")
        return 1
    print("ALL VECTORS PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
