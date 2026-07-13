"""Kanonak canonical form + content hash (canonicalFormVersion "1").

An independent conformant Python port of ``kanonak.org/canonical-form``, verified
byte-for-byte against the golden vectors. Identity of a literal is
``(carrier, canonical lexical)``; the wire form is compact JSON with UTF-8 byte
ordering, RFC 8785 escaping, and a fixed per-blob field order; the content
address is the SHA-256 of those bytes.
"""

from __future__ import annotations

import base64 as _base64
import hashlib
import re
import struct
import unicodedata
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import List, Optional

CANONICAL_FORM_VERSION = "1"

# ===========================================================================
# Carriers
# ===========================================================================


class Carrier(str, Enum):
    INTEGER = "integer"
    DECIMAL = "decimal"
    DOUBLE = "double"
    FLOAT = "float"
    BOOLEAN = "boolean"
    STRING = "string"
    ANY_URI = "anyURI"
    LANG_STRING = "langString"
    DATE_TIME = "dateTime"
    DATE = "date"
    TIME = "time"
    HEX_BINARY = "hexBinary"
    BASE64_BINARY = "base64Binary"


_XSD_CARRIER = {
    name: Carrier.INTEGER
    for name in (
        "integer", "long", "int", "short", "byte", "unsignedLong", "unsignedInt",
        "unsignedShort", "unsignedByte", "nonNegativeInteger", "positiveInteger",
        "nonPositiveInteger", "negativeInteger",
    )
}
_XSD_CARRIER.update({
    "decimal": Carrier.DECIMAL, "double": Carrier.DOUBLE, "float": Carrier.FLOAT,
    "boolean": Carrier.BOOLEAN, "string": Carrier.STRING,
    "normalizedString": Carrier.STRING, "token": Carrier.STRING, "anyURI": Carrier.ANY_URI,
    "dateTime": Carrier.DATE_TIME, "date": Carrier.DATE, "time": Carrier.TIME,
    "hexBinary": Carrier.HEX_BINARY, "base64Binary": Carrier.BASE64_BINARY,
})


def carrier_key(uri: str) -> str:
    """``publisher/package/name`` carrier key from a datatype URI."""
    idx = uri.rfind("/")
    name = uri[idx + 1:]
    head = uri[:idx]
    slash = head.find("/")
    publisher = head[:slash]
    pkg = head[slash + 1:].split("@", 1)[0]
    return f"{publisher}/{pkg}/{name}"


def carrier_of(datatype_uri: str) -> Optional[Carrier]:
    """Carrier for a datatype URI, or ``None`` (out-of-set → raw-token tier)."""
    key = carrier_key(datatype_uri)
    if key == "kanonak.org/core-rdf/langString":
        return Carrier.LANG_STRING
    prefix = "kanonak.org/core-xsd/"
    if not key.startswith(prefix):
        return None
    return _XSD_CARRIER.get(key[len(prefix):])


# ===========================================================================
# Per-carrier canonical lexical forms
# ===========================================================================

_INTEGER = re.compile(r"^[+-]?\d+$")
_DECIMAL = re.compile(r"^([+-]?)(\d*)(?:\.(\d*))?$")
_IEEE = re.compile(r"^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$")
_HEX = re.compile(r"^([0-9A-Fa-f]{2})*$")
_BASE64 = re.compile(r"^[A-Za-z0-9+/]*={0,2}$")
_DATE_TIME = re.compile(r"^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$")
_DATE = re.compile(r"^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$")
_TIME = re.compile(r"^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$")


def canonical_integer(raw: str) -> str:
    t = raw.strip()
    if not _INTEGER.match(t):
        raise ValueError(f"canonicalInteger: '{raw}' invalid")
    sign, digits = "", t
    if t.startswith("-"):
        sign, digits = "-", t[1:]
    elif t.startswith("+"):
        digits = t[1:]
    stripped = digits.lstrip("0") or "0"
    if stripped == "0":
        return "0"
    return sign + stripped


def canonical_decimal(raw: str) -> str:
    t = raw.strip()
    m = _DECIMAL.match(t)
    int_raw = m.group(2) if m else ""
    frac_raw = m.group(3) or "" if m else ""
    if not m or (int_raw == "" and frac_raw == ""):
        raise ValueError(f"canonicalDecimal: '{raw}' invalid")
    sign = "-" if m.group(1) == "-" else ""
    int_part = int_raw.lstrip("0") or "0"
    frac_part = frac_raw.rstrip("0")
    magnitude = f"{int_part}.{frac_part}" if frac_part else int_part
    if magnitude == "0":
        return "0"
    return sign + magnitude


def _to_float32(x: float) -> float:
    return struct.unpack("<f", struct.pack("<f", x))[0]


def _shortest(s: str) -> str:
    return s[:-2] if s.endswith(".0") else s


def _canonical_ieee(raw: str, single: bool, label: str) -> str:
    t = raw.strip()
    if t == "NaN":
        return "NaN"
    if t == "INF":
        return "INF"
    if t == "-INF":
        return "-INF"
    if not _IEEE.match(t):
        raise ValueError(f"canonical{label}: '{raw}' invalid")
    n = float(t)
    if single:
        n = _to_float32(n)
    if n != n or n in (float("inf"), float("-inf")):
        raise ValueError(f"canonical{label}: '{raw}' out of range")
    if n == 0.0:
        return "0"
    return _shortest(repr(n))


def canonical_double(raw: str) -> str:
    return _canonical_ieee(raw, False, "Double")


def canonical_float(raw: str) -> str:
    return _canonical_ieee(raw, True, "Float")


def canonical_boolean(raw: str) -> str:
    t = raw.strip()
    if t in ("true", "1"):
        return "true"
    if t in ("false", "0"):
        return "false"
    raise ValueError(f"canonicalBoolean: '{raw}' invalid")


def canonical_string(raw: str) -> str:
    return unicodedata.normalize("NFC", raw)


def canonical_language_tag(tag: str) -> str:
    subs = tag.strip().split("-")
    if not subs or subs[0] == "":
        raise ValueError(f"canonicalLanguageTag: '{tag}' invalid")
    out = []
    for i, sub in enumerate(subs):
        if i == 0:
            out.append(sub.lower())
        elif len(sub) == 4 and sub.isalpha():
            out.append(sub[0].upper() + sub[1:].lower())
        elif len(sub) == 2 and sub.isalpha():
            out.append(sub.upper())
        else:
            out.append(sub.lower())
    return "-".join(out)


def canonical_hex_binary(raw: str) -> str:
    t = raw.strip()
    if not _HEX.match(t):
        raise ValueError(f"canonicalHexBinary: '{raw}' invalid")
    return t.upper()


def canonical_base64(raw: str) -> str:
    stripped = re.sub(r"\s+", "", raw)
    if not _BASE64.match(stripped) or len(stripped) % 4 != 0:
        raise ValueError(f"canonicalBase64: '{raw}' invalid")
    data = _base64.b64decode(stripped, validate=True)
    return _base64.b64encode(data).decode("ascii")


# -- temporal ---------------------------------------------------------------


def _canonical_year(raw: str) -> str:
    neg = raw.startswith("-")
    digits = (raw[1:] if neg else raw).lstrip("0") or "0"
    if len(digits) < 4:
        digits = digits.rjust(4, "0")
    return ("-" if neg else "") + digits


def _canonical_fraction(frac: Optional[str]) -> str:
    if not frac:
        return ""
    trimmed = frac.lstrip(".").rstrip("0")
    return f".{trimmed}" if trimmed else ""


def _canonical_tz(tz: Optional[str]) -> str:
    if not tz:
        return ""
    if tz in ("Z", "+00:00", "-00:00"):
        return "Z"
    return tz


def _tz_offset_minutes(tz: str) -> int:
    if tz == "Z":
        return 0
    sign = -1 if tz[0] == "-" else 1
    return sign * (int(tz[1:3]) * 60 + int(tz[4:6]))


def canonical_date_time(raw: str) -> str:
    m = _DATE_TIME.match(raw.strip())
    if not m:
        raise ValueError(f"canonicalDateTime: '{raw}' invalid")
    fraction = _canonical_fraction(m.group(7))
    tz = m.group(8)
    if not tz:
        return f"{_canonical_year(m.group(1))}-{m.group(2)}-{m.group(3)}T{m.group(4)}:{m.group(5)}:{m.group(6)}{fraction}"
    dt = datetime(int(m.group(1)), int(m.group(2)), int(m.group(3)),
                  int(m.group(4)), int(m.group(5)), int(m.group(6)), tzinfo=timezone.utc)
    s = dt - timedelta(minutes=_tz_offset_minutes(tz))
    return (f"{_canonical_year(str(s.year))}-{s.month:02d}-{s.day:02d}"
            f"T{s.hour:02d}:{s.minute:02d}:{s.second:02d}{fraction}Z")


def canonical_date(raw: str) -> str:
    m = _DATE.match(raw.strip())
    if not m:
        raise ValueError(f"canonicalDate: '{raw}' invalid")
    return f"{_canonical_year(m.group(1))}-{m.group(2)}-{m.group(3)}{_canonical_tz(m.group(4))}"


def canonical_time(raw: str) -> str:
    m = _TIME.match(raw.strip())
    if not m:
        raise ValueError(f"canonicalTime: '{raw}' invalid")
    hh = m.group(1)
    fraction = _canonical_fraction(m.group(4))
    if hh == "24" and m.group(2) == "00" and m.group(3) == "00" and fraction == "":
        hh = "00"
    return f"{hh}:{m.group(2)}:{m.group(3)}{fraction}{_canonical_tz(m.group(5))}"


_DISPATCH = {
    Carrier.INTEGER: canonical_integer,
    Carrier.DECIMAL: canonical_decimal,
    Carrier.DOUBLE: canonical_double,
    Carrier.FLOAT: canonical_float,
    Carrier.BOOLEAN: canonical_boolean,
    Carrier.STRING: canonical_string,
    Carrier.ANY_URI: canonical_string,
    Carrier.LANG_STRING: canonical_string,
    Carrier.DATE_TIME: canonical_date_time,
    Carrier.DATE: canonical_date,
    Carrier.TIME: canonical_time,
    Carrier.HEX_BINARY: canonical_hex_binary,
    Carrier.BASE64_BINARY: canonical_base64,
}


def canonical_scalar_lexical(carrier: Carrier, raw: str) -> str:
    return _DISPATCH[carrier](raw)


# ===========================================================================
# Value model + wire form
# ===========================================================================


@dataclass
class TypedScalar:
    carrier: Carrier
    lexical: str


@dataclass
class RawScalar:
    token: str


@dataclass
class Reference:
    uri: str


@dataclass
class Embedded:
    name: Optional[str]
    statements: "List[Statement]"


@dataclass
class KList:
    items: "List[object]"


@dataclass
class Statement:
    predicate: str
    value: object


@dataclass
class Subject:
    uri: str
    statements: List[Statement]


@dataclass
class Package:
    subjects: List[Subject]


def canonical_form(pkg: Package) -> str:
    out: List[str] = ['{"subjects":[']
    subjects = sorted(pkg.subjects, key=lambda s: s.uri.encode("utf-8"))
    for i, s in enumerate(subjects):
        if i:
            out.append(",")
        out.append('{"uri":')
        _emit_string(out, s.uri)
        out.append(',"statements":[')
        _emit_statements(out, s.statements)
        out.append("]}")
    out.append("]}")
    return "".join(out)


def canonical_hash(pkg: Package) -> str:
    form = canonical_form(pkg)
    return "sha256:" + hashlib.sha256(form.encode("utf-8")).hexdigest()


def _serialize_statement(st: Statement) -> str:
    out: List[str] = ['{"predicate":']
    _emit_string(out, st.predicate)
    out.append(",")
    _emit_value_tail(out, st.value)
    out.append("}")
    return "".join(out)


def _emit_statements(out: List[str], stmts: List[Statement]) -> None:
    # Order by predicate UTF-8 bytes; equal predicates (possible since
    # multi-typed subjects — several type statements share the type predicate)
    # order by the serialized statement blob's UTF-8 bytes. The tie-break makes
    # the declared invariance under statement ordering TRUE for same-predicate
    # statements rather than an accident of sort stability; no distinct-predicate
    # ordering is affected.
    rendered = [(st.predicate.encode("utf-8"), _serialize_statement(st)) for st in stmts]
    rendered.sort(key=lambda pair: (pair[0], pair[1].encode("utf-8")))
    out.append(",".join(serialized for _, serialized in rendered))


def _emit_value_tail(out: List[str], v: object) -> None:
    if isinstance(v, TypedScalar):
        out.append('"type":"typed","carrier":')
        _emit_string(out, v.carrier.value)
        out.append(',"value":')
        _emit_string(out, canonical_scalar_lexical(v.carrier, v.lexical))
    elif isinstance(v, RawScalar):
        out.append('"type":"string","value":')
        _emit_string(out, v.token)
    elif isinstance(v, Reference):
        out.append('"type":"ref","value":')
        _emit_string(out, v.uri)
    elif isinstance(v, Embedded):
        out.append('"type":"embedded"')
        if v.name is not None:
            out.append(',"name":')
            _emit_string(out, v.name)
        out.append(',"statements":[')
        _emit_statements(out, v.statements)
        out.append("]")
    elif isinstance(v, KList):
        out.append('"type":"list","items":[')
        for i, item in enumerate(v.items):
            if i:
                out.append(",")
            out.append("{")
            _emit_value_tail(out, item)
            out.append("}")
        out.append("]")
    else:
        raise ValueError(f"canonicalForm: unrecognized value kind {type(v).__name__}")


_ESCAPES = {'"': '\\"', "\\": "\\\\", "\b": "\\b", "\f": "\\f", "\n": "\\n", "\r": "\\r", "\t": "\\t"}


def _emit_string(out: List[str], s: str) -> None:
    out.append('"')
    for c in s:
        esc = _ESCAPES.get(c)
        if esc is not None:
            out.append(esc)
        elif ord(c) < 0x20:
            out.append(f"\\u{ord(c):04x}")
        else:
            out.append(c)
    out.append('"')
