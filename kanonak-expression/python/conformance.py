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
    AlignedNode,
    EvalOptions,
    ExpressionError,
    Ref,
    TraceNode,
    align,
    evaluate,
    explain,
)

VARREF = "kanonak.org/transformations/VarRef"
PROPERTY_READ = "kanonak.org/transformations/PropertyRead"


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


def make_resolve(env, graph):
    """The caller's resolve: tx.VarRef -> env binding; tx.PropertyRead -> a host
    graph read (the documented caller-leaf shape -- the kernel never touches a
    graph). PropertyRead is the reference engine's convention, pinned by the
    vectors so every port's harness agrees: ``readSource`` is evaluated through
    the handed-back ``evaluate`` (so a loopVar bound by an enclosing iterator is
    visible -- runtime#25) and MUST yield a ref; then graph[ref][readProp] is
    absent -> the empty list, several values -> a list, one value -> itself."""
    def resolve(node, ctx, evaluate):
        typ = node.get("type")
        if typ == VARREF:
            name = node.get("varName")
            if name not in env:
                raise ExpressionError(f'Unbound variable "{name}"')
            return env[name]
        if typ == PROPERTY_READ:
            source = evaluate(node["readSource"], ctx)
            if not isinstance(source, Ref):
                raise ExpressionError(f"PropertyRead over a non-ref: {source!r}")
            values = graph.get(source.ref, {}).get(node.get("readProp"))
            return [] if values is None else values
        raise ExpressionError(f"No resolver for leaf '{typ}'")
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
        graph = {
            uri: {prop: value_of(x) for prop, x in props.items()}
            for uri, props in (v.get("graph") or {}).items()
        }
        options = EvalOptions(closures=v.get("closures"), resolve_ref=make_resolve_ref(ref_env))
        resolve = make_resolve(env, graph)

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


def aligned_matches(got: AlignedNode, want: dict) -> bool:
    """Structural equality of an aligned tree against the vector's expected tree."""
    if got.expr.get("type") != want.get("type") or got.trace.type != want.get("type"):
        return False
    if got.operand != want.get("operand") or got.index != want.get("index"):
        return False
    wel = want.get("element")
    if (got.element is None) != (wel is None):
        return False
    if got.element is not None and (got.element["loopVar"] != wel.get("loopVar")
                                    or not values_deep_equal(got.element["value"], value_of(wel["value"]))):
        return False
    if "value" not in want or not values_deep_equal(got.trace.value, value_of(want["value"])):
        return False
    want_children = want.get("children", [])
    if len(want_children) != len(got.children):
        return False
    return all(aligned_matches(g, w) for g, w in zip(got.children, want_children))


def run_align_file(vectors_dir: Path, name: str) -> tuple[int, int]:
    """The alignment vectors: explain ``expr``, then ``align`` — the pairing
    must be exactly the expected tree, or an error where one is expected."""
    data = json.loads((vectors_dir / name).read_text(encoding="utf-8"))
    vectors = data["vectors"]
    passed = 0
    for v in vectors:
        vid = v["id"]
        env = {k: value_of(x) for k, x in (v.get("env") or {}).items()}
        graph = {uri: {p: value_of(x) for p, x in props.items()} for uri, props in (v.get("graph") or {}).items()}
        options = EvalOptions(closures=v.get("closures"), resolve_ref=make_resolve_ref(dict(v.get("refEnv") or {})))
        resolve = make_resolve(env, graph)
        try:
            trace = explain(v["expr"], None, resolve, options)
        except ExpressionError as exc:
            print(f"{name}/{vid}: explain raised {exc}")
            continue
        if v.get("expectError"):
            try:
                align(v.get("alignExpr") or v["expr"], trace)
                print(f"{name}/{vid}: expected align to reject the trace")
            except ExpressionError:
                passed += 1
            continue
        try:
            got = align(v["expr"], trace)
        except ExpressionError as exc:
            print(f"{name}/{vid}: align raised {exc}")
            continue
        if not aligned_matches(got, v["expected"]):
            print(f"{name}/{vid}: alignment mismatch")
            continue
        passed += 1
    print(f"{name}: {passed}/{len(vectors)} pass")
    return passed, len(vectors)


def main() -> int:
    vectors_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("../vectors")
    p1, t1 = run_file(vectors_dir, "expression-vectors.json")
    p2, t2 = run_file(vectors_dir, "expression-vectors-2.json")
    p3, t3 = run_align_file(vectors_dir, "expression-alignment-vectors.json")
    if p1 != t1 or p2 != t2 or p3 != t3:
        print(f"\n{(t1 - p1) + (t2 - p2) + (t3 - p3)} FAILURES")
        return 1
    print("ALL VECTORS PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
