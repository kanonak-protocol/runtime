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
from enum import Enum
from typing import List, Optional

# The public API — what a 1.x release keeps compatible (runtime#29). Everything
# else here is an implementation detail and may change in any release.
__all__ = [
    "CANONICAL_FORM_VERSION",
    "Carrier",
    "Coordinate",
    "CoordinateVersion",
    "Embedded",
    "KList",
    "Package",
    "RawScalar",
    "Reference",
    "Statement",
    "Subject",
    "TypedScalar",
    "canonical_base64",
    "canonical_boolean",
    "canonical_date",
    "canonical_date_time",
    "canonical_decimal",
    "canonical_double",
    "canonical_float",
    "canonical_form",
    "canonical_hash",
    "canonical_hex_binary",
    "canonical_integer",
    "canonical_language_tag",
    "canonical_scalar_lexical",
    "canonical_string",
    "canonical_time",
    "carrier_of",
    "display_name",
    "is_readable_by",
    "lenient_versionless_key",
    "local_name",
    "parse_coordinate",
    "versionless_key",
]

CANONICAL_FORM_VERSION = "1"

# ===========================================================================
# Coordinates — publisher/package[@version]/name (issue #17)
# ===========================================================================
#
# Two strictness levels, both pinned by ``vectors/coordinate-vectors.json``:
# ``parse_coordinate`` (with the ``versionless_key`` / ``local_name``
# conveniences) is STRICT and raises rather than returning a plausible-looking
# tail; ``lenient_versionless_key`` is the total, non-raising structural
# variant that carrier routing uses, so ``carrier_of`` stays total and
# byte-identical under canonicalFormVersion "1" — an unparseable datatype URI
# routes to no carrier (raw token preserved), never to a guess. No ordering
# API on purpose: runtime consumers compare coordinates for equality only.
# ``#`` is reserved for embedded resources and rejected by the strict parser.
# One version relation lives here too (runtime#28): ``is_readable_by``.


@dataclass(frozen=True)
class CoordinateVersion:
    major: int
    minor: int
    patch: int


@dataclass(frozen=True)
class Coordinate:
    publisher: str
    package: str
    name: str
    version: Optional[CoordinateVersion] = None  # None for the versionless form


_VERSION_PART = re.compile(r"0|[1-9][0-9]*")  # exactly 0, or no leading zero
_WHITESPACE = re.compile(r"\s")


def _invalid_coordinate(uri: str, reason: str) -> ValueError:
    return ValueError(
        f"parse_coordinate: '{uri}' is not a valid Kanonak coordinate ({reason}); "
        f"expected publisher/package[@major.minor.patch]/name"
    )


def parse_coordinate(uri: str) -> Coordinate:
    """Parse ``publisher/package[@version]/name``. Strict: raises on malformed input."""
    if uri == "":
        raise _invalid_coordinate(uri, "empty string")
    if _WHITESPACE.search(uri):
        raise _invalid_coordinate(uri, "whitespace is not allowed")
    if "#" in uri:
        raise _invalid_coordinate(uri, "'#' is reserved for embedded resources")
    segments = uri.split("/")
    if len(segments) != 3:
        raise _invalid_coordinate(uri, f"expected exactly 3 '/'-separated segments, got {len(segments)}")
    publisher, middle, name = segments
    if not publisher or not middle or not name:
        raise _invalid_coordinate(uri, "empty segment")
    if "@" in publisher or "@" in name:
        raise _invalid_coordinate(uri, "'@' is only valid after the package name")
    if "@" not in middle:
        return Coordinate(publisher, middle, name)
    package, version_part = middle.split("@", 1)
    if not package:
        raise _invalid_coordinate(uri, "empty package name before @")
    if "@" in version_part:
        raise _invalid_coordinate(uri, "more than one '@'")
    parts = version_part.split(".")
    if len(parts) != 3 or not all(_VERSION_PART.fullmatch(p) for p in parts):
        raise _invalid_coordinate(
            uri, f"version '{version_part}' is not exactly major.minor.patch (digits, no leading zeros)"
        )
    return Coordinate(publisher, package, name,
                      CoordinateVersion(int(parts[0]), int(parts[1]), int(parts[2])))


def versionless_key(uri: str) -> str:
    """The versionless key ``publisher/package/name``. Strict: raises on malformed input."""
    c = parse_coordinate(uri)
    return f"{c.publisher}/{c.package}/{c.name}"


def local_name(uri: str) -> str:
    """The local name of a coordinate. Strict: raises on malformed input."""
    return parse_coordinate(uri).name


def display_name(uri: str) -> str:
    """The TOTAL display accessor - for rendering a coordinate to a human (an
    evaluation trace, an error message, a form label), where the string may
    not be one the caller constructed and an exception is strictly worse than
    showing the raw URI. A valid coordinate displays as its local name;
    anything else is returned VERBATIM - never a best-effort tail, so a full
    URI where a short name was expected is an honest signal of upstream
    malformation. Display uses the STRICT grammar: only a full parse earns a
    shortened name (an input the lenient carrier key would route still
    displays verbatim)."""
    try:
        return parse_coordinate(uri).name
    except ValueError:
        return uri


def is_readable_by(written: "CoordinateVersion", reader: "CoordinateVersion") -> bool:
    """Whether data whose terms were written at version ``written`` can be read
    by a reader whose schema is at version ``reader`` (runtime#28), pinned by
    ``readableVectors``: a different major is never readable; from 1.0.0 the
    reader needs a minor at least the writer's (a patch changes no term, so it
    is not compared); below 1.0.0 the minor is the incompatible line and the
    reader needs the same minor and a patch at least the writer's."""
    if written.major != reader.major:
        return False
    if written.major == 0:
        return reader.minor == written.minor and reader.patch >= written.patch
    return reader.minor >= written.minor


def lenient_versionless_key(uri: str) -> Optional[str]:
    """Total structural reduction to the versionless key, for carrier routing.

    Three non-empty ``/``-segments required; everything from the first ``@`` in
    the middle segment is discarded WITHOUT validating it (the part before the
    ``@`` must be non-empty). Returns ``None`` for anything else — never raises.
    """
    segments = uri.split("/")
    if len(segments) != 3:
        return None
    publisher, middle, name = segments
    if not publisher or not middle or not name:
        return None
    pkg = middle.split("@", 1)[0]
    if not pkg:
        return None
    return f"{publisher}/{pkg}/{name}"


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


def carrier_of(datatype_uri: str) -> Optional[Carrier]:
    """Carrier for a datatype URI, or ``None`` (out-of-set → raw-token tier).

    Routes on the total ``lenient_versionless_key``, not the raising parser —
    a structurally unparseable datatype URI is a raw token, never a guess.
    """
    key = lenient_versionless_key(datatype_uri)
    if key is None:
        return None
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


def _ecmascript_double_token(d: float) -> str:
    """ECMAScript's Number::toString of a finite, non-zero double: RFC 8785's number notation.

    repr gives the shortest round-tripping digits; only the notation is re-derived, plain while -6 < n <= 21.
    """
    sign = "-" if d < 0 else ""
    mantissa, _, exponent = repr(abs(d)).partition("e")
    int_part, _, frac_part = mantissa.partition(".")
    combined = int_part + frac_part
    lead = len(combined) - len(combined.lstrip("0"))
    digits = combined[lead:].rstrip("0")
    n = len(int_part) - lead + (int(exponent) if exponent else 0)
    k = len(digits)
    if k <= n <= 21:
        body = digits + "0" * (n - k)
    elif 0 < n <= 21:
        body = f"{digits[:n]}.{digits[n:]}"
    elif -6 < n <= 0:
        body = "0." + "0" * -n + digits
    else:
        e = n - 1
        head = digits if k == 1 else f"{digits[0]}.{digits[1:]}"
        body = f"{head}e{'+' if e >= 0 else '-'}{abs(e)}"
    return sign + body


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
    return _ecmascript_double_token(n)


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


# -- temporal: XSD 1.1's rules, on the proleptic Gregorian calendar with a year 0 ----
# A year of any length is an exact int, so a UTC shift across a year boundary has no range limit (runtime#36).

_DAYS_IN_MONTH = (31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)


def _is_leap_year(year: int) -> bool:
    r = year % 400
    return r % 4 == 0 and (r % 100 != 0 or r == 0)


def _days_in_month(year: int, month: int) -> int:
    return 29 if month == 2 and _is_leap_year(year) else _DAYS_IN_MONTH[month - 1]


def _year_text(year: int) -> str:
    """A year at least four digits wide, its sign kept; year 0 has none."""
    digits = str(abs(year)).rjust(4, "0")
    return f"-{digits}" if year < 0 else digits


def _civil_of(yy: str, mo: str, dd: str, what: str, raw: str) -> "tuple[int, int, int]":
    year, month, day = int(yy), int(mo), int(dd)
    if not 1 <= month <= 12:
        raise ValueError(f"{what}: '{raw}' has month {mo}; a month is 01 to 12")
    if not 1 <= day <= _days_in_month(year, month):
        raise ValueError(f"{what}: '{raw}' has day {dd}, which month {mo} of year {_year_text(year)} doesn't have")
    return year, month, day


def _next_day(year: int, month: int, day: int) -> "tuple[int, int, int]":
    if day < _days_in_month(year, month):
        return year, month, day + 1
    if month < 12:
        return year, month + 1, 1
    return year + 1, 1, 1


def _previous_day(year: int, month: int, day: int) -> "tuple[int, int, int]":
    if day > 1:
        return year, month, day - 1
    if month > 1:
        return year, month - 1, _days_in_month(year, month - 1)
    return year - 1, 12, 31


def _offset_minutes(tz: Optional[str], what: str, raw: str) -> Optional[int]:
    """Minutes east of UTC, or None with no offset. XSD 1.1 bounds an offset at 14:00."""
    if not tz:
        return None
    if tz == "Z":
        return 0
    hours, minutes = int(tz[1:3]), int(tz[4:6])
    if minutes > 59 or hours > 14 or (hours == 14 and minutes > 0):
        raise ValueError(f"{what}: '{raw}' has offset {tz}; an offset is at most 14:00 either way")
    return (-1 if tz[0] == "-" else 1) * (hours * 60 + minutes)


def _clock_of(hh: str, mi: str, ss: str, fraction: str, what: str, raw: str) -> "tuple[int, int]":
    """XSD 1.1: no 60th second, and 24:00:00 only as the first instant of the next day."""
    hour, minute, second = int(hh), int(mi), int(ss)
    end_of_day = hour == 24 and minute == 0 and second == 0 and fraction == ""
    if minute > 59 or second > 59 or (hour > 23 and not end_of_day):
        raise ValueError(f"{what}: '{raw}' has time {hh}:{mi}:{ss}{fraction}; "
                         "XSD 1.1 allows 00:00:00 to 23:59:59, and 24:00:00 exactly")
    return hour, minute


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


def canonical_date_time(raw: str) -> str:
    what = "canonicalDateTime"
    m = _DATE_TIME.match(raw.strip())
    if not m:
        raise ValueError(f"{what}: '{raw}' invalid")
    yy, mo, dd, hh, mi, ss, frac, tz = m.groups()
    fraction = _canonical_fraction(frac)
    civil = _civil_of(yy, mo, dd, what, raw)
    hour, minute = _clock_of(hh, mi, ss, fraction, what, raw)
    minutes = hour * 60 + minute - (_offset_minutes(tz, what, raw) or 0)
    if minutes < 0:
        minutes += 1440
        civil = _previous_day(*civil)
    elif minutes >= 1440:
        minutes -= 1440
        civil = _next_day(*civil)
    year, month, day = civil
    return (f"{_year_text(year)}-{month:02d}-{day:02d}"
            f"T{minutes // 60:02d}:{minutes % 60:02d}:{ss}{fraction}{'Z' if tz else ''}")


def canonical_date(raw: str) -> str:
    what = "canonicalDate"
    m = _DATE.match(raw.strip())
    if not m:
        raise ValueError(f"{what}: '{raw}' invalid")
    yy, mo, dd, tz = m.groups()
    year, _, _ = _civil_of(yy, mo, dd, what, raw)
    _offset_minutes(tz, what, raw)
    return f"{_year_text(year)}-{mo}-{dd}{_canonical_tz(tz)}"


def canonical_time(raw: str) -> str:
    what = "canonicalTime"
    m = _TIME.match(raw.strip())
    if not m:
        raise ValueError(f"{what}: '{raw}' invalid")
    hh, mi, ss, frac, tz = m.groups()
    fraction = _canonical_fraction(frac)
    hour, _ = _clock_of(hh, mi, ss, fraction, what, raw)
    _offset_minutes(tz, what, raw)
    return f"{'00' if hour == 24 else hh}:{mi}:{ss}{fraction}{_canonical_tz(tz)}"


def _refuse_lang_string(raw: str) -> str:
    raise ValueError(f"canonicalLangString: a language string is a structured value, an embedded {{ lexicalForm, language }}, so its tag is hashed with it; the langString carrier has no slot for the tag and refuses the scalar '{raw}'")


_DISPATCH = {
    Carrier.INTEGER: canonical_integer,
    Carrier.DECIMAL: canonical_decimal,
    Carrier.DOUBLE: canonical_double,
    Carrier.FLOAT: canonical_float,
    Carrier.BOOLEAN: canonical_boolean,
    Carrier.STRING: canonical_string,
    Carrier.ANY_URI: canonical_string,
    Carrier.LANG_STRING: _refuse_lang_string,
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
