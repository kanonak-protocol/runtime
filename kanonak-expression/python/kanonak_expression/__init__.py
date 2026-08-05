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

# ``resolve_ref(node, ctx) -> member URI`` resolves an identity leaf inside an
# ordered comparison -- any operand node that is not a ``tx.UriLiteral`` -- to a
# member's canonical versionless URI (``publisher/package/name``). The
# identity-domain mirror of ``Resolve``: the kernel owns the constant leaf, the
# caller owns bindings.
ResolveRef = Callable[[ExprNode, Any], str]

# The transitive closures ordered comparisons consult, keyed by the ordering
# property's canonical URI, then by member: ``closures[property][from]`` is the
# set of members ``from`` reaches. Flat, already-closed data -- typically the SDK
# reasoner's prp-trp saturation emitted at code-generation time. The kernel does
# set membership only; it never computes a closure, resolves a package, or
# reasons.
ClosureTable = Mapping[str, Mapping[str, Any]]


class ExpressionError(Exception):
    """Raised on any structural or domain error during evaluation."""


class EvalOptions:
    """Optional evaluation context for the ordered comparisons (``IsAtLeast``,
    ``Dominates``). Absent (or missing a needed entry), an ordered comparison
    fails loudly -- never a silent false from a missing table."""

    def __init__(self, closures: ClosureTable | None = None, resolve_ref: ResolveRef | None = None):
        self.closures = closures
        self.resolve_ref = resolve_ref


class TraceNode:
    """One node of an evaluation trace -- the verdict tree ``explain`` returns.

    Mirrors the expression: ``type`` is the node's type URI, ``value`` its
    result (``1``/``0`` for booleans), ``children`` the operand traces in
    evaluation order. A short-circuited operand is simply ABSENT from
    ``children`` -- the trace is truthful about what ran. Ordered comparisons
    carry their resolved operand identities as ``left_ref``/``right_ref``
    instead of children. A runtime return shape, not an ontology class.
    """

    def __init__(self, type_: str, value: float, children=None, left_ref=None, right_ref=None):
        self.type = type_
        self.value = value
        self.children = children if children is not None else []
        self.left_ref = left_ref
        self.right_ref = right_ref


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


def _identity_of(node: ExprNode, ctx: Any, options: EvalOptions | None) -> str:
    """The identity an ordered comparison compares -- a member's canonical
    versionless URI. ``tx.UriLiteral`` is the kernel-known constant leaf (its
    ``refTo`` IS the identity, the way a literal's value is its number); every
    other node is the caller's, through ``options.resolve_ref``."""
    if node.get("type") == f"{TX}/UriLiteral":
        ref = node.get("refTo")
        if not isinstance(ref, str) or not ref:
            raise ExpressionError("UriLiteral is missing refTo")
        return ref
    if options is None or options.resolve_ref is None:
        raise ExpressionError(f"No resolveRef supplied for identity leaf '{node.get('type')}'")
    return options.resolve_ref(node, ctx)


def _fold_ordered(node: ExprNode, ctx: Any, options: EvalOptions | None):
    """Fold ``IsAtLeast`` / ``Dominates`` to ``(value, left, right)``.

    The ordering is the supplied closure for the node's ``viaProperty`` --
    membership in already-closed data, nothing more. Identity is canonical
    versionless URI string equality, matching ``tx.Equals``' identity rule.
    ``IsAtLeast`` folds reflexivity into the operator (same member -> 1);
    ``Dominates`` is strict (same member -> 0). Two members with no path yield
    0 -- fail-closed -- but a MISSING closure table is a configuration failure
    and errors loudly.
    """
    node_type = node.get("type")
    via = node.get("viaProperty")
    if not isinstance(via, str) or not via:
        raise ExpressionError(f"{node_type} is missing viaProperty")
    left = _identity_of(_operand(node, "compareLeft"), ctx, options)
    right = _identity_of(_operand(node, "compareRight"), ctx, options)
    closure = None
    if options is not None and options.closures is not None:
        closure = options.closures.get(via)
    if closure is None:
        raise ExpressionError(f"No closure supplied for ordering property '{via}'")
    if left == right:
        return _bool(node_type == f"{TX}/IsAtLeast"), left, right
    return _bool(right in (closure.get(left) or [])), left, right


def evaluate(node: ExprNode, ctx: Any, resolve: Resolve, options: EvalOptions | None = None) -> float:
    """Evaluate an expression tree to a number.

    Operators fold via the frozen dispatch + primitive tables; literals yield
    their numeric value; any other node is delegated to ``resolve``.
    ``options`` carries the ordered-comparison context (closures +
    identity-leaf resolution) and is only consulted when an ``IsAtLeast`` /
    ``Dominates`` node is reached.
    """

    def recurse(n: ExprNode, c: Any) -> float:
        return evaluate(n, c, resolve, options)

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

    if node_type in (f"{TX}/IsAtLeast", f"{TX}/Dominates"):
        return _fold_ordered(node, ctx, options)[0]

    lit = _literal_value(node)
    if lit is not None:
        return lit

    # Not an operator or literal -- a binding or domain leaf. The caller owns it.
    return resolve(node, ctx, recurse)


def explain(node: ExprNode, ctx: Any, resolve: Resolve, options: EvalOptions | None = None) -> TraceNode:
    """Evaluate an expression tree and return the verdict tree.

    The regex-debugger view: every evaluated node, its own result, and (for
    ordered comparisons) the identities it compared. The root's ``value`` is
    exactly what ``evaluate`` returns for the same inputs; the conformance
    suite runs every vector through both and requires agreement, so the two
    entry points cannot drift. Kept separate from ``evaluate`` so the hot path
    never pays for trace allocation. Errors propagate exactly as in
    ``evaluate`` -- a failed evaluation raises, never a partial trace.
    """

    # Numeric recursion for subtrees the caller's ``resolve`` re-enters: those
    # folds happen inside the caller and are invisible to the trace. Only
    # kernel-visited nodes appear.
    def recurse_value(n: ExprNode, c: Any) -> float:
        return evaluate(n, c, resolve, options)

    def trace(n: ExprNode, c: Any) -> TraceNode:
        node_type = n.get("type")
        arity = OPERATOR_ARITY.get(node_type)
        if arity is not None:
            kind = arity[0]
            if kind == "unary":
                x = trace(_operand(n, arity[1]), c)
                return TraceNode(node_type, UNARY[node_type](x.value), [x])
            if kind == "binary":
                a = trace(_operand(n, arity[1]), c)
                b = trace(_operand(n, arity[2]), c)
                return TraceNode(node_type, BINARY[node_type](a.value, b.value), [a, b])
            if kind == "nary":
                items = n.get(arity[1])
                if not isinstance(items, (list, tuple)):
                    raise ExpressionError(f"{node_type} expects an '{arity[1]}' list")
                is_and = node_type == f"{TX}/And"
                children = []
                for item in items:
                    child = trace(item, c)
                    children.append(child)
                    v = _truthy(child.value)
                    # Same short-circuit as ``evaluate``: operands after the
                    # deciding one are never evaluated and never appear.
                    if is_and and not v:
                        return TraceNode(node_type, 0.0, children)
                    if not is_and and v:
                        return TraceNode(node_type, 1.0, children)
                return TraceNode(node_type, _bool(is_and), children)
            if kind == "ternary":
                v = trace(_operand(n, arity[1]), c)
                lo = trace(_operand(n, arity[2]), c)
                hi = trace(_operand(n, arity[3]), c)
                return TraceNode(node_type, min(max(v.value, lo.value), hi.value), [v, lo, hi])

        if node_type == f"{TX}/Not":
            x = trace(_operand(n, "operand"), c)
            return TraceNode(node_type, _bool(not _truthy(x.value)), [x])

        if node_type in (f"{TX}/IsAtLeast", f"{TX}/Dominates"):
            value, left, right = _fold_ordered(n, c, options)
            return TraceNode(node_type, value, [], left, right)

        lit = _literal_value(n)
        if lit is not None:
            return TraceNode(node_type, lit)

        return TraceNode(node_type, resolve(n, c, recurse_value))

    return trace(node, ctx)


__all__ = [
    "EXPRESSION_RUNTIME_VERSION",
    "ExpressionError",
    "EvalOptions",
    "TraceNode",
    "OPERATOR_ARITY",
    "UNARY",
    "BINARY",
    "evaluate",
    "explain",
]
