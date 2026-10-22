//! Kanonak canonical form + content hash (canonicalFormVersion "1").
//!
//! An independent conformant implementation of `kanonak.org/canonical-form`,
//! verified byte-for-byte against the golden vectors. Identity of a literal is
//! `(carrier, canonical lexical)`; the wire form is compact JSON with UTF-8 byte
//! ordering, RFC 8785 escaping, and a fixed per-blob field order; the content
//! address is the SHA-256 of those bytes.

use regex::Regex;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

pub mod coordinate;
pub use coordinate::{
    display_name, lenient_versionless_key, local_name, parse_coordinate, versionless_key,
    Coordinate,
    CoordinateVersion,
    is_readable_by
};

pub const CANONICAL_FORM_VERSION: &str = "1";

// ===========================================================================
// Carriers
// ===========================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Carrier {
    Integer,
    Decimal,
    Double,
    Float,
    Boolean,
    String,
    AnyUri,
    LangString,
    DateTime,
    Date,
    Time,
    HexBinary,
    Base64Binary,
}

impl Carrier {
    pub fn tag(self) -> &'static str {
        match self {
            Carrier::Integer => "integer",
            Carrier::Decimal => "decimal",
            Carrier::Double => "double",
            Carrier::Float => "float",
            Carrier::Boolean => "boolean",
            Carrier::String => "string",
            Carrier::AnyUri => "anyURI",
            Carrier::LangString => "langString",
            Carrier::DateTime => "dateTime",
            Carrier::Date => "date",
            Carrier::Time => "time",
            Carrier::HexBinary => "hexBinary",
            Carrier::Base64Binary => "base64Binary",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Carrier> {
        Some(match tag {
            "integer" => Carrier::Integer,
            "decimal" => Carrier::Decimal,
            "double" => Carrier::Double,
            "float" => Carrier::Float,
            "boolean" => Carrier::Boolean,
            "string" => Carrier::String,
            "anyURI" => Carrier::AnyUri,
            "langString" => Carrier::LangString,
            "dateTime" => Carrier::DateTime,
            "date" => Carrier::Date,
            "time" => Carrier::Time,
            "hexBinary" => Carrier::HexBinary,
            "base64Binary" => Carrier::Base64Binary,
            _ => return None,
        })
    }
}

/// Carrier for a datatype URI, or `None` (out-of-set → raw-token tier).
/// Routing is total: a structurally unparseable URI has no carrier and its
/// raw token is preserved — never a guess, never a panic.
pub fn carrier_of(datatype_uri: &str) -> Option<Carrier> {
    let key = lenient_versionless_key(datatype_uri)?;
    if key == "kanonak.org/core-rdf/langString" {
        return Some(Carrier::LangString);
    }
    let name = match key.strip_prefix("kanonak.org/core-xsd/") {
        Some(n) => n,
        None => return None,
    };
    Some(match name {
        "integer" | "long" | "int" | "short" | "byte" | "unsignedLong" | "unsignedInt"
        | "unsignedShort" | "unsignedByte" | "nonNegativeInteger" | "positiveInteger"
        | "nonPositiveInteger" | "negativeInteger" => Carrier::Integer,
        "decimal" => Carrier::Decimal,
        "double" => Carrier::Double,
        "float" => Carrier::Float,
        "boolean" => Carrier::Boolean,
        "string" | "normalizedString" | "token" => Carrier::String,
        "anyURI" => Carrier::AnyUri,
        "dateTime" => Carrier::DateTime,
        "date" => Carrier::Date,
        "time" => Carrier::Time,
        "hexBinary" => Carrier::HexBinary,
        "base64Binary" => Carrier::Base64Binary,
        _ => return None,
    })
}

// ===========================================================================
// Per-carrier canonical lexical forms
// ===========================================================================

#[derive(Debug)]
pub struct CanonError(pub String);

macro_rules! re {
    ($s:expr) => {{
        static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| Regex::new($s).unwrap())
    }};
}

pub fn canonical_integer(raw: &str) -> Result<String, CanonError> {
    let t = raw.trim();
    if !re!(r"^[+-]?\d+$").is_match(t) {
        return Err(CanonError(format!("canonicalInteger: '{}' invalid", raw)));
    }
    let (sign, digits) = if let Some(r) = t.strip_prefix('-') {
        ("-", r)
    } else if let Some(r) = t.strip_prefix('+') {
        ("", r)
    } else {
        ("", t)
    };
    let stripped = digits.trim_start_matches('0');
    let stripped = if stripped.is_empty() { "0" } else { stripped };
    if stripped == "0" {
        Ok("0".to_string())
    } else {
        Ok(format!("{}{}", sign, stripped))
    }
}

pub fn canonical_decimal(raw: &str) -> Result<String, CanonError> {
    let t = raw.trim();
    let caps = re!(r"^([+-]?)(\d*)(?:\.(\d*))?$")
        .captures(t)
        .ok_or_else(|| CanonError(format!("canonicalDecimal: '{}' invalid", raw)))?;
    let int_raw = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    let frac_raw = caps.get(3).map(|m| m.as_str()).unwrap_or("");
    if int_raw.is_empty() && frac_raw.is_empty() {
        return Err(CanonError(format!("canonicalDecimal: '{}' invalid", raw)));
    }
    let sign = if caps.get(1).map(|m| m.as_str()).unwrap_or("") == "-" {
        "-"
    } else {
        ""
    };
    let int_part = {
        let s = int_raw.trim_start_matches('0');
        if s.is_empty() {
            "0"
        } else {
            s
        }
    };
    let frac_part = frac_raw.trim_end_matches('0');
    let magnitude = if !frac_part.is_empty() {
        format!("{}.{}", int_part, frac_part)
    } else {
        int_part.to_string()
    };
    if magnitude == "0" {
        Ok("0".to_string())
    } else {
        Ok(format!("{}{}", sign, magnitude))
    }
}

fn canonical_ieee(raw: &str, single: bool, label: &str) -> Result<String, CanonError> {
    let t = raw.trim();
    if t == "NaN" {
        return Ok("NaN".to_string());
    }
    if t == "INF" {
        return Ok("INF".to_string());
    }
    if t == "-INF" {
        return Ok("-INF".to_string());
    }
    if !re!(r"^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$").is_match(t) {
        return Err(CanonError(format!("canonical{}: '{}' invalid", label, raw)));
    }
    let n: f64 = t
        .parse()
        .map_err(|_| CanonError(format!("canonical{}: '{}' invalid", label, raw)))?;
    // A float is its value rounded to 32 bits, then written as that double (runtime#35).
    let value = if single { (n as f32) as f64 } else { n };
    if !value.is_finite() {
        return Err(CanonError(format!(
            "canonical{}: '{}' out of range",
            label, raw
        )));
    }
    if value == 0.0 {
        return Ok("0".to_string());
    }
    Ok(ecmascript_double_token(value))
}

/// ECMAScript's Number::toString of a finite, non-zero double: RFC 8785's number notation. `{:e}` gives the
/// shortest round-tripping digits; only the notation is re-derived, plain while -6 < n <= 21.
fn ecmascript_double_token(d: f64) -> String {
    let sign = if d < 0.0 { "-" } else { "" };
    let scientific = format!("{:e}", d.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("Rust's {:e} always writes an exponent");
    let exponent: i32 = exponent.parse().expect("Rust's {:e} exponent is an integer");
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let combined = format!("{}{}", int_part, frac_part);
    let lead = combined.len() - combined.trim_start_matches('0').len();
    let digits = combined[lead..].trim_end_matches('0');
    let n = int_part.len() as i32 - lead as i32 + exponent;
    let k = digits.len() as i32;
    let body = if k <= n && n <= 21 {
        format!("{}{}", digits, "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        let e = n - 1;
        let head = if k == 1 {
            digits.to_string()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!("{}e{}{}", head, if e >= 0 { "+" } else { "-" }, e.abs())
    };
    format!("{}{}", sign, body)
}

pub fn canonical_double(raw: &str) -> Result<String, CanonError> {
    canonical_ieee(raw, false, "Double")
}

pub fn canonical_float(raw: &str) -> Result<String, CanonError> {
    canonical_ieee(raw, true, "Float")
}

pub fn canonical_boolean(raw: &str) -> Result<String, CanonError> {
    let t = raw.trim();
    if t == "true" || t == "1" {
        Ok("true".to_string())
    } else if t == "false" || t == "0" {
        Ok("false".to_string())
    } else {
        Err(CanonError(format!("canonicalBoolean: '{}' invalid", raw)))
    }
}

pub fn canonical_string(raw: &str) -> String {
    raw.nfc().collect()
}

pub fn canonical_language_tag(tag: &str) -> Result<String, CanonError> {
    let subs: Vec<&str> = tag.trim().split('-').collect();
    if subs.is_empty() || subs[0].is_empty() {
        return Err(CanonError(format!(
            "canonicalLanguageTag: '{}' invalid",
            tag
        )));
    }
    let out: Vec<String> = subs
        .iter()
        .enumerate()
        .map(|(i, sub)| {
            if i == 0 {
                sub.to_lowercase()
            } else if sub.len() == 4 && sub.chars().all(|c| c.is_ascii_alphabetic()) {
                let mut s = String::new();
                let mut chars = sub.chars();
                s.push(chars.next().unwrap().to_ascii_uppercase());
                s.extend(chars.map(|c| c.to_ascii_lowercase()));
                s
            } else if sub.len() == 2 && sub.chars().all(|c| c.is_ascii_alphabetic()) {
                sub.to_uppercase()
            } else {
                sub.to_lowercase()
            }
        })
        .collect();
    Ok(out.join("-"))
}

pub fn canonical_hex_binary(raw: &str) -> Result<String, CanonError> {
    let t = raw.trim();
    if !re!(r"^([0-9A-Fa-f]{2})*$").is_match(t) {
        return Err(CanonError(format!("canonicalHexBinary: '{}' invalid", raw)));
    }
    Ok(t.to_uppercase())
}

pub fn canonical_base64(raw: &str) -> Result<String, CanonError> {
    let stripped: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if !re!(r"^[A-Za-z0-9+/]*={0,2}$").is_match(&stripped) || stripped.len() % 4 != 0 {
        return Err(CanonError(format!("canonicalBase64: '{}' invalid", raw)));
    }
    let bytes = base64_decode(&stripped)
        .ok_or_else(|| CanonError(format!("canonicalBase64: '{}' invalid", raw)))?;
    Ok(base64_encode(&bytes))
}

// -- temporal: XSD 1.1's rules, on the proleptic Gregorian calendar with a year 0 ---------
// A year of any length is kept as its sign and digits, so a UTC shift across a year boundary has no range limit
// (runtime#36). A leap year needs only the year's remainder by 400, and a shift moves the year by one at most.

fn pad2(n: i64) -> String {
    if n < 10 {
        format!("0{}", n)
    } else {
        n.to_string()
    }
}

/// A year of any length: its sign and its digits, without leading zeros. Year 0 is "0", with no sign.
struct Year {
    negative: bool,
    digits: String,
}

impl Year {
    fn parse(raw: &str) -> Year {
        let digits = raw.trim_start_matches('-').trim_start_matches('0');
        Year::of(raw.starts_with('-'), if digits.is_empty() { "0".to_string() } else { digits.to_string() })
    }

    fn of(negative: bool, digits: String) -> Year {
        Year { negative: negative && digits != "0", digits }
    }

    /// The year's remainder by 400, never negative.
    fn rem_400(&self) -> u32 {
        let r = self.digits.bytes().fold(0u32, |acc, b| (acc * 10 + u32::from(b - b'0')) % 400);
        if self.negative && r != 0 {
            400 - r
        } else {
            r
        }
    }

    fn is_leap(&self) -> bool {
        let r = self.rem_400();
        r % 4 == 0 && (r % 100 != 0 || r == 0)
    }

    /// At least four digits wide, its sign kept.
    fn text(&self) -> String {
        format!("{}{:0>4}", if self.negative { "-" } else { "" }, self.digits)
    }

    fn next(&self) -> Year {
        if self.negative {
            Year::of(true, decrement_digits(&self.digits))
        } else {
            Year::of(false, increment_digits(&self.digits))
        }
    }

    fn previous(&self) -> Year {
        if self.negative || self.digits == "0" {
            Year::of(true, increment_digits(&self.digits))
        } else {
            Year::of(false, decrement_digits(&self.digits))
        }
    }
}

fn increment_digits(digits: &str) -> String {
    let mut bytes = digits.as_bytes().to_vec();
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'9' {
            bytes[i] = b'0';
        } else {
            bytes[i] += 1;
            return String::from_utf8(bytes).expect("ASCII digits");
        }
    }
    format!("1{}", String::from_utf8(bytes).expect("ASCII digits"))
}

/// One less than a positive number written in digits.
fn decrement_digits(digits: &str) -> String {
    let mut bytes = digits.as_bytes().to_vec();
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'0' {
            bytes[i] = b'9';
        } else {
            bytes[i] -= 1;
            break;
        }
    }
    let text = String::from_utf8(bytes).expect("ASCII digits");
    let trimmed = text.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn days_in_month(year: &Year, month: u32) -> u32 {
    if month == 2 && year.is_leap() {
        29
    } else {
        DAYS_IN_MONTH[(month - 1) as usize]
    }
}

struct Civil {
    year: Year,
    month: u32,
    day: u32,
}

fn two_digits(field: &str) -> u32 {
    field.parse().expect("the pattern matched two digits")
}

fn civil_of(yy: &str, mo: &str, dd: &str, what: &str, raw: &str) -> Result<Civil, CanonError> {
    let year = Year::parse(yy);
    let (month, day) = (two_digits(mo), two_digits(dd));
    if !(1..=12).contains(&month) {
        return Err(CanonError(format!("{}: '{}' has month {}; a month is 01 to 12", what, raw, mo)));
    }
    if day < 1 || day > days_in_month(&year, month) {
        return Err(CanonError(format!(
            "{}: '{}' has day {}, which month {} of year {} doesn't have",
            what,
            raw,
            dd,
            mo,
            year.text()
        )));
    }
    Ok(Civil { year, month, day })
}

fn next_day(c: Civil) -> Civil {
    if c.day < days_in_month(&c.year, c.month) {
        Civil { day: c.day + 1, ..c }
    } else if c.month < 12 {
        Civil { month: c.month + 1, day: 1, ..c }
    } else {
        Civil { year: c.year.next(), month: 1, day: 1 }
    }
}

fn previous_day(c: Civil) -> Civil {
    if c.day > 1 {
        Civil { day: c.day - 1, ..c }
    } else if c.month > 1 {
        let day = days_in_month(&c.year, c.month - 1);
        Civil { month: c.month - 1, day, ..c }
    } else {
        Civil { year: c.year.previous(), month: 12, day: 31 }
    }
}

/// Minutes east of UTC, or None with no offset. XSD 1.1 bounds an offset at 14:00.
fn offset_minutes(tz: &str, what: &str, raw: &str) -> Result<Option<i64>, CanonError> {
    if tz.is_empty() {
        return Ok(None);
    }
    if tz == "Z" {
        return Ok(Some(0));
    }
    let (hours, minutes) = (i64::from(two_digits(&tz[1..3])), i64::from(two_digits(&tz[4..6])));
    if minutes > 59 || hours > 14 || (hours == 14 && minutes > 0) {
        return Err(CanonError(format!(
            "{}: '{}' has offset {}; an offset is at most 14:00 either way",
            what, raw, tz
        )));
    }
    let sign = if tz.starts_with('-') { -1 } else { 1 };
    Ok(Some(sign * (hours * 60 + minutes)))
}

/// XSD 1.1: no 60th second, and 24:00:00 only as the first instant of the next day.
fn clock_of(hh: &str, mi: &str, ss: &str, fraction: &str, what: &str, raw: &str) -> Result<(i64, i64), CanonError> {
    let (hour, minute, second) = (i64::from(two_digits(hh)), i64::from(two_digits(mi)), i64::from(two_digits(ss)));
    let end_of_day = hour == 24 && minute == 0 && second == 0 && fraction.is_empty();
    if minute > 59 || second > 59 || (hour > 23 && !end_of_day) {
        return Err(CanonError(format!(
            "{}: '{}' has time {}:{}:{}{}; XSD 1.1 allows 00:00:00 to 23:59:59, and 24:00:00 exactly",
            what, raw, hh, mi, ss, fraction
        )));
    }
    Ok((hour, minute))
}

fn canonical_fraction(frac: &str) -> String {
    if frac.is_empty() {
        return String::new();
    }
    let trimmed = frac.trim_start_matches('.').trim_end_matches('0');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!(".{}", trimmed)
    }
}

fn canonical_tz(tz: &str) -> String {
    if tz.is_empty() {
        String::new()
    } else if tz == "Z" || tz == "+00:00" || tz == "-00:00" {
        "Z".to_string()
    } else {
        tz.to_string()
    }
}

pub fn canonical_date_time(raw: &str) -> Result<String, CanonError> {
    let what = "canonicalDateTime";
    let caps =
        re!(r"^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$")
            .captures(raw.trim())
            .ok_or_else(|| CanonError(format!("{}: '{}' invalid", what, raw)))?;
    let g = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("");
    let fraction = canonical_fraction(g(7));
    let tz = g(8);
    let mut civil = civil_of(g(1), g(2), g(3), what, raw)?;
    let (hour, minute) = clock_of(g(4), g(5), g(6), &fraction, what, raw)?;
    let mut minutes = hour * 60 + minute - offset_minutes(tz, what, raw)?.unwrap_or(0);
    if minutes < 0 {
        minutes += 1440;
        civil = previous_day(civil);
    } else if minutes >= 1440 {
        minutes -= 1440;
        civil = next_day(civil);
    }
    Ok(format!(
        "{}-{}-{}T{}:{}:{}{}{}",
        civil.year.text(),
        pad2(i64::from(civil.month)),
        pad2(i64::from(civil.day)),
        pad2(minutes / 60),
        pad2(minutes % 60),
        g(6),
        fraction,
        if tz.is_empty() { "" } else { "Z" }
    ))
}

pub fn canonical_date(raw: &str) -> Result<String, CanonError> {
    let what = "canonicalDate";
    let caps = re!(r"^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$")
        .captures(raw.trim())
        .ok_or_else(|| CanonError(format!("{}: '{}' invalid", what, raw)))?;
    let g = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("");
    let civil = civil_of(g(1), g(2), g(3), what, raw)?;
    offset_minutes(g(4), what, raw)?;
    Ok(format!("{}-{}-{}{}", civil.year.text(), g(2), g(3), canonical_tz(g(4))))
}

pub fn canonical_time(raw: &str) -> Result<String, CanonError> {
    let what = "canonicalTime";
    let caps = re!(r"^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$")
        .captures(raw.trim())
        .ok_or_else(|| CanonError(format!("{}: '{}' invalid", what, raw)))?;
    let g = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("");
    let fraction = canonical_fraction(g(4));
    let (hour, _) = clock_of(g(1), g(2), g(3), &fraction, what, raw)?;
    offset_minutes(g(5), what, raw)?;
    Ok(format!(
        "{}:{}:{}{}{}",
        if hour == 24 { "00" } else { g(1) },
        g(2),
        g(3),
        fraction,
        canonical_tz(g(5))
    ))
}

fn lang_string_refused(raw: &str) -> CanonError {
    CanonError(format!("canonicalLangString: a language string is a structured value, an embedded {{ lexicalForm, language }}, so its tag is hashed with it; the langString carrier has no slot for the tag and refuses the scalar '{}'", raw))
}

pub fn canonical_scalar_lexical(carrier: Carrier, raw: &str) -> Result<String, CanonError> {
    Ok(match carrier {
        Carrier::Integer => canonical_integer(raw)?,
        Carrier::Decimal => canonical_decimal(raw)?,
        Carrier::Double => canonical_double(raw)?,
        Carrier::Float => canonical_float(raw)?,
        Carrier::Boolean => canonical_boolean(raw)?,
        Carrier::String => canonical_string(raw),
        Carrier::AnyUri => canonical_string(raw),
        Carrier::LangString => return Err(lang_string_refused(raw)),
        Carrier::DateTime => canonical_date_time(raw)?,
        Carrier::Date => canonical_date(raw)?,
        Carrier::Time => canonical_time(raw)?,
        Carrier::HexBinary => canonical_hex_binary(raw)?,
        Carrier::Base64Binary => canonical_base64(raw)?,
    })
}

// -- base64 (RFC 4648, standard alphabet) -----------------------------------

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        out.push(B64[(b[0] >> 2) as usize] as char);
        out.push(B64[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64[(((b[1] & 0x0f) << 2) | (b[2] >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64[(b[2] & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    for chunk in bytes.chunks(4) {
        let mut acc: u32 = 0;
        let mut pads = 0;
        for &c in chunk {
            acc <<= 6;
            if c == b'=' {
                pads += 1;
            } else {
                acc |= val(c)? as u32;
            }
        }
        out.push((acc >> 16) as u8);
        if pads < 2 {
            out.push((acc >> 8) as u8);
        }
        if pads < 1 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

// ===========================================================================
// Value model + wire form
// ===========================================================================

pub enum Value {
    Typed {
        carrier: Carrier,
        lexical: String,
    },
    Raw(String),
    Reference(String),
    Embedded {
        name: Option<String>,
        statements: Vec<Statement>,
    },
    List(Vec<Value>),
}

pub struct Statement {
    pub predicate: String,
    pub value: Value,
}

pub struct Subject {
    pub uri: String,
    pub statements: Vec<Statement>,
}

pub struct Package {
    pub subjects: Vec<Subject>,
}

pub fn canonical_form(pkg: &Package) -> Result<String, CanonError> {
    let mut out = String::from("{\"subjects\":[");
    let mut subjects: Vec<&Subject> = pkg.subjects.iter().collect();
    subjects.sort_by(|a, b| a.uri.as_bytes().cmp(b.uri.as_bytes()));
    for (i, s) in subjects.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"uri\":");
        emit_json_string(&mut out, &s.uri);
        out.push_str(",\"statements\":[");
        emit_statements(&mut out, &s.statements)?;
        out.push_str("]}");
    }
    out.push_str("]}");
    Ok(out)
}

pub fn canonical_hash(pkg: &Package) -> Result<String, CanonError> {
    let form = canonical_form(pkg)?;
    let mut hasher = Sha256::new();
    hasher.update(form.as_bytes());
    let digest = hasher.finalize();
    Ok(format!("sha256:{:x}", digest))
}

fn serialize_statement(st: &Statement) -> Result<String, CanonError> {
    let mut out = String::from("{\"predicate\":");
    emit_json_string(&mut out, &st.predicate);
    out.push(',');
    emit_value_tail(&mut out, &st.value)?;
    out.push('}');
    Ok(out)
}

// Order by predicate UTF-8 bytes; equal predicates (possible since multi-typed
// subjects — several type statements share the type predicate) order by the
// serialized statement blob's UTF-8 bytes. The tie-break makes the declared
// invariance under statement ordering TRUE for same-predicate statements
// rather than an accident of sort stability; no distinct-predicate ordering
// is affected.
fn emit_statements(out: &mut String, stmts: &[Statement]) -> Result<(), CanonError> {
    let mut rendered: Vec<(&[u8], String)> = Vec::with_capacity(stmts.len());
    for st in stmts {
        rendered.push((st.predicate.as_bytes(), serialize_statement(st)?));
    }
    rendered.sort_by(|a, b| {
        a.0.cmp(b.0)
            .then_with(|| a.1.as_bytes().cmp(b.1.as_bytes()))
    });
    for (i, (_, serialized)) in rendered.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(serialized);
    }
    Ok(())
}

fn emit_value_tail(out: &mut String, v: &Value) -> Result<(), CanonError> {
    match v {
        Value::Typed { carrier, lexical } => {
            out.push_str("\"type\":\"typed\",\"carrier\":");
            emit_json_string(out, carrier.tag());
            out.push_str(",\"value\":");
            emit_json_string(out, &canonical_scalar_lexical(*carrier, lexical)?);
        }
        Value::Raw(s) => {
            out.push_str("\"type\":\"string\",\"value\":");
            emit_json_string(out, s);
        }
        Value::Reference(uri) => {
            out.push_str("\"type\":\"ref\",\"value\":");
            emit_json_string(out, uri);
        }
        Value::Embedded { name, statements } => {
            out.push_str("\"type\":\"embedded\"");
            if let Some(n) = name {
                out.push_str(",\"name\":");
                emit_json_string(out, n);
            }
            out.push_str(",\"statements\":[");
            emit_statements(out, statements)?;
            out.push(']');
        }
        Value::List(items) => {
            out.push_str("\"type\":\"list\",\"items\":[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('{');
                emit_value_tail(out, item)?;
                out.push('}');
            }
            out.push(']');
        }
    }
    Ok(())
}

/// RFC 8785 / JSON.stringify string escaping.
fn emit_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
