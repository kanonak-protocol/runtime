"""Kanonak expression runtime (expressionRuntimeVersion "2").

A small, deterministic tree-walker that folds a ``kanonak.org/transformations``
(``tx``) + ``kanonak.org/math`` expression tree to a VALUE. An independent
conformant Python port of ``@kanonak-protocol/expression``, verified against
the shared parity vectors. Standard library only.

VALUE DOMAIN (v2): ``number | string | ref | list``. Booleans and comparison
results remain ``1.0``/``0.0`` numbers; a :class:`Ref` is a canonical
versionless URI identity (distinct from ``str`` so ``Equals`` holds the
cross-kind-is-false rule); ABSENCE IS THE EMPTY LIST -- there is no None. The
caller's ``resolve`` may return any value: a property read is just a caller
leaf that returns a list. The kernel NEVER touches a graph.

ERROR CONTRACT: computations fail LOUD (arithmetic on a non-number, an
aggregate over non-numeric elements, Min/Max/Average on empty, a nested list
in Join, an out-of-subset Matches pattern); predicates fail CLOSED (``Equals``
cross-kind and the ordering comparisons on non-numbers yield ``0.0``).

LAMBDA BINDING: Filter/ListMap/ForEach bind their ``loopVar`` per element;
within their bodies -- and only there -- a ``tx.VarRef`` naming a
lexically-enclosing loopVar is resolved by the kernel (innermost binder wins).
Recursion re-entered from inside ``resolve`` carries no frames.

MATCHES: the pinned RE2-compatible XSD-regex subset. ``.`` and quantifiers
count code points (Python's native model); the shorthand classes are ASCII by
textual expansion; the pattern compiles with ``re.ASCII`` so the remaining
``\\b``/``\\B`` are ASCII word boundaries. Out-of-subset constructs are loud
errors.

``EXPRESSION_RUNTIME_VERSION`` freezes the determinism contract; a change to
any primitive, value rule, or dispatch entry requires a NEW version, never an
edit in place. Adding an operator (or widening the Matches subset) is additive
WITHIN a version.
"""

from __future__ import annotations

import math
import re as _re
from typing import Any, Callable, Dict, List, Mapping, Optional, Tuple, Union

# The frozen expression-runtime version (determinism contract). Not hashed.
EXPRESSION_RUNTIME_VERSION = "2"

TX = "kanonak.org/transformations"
MATH = "kanonak.org/math"


class Ref:
    """A reference value -- a member's canonical versionless URI identity."""

    __slots__ = ("ref",)

    def __init__(self, ref: str) -> None:
        self.ref = ref

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Ref) and other.ref == self.ref

    def __hash__(self) -> int:
        return hash(("Ref", self.ref))

    def __repr__(self) -> str:
        return f"Ref({self.ref!r})"


# The v2 value domain. Booleans are 1.0/0.0 floats; absence is the empty list.
Value = Union[float, str, Ref, List["Value"]]

ExprNode = Mapping[str, Any]
Resolve = Callable[[ExprNode, Any, Callable[[ExprNode, Any], "Value"]], "Value"]
ResolveRef = Callable[[ExprNode, Any], str]
ClosureTable = Mapping[str, Mapping[str, Any]]


class ExpressionError(Exception):
    """Raised on any structural or domain error during evaluation."""


class EvalOptions:
    """Optional evaluation context for the ordered comparisons."""

    def __init__(
        self,
        closures: Optional[ClosureTable] = None,
        resolve_ref: Optional[ResolveRef] = None,
    ) -> None:
        self.closures = closures
        self.resolve_ref = resolve_ref


def _is_number(v: Value) -> bool:
    # bool is an int subclass in Python; the domain has no booleans.
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _kind(v: Value) -> str:
    if _is_number(v):
        return "number"
    if isinstance(v, str):
        return "string"
    if isinstance(v, Ref):
        return "ref"
    return "list"


def _require_num(v: Value, op: str) -> float:
    if not _is_number(v):
        raise ExpressionError(f"{op} requires a numeric operand, got {_kind(v)}")
    return float(v)


def _truthy(n: float) -> bool:
    return n != 0.0


def _bool(b: bool) -> float:
    return 1.0 if b else 0.0


# ---------------------------------------------------------------------------
# Dispatch — derived from the ontology, frozen.
# ---------------------------------------------------------------------------

_ARITH = ("binary", "arithLeft", "arithRight")
_COMPARE = ("binary", "compareLeft", "compareRight")
_VALUE = ("unary", "value")

OPERATOR_ARITY: Dict[str, tuple] = {
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
    f"{MATH}/Clip": ("ternary", "clipValue", "clipLower", "clipUpper"),
}

_ITERATOR_BODY: Dict[str, str] = {
    f"{TX}/ForEach": "emit",
    f"{TX}/ListMap": "mapBody",
    f"{TX}/Filter": "predicate",
}

_LIST_FOLDS = {
    f"{TX}/Count",
    f"{TX}/Sum",
    f"{TX}/Min",
    f"{TX}/Max",
    f"{TX}/Average",
    f"{TX}/Join",
    f"{TX}/Reverse",
}

_KIND_PREDICATES: Dict[str, Callable[[Value], bool]] = {
    f"{TX}/IsString": lambda v: isinstance(v, str),
    f"{TX}/IsNumber": _is_number,
    f"{TX}/IsReference": lambda v: isinstance(v, Ref),
    f"{TX}/IsList": lambda v: isinstance(v, list),
}


# ---------------------------------------------------------------------------
# Primitives — the authored, determinism-bearing folds.
# ---------------------------------------------------------------------------

def _floored_mod(a: float, b: float) -> float:
    if b == 0.0:
        raise ExpressionError("Modulo by zero")
    return a - b * math.floor(a / b)


def _round_half_away(a: float) -> float:
    return -math.floor(-a + 0.5) if a < 0 else math.floor(a + 0.5)


def _require_domain(ok: bool, msg: str) -> None:
    if not ok:
        raise ExpressionError(msg)


def _ln(x: float) -> float:
    _require_domain(x > 0, "Ln of a non-positive number")
    return math.log(x)


def _log10(x: float) -> float:
    _require_domain(x > 0, "Log10 of a non-positive number")
    return math.log10(x)


def _sqrt(x: float) -> float:
    _require_domain(x >= 0, "Sqrt of a negative number")
    return math.sqrt(x)


def _divide(a: float, b: float) -> float:
    _require_domain(b != 0, "Divide by zero")
    return a / b


UNARY: Dict[str, Callable[[float], float]] = {
    f"{TX}/Abs": abs,
    f"{TX}/Negate": lambda x: -x,
    f"{MATH}/Exp": math.exp,
    f"{MATH}/Ln": _ln,
    f"{MATH}/Log10": _log10,
    f"{MATH}/Sqrt": _sqrt,
    f"{MATH}/Floor": lambda x: float(math.floor(x)),
    f"{MATH}/Ceil": lambda x: float(math.ceil(x)),
    f"{MATH}/Round": _round_half_away,
    f"{MATH}/Sign": lambda x: math.copysign(1.0, x) if x != 0 else 0.0,
}

BINARY_ARITH: Dict[str, Callable[[float, float], float]] = {
    f"{TX}/Add": lambda a, b: a + b,
    f"{TX}/Subtract": lambda a, b: a - b,
    f"{TX}/Multiply": lambda a, b: a * b,
    f"{TX}/Divide": _divide,
    f"{MATH}/Power": lambda a, b: math.pow(a, b),
    f"{MATH}/Modulo": _floored_mod,
    f"{MATH}/Minimum": min,
    f"{MATH}/Maximum": max,
}

# Ordering comparisons are PREDICATES: on non-numeric operands they fail
# CLOSED (0.0) — see the fold. This table receives numbers only.
BINARY_ORDER: Dict[str, Callable[[float, float], float]] = {
    f"{TX}/GreaterThan": lambda a, b: _bool(a > b),
    f"{TX}/LessThan": lambda a, b: _bool(a < b),
    f"{TX}/GreaterThanOrEqual": lambda a, b: _bool(a >= b),
    f"{TX}/LessThanOrEqual": lambda a, b: _bool(a <= b),
}


def _values_equal(a: Value, b: Value) -> bool:
    """Polymorphic ``tx.Equals``: scalars by value, refs by URI identity,
    lists never equal, cross-kind false. Never errors."""
    if _is_number(a) and _is_number(b):
        return float(a) == float(b)
    if isinstance(a, str) and isinstance(b, str):
        return a == b
    if isinstance(a, Ref) and isinstance(b, Ref):
        return a.ref == b.ref
    return False


def _literal_value(node: ExprNode, typ: str) -> Optional[Value]:
    if typ == f"{TX}/IntegerLiteral":
        return float(node.get("integerLiteral"))
    if typ == f"{TX}/DecimalLiteral":
        return float(node.get("decimalLiteral"))
    if typ == f"{TX}/BooleanLiteral":
        raw = node.get("booleanLiteral")
        return _bool(raw is True or raw == "true")
    if typ == f"{TX}/StringLiteral":
        s = node.get("stringLiteral")
        if not isinstance(s, str):
            raise ExpressionError("StringLiteral is missing stringLiteral")
        return s
    if typ == f"{TX}/UriLiteral":
        ref = node.get("refTo")
        if not isinstance(ref, str) or not ref:
            raise ExpressionError("UriLiteral is missing refTo")
        return Ref(ref)
    return None


def _format_number(n: float) -> str:
    """ECMAScript-style number-to-string for the always-finite domain:
    integral values render without a decimal point (the RFC 8785 rule)."""
    if n == int(n) and abs(n) < 1e21:
        return str(int(n))
    return repr(n)


def _join_element(v: Value) -> str:
    if isinstance(v, str):
        return v
    if _is_number(v):
        return _format_number(float(v))
    if isinstance(v, Ref):
        return v.ref.rsplit("/", 1)[-1]
    raise ExpressionError("Join cannot stringify a nested list")


def _is_set(v: Value) -> bool:
    if isinstance(v, str):
        return len(v) > 0
    if isinstance(v, list):
        return len(v) > 0
    return True


def _list_fold(typ: str, items: List[Value], node: ExprNode) -> Value:
    name = typ.rsplit("/", 1)[-1]
    if typ == f"{TX}/Count":
        return float(len(items))
    if typ == f"{TX}/Sum":
        return float(sum(_require_num(el, "Sum") for el in items))
    if typ in (f"{TX}/Min", f"{TX}/Max"):
        if not items:
            raise ExpressionError(f"{name} on an empty list is undefined; guard with IsSet")
        nums = [_require_num(el, name) for el in items]
        return min(nums) if name == "Min" else max(nums)
    if typ == f"{TX}/Average":
        if not items:
            raise ExpressionError("Average on an empty list is undefined; guard with IsSet")
        return float(sum(_require_num(el, "Average") for el in items)) / len(items)
    if typ == f"{TX}/Join":
        sep = node.get("separator")
        sep = sep if isinstance(sep, str) else ""
        return sep.join(_join_element(el) for el in items)
    if typ == f"{TX}/Reverse":
        return list(reversed(items))
    raise ExpressionError(f"No list fold for {typ}")


# ---------------------------------------------------------------------------
# Matches — the pinned RE2-compatible XSD-regex subset.
# ---------------------------------------------------------------------------

_ALLOWED_ESCAPES = set("dDwWsSbBnrtfv.*+?()[]{}|^$\\/")

_FLAG_PREFIX = _re.compile(r"^\(\?([ims]+)\)")


def _split_flag_prefix(pattern: str) -> Tuple[str, str]:
    m = _FLAG_PREFIX.match(pattern)
    if not m:
        return "", pattern
    return m.group(1), pattern[m.end():]


def validate_matches_pattern(pattern: str) -> None:
    """The same subset scanner as the reference kernel — every port rejects
    the same constructs with a loud error."""

    def fail(what: str) -> None:
        raise ExpressionError(
            f"Matches pattern is outside the pinned regex subset ({what}): {pattern}"
        )

    in_class = False
    i = 0
    n = len(pattern)
    while i < n:
        c = pattern[i]
        if c == "\\":
            if i + 1 >= n:
                fail("trailing backslash")
            e = pattern[i + 1]
            if e == "x":
                rest = pattern[i + 2 : i + 4]
                if len(rest) > 0 and rest[0] == "{":
                    fail("\\x{…} escape")
                if not _re.fullmatch(r"[0-9a-fA-F]{2}", rest or ""):
                    fail("\\x escape must be \\xHH")
                i += 4
                continue
            if e.isdigit():
                fail(f"backreference or octal escape \\{e}")
            if e in ("p", "P"):
                fail(f"unicode property class \\{e}{{…}}")
            if e == "k":
                fail("named backreference \\k")
            if e == "u":
                fail("\\u escape")
            if e == "-":
                if not in_class:
                    fail("\\- outside a character class")
                i += 2
                continue
            if e in ("b", "B", "D", "W", "S") and in_class:
                fail(f"\\{e} inside a character class")
            if e not in _ALLOWED_ESCAPES:
                fail(f"escape \\{e}")
            i += 2
            continue
        if in_class:
            if c == "]":
                in_class = False
            elif c == "&" and i + 1 < n and pattern[i + 1] == "&":
                fail("character-class intersection &&")
            elif c == "[" and i + 1 < n and pattern[i + 1] == ":":
                fail("POSIX class [[:…:]]")
            i += 1
            continue
        if c == "[":
            in_class = True
            i += 1
            continue
        if c == "(" and i + 1 < n and pattern[i + 1] == "?":
            # Only (?: survives mid-pattern; the flag prefix splits off first.
            if i + 2 >= n or pattern[i + 2] != ":":
                after = pattern[i + 2] if i + 2 < n else ""
                fail(f"group construct (?{after}")
            i += 3
            continue
        if c == "{":
            # A bare `{` must start a valid quantifier — engines disagree on
            # the lenient literal reading, so the SCANNER enforces the rule
            # uniformly (a literal brace is written \{).
            if not _re.match(r"\{\d+(,\d*)?\}", pattern[i:]):
                fail("bare '{' that is not a quantifier (write \\{)")
            i += 1
            continue
        i += 1
    if in_class:
        fail("unterminated character class")


_EXPANSION_OUT = {
    "d": "[0-9]",
    "D": "[^0-9]",
    "w": "[0-9A-Za-z_]",
    "W": "[^0-9A-Za-z_]",
    "s": "[ \\t\\n\\r\\f\\x0B]",
    "S": "[^ \\t\\n\\r\\f\\x0B]",
}
_EXPANSION_IN = {
    "d": "0-9",
    "w": "0-9A-Za-z_",
    "s": " \\t\\n\\r\\f\\x0B",
}


def _expand_shorthand_classes(body: str) -> str:
    """The pinned ASCII expansions — the dialect DEFINES the shorthands by
    these, applied textually before compiling. ``\\b``/``\\B`` stay as-written;
    ``re.ASCII`` at compile time makes them the ASCII word boundary."""
    out: List[str] = []
    in_class = False
    i = 0
    n = len(body)
    while i < n:
        c = body[i]
        if c == "\\":
            e = body[i + 1]  # subset-validated: never a trailing backslash
            table = _EXPANSION_IN if in_class else _EXPANSION_OUT
            expansion = table.get(e)
            out.append(expansion if expansion is not None else c + e)
            i += 2
            continue
        if not in_class and c == "[":
            in_class = True
        elif in_class and c == "]":
            in_class = False
        out.append(c)
        i += 1
    return "".join(out)


def _matches_pattern(text: str, pattern: str) -> bool:
    """fn:matches semantics: UNANCHORED. ``.`` counts code points (native);
    ``re.ASCII`` pins the remaining ``\\b``/``\\B`` to the ASCII boundary."""
    flags_str, body = _split_flag_prefix(pattern)
    validate_matches_pattern(body)
    flags = _re.ASCII
    if "i" in flags_str:
        flags |= _re.IGNORECASE
    if "m" in flags_str:
        flags |= _re.MULTILINE
    if "s" in flags_str:
        flags |= _re.DOTALL
    try:
        compiled = _re.compile(_expand_shorthand_classes(body), flags)
    except _re.error as exc:
        raise ExpressionError(f"Matches pattern does not compile: {pattern}") from exc
    return compiled.search(text) is not None


# ---------------------------------------------------------------------------
# Ordered comparisons (unchanged from v1).
# ---------------------------------------------------------------------------

def _identity_of(node: ExprNode, ctx: Any, options: Optional[EvalOptions]) -> str:
    typ = node.get("type")
    if typ == f"{TX}/UriLiteral":
        ref = node.get("refTo")
        if not isinstance(ref, str) or not ref:
            raise ExpressionError("UriLiteral is missing refTo")
        return ref
    if options is None or options.resolve_ref is None:
        raise ExpressionError(f"No resolveRef supplied for identity leaf '{typ}'")
    return options.resolve_ref(node, ctx)


def _fold_ordered(
    node: ExprNode, typ: str, ctx: Any, options: Optional[EvalOptions]
) -> Tuple[float, str, str]:
    via = node.get("viaProperty")
    if not isinstance(via, str) or not via:
        raise ExpressionError(f"{typ} is missing viaProperty")
    left = _identity_of(_operand(node, typ, "compareLeft"), ctx, options)
    right = _identity_of(_operand(node, typ, "compareRight"), ctx, options)
    closure = None if options is None or options.closures is None else options.closures.get(via)
    if closure is None:
        raise ExpressionError(f"No closure supplied for ordering property '{via}'")
    if left == right:
        value = _bool(typ == f"{TX}/IsAtLeast")
    else:
        value = _bool(right in (closure.get(left) or []))
    return value, left, right


def _operand(node: ExprNode, typ: str, key: str) -> ExprNode:
    v = node.get(key)
    if not isinstance(v, Mapping):
        raise ExpressionError(f"{typ} is missing operand '{key}'")
    return v


# ---------------------------------------------------------------------------
# The fold.
# ---------------------------------------------------------------------------

Frames = List[Tuple[str, Value]]


def _bound_value(frames: Frames, name: str) -> Optional[Value]:
    for n, v in reversed(frames):
        if n == name:
            return v
    return None


def evaluate(
    node: ExprNode,
    ctx: Any,
    resolve: Resolve,
    options: Optional[EvalOptions] = None,
) -> Value:
    """Evaluate an expression tree to a value."""
    return _go(node, ctx, resolve, options, [])


def _source_list(
    node: ExprNode, typ: str, ctx: Any, resolve: Resolve,
    options: Optional[EvalOptions], frames: Frames,
) -> List[Value]:
    v = _go(_operand(node, typ, "source"), ctx, resolve, options, frames)
    return v if isinstance(v, list) else [v]


def _go(
    node: ExprNode, ctx: Any, resolve: Resolve,
    options: Optional[EvalOptions], frames: Frames,
) -> Value:
    typ = node.get("type")
    if not isinstance(typ, str):
        raise ExpressionError("Expression node has no 'type'")

    arity = OPERATOR_ARITY.get(typ)
    if arity is not None:
        kind = arity[0]
        if kind == "unary":
            x = _go(_operand(node, typ, arity[1]), ctx, resolve, options, frames)
            return UNARY[typ](_require_num(x, typ))
        if kind == "binary":
            a = _go(_operand(node, typ, arity[1]), ctx, resolve, options, frames)
            b = _go(_operand(node, typ, arity[2]), ctx, resolve, options, frames)
            if typ == f"{TX}/Equals":
                return _bool(_values_equal(a, b))
            order = BINARY_ORDER.get(typ)
            if order is not None:
                # Predicate: non-numeric operands fail CLOSED.
                if not _is_number(a) or not _is_number(b):
                    return 0.0
                return order(float(a), float(b))
            # Arithmetic: computation — fail LOUD on non-numbers.
            return BINARY_ARITH[typ](_require_num(a, typ), _require_num(b, typ))
        if kind == "nary":
            items = node.get(arity[1])
            if not isinstance(items, list):
                raise ExpressionError(f"{typ} expects an '{arity[1]}' list")
            is_and = typ == f"{TX}/And"
            # Short-circuit; empty And vacuously true, empty Or false.
            for item in items:
                v = _truthy(_require_num(_go(item, ctx, resolve, options, frames), typ))
                if is_and and not v:
                    return 0.0
                if not is_and and v:
                    return 1.0
            return _bool(is_and)
        # ternary — only Clip today.
        v = _require_num(_go(_operand(node, typ, arity[1]), ctx, resolve, options, frames), typ)
        lo = _require_num(_go(_operand(node, typ, arity[2]), ctx, resolve, options, frames), typ)
        hi = _require_num(_go(_operand(node, typ, arity[3]), ctx, resolve, options, frames), typ)
        return min(max(v, lo), hi)

    if typ == f"{TX}/Not":
        x = _go(_operand(node, typ, "operand"), ctx, resolve, options, frames)
        return _bool(not _truthy(_require_num(x, typ)))

    if typ in (f"{TX}/IsAtLeast", f"{TX}/Dominates"):
        value, _, _ = _fold_ordered(node, typ, ctx, options)
        return value

    if typ in _LIST_FOLDS:
        return _list_fold(typ, _source_list(node, typ, ctx, resolve, options, frames), node)

    body_key = _ITERATOR_BODY.get(typ)
    if body_key is not None:
        loop_var = node.get("loopVar")
        if not isinstance(loop_var, str) or not loop_var:
            raise ExpressionError(f"{typ} is missing loopVar")
        items = _source_list(node, typ, ctx, resolve, options, frames)
        body = _operand(node, typ, body_key)
        out: List[Value] = []
        for el in items:
            frames.append((loop_var, el))
            try:
                v = _go(body, ctx, resolve, options, frames)
            finally:
                frames.pop()
            if typ == f"{TX}/Filter":
                if _truthy(_require_num(v, "Filter predicate")):
                    out.append(el)
            elif typ == f"{TX}/ForEach":
                # Flatten one level; an empty list contributes nothing —
                # the absence rule doing the reference engine's skip.
                if isinstance(v, list):
                    out.extend(v)
                else:
                    out.append(v)
            else:
                out.append(v)
        return out

    if typ == f"{TX}/Contains":
        hay = _go(_operand(node, typ, "haystack"), ctx, resolve, options, frames)
        needle = _go(_operand(node, typ, "needle"), ctx, resolve, options, frames)
        items = hay if isinstance(hay, list) else [hay]
        return _bool(any(_values_equal(el, needle) for el in items))

    if typ == f"{TX}/IsSet":
        return _bool(_is_set(_go(_operand(node, typ, "checkExpr"), ctx, resolve, options, frames)))

    if typ == f"{TX}/ListItemAt":
        items = _source_list(node, typ, ctx, resolve, options, frames)
        idx = _go(_operand(node, typ, "itemIndex"), ctx, resolve, options, frames)
        if not _is_number(idx) or float(idx) != int(idx) or idx < 0:
            raise ExpressionError("ListItemAt itemIndex must be a non-negative integer")
        i = int(idx)
        # Past the end is ABSENCE (the empty list); guard with IsSet.
        return items[i] if i < len(items) else []

    if typ == f"{TX}/Matches":
        src = _go(_operand(node, typ, "matchSource"), ctx, resolve, options, frames)
        if not isinstance(src, str):
            raise ExpressionError(f"Matches requires a string matchSource, got {_kind(src)}")
        pattern = node.get("pattern")
        if not isinstance(pattern, str):
            raise ExpressionError("Matches is missing pattern")
        return _bool(_matches_pattern(src, pattern))

    kind_pred = _KIND_PREDICATES.get(typ)
    if kind_pred is not None:
        return _bool(kind_pred(_go(_operand(node, typ, "kindCheck"), ctx, resolve, options, frames)))

    lit = _literal_value(node, typ)
    if lit is not None:
        return lit

    # A VarRef naming a lexically-enclosing loopVar is the kernel's own bound
    # variable — the ONLY leaf the kernel answers. Everything else is the
    # caller's; recursion from inside `resolve` re-enters WITHOUT frames.
    if typ == f"{TX}/VarRef":
        name = node.get("varName")
        if isinstance(name, str):
            bound = _bound_value(frames, name)
            if bound is not None:
                return bound

    return resolve(node, ctx, lambda n, c: _go(n, c, resolve, options, []))


# ---------------------------------------------------------------------------
# explain — the verdict tree.
# ---------------------------------------------------------------------------

class TraceNode:
    """One node of an evaluation trace. A runtime return shape, not an
    ontology class. Short-circuited operands are absent from ``children``;
    an iterating operator's children are its source trace followed by one
    body trace per visited element."""

    __slots__ = ("type", "value", "children", "left_ref", "right_ref")

    def __init__(
        self,
        type_: str,
        value: Value,
        children: Optional[List["TraceNode"]] = None,
        left_ref: Optional[str] = None,
        right_ref: Optional[str] = None,
    ) -> None:
        self.type = type_
        self.value = value
        self.children = children or []
        self.left_ref = left_ref
        self.right_ref = right_ref


def explain(
    node: ExprNode,
    ctx: Any,
    resolve: Resolve,
    options: Optional[EvalOptions] = None,
) -> TraceNode:
    """Evaluate and return the verdict tree. The root's value is exactly what
    :func:`evaluate` returns for the same inputs; the conformance suite runs
    every vector through both and requires agreement."""
    return _trace(node, ctx, resolve, options, [])


def _trace(
    node: ExprNode, ctx: Any, resolve: Resolve,
    options: Optional[EvalOptions], frames: Frames,
) -> TraceNode:
    typ = node.get("type")
    if not isinstance(typ, str):
        raise ExpressionError("Expression node has no 'type'")

    arity = OPERATOR_ARITY.get(typ)
    if arity is not None:
        kind = arity[0]
        if kind == "unary":
            x = _trace(_operand(node, typ, arity[1]), ctx, resolve, options, frames)
            return TraceNode(typ, UNARY[typ](_require_num(x.value, typ)), [x])
        if kind == "binary":
            a = _trace(_operand(node, typ, arity[1]), ctx, resolve, options, frames)
            b = _trace(_operand(node, typ, arity[2]), ctx, resolve, options, frames)
            if typ == f"{TX}/Equals":
                value: Value = _bool(_values_equal(a.value, b.value))
            else:
                order = BINARY_ORDER.get(typ)
                if order is not None:
                    if not _is_number(a.value) or not _is_number(b.value):
                        value = 0.0
                    else:
                        value = order(float(a.value), float(b.value))
                else:
                    value = BINARY_ARITH[typ](
                        _require_num(a.value, typ), _require_num(b.value, typ)
                    )
            return TraceNode(typ, value, [a, b])
        if kind == "nary":
            items = node.get(arity[1])
            if not isinstance(items, list):
                raise ExpressionError(f"{typ} expects an '{arity[1]}' list")
            is_and = typ == f"{TX}/And"
            children: List[TraceNode] = []
            for item in items:
                child = _trace(item, ctx, resolve, options, frames)
                children.append(child)
                v = _truthy(_require_num(child.value, typ))
                # Same short-circuit as evaluate: later operands never run
                # and never appear in the trace.
                if is_and and not v:
                    return TraceNode(typ, 0.0, children)
                if not is_and and v:
                    return TraceNode(typ, 1.0, children)
            return TraceNode(typ, _bool(is_and), children)
        tv = _trace(_operand(node, typ, arity[1]), ctx, resolve, options, frames)
        tlo = _trace(_operand(node, typ, arity[2]), ctx, resolve, options, frames)
        thi = _trace(_operand(node, typ, arity[3]), ctx, resolve, options, frames)
        v = _require_num(tv.value, typ)
        lo = _require_num(tlo.value, typ)
        hi = _require_num(thi.value, typ)
        return TraceNode(typ, min(max(v, lo), hi), [tv, tlo, thi])

    if typ == f"{TX}/Not":
        x = _trace(_operand(node, typ, "operand"), ctx, resolve, options, frames)
        return TraceNode(typ, _bool(not _truthy(_require_num(x.value, typ))), [x])

    if typ in (f"{TX}/IsAtLeast", f"{TX}/Dominates"):
        value, left, right = _fold_ordered(node, typ, ctx, options)
        return TraceNode(typ, value, [], left, right)

    if typ in _LIST_FOLDS:
        src = _trace(_operand(node, typ, "source"), ctx, resolve, options, frames)
        items = src.value if isinstance(src.value, list) else [src.value]
        return TraceNode(typ, _list_fold(typ, items, node), [src])

    body_key = _ITERATOR_BODY.get(typ)
    if body_key is not None:
        loop_var = node.get("loopVar")
        if not isinstance(loop_var, str) or not loop_var:
            raise ExpressionError(f"{typ} is missing loopVar")
        src = _trace(_operand(node, typ, "source"), ctx, resolve, options, frames)
        items = src.value if isinstance(src.value, list) else [src.value]
        body = _operand(node, typ, body_key)
        children = [src]
        out: List[Value] = []
        for el in items:
            frames.append((loop_var, el))
            try:
                bt = _trace(body, ctx, resolve, options, frames)
            finally:
                frames.pop()
            children.append(bt)
            v = bt.value
            if typ == f"{TX}/Filter":
                if _truthy(_require_num(v, "Filter predicate")):
                    out.append(el)
            elif typ == f"{TX}/ForEach":
                if isinstance(v, list):
                    out.extend(v)
                else:
                    out.append(v)
            else:
                out.append(v)
        return TraceNode(typ, out, children)

    if typ == f"{TX}/Contains":
        hay = _trace(_operand(node, typ, "haystack"), ctx, resolve, options, frames)
        needle = _trace(_operand(node, typ, "needle"), ctx, resolve, options, frames)
        items = hay.value if isinstance(hay.value, list) else [hay.value]
        v = _bool(any(_values_equal(el, needle.value) for el in items))
        return TraceNode(typ, v, [hay, needle])

    if typ == f"{TX}/IsSet":
        x = _trace(_operand(node, typ, "checkExpr"), ctx, resolve, options, frames)
        return TraceNode(typ, _bool(_is_set(x.value)), [x])

    if typ == f"{TX}/ListItemAt":
        src = _trace(_operand(node, typ, "source"), ctx, resolve, options, frames)
        idx = _trace(_operand(node, typ, "itemIndex"), ctx, resolve, options, frames)
        items = src.value if isinstance(src.value, list) else [src.value]
        iv = idx.value
        if not _is_number(iv) or float(iv) != int(iv) or iv < 0:
            raise ExpressionError("ListItemAt itemIndex must be a non-negative integer")
        i = int(iv)
        value = items[i] if i < len(items) else []
        return TraceNode(typ, value, [src, idx])

    if typ == f"{TX}/Matches":
        src = _trace(_operand(node, typ, "matchSource"), ctx, resolve, options, frames)
        if not isinstance(src.value, str):
            raise ExpressionError(f"Matches requires a string matchSource, got {_kind(src.value)}")
        pattern = node.get("pattern")
        if not isinstance(pattern, str):
            raise ExpressionError("Matches is missing pattern")
        return TraceNode(typ, _bool(_matches_pattern(src.value, pattern)), [src])

    kind_pred = _KIND_PREDICATES.get(typ)
    if kind_pred is not None:
        x = _trace(_operand(node, typ, "kindCheck"), ctx, resolve, options, frames)
        return TraceNode(typ, _bool(kind_pred(x.value)), [x])

    lit = _literal_value(node, typ)
    if lit is not None:
        return TraceNode(typ, lit)

    if typ == f"{TX}/VarRef":
        name = node.get("varName")
        if isinstance(name, str):
            bound = _bound_value(frames, name)
            if bound is not None:
                return TraceNode(typ, bound)

    value = resolve(node, ctx, lambda n, c: _go(n, c, resolve, options, []))
    return TraceNode(typ, value)
