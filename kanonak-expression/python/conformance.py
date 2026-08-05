"""Drives the shared parity vectors through the Python kanonak-expression port.

Every vector runs through ``evaluate`` AND ``explain`` and their values must
agree; ordered-comparison vectors supply ``closures`` and ``refEnv``, and
vectors with a ``trace`` assert the verdict tree structurally.

    python conformance.py <vectors-dir>
"""

import json
import sys
from pathlib import Path

from kanonak_expression import EvalOptions, ExpressionError, evaluate, explain

VAR_REF = "kanonak.org/transformations/VarRef"


def make_resolve(env):
    """Conformance resolve: tx.VarRef -> env[varName]; any other leaf -> error."""

    def resolve(node, ctx, _evaluate):
        if node.get("type") == VAR_REF:
            name = node["varName"]
            if name not in env:
                raise ExpressionError(f"unbound variable '{name}'")
            return float(env[name])
        raise ExpressionError(f"unresolved leaf '{node.get('type')}'")

    return resolve


def make_resolve_ref(ref_env):
    """The identity-domain mirror: tx.VarRef -> refEnv[varName] member URI."""

    def resolve_ref(node, ctx):
        if node.get("type") == VAR_REF:
            name = node["varName"]
            if name not in ref_env:
                raise ExpressionError(f"unbound reference '{name}'")
            return ref_env[name]
        raise ExpressionError(f"no reference resolver for leaf '{node.get('type')}'")

    return resolve_ref


def trace_matches(got, want):
    """Structural equality of verdict trees, including absent-vs-present refs."""
    if got.type != want.get("type") or got.value != want.get("value"):
        return False
    if got.left_ref != want.get("leftRef") or got.right_ref != want.get("rightRef"):
        return False
    want_children = want.get("children", [])
    if len(got.children) != len(want_children):
        return False
    return all(trace_matches(g, w) for g, w in zip(got.children, want_children))


def run_vectors(path):
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    total = pas = fail = 0
    for v in doc["vectors"]:
        total += 1
        vid = v["id"]
        env = v.get("env", {})
        ref_env = v.get("refEnv", {})
        expect_error = v.get("expectError", False)
        options = EvalOptions(closures=v.get("closures"), resolve_ref=make_resolve_ref(ref_env))
        resolve = make_resolve(env)

        if expect_error:
            eval_threw = explain_threw = False
            try:
                evaluate(v["expr"], None, resolve, options)
            except Exception:  # noqa: BLE001
                eval_threw = True
            try:
                explain(v["expr"], None, resolve, options)
            except Exception:  # noqa: BLE001
                explain_threw = True
            if eval_threw and explain_threw:
                pas += 1
            else:
                fail += 1
                print(f"  FAIL [{vid}] expected an error from evaluate AND explain")
            continue

        try:
            actual = evaluate(v["expr"], None, resolve, options)
            trace = explain(v["expr"], None, resolve, options)
        except Exception as e:  # noqa: BLE001
            fail += 1
            print(f"  FAIL [{vid}] threw: {e}")
            continue

        expected = v["expected"]
        tol = v.get("tolerance")
        if tol is not None:
            ok = abs(actual - expected) <= tol
        else:
            ok = actual == expected
        if not ok:
            fail += 1
            print(f"  FAIL [{vid}] expected {expected}, got {actual}")
            continue
        if trace.value != actual:
            fail += 1
            print(f"  FAIL [{vid}] explain value {trace.value} != evaluate value {actual}")
            continue
        if "trace" in v and not trace_matches(trace, v["trace"]):
            fail += 1
            print(f"  FAIL [{vid}] trace mismatch")
            continue
        pas += 1
    print(f"expression-vectors: {pas}/{total} pass")
    return fail


def main():
    vdir = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else Path(__file__).resolve().parent.parent / "vectors"
    )
    fails = run_vectors(vdir / "expression-vectors.json")
    print("\nALL VECTORS PASS" if fails == 0 else f"\n{fails} VECTOR(S) FAILED")
    sys.exit(0 if fails == 0 else 1)


if __name__ == "__main__":
    main()
