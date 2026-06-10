"""Drives the shared parity vectors through the Python kanonak-expression port.

    python conformance.py <vectors-dir>
"""

import json
import sys
from pathlib import Path

from kanonak_expression import ExpressionError, evaluate

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


def run_vectors(path):
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    total = pas = fail = 0
    for v in doc["vectors"]:
        total += 1
        vid = v["id"]
        env = v.get("env", {})
        expect_error = v.get("expectError", False)
        try:
            actual = evaluate(v["expr"], None, make_resolve(env))
            if expect_error:
                fail += 1
                print(f"  FAIL [{vid}] expected error, got {actual}")
                continue
            expected = v["expected"]
            tol = v.get("tolerance")
            if tol is not None:
                ok = abs(actual - expected) <= tol
            else:
                ok = actual == expected
            if ok:
                pas += 1
            else:
                fail += 1
                print(f"  FAIL [{vid}] expected {expected}, got {actual}")
        except Exception as e:  # noqa: BLE001
            if expect_error:
                pas += 1
            else:
                fail += 1
                print(f"  FAIL [{vid}] threw: {e}")
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
