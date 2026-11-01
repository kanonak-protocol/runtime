"""The pydantic binding of the JSON forms (runtime#54): each read and write case
of the shared vectors through pydantic, with each field typed as a generated
model types it. Every type here is the binding's, so every refusal names
``[json-number-form]``. Needs the ``pydantic`` extra."""

import json
import os
import sys
from decimal import Decimal as PyDecimal
from typing import List

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_HERE, "..", "..", "kanonak-canonical", "python"))

from pydantic import BaseModel, TypeAdapter, ValidationError  # noqa: E402

from kanonak_codec import pydantic as kp  # noqa: E402

VECTOR_FILE = os.path.join(_HERE, "..", "vectors", "codec-vectors-json-numbers.json")


class Amount(BaseModel):
    """The embedded Amount, as a generated model holds it."""

    value: kp.Decimal


FIELDS = {
    "integer": kp.Integer,
    "nonNegativeInteger": kp.NonNegativeInteger,
    "positiveInteger": kp.PositiveInteger,
    "nonPositiveInteger": kp.NonPositiveInteger,
    "negativeInteger": kp.NegativeInteger,
    "long": kp.Long,
    "unsignedLong": kp.UnsignedLong,
    "decimal": kp.Decimal,
    "int": kp.Int,
    "short": kp.Short,
    "byte": kp.Byte,
    "unsignedInt": kp.UnsignedInt,
    "unsignedShort": kp.UnsignedShort,
    "unsignedByte": kp.UnsignedByte,
    "counts": List[kp.Integer],
    "amount": Amount,
}


def lexical(value):
    """What a read value reads as, compared with a case's expected: each value's canonical lexical form."""
    if isinstance(value, Amount):
        return {"value": lexical(value.value)}
    if isinstance(value, list):
        return [lexical(v) for v in value]
    if isinstance(value, PyDecimal):
        return format(value, "f")
    return str(value)


def held(field, value):
    """The Python value a write case holds, from its canonical lexical form."""
    if field == "amount":
        return Amount(value=PyDecimal(value["value"]))
    if field == "counts":
        return [int(v) for v in value]
    return PyDecimal(value) if field == "decimal" else int(value)


def refused(error):
    return "[json-number-form]" in str(error)


def _refuses(adapter, value):
    try:
        adapter.validate_python(value)
    except ValidationError as e:
        assert refused(e), str(e)
        return
    raise AssertionError(f"{value!r} was taken")


def _python_int_past_2_53():
    # From Python an int holds any integer exactly, so one past 2^53 is taken, where JSON's number is refused.
    assert TypeAdapter(kp.Integer).validate_python(2**70) == 2**70


def _bool_is_no_int():
    _refuses(TypeAdapter(kp.Int), True)


def _float_is_no_decimal():
    _refuses(TypeAdapter(kp.Decimal), 1.5)


def _python_mode_keeps_the_value():
    assert TypeAdapter(kp.Long).dump_python(5) == 5


EDGES = [
    ("a python int past 2^53", _python_int_past_2_53),
    ("a bool is no int", _bool_is_no_int),
    ("a float is no decimal", _float_is_no_decimal),
    ("python mode keeps the value", _python_mode_keeps_the_value),
]


def main():
    with open(VECTOR_FILE, encoding="utf-8") as f:
        doc = json.load(f)
    fails = []
    checked = 0
    for c in doc["readCases"]:
        checked += 1
        adapter = TypeAdapter(FIELDS[c["field"]])
        try:
            got = adapter.validate_json(c["json"])
        except ValidationError as e:
            if not c.get("expectError"):
                fails.append(f"read {c['id']}: refused: {e}")
            elif not refused(e):
                fails.append(f"read {c['id']}: refused without [json-number-form]: {e}")
            continue
        if c.get("expectError"):
            fails.append(f"read {c['id']}: expected a refusal, got {lexical(got)}")
        elif lexical(got) != c["expected"]:
            fails.append(f"read {c['id']}: expected {c['expected']!r}, got {lexical(got)!r}")
    for c in doc["writeCases"]:
        checked += 1
        adapter = TypeAdapter(FIELDS[c["field"]])
        try:
            written = json.loads(adapter.dump_json(adapter.validate_python(held(c["field"], c["value"]))))
        except ValidationError as e:
            if "json" in c:
                fails.append(f"write {c['id']}: refused: {e}")
            elif not refused(e):
                fails.append(f"write {c['id']}: refused without [json-number-form]: {e}")
            continue
        if "json" not in c:
            fails.append(f"write {c['id']}: expected a refusal, wrote {written!r}")
        elif written != json.loads(c["json"]):
            fails.append(f"write {c['id']}: expected {c['json']}, got {json.dumps(written)}")
    checked += len(EDGES)
    for name, check in EDGES:
        try:
            check()
        except (AssertionError, ValidationError) as e:
            fails.append(f"edge {name}: {e!r}")
    for f in fails:
        print(f"FAIL {f}")
    print(f"codec-vectors-json-numbers.json (pydantic binding): {checked - len(fails)}/{checked} pass")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
