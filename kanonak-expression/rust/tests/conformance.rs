//! Drives the shared parity vectors through the Rust kanonak-expression port.

use kanonak_expression::*;
use serde_json::Value as J;
use std::collections::HashMap;
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

/// Conformance resolve hook: a `tx.VarRef` returns `env[varName]` (error if
/// absent); any other unknown leaf is an error.
fn resolve_vector(
    node: &J,
    env: &mut HashMap<String, f64>,
    _recurse: &mut dyn FnMut(&J, &mut HashMap<String, f64>) -> Result<f64, ExpressionError>,
) -> Result<f64, ExpressionError> {
    let typ = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if typ == "kanonak.org/transformations/VarRef" {
        let name = node
            .get("varName")
            .and_then(|n| n.as_str())
            .ok_or_else(|| ExpressionError("VarRef missing varName".into()))?;
        match env.get(name) {
            Some(v) => Ok(*v),
            None => Err(ExpressionError(format!("unbound variable '{name}'"))),
        }
    } else {
        Err(ExpressionError(format!("unresolved leaf '{typ}'")))
    }
}

fn env_of(v: &J) -> HashMap<String, f64> {
    let mut env = HashMap::new();
    if let Some(obj) = v.get("env").and_then(|e| e.as_object()) {
        for (k, val) in obj {
            if let Some(n) = val.as_f64() {
                env.insert(k.clone(), n);
            }
        }
    }
    env
}

#[test]
fn expression_vectors() {
    let doc = read("expression-vectors.json");
    let vectors = doc["vectors"].as_array().unwrap();
    let total = vectors.len();
    let mut pass = 0;

    for v in vectors {
        let id = v["id"].as_str().unwrap();
        let expr = &v["expr"];
        let expect_error = v.get("expectError").and_then(|x| x.as_bool()).unwrap_or(false);
        let mut env = env_of(v);

        let result = evaluate(expr, &mut env, &resolve_vector);

        match result {
            Ok(got) => {
                if expect_error {
                    eprintln!("FAIL [{}] expected error, got {}", id, got);
                    continue;
                }
                let expected = v["expected"].as_f64().unwrap();
                let ok = match v.get("tolerance").and_then(|t| t.as_f64()) {
                    Some(tol) => (got - expected).abs() <= tol,
                    None => got == expected,
                };
                if ok {
                    pass += 1;
                } else {
                    eprintln!("FAIL [{}] expected {}, got {}", id, expected, got);
                }
            }
            Err(e) => {
                if expect_error {
                    pass += 1;
                } else {
                    eprintln!("FAIL [{}] raised: {}", id, e.0);
                }
            }
        }
    }

    println!("expression-vectors: {}/{} pass", pass, total);
    assert_eq!(pass, total, "{} expression vector(s) failed", total - pass);
}
