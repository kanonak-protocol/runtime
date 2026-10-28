//! The serde binding of the JSON forms (runtime#54): each read and write case
//! of the shared vectors through serde_json, with each field typed as a
//! generated struct types it. Our types refuse with `[json-number-form]`; `i32`
//! and the rest refuse with serde_json's own error, which the forms ask for.

use kanonak_codec::{
    Decimal, Integer, Long, NegativeInteger, NonNegativeInteger, NonPositiveInteger,
    PositiveInteger, UnsignedLong, XsdInteger,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value as J;
use std::fs;
use std::path::PathBuf;

fn read_doc(file: &str) -> J {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("vectors");
    p.push(file);
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

/// What a case reads as, compared with its `expected`: each value's canonical
/// lexical form.
trait Lexical {
    fn lexical(&self) -> J;
}

macro_rules! lexical_by_display {
    ($($t:ty),*) => {$(
        impl Lexical for $t {
            fn lexical(&self) -> J {
                J::String(self.to_string())
            }
        }
    )*};
}

lexical_by_display!(Long, UnsignedLong, Decimal, i32, i16, i8, u32, u16, u8);

impl<D: kanonak_codec::IntegerDatatype> Lexical for Integer<D> {
    fn lexical(&self) -> J {
        J::String(self.as_str().to_string())
    }
}

impl<T: Lexical> Lexical for Vec<T> {
    fn lexical(&self) -> J {
        J::Array(self.iter().map(Lexical::lexical).collect())
    }
}

/// The embedded `Amount`, as a generated struct holds it.
#[derive(Serialize, Deserialize)]
struct Amount {
    value: Decimal,
}

impl Lexical for Amount {
    fn lexical(&self) -> J {
        serde_json::json!({ "value": self.value.as_str() })
    }
}

/// Whether reading `json` as a `T` gives `expected`, or is refused when
/// `expected` is `None`. `ours` is whether `T` is one of this crate's types,
/// which name the refusal.
fn read<T: DeserializeOwned + Lexical>(json: &str, expected: Option<&J>, ours: bool) -> Result<(), String> {
    match (serde_json::from_str::<T>(json), expected) {
        (Ok(got), Some(want)) if &got.lexical() == want => Ok(()),
        (Ok(got), Some(want)) => Err(format!("expected {}, got {}", want, got.lexical())),
        (Ok(got), None) => Err(format!("expected a refusal, got {}", got.lexical())),
        (Err(e), None) if !ours || e.to_string().contains("[json-number-form]") => Ok(()),
        (Err(e), _) => Err(format!("refused: {}", e)),
    }
}

/// Whether the value whose canonical lexical form is `value` writes as `json`,
/// or can't be held, when `json` is `None`.
fn write<T: Serialize>(held: Result<T, String>, json: Option<&J>) -> Result<(), String> {
    match (held, json) {
        (Ok(v), Some(want)) => {
            let got = serde_json::to_value(&v).unwrap();
            if &got == want { Ok(()) } else { Err(format!("expected {}, got {}", want, got)) }
        }
        (Ok(v), None) => Err(format!("expected a refusal, wrote {}", serde_json::to_value(&v).unwrap())),
        (Err(_), None) => Ok(()),
        (Err(e), Some(_)) => Err(format!("refused: {}", e)),
    }
}

fn text(v: &J) -> &str {
    v.as_str().unwrap()
}

fn integer<D: kanonak_codec::IntegerDatatype>(v: &J) -> Result<Integer<D>, String> {
    Integer::<D>::parse(text(v)).map_err(|e| e.to_string())
}

fn narrow<T: std::str::FromStr>(v: &J) -> Result<T, String> {
    text(v).parse::<T>().map_err(|_| format!("{} doesn't fit the type", v))
}

#[test]
fn json_number_binding_vectors() {
    let doc = read_doc("codec-vectors-json-numbers.json");
    let mut fails = Vec::new();
    let mut checked = 0;
    for c in doc["readCases"].as_array().unwrap() {
        let (id, field, json) = (c["id"].as_str().unwrap(), c["field"].as_str().unwrap(), c["json"].as_str().unwrap());
        let expected = c.get("expected");
        let outcome = match field {
            "integer" => read::<Integer<XsdInteger>>(json, expected, true),
            "nonNegativeInteger" => read::<Integer<NonNegativeInteger>>(json, expected, true),
            "positiveInteger" => read::<Integer<PositiveInteger>>(json, expected, true),
            "nonPositiveInteger" => read::<Integer<NonPositiveInteger>>(json, expected, true),
            "negativeInteger" => read::<Integer<NegativeInteger>>(json, expected, true),
            "long" => read::<Long>(json, expected, true),
            "unsignedLong" => read::<UnsignedLong>(json, expected, true),
            "decimal" => read::<Decimal>(json, expected, true),
            "int" => read::<i32>(json, expected, false),
            "short" => read::<i16>(json, expected, false),
            "byte" => read::<i8>(json, expected, false),
            "unsignedInt" => read::<u32>(json, expected, false),
            "unsignedShort" => read::<u16>(json, expected, false),
            "unsignedByte" => read::<u8>(json, expected, false),
            "counts" => read::<Vec<Integer<XsdInteger>>>(json, expected, true),
            "amount" => read::<Amount>(json, expected, true),
            other => Err(format!("no binding type for field {}", other)),
        };
        checked += 1;
        if let Err(why) = outcome {
            fails.push(format!("read {}: {}", id, why));
        }
    }
    for c in doc["writeCases"].as_array().unwrap() {
        let (id, field, value) = (c["id"].as_str().unwrap(), c["field"].as_str().unwrap(), &c["value"]);
        let json: Option<J> = c.get("json").map(|t| serde_json::from_str(t.as_str().unwrap()).unwrap());
        let json = json.as_ref();
        let outcome = match field {
            "integer" => write(integer::<XsdInteger>(value), json),
            "nonNegativeInteger" => write(integer::<NonNegativeInteger>(value), json),
            "positiveInteger" => write(integer::<PositiveInteger>(value), json),
            "long" => write(Long::parse(text(value)).map_err(|e| e.to_string()), json),
            "unsignedLong" => write(UnsignedLong::parse(text(value)).map_err(|e| e.to_string()), json),
            "decimal" => write(Decimal::parse(text(value)).map_err(|e| e.to_string()), json),
            "int" => write(narrow::<i32>(value), json),
            "byte" => write(narrow::<i8>(value), json),
            "unsignedShort" => write(narrow::<u16>(value), json),
            "counts" => write(
                value.as_array().unwrap().iter().map(integer::<XsdInteger>).collect::<Result<Vec<_>, _>>(),
                json,
            ),
            "amount" => write(
                Decimal::parse(text(&value["value"])).map(|value| Amount { value }).map_err(|e| e.to_string()),
                json,
            ),
            other => Err(format!("no binding type for field {}", other)),
        };
        checked += 1;
        if let Err(why) = outcome {
            fails.push(format!("write {}: {}", id, why));
        }
    }
    for f in &fails {
        eprintln!("FAIL {}", f);
    }
    println!("codec-vectors-json-numbers.json (binding): {}/{} pass", checked - fails.len(), checked);
    assert!(fails.is_empty(), "{} binding case(s) failed", fails.len());
}

#[test]
fn a_number_a_double_rounds_is_judged_as_serde_json_reads_it() {
    // serde_json reads 9007199254740993 exactly, as a u64, and the form refuses it past ±(2^53 − 1).
    assert!(serde_json::from_str::<Integer<XsdInteger>>("9007199254740993").is_err());
    assert!(serde_json::from_str::<Long>("9007199254740993").is_err());
    assert_eq!(serde_json::from_str::<Long>("\"9223372036854775807\"").unwrap(), Long(i64::MAX));
}
