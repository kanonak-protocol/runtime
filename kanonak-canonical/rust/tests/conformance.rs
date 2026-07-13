//! Drives the shared golden vectors through the Rust kanonak-canonical port.

use kanonak_canonical::*;
use serde_json::Value as J;
use std::fs;
use std::path::PathBuf;

fn vectors_dir() -> PathBuf {
    // tests run from the crate root (rust/); vectors are at ../vectors.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("vectors");
    p
}

fn read(name: &str) -> J {
    let path = vectors_dir().join(name);
    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

#[test]
fn lexical_vectors() {
    let doc = read("lexical-vectors.json");
    let mut fails = 0;
    for v in doc["vectors"].as_array().unwrap() {
        let id = v["id"].as_str().unwrap();
        let carrier = Carrier::from_tag(v["carrier"].as_str().unwrap()).unwrap();
        let input = v["input"].as_str().unwrap();
        let expect_error = v
            .get("expectError")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        match canonical_scalar_lexical(carrier, input) {
            Ok(actual) => {
                if expect_error {
                    fails += 1;
                    eprintln!("FAIL [{}] expected error, got '{}'", id, actual);
                } else {
                    let expected = v["expected"].as_str().unwrap();
                    if actual != expected {
                        fails += 1;
                        eprintln!("FAIL [{}] expected '{}', got '{}'", id, expected, actual);
                    }
                }
            }
            Err(e) => {
                if !expect_error {
                    fails += 1;
                    eprintln!("FAIL [{}] threw: {}", id, e.0);
                }
            }
        }
    }
    assert_eq!(fails, 0, "{} lexical vector(s) failed", fails);
}

#[test]
fn full_form_vectors() {
    let doc = read("full-form-vectors.json");
    let mut fails = 0;
    for v in doc["vectors"].as_array().unwrap() {
        let id = v["id"].as_str().unwrap();
        let pkg = decode_subjects(&v["input"]);
        let form = canonical_form(&pkg).unwrap();
        let hash = canonical_hash(&pkg).unwrap();
        let exp_form = v["expectedCanonicalForm"].as_str().unwrap();
        let exp_hash = v["expectedHash"].as_str().unwrap();
        if form != exp_form {
            fails += 1;
            eprintln!(
                "FAIL [{}] form\n  expected: {}\n  actual:   {}",
                id, exp_form, form
            );
        }
        if hash != exp_hash {
            fails += 1;
            eprintln!("FAIL [{}] hash expected {} got {}", id, exp_hash, hash);
        }
    }
    assert_eq!(fails, 0, "{} full-form vector(s) failed", fails);
}

fn decode_subjects(input: &J) -> Package {
    let subjects = input["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Subject {
            uri: s["uri"].as_str().unwrap().to_string(),
            statements: decode_statements(s),
        })
        .collect();
    Package { subjects }
}

fn decode_statements(node: &J) -> Vec<Statement> {
    node.get("statements")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .map(|st| Statement {
                    predicate: st["predicate"].as_str().unwrap().to_string(),
                    value: decode_value(&st["value"]),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn decode_value(v: &J) -> Value {
    if let Some(lit) = v.get("lit").and_then(|x| x.as_str()) {
        let dt = v["datatype"].as_str().unwrap();
        match carrier_of(dt) {
            Some(c) => Value::Typed {
                carrier: c,
                lexical: lit.to_string(),
            },
            None => Value::Raw(lit.to_string()),
        }
    } else if let Some(raw) = v.get("raw").and_then(|x| x.as_str()) {
        Value::Raw(raw.to_string())
    } else if let Some(r) = v.get("ref").and_then(|x| x.as_str()) {
        Value::Reference(r.to_string())
    } else if let Some(emb) = v.get("embed") {
        Value::Embedded {
            name: emb
                .get("name")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            statements: decode_statements(emb),
        }
    } else if let Some(list) = v.get("list").and_then(|x| x.as_array()) {
        Value::List(list.iter().map(decode_value).collect())
    } else {
        panic!("decode: unknown value shape {}", v);
    }
}
