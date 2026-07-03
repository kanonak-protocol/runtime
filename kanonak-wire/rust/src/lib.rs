//! kanonak-wire — the Rust port of the Kanonak wire kernel
//! (`kanonak.org/wire-form`, `wireFormatVersion "1"`).
//!
//! A minimal, allocation-conscious binary reader/writer for hot-path wire
//! protocols. Generated protocol codecs call this kernel; it contains only
//! what is invariant across ALL protocols — bounds-checked cursor
//! reads/writes, big-endian integers, strict text validation, and a rich
//! error taxonomy.
//!
//! Zero-copy contract: `bytes(n)`, `rest()`, and `utf8(n)` return slices
//! borrowing the source buffer, never copies. Fail-loud contract: every
//! failure is a [`WireError`] stating what was expected, what was found, and
//! where. Writer numeric parameters use exact-width types (`u8`/`u16`/`u32`)
//! — the type is the range validation.

pub const WIRE_FORMAT_VERSION: &str = "1";

/// The error taxonomy of `wireFormatVersion "1"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireErrorKind {
    Truncated,
    LengthOverrun,
    TrailingBytes,
    InvalidUtf8,
    InvalidUuid,
    ValueOutOfRange,
    UnknownTag,
}

impl WireErrorKind {
    /// The kind name as it appears in the shared vectors.
    pub fn as_str(&self) -> &'static str {
        match self {
            WireErrorKind::Truncated => "Truncated",
            WireErrorKind::LengthOverrun => "LengthOverrun",
            WireErrorKind::TrailingBytes => "TrailingBytes",
            WireErrorKind::InvalidUtf8 => "InvalidUtf8",
            WireErrorKind::InvalidUuid => "InvalidUuid",
            WireErrorKind::ValueOutOfRange => "ValueOutOfRange",
            WireErrorKind::UnknownTag => "UnknownTag",
        }
    }
}

/// A wire kernel error: what was expected, what was found, and where.
#[derive(Debug, Clone)]
pub struct WireError {
    pub kind: WireErrorKind,
    /// Absolute byte offset where the failing read started (read-side errors).
    pub offset: Option<usize>,
    message: String,
}

impl WireError {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn truncated(needed: usize, remaining: usize, offset: usize, context: &str) -> Self {
        WireError {
            kind: WireErrorKind::Truncated,
            offset: Some(offset),
            message: format!(
                "Truncated: {context} needs {needed} byte(s) at offset {offset}, {remaining} remain"
            ),
        }
    }

    pub fn length_overrun(declared: usize, remaining: usize, offset: usize, context: &str) -> Self {
        WireError {
            kind: WireErrorKind::LengthOverrun,
            offset: Some(offset),
            message: format!(
                "LengthOverrun: {context} at offset {offset} declares {declared} byte(s), {remaining} remain after the length field"
            ),
        }
    }

    pub fn trailing_bytes(count: usize, offset: usize) -> Self {
        WireError {
            kind: WireErrorKind::TrailingBytes,
            offset: Some(offset),
            message: format!(
                "TrailingBytes: expected end of buffer at offset {offset}, {count} byte(s) remain"
            ),
        }
    }

    pub fn invalid_utf8(offset: usize, context: &str) -> Self {
        WireError {
            kind: WireErrorKind::InvalidUtf8,
            offset: Some(offset),
            message: format!("InvalidUtf8: {context} at offset {offset}"),
        }
    }

    pub fn invalid_uuid(context: &str) -> Self {
        WireError {
            kind: WireErrorKind::InvalidUuid,
            offset: None,
            message: format!("InvalidUuid: {context}"),
        }
    }

    pub fn value_out_of_range(value: impl std::fmt::Display, type_name: &str) -> Self {
        WireError {
            kind: WireErrorKind::ValueOutOfRange,
            offset: None,
            message: format!("ValueOutOfRange: {value} is not a valid {type_name}"),
        }
    }

    /// Constructor for generated union dispatch on an unrecognized tag byte.
    pub fn unknown_tag(tag: u8, context: &str) -> Self {
        WireError {
            kind: WireErrorKind::UnknownTag,
            offset: None,
            message: format!("UnknownTag: 0x{tag:02x} is not a known {context}"),
        }
    }
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WireError {}

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

/// A bounds-checked cursor over an immutable byte buffer. Never copies.
pub struct WireReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> WireReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        WireReader { buf, pos: 0 }
    }

    fn need(&self, n: usize, context: &str) -> Result<(), WireError> {
        let remaining = self.buf.len() - self.pos;
        if remaining < n {
            return Err(WireError::truncated(n, remaining, self.pos, context));
        }
        Ok(())
    }

    pub fn u8(&mut self) -> Result<u8, WireError> {
        self.need(1, "u8")?;
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn u16_be(&mut self) -> Result<u16, WireError> {
        self.need(2, "u16be")?;
        let v = u16::from_be_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    pub fn u32_be(&mut self) -> Result<u32, WireError> {
        self.need(4, "u32be")?;
        let p = self.pos;
        let v = u32::from_be_bytes([
            self.buf[p],
            self.buf[p + 1],
            self.buf[p + 2],
            self.buf[p + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    /// Exactly n bytes as a zero-copy slice of the source buffer.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        self.need(n, &format!("bytes({n})"))?;
        let v = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(v)
    }

    /// 16 bytes as a lowercase hyphenated UUID string. Any 16 bytes are legal.
    pub fn uuid(&mut self) -> Result<String, WireError> {
        self.need(16, "uuid")?;
        let b = &self.buf[self.pos..self.pos + 16];
        self.pos += 16;
        let mut s = String::with_capacity(36);
        for (i, byte) in b.iter().enumerate() {
            if i == 4 || i == 6 || i == 8 || i == 10 {
                s.push('-');
            }
            s.push(HEX_LOWER[(byte >> 4) as usize] as char);
            s.push(HEX_LOWER[(byte & 0x0f) as usize] as char);
        }
        Ok(s)
    }

    /// n bytes decoded as STRICT UTF-8 (zero-copy `&str`). Bounds are checked
    /// before validity.
    pub fn utf8(&mut self, n: usize) -> Result<&'a str, WireError> {
        let start = self.pos;
        let view = self.bytes(n)?;
        match std::str::from_utf8(view) {
            Ok(s) => Ok(s),
            Err(_) => {
                self.pos = start; // the read did not take effect
                Err(WireError::invalid_utf8(start, &format!("utf8({n})")))
            }
        }
    }

    /// u16be length L, then exactly L bytes (zero-copy slice).
    pub fn len_prefixed_bytes16(&mut self) -> Result<&'a [u8], WireError> {
        let start = self.pos;
        self.need(2, "lenPrefixedBytes16")?;
        let declared = u16::from_be_bytes([self.buf[self.pos], self.buf[self.pos + 1]]) as usize;
        let remaining_after_length = self.buf.len() - self.pos - 2;
        if declared > remaining_after_length {
            return Err(WireError::length_overrun(
                declared,
                remaining_after_length,
                start,
                "lenPrefixedBytes16",
            ));
        }
        self.pos += 2;
        let v = &self.buf[self.pos..self.pos + declared];
        self.pos += declared;
        Ok(v)
    }

    /// All remaining bytes (possibly empty), zero-copy. Never errors.
    pub fn rest(&mut self) -> &'a [u8] {
        let v = &self.buf[self.pos..];
        self.pos = self.buf.len();
        v
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn expect_end(&self) -> Result<(), WireError> {
        let count = self.buf.len() - self.pos;
        if count > 0 {
            return Err(WireError::trailing_bytes(count, self.pos));
        }
        Ok(())
    }
}

/// An append-only buffer builder. Numeric parameters use exact-width types —
/// the type is the range validation.
pub struct WireWriter {
    buf: Vec<u8>,
}

impl Default for WireWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl WireWriter {
    pub fn new() -> Self {
        WireWriter { buf: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        WireWriter {
            buf: Vec::with_capacity(capacity),
        }
    }

    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.buf.push(value);
        self
    }

    pub fn u16_be(&mut self, value: u16) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub fn u32_be(&mut self, value: u32) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self
    }

    /// Hyphenated 8-4-4-4-12 hex, case-insensitive input; emits the 16 bytes.
    /// The full shape is validated BEFORE any parsing: hyphens at exactly
    /// positions 8/13/18/23 and hex digits at every other position — a stray
    /// hyphen elsewhere is InvalidUuid, never a short or shifted parse.
    pub fn uuid(&mut self, s: &str) -> Result<&mut Self, WireError> {
        let b = s.as_bytes();
        let shape_ok = b.len() == 36
            && b.iter().enumerate().all(|(i, &c)| match i {
                8 | 13 | 18 | 23 => c == b'-',
                _ => hex_val(c).is_some(),
            });
        if !shape_ok {
            return Err(WireError::invalid_uuid(&format!(
                "\"{s}\" is not a hyphenated 8-4-4-4-12 UUID"
            )));
        }
        let mut out = [0u8; 16];
        let mut oi = 0;
        let mut i = 0;
        while i < 36 {
            if b[i] == b'-' {
                i += 1;
                continue;
            }
            out[oi] = (hex_val(b[i]).unwrap() << 4) | hex_val(b[i + 1]).unwrap();
            oi += 1;
            i += 2;
        }
        self.buf.extend_from_slice(&out);
        Ok(self)
    }

    /// UTF-8 encode. A Rust `&str` is well-formed by construction — infallible.
    pub fn utf8(&mut self, s: &str) -> &mut Self {
        self.buf.extend_from_slice(s.as_bytes());
        self
    }

    /// u16be length, then the bytes. Length above 0xFFFF is ValueOutOfRange.
    pub fn len_prefixed_bytes16(&mut self, b: &[u8]) -> Result<&mut Self, WireError> {
        if b.len() > 0xffff {
            return Err(WireError::value_out_of_range(
                b.len(),
                "lenPrefixedBytes16 length",
            ));
        }
        self.u16_be(b.len() as u16);
        Ok(self.bytes(b))
    }

    /// The written bytes, consuming the writer.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// A view of the written bytes without consuming the writer.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
