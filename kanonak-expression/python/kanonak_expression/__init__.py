"""Kanonak expression runtime (expressionRuntimeVersion "1").

A small, deterministic tree-walker that folds a ``kanonak.org/transformations``
(``tx``) + ``kanonak.org/math`` expression tree to a single number. An
independent conformant Python port of ``@kanonak-protocol/expression``, verified
against the shared parity vectors. Standard library only.

Three layers, exactly as the reference kernel establishes:

  1. DISPATCH -- ``OPERATOR_ARITY``, derived from the ``tx`` superclass hierarchy.
  2. PRIMITIVES -- ``UNARY`` / ``BINARY``, the authored determinism-bearing table.
  3. THE FOLD -- ``evaluate``: operators recurse + apply a primitive; literals
     yield their numeric value; EVERYTHING ELSE (a typed VarRef, a domain leaf,
     any future node) is handed to the caller's ``resolve(node, ctx, evaluate)``.

The runtime is a pure operator engine; binding and domain-leaf semantics are the
caller's business. It never privileges ``tx.VarRef`` -- that is just one leaf a
domain may resolve. ``EXPRESSION_RUNTIME_VERSION`` freezes the determinism
contract; a change to any primitive, value rule, or dispatch entry requires a NEW
version, never an edit in place.
"""

from __future__ import annotations

import math
from typing import Any, Callable, Dict, Mapping

# The frozen expression-runtime version (determinism contract). Not hashed.
EXPRESSION_RUNTIME_VERSION = "1"

TX = "kanonak.org/transformations"
MATH = "kanonak.org/math"

# A node is a mapping with a "type" field (canonical URI string) plus operand
# keys. ``resolve(node, ctx, evaluate) -> number`` resolves any node the kernel
# does not recognise as an operator or literal.
ExprNode = Mapping[str, Any]
Resolve = Callable[[ExprNode, Any, Callable[[ExprNode, Any], float]], float]


class ExpressionError(Exception):
    """Raised on any structural or domain error during evaluation."""


# ===========================================================================
# Dispatch -- operand shape per operator, derived from the tx superclass hierarchy
# ===========================================================================


def _un(operand: str):
    return ("unary", operand)


def _bin(left: str, right: str):
    return ("binary", left, right)


# UnaryNumericOp -> `value`; BinaryArithmetic -> arithLeft/arithRight;
# BinaryComparison -> compareLeft/compareRight; BooleanLogic -> operands list;
# Not -> operand (handled explicitly); Clip -> ternary.
_ARITH = _bin("arithLeft", "arithRight")
_COMPARE = _bin("compareLeft", "compareRight")
_VALUE = _un("value")

OPERATOR_ARITY: Dict[str, Any] = {
    f"{TX}/Add": _ARITH,
    f"{TX}/Subtract": _ARITH,
    f"{TX}/Multiply": _ARITH,
    f"{TX}/Divide": _ARITH,
    f"{MATH}/Power": _ARITH,
    f"{MATH}/Modulo": _ARITH,
    f"{MATH}/Minimum": _ARITH,
    f"{MATH}/Maximum": _ARITH,
    f"{TX}/Abs": _VALUE,
    f"{TX}/Negate": _VALUE,
    f"{MATH}/Exp": _VALUE,
    f"{MATH}/Ln": _VALUE,
    f"{MATH}/Log10": _VALUE,
    f"{MATH}/Sqrt": _VALUE,
    f"{MATH}/Floor": _VALUE,
    f"{MATH}/Ceil": _VALUE,
    f"{MATH}/Round": _VALUE,
    f"{MATH}/Sign": _VALUE,
    f"{TX}/Equals": _COMPARE,
    f"{TX}/GreaterThan": _COMPARE,
    f"{TX}/LessThan": _COMPARE,
    f"{TX}/GreaterThanOrEqual": _COMPARE,
    f"{TX}/LessThanOrEqual": _COMPARE,
    f"{TX}/And": ("nary", "operands"),
    f"{TX}/Or": ("nary", "operands"),
    # `Not` is a direct Expression subclass with boolean (not numeric-unary)
    # semantics -- handled explicitly in `evaluate`, not via the numeric tables.
    f"{MATH}/Clip": ("ternary", "clipValue", "clipLower", "clipUpper"),
}


# ===========================================================================
# Primitives -- the authored, determinism-bearing table (matched per language)
# ===========================================================================


def _require_domain(ok: bool, msg: str) -> None:
    if not ok:
        raise ExpressionError(msg)


def _floored_mod(a: float, b: float) -> float:
    """Floored modulo: Modulo(-7, 3) = 2, Modulo(7, -3) = -2."""
    if b == 0:
        raise ExpressionError("Modulo by zero")
    return a - b * math.floor(a / b)


def _round_half_away(a: float) -> float:
    """Round half away from zero: Round(2.5) = 3, Round(-2.5) = -3."""
    return math.copysign(math.floor(abs(a) + 0.5), a)


def _sign(x: float) -> float:
    if x > 0:
        return 1.0
    if x < 0:
        return -1.0
    return 0.0


def _truthy(n: float) -> bool:
    return n != 0


def _bool(b: bool) -> float:
    return 1.0 if b else 0.0


UNARY: Dict[str, Callable[[float], float]] = {
    f"{TX}/Abs": lambda x: abs(x),
    f"{TX}/Negate": lambda x: -x,
    f"{MATH}/Exp": lambda x: math.exp(x),
    f"{MATH}/Ln": lambda x: (_require_domain(x > 0, "Ln of a non-positive number"), math.log(x))[1],
    f"{MATH}/Log10": lambda x: (_require_domain(x > 0, "Log10 of a non-positive number"), math.log10(x))[1],
    f"{MATH}/Sqrt": lambda x: (_require_domain(x >= 0, "Sqrt of a negative number"), math.sqrt(x))[1],
    f"{MATH}/Floor": lambda x: float(math.floor(x)),
    f"{MATH}/Ceil": lambda x: float(math.ceil(x)),
    f"{MATH}/Round": _round_half_away,
    f"{MATH}/Sign": _sign,
}

BINARY: Dict[str, Callable[[float, float], float]] = {
    f"{TX}/Add": lambda a, b: a + b,
    f"{TX}/Subtract": lambda a, b: a - b,
    f"{TX}/Multiply": lambda a, b: a * b,
    f"{TX}/Divide": lambda a, b: (_require_domain(b != 0, "Divide by zero"), a / b)[1],
    f"{MATH}/Power": lambda a, b: math.pow(a, b),
    f"{MATH}/Modulo": _floored_mod,
    f"{MATH}/Minimum": lambda a, b: min(a, b),
    f"{MATH}/Maximum": lambda a, b: max(a, b),
    f"{TX}/Equals": lambda a, b: _bool(a == b),
    f"{TX}/GreaterThan": lambda a, b: _bool(a > b),
    f"{TX}/LessThan": lambda a, b: _bool(a < b),
    f"{TX}/GreaterThanOrEqual": lambda a, b: _bool(a >= b),
    f"{TX}/LessThanOrEqual": lambda a, b: _bool(a <= b),
}


def _literal_value(node: ExprNode):
    """Numeric value of a literal node, or ``None`` if it is not a literal."""
    t = node.get("type")
    if t == f"{TX}/IntegerLiteral":
        return float(node["integerLiteral"])
    if t == f"{TX}/DecimalLiteral":
        return float(node["decimalLiteral"])
    if t == f"{TX}/BooleanLiteral":
        v = node["booleanLiteral"]
        return _bool(v is True or v == "true")
    return None


def _operand(node: ExprNode, key: str) -> ExprNode:
    v = node.get(key)
    if not isinstance(v, Mapping):
        raise ExpressionError(f"{node.get('type')} is missing operand '{key}'")
    return v


def evaluate(node: ExprNode, ctx: Any, resolve: Resolve) -> float:
    """Evaluate an expression tree to a number.

    Operators fold via the frozen dispatch + primitive tables; literals yield
    their numeric value; any other node is delegated to ``resolve``.
    """

    def recurse(n: ExprNode, c: Any) -> float:
        return evaluate(n, c, resolve)

    node_type = node.get("type")
    arity = OPERATOR_ARITY.get(node_type)
    if arity is not None:
        kind = arity[0]
        if kind == "unary":
            x = recurse(_operand(node, arity[1]), ctx)
            return UNARY[node_type](x)
        if kind == "binary":
            a = recurse(_operand(node, arity[1]), ctx)
            b = recurse(_operand(node, arity[2]), ctx)
            return BINARY[node_type](a, b)
        if kind == "nary":
            items = node.get(arity[1])
            if not isinstance(items, (list, tuple)):
                raise ExpressionError(f"{node_type} expects an '{arity[1]}' list")
            is_and = node_type == f"{TX}/And"
            # Short-circuit; empty And is vacuously true, empty Or vacuously false.
            for item in items:
                v = _truthy(recurse(item, ctx))
                if is_and and not v:
                    return 0.0
                if not is_and and v:
                    return 1.0
            return _bool(is_and)
        if kind == "ternary":
            # Only Clip today: clamp clipValue into [clipLower, clipUpper].
            v = recurse(_operand(node, arity[1]), ctx)
            lo = recurse(_operand(node, arity[2]), ctx)
            hi = recurse(_operand(node, arity[3]), ctx)
            return min(max(v, lo), hi)

    if node_type == f"{TX}/Not":
        return _bool(not _truthy(recurse(_operand(node, "operand"), ctx)))

    lit = _literal_value(node)
    if lit is not None:
        return lit

    # Not an operator or literal -- a binding or domain leaf. The caller owns it.
    return resolve(node, ctx, recurse)


__all__ = [
    "EXPRESSION_RUNTIME_VERSION",
    "ExpressionError",
    "OPERATOR_ARITY",
    "UNARY",
    "BINARY",
    "evaluate",
]
