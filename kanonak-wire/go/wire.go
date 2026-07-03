// Package wire is the Go port of the Kanonak wire kernel
// (kanonak.org/wire-form, wireFormatVersion "1").
//
// A minimal, allocation-conscious binary reader/writer for hot-path wire
// protocols. Generated protocol codecs call this kernel; it contains only
// what is invariant across ALL protocols — bounds-checked cursor
// reads/writes, big-endian integers, strict text validation, and a rich
// error taxonomy.
//
// Zero-copy contract: Bytes(n) and Rest() return subslices of the source
// buffer, never copies. Fail-loud contract: every failure is a *WireError
// stating what was expected, what was found, and where. Writer numeric
// parameters use exact-width types (byte/uint16/uint32) — the type is the
// range validation.
package wire

import (
	"fmt"
	"unicode/utf8"
)

// WireFormatVersion is the wire-form contract version this port implements.
const WireFormatVersion = "1"

// The error taxonomy of wireFormatVersion "1".
const (
	KindTruncated       = "Truncated"
	KindLengthOverrun   = "LengthOverrun"
	KindTrailingBytes   = "TrailingBytes"
	KindInvalidUtf8     = "InvalidUtf8"
	KindInvalidUuid     = "InvalidUuid"
	KindValueOutOfRange = "ValueOutOfRange"
	KindUnknownTag      = "UnknownTag"
)

// WireError is a wire kernel error: what was expected, what was found, and
// where. It works with errors.As.
type WireError struct {
	// Kind is one of the Kind* constants.
	Kind string
	// Offset is the absolute byte offset where the failing read started
	// (read-side errors). Meaningful only when HasOffset is true.
	Offset    int
	HasOffset bool
	message   string
}

func (e *WireError) Error() string {
	return e.message
}

func truncatedError(needed, remaining, offset int, context string) *WireError {
	return &WireError{
		Kind:      KindTruncated,
		Offset:    offset,
		HasOffset: true,
		message: fmt.Sprintf(
			"Truncated: %s needs %d byte(s) at offset %d, %d remain",
			context, needed, offset, remaining),
	}
}

func lengthOverrunError(declared, remaining, offset int, context string) *WireError {
	return &WireError{
		Kind:      KindLengthOverrun,
		Offset:    offset,
		HasOffset: true,
		message: fmt.Sprintf(
			"LengthOverrun: %s at offset %d declares %d byte(s), %d remain after the length field",
			context, offset, declared, remaining),
	}
}

func trailingBytesError(count, offset int) *WireError {
	return &WireError{
		Kind:      KindTrailingBytes,
		Offset:    offset,
		HasOffset: true,
		message: fmt.Sprintf(
			"TrailingBytes: expected end of buffer at offset %d, %d byte(s) remain",
			offset, count),
	}
}

func invalidUtf8ReadError(offset int, context string) *WireError {
	return &WireError{
		Kind:      KindInvalidUtf8,
		Offset:    offset,
		HasOffset: true,
		message:   fmt.Sprintf("InvalidUtf8: %s at offset %d", context, offset),
	}
}

func invalidUtf8WriteError(context string) *WireError {
	return &WireError{
		Kind:    KindInvalidUtf8,
		message: fmt.Sprintf("InvalidUtf8: %s", context),
	}
}

func invalidUuidError(context string) *WireError {
	return &WireError{
		Kind:    KindInvalidUuid,
		message: fmt.Sprintf("InvalidUuid: %s", context),
	}
}

func valueOutOfRangeError(value int, typeName string) *WireError {
	return &WireError{
		Kind:    KindValueOutOfRange,
		message: fmt.Sprintf("ValueOutOfRange: %d is not a valid %s", value, typeName),
	}
}

// NewUnknownTagError is the constructor for generated union dispatch on an
// unrecognized tag byte (not exercised by the kernel vectors).
func NewUnknownTagError(tag byte, context string) *WireError {
	return &WireError{
		Kind:    KindUnknownTag,
		message: fmt.Sprintf("UnknownTag: 0x%02x is not a known %s", tag, context),
	}
}

const hexLower = "0123456789abcdef"

// WireReader is a bounds-checked cursor over an immutable byte buffer.
// It never copies.
type WireReader struct {
	buf []byte
	pos int
}

// NewWireReader wraps buf without copying it.
func NewWireReader(buf []byte) *WireReader {
	return &WireReader{buf: buf}
}

func (r *WireReader) need(n int, context string) *WireError {
	remaining := len(r.buf) - r.pos
	if remaining < n {
		return truncatedError(n, remaining, r.pos, context)
	}
	return nil
}

// U8 reads one byte, 0..255.
func (r *WireReader) U8() (byte, error) {
	if err := r.need(1, "u8"); err != nil {
		return 0, err
	}
	v := r.buf[r.pos]
	r.pos++
	return v, nil
}

// U16BE reads two bytes, big-endian, 0..65535.
func (r *WireReader) U16BE() (uint16, error) {
	if err := r.need(2, "u16be"); err != nil {
		return 0, err
	}
	v := uint16(r.buf[r.pos])<<8 | uint16(r.buf[r.pos+1])
	r.pos += 2
	return v, nil
}

// U32BE reads four bytes, big-endian, 0..2^32-1.
func (r *WireReader) U32BE() (uint32, error) {
	if err := r.need(4, "u32be"); err != nil {
		return 0, err
	}
	p := r.pos
	v := uint32(r.buf[p])<<24 | uint32(r.buf[p+1])<<16 | uint32(r.buf[p+2])<<8 | uint32(r.buf[p+3])
	r.pos += 4
	return v, nil
}

// Bytes reads exactly n bytes as a zero-copy subslice of the source buffer.
func (r *WireReader) Bytes(n int) ([]byte, error) {
	if err := r.need(n, fmt.Sprintf("bytes(%d)", n)); err != nil {
		return nil, err
	}
	v := r.buf[r.pos : r.pos+n]
	r.pos += n
	return v, nil
}

// UUID reads 16 bytes as a lowercase hyphenated 8-4-4-4-12 UUID string.
// Any 16 bytes are legal — no version/variant validation.
func (r *WireReader) UUID() (string, error) {
	if err := r.need(16, "uuid"); err != nil {
		return "", err
	}
	b := r.buf[r.pos : r.pos+16]
	r.pos += 16
	out := make([]byte, 0, 36)
	for i, byt := range b {
		if i == 4 || i == 6 || i == 8 || i == 10 {
			out = append(out, '-')
		}
		out = append(out, hexLower[byt>>4], hexLower[byt&0x0f])
	}
	return string(out), nil
}

// UTF8 reads n bytes decoded as STRICT UTF-8 — invalid sequences, overlong
// encodings, and surrogate-range encodings are InvalidUtf8; never lossy,
// never U+FFFD. Bounds are checked before validity.
func (r *WireReader) UTF8(n int) (string, error) {
	start := r.pos
	view, err := r.Bytes(n)
	if err != nil {
		return "", err
	}
	if !utf8.Valid(view) {
		r.pos = start // the read did not take effect
		return "", invalidUtf8ReadError(start, fmt.Sprintf("utf8(%d)", n))
	}
	return string(view), nil
}

// LenPrefixedBytes16 reads a u16be length L, then exactly L bytes as a
// zero-copy subslice. L > remaining is LengthOverrun (NOT Truncated — the
// length field itself is suspect).
func (r *WireReader) LenPrefixedBytes16() ([]byte, error) {
	start := r.pos
	if err := r.need(2, "lenPrefixedBytes16"); err != nil {
		return nil, err
	}
	declared := int(r.buf[r.pos])<<8 | int(r.buf[r.pos+1])
	remainingAfterLength := len(r.buf) - r.pos - 2
	if declared > remainingAfterLength {
		return nil, lengthOverrunError(declared, remainingAfterLength, start, "lenPrefixedBytes16")
	}
	r.pos += 2
	v := r.buf[r.pos : r.pos+declared]
	r.pos += declared
	return v, nil
}

// Rest returns all remaining bytes (possibly empty) as a zero-copy subslice.
// It never errors and advances the cursor to the end.
func (r *WireReader) Rest() []byte {
	v := r.buf[r.pos:]
	r.pos = len(r.buf)
	return v
}

// Remaining is the count of unread bytes.
func (r *WireReader) Remaining() int {
	return len(r.buf) - r.pos
}

// ExpectEnd errors TrailingBytes if any bytes remain.
func (r *WireReader) ExpectEnd() error {
	count := len(r.buf) - r.pos
	if count > 0 {
		return trailingBytesError(count, r.pos)
	}
	return nil
}

// WireWriter is an append-only buffer builder. Numeric parameters use
// exact-width types — the type is the range validation.
type WireWriter struct {
	buf []byte
}

// NewWireWriter creates an empty writer.
func NewWireWriter() *WireWriter {
	return &WireWriter{}
}

// NewWireWriterWithCapacity preallocates so generated encoders can compute
// exact sizes and write once.
func NewWireWriterWithCapacity(capacity int) *WireWriter {
	return &WireWriter{buf: make([]byte, 0, capacity)}
}

// U8 appends one byte.
func (w *WireWriter) U8(v byte) *WireWriter {
	w.buf = append(w.buf, v)
	return w
}

// U16BE appends two bytes, big-endian.
func (w *WireWriter) U16BE(v uint16) *WireWriter {
	w.buf = append(w.buf, byte(v>>8), byte(v))
	return w
}

// U32BE appends four bytes, big-endian.
func (w *WireWriter) U32BE(v uint32) *WireWriter {
	w.buf = append(w.buf, byte(v>>24), byte(v>>16), byte(v>>8), byte(v))
	return w
}

// Bytes appends b verbatim.
func (w *WireWriter) Bytes(b []byte) *WireWriter {
	w.buf = append(w.buf, b...)
	return w
}

// UUID parses hyphenated 8-4-4-4-12 hex, case-insensitively, and appends the
// 16 bytes. Anything else (un-hyphenated, wrong length, non-hex) is
// InvalidUuid.
func (w *WireWriter) UUID(s string) error {
	b := []byte(s)
	if len(b) != 36 || b[8] != '-' || b[13] != '-' || b[18] != '-' || b[23] != '-' {
		return invalidUuidError(fmt.Sprintf("%q is not a hyphenated 8-4-4-4-12 UUID", s))
	}
	var out [16]byte
	oi := 0
	for i := 0; i < 36; {
		if b[i] == '-' {
			i++
			continue
		}
		hi, hiOK := hexVal(b[i])
		lo, loOK := hexVal(b[i+1])
		if !hiOK || !loOK {
			return invalidUuidError(fmt.Sprintf("%q is not a hyphenated 8-4-4-4-12 UUID", s))
		}
		out[oi] = hi<<4 | lo
		oi++
		i += 2
	}
	w.buf = append(w.buf, out[:]...)
	return nil
}

// UTF8 appends the UTF-8 encoding of s. Go strings can hold arbitrary bytes,
// so the never-lossy rule requires validation: an ill-formed string is
// InvalidUtf8 — never replacement characters.
func (w *WireWriter) UTF8(s string) error {
	if !utf8.ValidString(s) {
		return invalidUtf8WriteError("string is not well-formed UTF-8")
	}
	w.buf = append(w.buf, s...)
	return nil
}

// LenPrefixedBytes16 appends a u16be length, then the bytes. A length above
// 0xFFFF is ValueOutOfRange.
func (w *WireWriter) LenPrefixedBytes16(b []byte) error {
	if len(b) > 0xffff {
		return valueOutOfRangeError(len(b), "lenPrefixedBytes16 length")
	}
	w.U16BE(uint16(len(b)))
	w.Bytes(b)
	return nil
}

// ToBytes returns the written bytes, exact length (a copy — the builder
// stays reusable).
func (w *WireWriter) ToBytes() []byte {
	out := make([]byte, len(w.buf))
	copy(out, w.buf)
	return out
}

func hexVal(c byte) (byte, bool) {
	switch {
	case c >= '0' && c <= '9':
		return c - '0', true
	case c >= 'a' && c <= 'f':
		return c - 'a' + 10, true
	case c >= 'A' && c <= 'F':
		return c - 'A' + 10, true
	default:
		return 0, false
	}
}
