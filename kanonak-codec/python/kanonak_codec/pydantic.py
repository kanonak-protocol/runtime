"""The pydantic binding of the JSON forms of integer and decimal values
(kanonak-protocol 4.11.0's ``numbers-in-json``, runtime#54): the annotated types
a generated model's fields take, so pydantic reads and writes them as
:func:`kanonak_codec.serialize` and :func:`kanonak_codec.deserialize` do.

Installed with the ``pydantic`` extra, ``kanonak-codec[pydantic]``, so the codec
itself needs nothing. Each type is named for its datatype, since pydantic's own
``int`` reads ``"5"`` and its own ``Decimal`` reads a number. From JSON, a value
is read by the forms. From Python, a value is taken when it holds the datatype's
value exactly, an ``int``, a ``decimal.Decimal`` or a string of a lexical form,
and is checked as writing it would check it.
"""

from decimal import Decimal as _PyDecimal
from typing import Any, Dict

try:
    from typing import Annotated
except ImportError:  # Python 3.8
    from typing_extensions import Annotated

from pydantic import PlainSerializer, PlainValidator, ValidationInfo, WithJsonSchema

from . import _JSON_NUMBER_FORMS, _XSD, _Form, _read_json_number, _write_json_number

__all__ = [
    "Integer", "NonNegativeInteger", "PositiveInteger", "NonPositiveInteger", "NegativeInteger",
    "Long", "UnsignedLong", "Decimal",
    "Int", "Short", "Byte", "UnsignedInt", "UnsignedShort", "UnsignedByte",
]


def _json_schema(local: str) -> Dict[str, Any]:
    form = _JSON_NUMBER_FORMS[f"{_XSD}/{local}"]
    if form.form is _Form.NARROW:
        return {"type": "integer", "minimum": form.minimum, "maximum": form.maximum}
    if form.form is _Form.DECIMAL:
        return {"type": "string", "pattern": r"^[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)$"}
    return {"type": "string", "pattern": r"^[+-]?[0-9]+$"}


def _bound(local: str, held: type) -> Any:
    datatype = f"{_XSD}/{local}"
    prop = {"predicate": f"xsd:{local}", "datatype": datatype}
    form = _JSON_NUMBER_FORMS[datatype]

    def validate(value: Any, info: ValidationInfo) -> Any:
        if info.mode == "json":
            return _read_json_number(prop, form, value)
        return _read_json_number(prop, form, _write_json_number(prop, form, value))

    def serialize(value: Any) -> Any:
        return _write_json_number(prop, form, value)

    return Annotated[
        held,
        PlainValidator(validate),
        PlainSerializer(serialize, when_used="json"),
        WithJsonSchema(_json_schema(local)),
    ]


#: ``xsd:integer``: an ``int``, which JSON carries as a string of its digits.
Integer = _bound("integer", int)
#: ``xsd:nonNegativeInteger``: an ``int`` of 0 or more, as a string of its digits.
NonNegativeInteger = _bound("nonNegativeInteger", int)
#: ``xsd:positiveInteger``: an ``int`` of 1 or more, as a string of its digits.
PositiveInteger = _bound("positiveInteger", int)
#: ``xsd:nonPositiveInteger``: an ``int`` of 0 or less, as a string of its digits.
NonPositiveInteger = _bound("nonPositiveInteger", int)
#: ``xsd:negativeInteger``: an ``int`` of -1 or less, as a string of its digits.
NegativeInteger = _bound("negativeInteger", int)
#: ``xsd:long``: an ``int`` within 64 bits, as a string of its digits.
Long = _bound("long", int)
#: ``xsd:unsignedLong``: an ``int`` within 64 unsigned bits, as a string of its digits.
UnsignedLong = _bound("unsignedLong", int)
#: ``xsd:decimal``: a ``decimal.Decimal``, as a string of its digits, read from a string only.
Decimal = _bound("decimal", _PyDecimal)
#: ``xsd:int``: an ``int`` within 32 bits, read from and written as a whole JSON number.
Int = _bound("int", int)
#: ``xsd:short``: an ``int`` within 16 bits, as a whole JSON number.
Short = _bound("short", int)
#: ``xsd:byte``: an ``int`` within 8 bits, as a whole JSON number.
Byte = _bound("byte", int)
#: ``xsd:unsignedInt``: an ``int`` within 32 unsigned bits, as a whole JSON number.
UnsignedInt = _bound("unsignedInt", int)
#: ``xsd:unsignedShort``: an ``int`` within 16 unsigned bits, as a whole JSON number.
UnsignedShort = _bound("unsignedShort", int)
#: ``xsd:unsignedByte``: an ``int`` within 8 unsigned bits, as a whole JSON number.
UnsignedByte = _bound("unsignedByte", int)
