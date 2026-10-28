//! The serde binding of the JSON forms of integer and decimal values
//! (kanonak-protocol 4.11.0's `numbers-in-json`, runtime#54): the types a
//! generated struct's fields take, so serde_json reads and writes them as
//! `serialize_exact` and `deserialize_exact` do. `int` and narrower take `i32`
//! and the rest, since serde_json already refuses a string, a fraction, or a
//! value past their width for those.

use std::fmt;
use std::marker::PhantomData;

use kanonak_canonical::{canonical_decimal, canonical_integer};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use crate::{outside_bounds, within_double_precision, xsd_number_form, CodecError};

/// `integer` or one of its four sign-restricted forms, as a marker an
/// [`Integer`] is parameterized by, so the field states which datatype it holds
/// and its range is checked.
pub trait IntegerDatatype {
    /// The datatype's local name in core-xsd, by which the codec's one table
    /// gives its bounds.
    const LOCAL_NAME: &'static str;
}

macro_rules! integer_datatype {
    ($name:ident, $local:literal) => {
        #[doc = concat!("`xsd:", $local, "`, for [`Integer`].")]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub struct $name;

        impl IntegerDatatype for $name {
            const LOCAL_NAME: &'static str = $local;
        }
    };
}

integer_datatype!(XsdInteger, "integer");
integer_datatype!(NonNegativeInteger, "nonNegativeInteger");
integer_datatype!(PositiveInteger, "positiveInteger");
integer_datatype!(NonPositiveInteger, "nonPositiveInteger");
integer_datatype!(NegativeInteger, "negativeInteger");

fn refusal(why: impl fmt::Display) -> CodecError {
    CodecError::Malformed(format!("{} [json-number-form]", why))
}

fn integer_of_lexical(datatype: &str, lexical: &str) -> Result<String, CodecError> {
    canonical_integer(lexical)
        .map_err(|_| refusal(format!("{:?} is not an xsd:{} lexical form", lexical, datatype)))
}

/// A whole JSON number within ±(2^53 − 1), as canonical digits.
fn safe_whole(digits: String) -> Option<String> {
    Some(digits).filter(|d| within_double_precision(d))
}

fn whole_f64(v: f64) -> Option<String> {
    if v.fract() != 0.0 {
        return None;
    }
    canonical_integer(&format!("{:.0}", v)).ok().and_then(safe_whole)
}

/// An integer of datatype `D`, held as its canonical digits, since no Rust
/// integer holds every value. JSON carries it as a string of those digits, and
/// it reads from a string or a whole number within ±(2^53 − 1), inside `D`'s
/// range.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Integer<D: IntegerDatatype> {
    digits: String,
    datatype: PhantomData<D>,
}

impl<D: IntegerDatatype> Integer<D> {
    /// From any lexical form of the datatype, refused outside its range.
    pub fn parse(lexical: &str) -> Result<Self, CodecError> {
        Self::from_digits(integer_of_lexical(D::LOCAL_NAME, lexical)?)
    }

    fn from_digits(digits: String) -> Result<Self, CodecError> {
        let (_, min, max) = xsd_number_form(D::LOCAL_NAME)
            .expect("every IntegerDatatype names a datatype in the codec's table");
        if outside_bounds(&digits, min, max) {
            return Err(refusal(format!("{} is outside xsd:{}'s range", digits, D::LOCAL_NAME)));
        }
        Ok(Integer {
            digits,
            datatype: PhantomData,
        })
    }

    /// Its canonical digits.
    pub fn as_str(&self) -> &str {
        &self.digits
    }
}

impl<D: IntegerDatatype> fmt::Display for Integer<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.digits)
    }
}

impl<D: IntegerDatatype> Serialize for Integer<D> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.digits)
    }
}

impl<'de, D: IntegerDatatype> Deserialize<'de> for Integer<D> {
    fn deserialize<De: Deserializer<'de>>(deserializer: De) -> Result<Self, De::Error> {
        deserializer.deserialize_any(WholeVisitor(D::LOCAL_NAME, Integer::<D>::from_digits))
    }
}

/// A `long`, which JSON carries as a string of its digits. It reads from a
/// string or a whole number within ±(2^53 − 1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Long(pub i64);

/// An `unsignedLong`, which JSON carries as a string of its digits. It reads
/// from a string or a whole number within ±(2^53 − 1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct UnsignedLong(pub u64);

impl Long {
    /// From any lexical form of `xsd:long`, refused outside its width.
    pub fn parse(lexical: &str) -> Result<Self, CodecError> {
        Self::from_digits(integer_of_lexical("long", lexical)?)
    }

    fn from_digits(digits: String) -> Result<Self, CodecError> {
        digits
            .parse::<i64>()
            .map(Long)
            .map_err(|_| refusal(format!("{} is outside xsd:long's range", digits)))
    }
}

impl UnsignedLong {
    /// From any lexical form of `xsd:unsignedLong`, refused outside its width.
    pub fn parse(lexical: &str) -> Result<Self, CodecError> {
        Self::from_digits(integer_of_lexical("unsignedLong", lexical)?)
    }

    fn from_digits(digits: String) -> Result<Self, CodecError> {
        digits
            .parse::<u64>()
            .map(UnsignedLong)
            .map_err(|_| refusal(format!("{} is outside xsd:unsignedLong's range", digits)))
    }
}

impl fmt::Display for Long {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for UnsignedLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Long {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl Serialize for UnsignedLong {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Long {
    fn deserialize<De: Deserializer<'de>>(deserializer: De) -> Result<Self, De::Error> {
        deserializer.deserialize_any(WholeVisitor("long", Long::from_digits))
    }
}

impl<'de> Deserialize<'de> for UnsignedLong {
    fn deserialize<De: Deserializer<'de>>(deserializer: De) -> Result<Self, De::Error> {
        deserializer.deserialize_any(WholeVisitor("unsignedLong", UnsignedLong::from_digits))
    }
}

/// Reads a wide integer: from a string holding a lexical form of the datatype,
/// or from a whole JSON number within ±(2^53 − 1), then through `build`, which
/// checks the datatype's range.
struct WholeVisitor<T, F: FnOnce(String) -> Result<T, CodecError>>(&'static str, F);

impl<'de, T, F: FnOnce(String) -> Result<T, CodecError>> Visitor<'de> for WholeVisitor<T, F> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "an xsd:{} as a string of its digits, or a whole number within ±(2^53 − 1) [json-number-form]",
            self.0
        )
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<T, E> {
        integer_of_lexical(self.0, v)
            .and_then(self.1)
            .map_err(E::custom)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<T, E> {
        let digits = safe_whole(v.to_string()).ok_or_else(|| E::custom(past_double(self.0, v)))?;
        (self.1)(digits).map_err(E::custom)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<T, E> {
        let digits = safe_whole(v.to_string()).ok_or_else(|| E::custom(past_double(self.0, v)))?;
        (self.1)(digits).map_err(E::custom)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<T, E> {
        let digits = whole_f64(v).ok_or_else(|| {
            E::custom(refusal(format!(
                "an xsd:{} is read from a string, or from a whole JSON number within ±(2^53 − 1); got the number {}",
                self.0, v
            )))
        })?;
        (self.1)(digits).map_err(E::custom)
    }
}

fn past_double(datatype: &str, v: impl fmt::Display) -> CodecError {
    refusal(format!(
        "the number {} is past ±(2^53 − 1), where a JSON reader may have rounded it, so an xsd:{} past it is read from a string",
        v, datatype
    ))
}

/// A `decimal`, held as its canonical digits, since no Rust type in the
/// standard library holds every value. JSON carries it as a string of those
/// digits, and it reads from a string only, so it can't have been rounded.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Decimal(String);

impl Decimal {
    /// From any lexical form of `xsd:decimal`.
    pub fn parse(lexical: &str) -> Result<Self, CodecError> {
        canonical_decimal(lexical)
            .map(Decimal)
            .map_err(|_| refusal(format!("{:?} is not an xsd:decimal lexical form", lexical)))
    }

    /// Its canonical digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<De: Deserializer<'de>>(deserializer: De) -> Result<Self, De::Error> {
        struct DecimalVisitor;

        impl<'de> Visitor<'de> for DecimalVisitor {
            type Value = Decimal;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an xsd:decimal as a string of its digits, never a number, which may have been rounded [json-number-form]")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Decimal, E> {
                Decimal::parse(v).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(DecimalVisitor)
    }
}
