"""kanonak-wire -- the Python port of the Kanonak wire kernel
(``kanonak.org/wire-form``, ``wireFormatVersion "1"``).

A minimal, allocation-conscious binary reader/writer for hot-path wire
protocols. Generated protocol codecs call this kernel; it contains only what
is invariant across ALL protocols -- bounds-checked cursor reads/writes,
big-endian integers, strict text validation, and a rich error taxonomy.

Zero-copy contract: ``bytes(n)`` and ``rest()`` return ``memoryview`` slices
over the source buffer, never copies. Fail-loud contract: no ``None`` returns,
no partial values, no lossy decodes -- every failure is a ``WireError``
stating what was expected, what was found, and where.
"""

from __future__ import annotations

import re
from typing import Optional, Union

__all__ = ["WIRE_FORMAT_VERSION", "WireError", "WireReader", "WireWriter"]

WIRE_FORMAT_VERSION = "1"

# The closed error-kind vocabulary of wireFormatVersion "1".
_ERROR_KINDS = (
    "Truncated",
    "LengthOverrun",
    "TrailingBytes",
    "InvalidUtf8",
    "InvalidUuid",
    "ValueOutOfRange",
    "UnknownTag",
)

_UUID_RE = re.compile(
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)


class WireError(ValueError):
    """A wire-kernel failure carrying ``kind`` and (for reads) ``offset``.

    ``kind`` is one of the closed taxonomy: ``Truncated``, ``LengthOverrun``,
    ``TrailingBytes``, ``InvalidUtf8``, ``InvalidUuid``, ``ValueOutOfRange``,
    ``UnknownTag``. ``offset`` is the absolute byte offset where the failing
    read started (read-side errors), else ``None``.
    """

    def __init__(self, kind, message, offset=None):
        # type: (str, str, Optional[int]) -> None
        super(WireError, self).__init__(message)
        self.kind = kind
        self.offset = offset

    @staticmethod
    def truncated(needed, remaining, offset, context):
        # type: (int, int, int, str) -> WireError
        return WireError(
            "Truncated",
            "Truncated: {0} needs {1} byte(s) at offset {2}, {3} remain".format(
                context, needed, offset, remaining
            ),
            offset,
        )

    @staticmethod
    def length_overrun(declared, remaining, offset, context):
        # type: (int, int, int, str) -> WireError
        return WireError(
            "LengthOverrun",
            "LengthOverrun: {0} at offset {1} declares {2} byte(s), "
            "{3} remain after the length field".format(
                context, offset, declared, remaining
            ),
            offset,
        )

    @staticmethod
    def trailing_bytes(count, offset):
        # type: (int, int) -> WireError
        return WireError(
            "TrailingBytes",
            "TrailingBytes: expected end of buffer at offset {0}, "
            "{1} byte(s) remain".format(offset, count),
            offset,
        )

    @staticmethod
    def invalid_utf8(offset, context):
        # type: (Optional[int], str) -> WireError
        if offset is None:
            message = "InvalidUtf8: {0}".format(context)
        else:
            message = "InvalidUtf8: {0} at offset {1}".format(context, offset)
        return WireError("InvalidUtf8", message, offset)

    @staticmethod
    def invalid_uuid(context):
        # type: (str) -> WireError
        return WireError("InvalidUuid", "InvalidUuid: {0}".format(context))

    @staticmethod
    def value_out_of_range(value, type_name):
        # type: (object, str) -> WireError
        return WireError(
            "ValueOutOfRange",
            "ValueOutOfRange: {0} is not a valid {1}".format(value, type_name),
        )

    @staticmethod
    def unknown_tag(tag, context):
        # type: (int, str) -> WireError
        return WireError(
            "UnknownTag",
            "UnknownTag: 0x{0:02x} is not a known {1}".format(tag, context),
        )


def _uuid_from_bytes(b):
    # type: (bytes) -> str
    h = b.hex()
    return "-".join((h[0:8], h[8:12], h[12:16], h[16:20], h[20:32]))


def _is_uint(value):
    # type: (object) -> bool
    return isinstance(value, int) and not isinstance(value, bool)


class WireReader(object):
    """A bounds-checked cursor over an immutable byte buffer. Never copies.

    Accepts ``bytes``, ``bytearray``, or ``memoryview``; ``bytes(n)`` and
    ``rest()`` return ``memoryview`` slices over the source.
    """

    def __init__(self, buf):
        # type: (Union[bytes, bytearray, memoryview]) -> None
        self._buf = memoryview(buf)
        self._pos = 0

    def _need(self, n, context):
        # type: (int, str) -> None
        remaining = len(self._buf) - self._pos
        if remaining < n:
            raise WireError.truncated(n, remaining, self._pos, context)

    def u8(self):
        # type: () -> int
        self._need(1, "u8")
        v = self._buf[self._pos]
        self._pos += 1
        return v

    def u16_be(self):
        # type: () -> int
        self._need(2, "u16be")
        p = self._pos
        v = (self._buf[p] << 8) | self._buf[p + 1]
        self._pos += 2
        return v

    def u32_be(self):
        # type: () -> int
        self._need(4, "u32be")
        b = self._buf
        p = self._pos
        v = (b[p] << 24) | (b[p + 1] << 16) | (b[p + 2] << 8) | b[p + 3]
        self._pos += 4
        return v

    def bytes(self, n):
        # type: (int) -> memoryview
        """Exactly n bytes as a zero-copy memoryview slice."""
        self._need(n, "bytes({0})".format(n))
        v = self._buf[self._pos : self._pos + n]
        self._pos += n
        return v

    def uuid(self):
        # type: () -> str
        """16 bytes as a lowercase hyphenated UUID string. Any 16 bytes are legal."""
        self._need(16, "uuid")
        v = _uuid_from_bytes(bytes(self._buf[self._pos : self._pos + 16]))
        self._pos += 16
        return v

    def utf8(self, n):
        # type: (int) -> str
        """n bytes decoded as STRICT UTF-8. Bounds are checked before validity."""
        start = self._pos
        self._need(n, "utf8({0})".format(n))
        view = self._buf[start : start + n]
        try:
            s = bytes(view).decode("utf-8")  # strict is the default
        except UnicodeDecodeError:
            # The read did not take effect: position stays at start.
            raise WireError.invalid_utf8(start, "utf8({0})".format(n))
        self._pos = start + n
        return s

    def len_prefixed_bytes16(self):
        # type: () -> memoryview
        """u16be length L, then exactly L bytes (zero-copy view)."""
        start = self._pos
        self._need(2, "lenPrefixedBytes16")
        declared = (self._buf[start] << 8) | self._buf[start + 1]
        remaining_after_length = len(self._buf) - start - 2
        if declared > remaining_after_length:
            raise WireError.length_overrun(
                declared, remaining_after_length, start, "lenPrefixedBytes16"
            )
        self._pos = start + 2
        v = self._buf[self._pos : self._pos + declared]
        self._pos += declared
        return v

    def rest(self):
        # type: () -> memoryview
        """All remaining bytes (possibly empty) as a zero-copy view. Never errors."""
        v = self._buf[self._pos :]
        self._pos = len(self._buf)
        return v

    def remaining(self):
        # type: () -> int
        return len(self._buf) - self._pos

    def expect_end(self):
        # type: () -> None
        count = len(self._buf) - self._pos
        if count > 0:
            raise WireError.trailing_bytes(count, self._pos)


class WireWriter(object):
    """An append-only buffer builder with validated writes."""

    def __init__(self):
        # type: () -> None
        self._buf = bytearray()

    def _uint(self, value, max_value, type_name):
        # type: (object, int, str) -> None
        if not _is_uint(value) or value < 0 or value > max_value:
            raise WireError.value_out_of_range(value, type_name)

    def u8(self, value):
        # type: (int) -> WireWriter
        self._uint(value, 0xFF, "u8")
        self._buf.append(value)
        return self

    def u16_be(self, value):
        # type: (int) -> WireWriter
        self._uint(value, 0xFFFF, "u16be")
        self._buf.append(value >> 8)
        self._buf.append(value & 0xFF)
        return self

    def u32_be(self, value):
        # type: (int) -> WireWriter
        self._uint(value, 0xFFFFFFFF, "u32be")
        self._buf.append(value >> 24)
        self._buf.append((value >> 16) & 0xFF)
        self._buf.append((value >> 8) & 0xFF)
        self._buf.append(value & 0xFF)
        return self

    def bytes(self, b):
        # type: (Union[bytes, bytearray, memoryview]) -> WireWriter
        self._buf.extend(b)
        return self

    def uuid(self, s):
        # type: (str) -> WireWriter
        """Hyphenated 8-4-4-4-12 hex, case-insensitive input; emits the 16 bytes."""
        if not isinstance(s, str) or not _UUID_RE.match(s):
            raise WireError.invalid_uuid(
                '"{0}" is not a hyphenated 8-4-4-4-12 UUID'.format(s)
            )
        self._buf.extend(bytes.fromhex(s.replace("-", "")))
        return self

    def utf8(self, s):
        # type: (str) -> WireWriter
        """UTF-8 encode. Unpaired surrogates are InvalidUtf8 -- never U+FFFD."""
        try:
            encoded = s.encode("utf-8")  # strict is the default
        except UnicodeEncodeError:
            raise WireError.invalid_utf8(
                None, "string contains an unpaired surrogate"
            )
        self._buf.extend(encoded)
        return self

    def len_prefixed_bytes16(self, b):
        # type: (Union[bytes, bytearray, memoryview]) -> WireWriter
        """u16be length, then the bytes. Length above 0xFFFF is ValueOutOfRange."""
        n = len(b)
        if n > 0xFFFF:
            raise WireError.value_out_of_range(n, "lenPrefixedBytes16 length")
        self.u16_be(n)
        return self.bytes(b)

    def to_bytes(self):
        # type: () -> bytes
        """The written bytes, exact length (a copy -- the builder stays reusable)."""
        return bytes(self._buf)
