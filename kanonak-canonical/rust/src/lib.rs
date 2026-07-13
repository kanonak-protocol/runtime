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

/// `publisher/package/name` carrier key from a datatype URI
/// (`publisher/package@ver/name` or `publisher/package/name`).
pub fn carrier_key(uri: &str) -> String {
    let idx = uri.rfind('/').unwrap();
    let name = &uri[idx + 1..];
    let head = &uri[..idx];
    let slash = head.find('/').unwrap();
    let publisher = &head[..slash];
    let pkg_full = &head[slash + 1..];
    let pkg = pkg_full.split('@').next().unwrap();
    format!("{}/{}/{}", publisher, pkg, name)
}

/// Carrier for a datatype URI, or `None` (out-of-set → raw-token tier).
pub fn carrier_of(datatype_uri: &str) -> Option<Carrier> {
    let key = carrier_key(datatype_uri);
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
    if single {
        let f = n as f32;
        if !f.is_finite() {
            return Err(CanonError(format!(
                "canonical{}: '{}' out of range",
                label, raw
            )));
        }
        if f == 0.0 {
            return Ok("0".to_string());
        }
        Ok(format!("{}", f))
    } else {
        if !n.is_finite() {
            return Err(CanonError(format!(
                "canonical{}: '{}' out of range",
                label, raw
            )));
        }
        if n == 0.0 {
            return Ok("0".to_string());
        }
        Ok(format!("{}", n))
    }
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

// -- temporal ---------------------------------------------------------------

fn pad2(n: i64) -> String {
    if n < 10 {
        format!("0{}", n)
    } else {
        n.to_string()
    }
}

fn canonical_year(raw: &str) -> String {
    let neg = raw.starts_with('-');
    let digits = if neg { &raw[1..] } else { raw }.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let padded = if digits.len() < 4 {
        format!("{:0>4}", digits)
    } else {
        digits.to_string()
    };
    format!("{}{}", if neg { "-" } else { "" }, padded)
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

fn tz_offset_minutes(tz: &str) -> i64 {
    if tz == "Z" {
        return 0;
    }
    let sign = if tz.starts_with('-') { -1 } else { 1 };
    let hh: i64 = tz[1..3].parse().unwrap();
    let mm: i64 = tz[4..6].parse().unwrap();
    sign * (hh * 60 + mm)
}

pub fn canonical_date_time(raw: &str) -> Result<String, CanonError> {
    let caps =
        re!(r"^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$")
            .captures(raw.trim())
            .ok_or_else(|| CanonError(format!("canonicalDateTime: '{}' invalid", raw)))?;
    let g = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("");
    let (yy, mo, dd, hh, mi, ss) = (g(1), g(2), g(3), g(4), g(5), g(6));
    let fraction = canonical_fraction(g(7));
    let tz = g(8);

    if tz.is_empty() {
        return Ok(format!(
            "{}-{}-{}T{}:{}:{}{}",
            canonical_year(yy),
            mo,
            dd,
            hh,
            mi,
            ss,
            fraction
        ));
    }
    // Proleptic-Gregorian UTC shift on the integer fields (days-from-epoch).
    let days = days_from_civil(
        yy.parse().unwrap(),
        mo.parse().unwrap(),
        dd.parse().unwrap(),
    );
    let secs = days * 86400
        + hh.parse::<i64>().unwrap() * 3600
        + mi.parse::<i64>().unwrap() * 60
        + ss.parse::<i64>().unwrap();
    let shifted = secs - tz_offset_minutes(tz) * 60;
    let (y, m, d, sod) = civil_from_secs(shifted);
    let (sh, sm, sss) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    Ok(format!(
        "{}-{}-{}T{}:{}:{}{}Z",
        canonical_year(&y.to_string()),
        pad2(m),
        pad2(d),
        pad2(sh),
        pad2(sm),
        pad2(sss),
        fraction
    ))
}

pub fn canonical_date(raw: &str) -> Result<String, CanonError> {
    let caps = re!(r"^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$")
        .captures(raw.trim())
        .ok_or_else(|| CanonError(format!("canonicalDate: '{}' invalid", raw)))?;
    let g = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("");
    Ok(format!(
        "{}-{}-{}{}",
        canonical_year(g(1)),
        g(2),
        g(3),
        canonical_tz(g(4))
    ))
}

pub fn canonical_time(raw: &str) -> Result<String, CanonError> {
    let caps = re!(r"^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$")
        .captures(raw.trim())
        .ok_or_else(|| CanonError(format!("canonicalTime: '{}' invalid", raw)))?;
    let g = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or("");
    let (mut hh, mi, ss) = (g(1).to_string(), g(2), g(3));
    let fraction = canonical_fraction(g(4));
    if hh == "24" && mi == "00" && ss == "00" && fraction.is_empty() {
        hh = "00".to_string();
    }
    Ok(format!(
        "{}:{}:{}{}{}",
        hh,
        mi,
        ss,
        fraction,
        canonical_tz(g(5))
    ))
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
        Carrier::LangString => canonical_string(raw),
        Carrier::DateTime => canonical_date_time(raw)?,
        Carrier::Date => canonical_date(raw)?,
        Carrier::Time => canonical_time(raw)?,
        Carrier::HexBinary => canonical_hex_binary(raw)?,
        Carrier::Base64Binary => canonical_base64(raw)?,
    })
}

// -- proleptic-Gregorian calendar (Howard Hinnant's algorithms) -------------

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_secs(secs: i64) -> (i64, i64, i64, i64) {
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d, sod)
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
