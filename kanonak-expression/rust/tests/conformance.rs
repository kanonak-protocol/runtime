//! Drives the shared parity vectors through the Rust kanonak-expression port.
//! Every vector runs through `evaluate` AND `explain` and their values must
//! agree; ordered-comparison vectors supply `closures` and `refEnv`, and
//! vectors with a `trace` assert the verdict tree structurally.

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

struct Ctx {
    env: HashMap<String, f64>,
    ref_env: HashMap<String, String>,
}

/// Conformance resolve hook: a `tx.VarRef` returns `env[varName]` (error if
/// absent); any other unknown leaf is an error.
fn resolve_vector(
    node: &J,
    ctx: &mut Ctx,
    _recurse: &mut dyn FnMut(&J, &mut Ctx) -> Result<f64, ExpressionError>,
) -> Result<f64, ExpressionError> {
    let typ = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if typ == "kanonak.org/transformations/VarRef" {
        let name = node
            .get("varName")
            .and_then(|n| n.as_str())
            .ok_or_else(|| ExpressionError("VarRef missing varName".into()))?;
        match ctx.env.get(name) {
            Some(v) => Ok(*v),
            None => Err(ExpressionError(format!("unbound variable '{name}'"))),
        }
    } else {
        Err(ExpressionError(format!("unresolved leaf '{typ}'")))
    }
}

/// The identity-domain mirror: a `tx.VarRef` returns `refEnv[varName]` as a
/// member URI. Same division as `resolve_vector` — the kernel owns UriLiteral,
/// the caller owns bindings.
fn resolve_ref_vector(node: &J, ctx: &mut Ctx) -> Result<String, ExpressionError> {
    let typ = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if typ == "kanonak.org/transformations/VarRef" {
        let name = node
            .get("varName")
            .and_then(|n| n.as_str())
            .ok_or_else(|| ExpressionError("VarRef missing varName".into()))?;
        match ctx.ref_env.get(name) {
            Some(v) => Ok(v.clone()),
            None => Err(ExpressionError(format!("unbound reference '{name}'"))),
        }
    } else {
        Err(ExpressionError(format!("no reference resolver for leaf '{typ}'")))
    }
}

fn ctx_of(v: &J) -> Ctx {
    let mut env = HashMap::new();
    if let Some(obj) = v.get("env").and_then(|e| e.as_object()) {
        for (k, val) in obj {
            if let Some(n) = val.as_f64() {
                env.insert(k.clone(), n);
            }
        }
    }
    let mut ref_env = HashMap::new();
    if let Some(obj) = v.get("refEnv").and_then(|e| e.as_object()) {
        for (k, val) in obj {
            if let Some(s) = val.as_str() {
                ref_env.insert(k.clone(), s.to_string());
            }
        }
    }
    Ctx { env, ref_env }
}

fn closures_of(v: &J) -> Option<ClosureTable> {
    let obj = v.get("closures")?.as_object()?;
    let mut table = ClosureTable::new();
    for (prop, members) in obj {
        let mut inner = HashMap::new();
        if let Some(m) = members.as_object() {
            for (from, reachable) in m {
                let set: Vec<String> = reachable
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                inner.insert(from.clone(), set);
            }
        }
        table.insert(prop.clone(), inner);
    }
    Some(table)
}

/// Structural equality of a produced verdict tree against the vector's expected
/// JSON tree, including absent-vs-present refs.
fn trace_matches(got: &TraceNode, want: &J) -> bool {
    if want.get("type").and_then(|t| t.as_str()) != Some(got.typ.as_str()) {
        return false;
    }
    if want.get("value").and_then(|v| v.as_f64()) != Some(got.value) {
        return false;
    }
    let want_left = want.get("leftRef").and_then(|v| v.as_str());
    if want_left != got.left_ref.as_deref() {
        return false;
    }
    let want_right = want.get("rightRef").and_then(|v| v.as_str());
    if want_right != got.right_ref.as_deref() {
        return false;
    }
    let want_children: &[J] = want
        .get("children")
        .and_then(|c| c.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    if want_children.len() != got.children.len() {
        return false;
    }
    got.children
        .iter()
        .zip(want_children.iter())
        .all(|(g, w)| trace_matches(g, w))
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
        let closures = closures_of(v);
        let options = EvalOptions {
            closures: closures.as_ref(),
            resolve_ref: Some(&resolve_ref_vector),
        };

        let mut ctx = ctx_of(v);
        let result = evaluate_with_options(expr, &mut ctx, &resolve_vector, Some(&options));
        let mut trace_ctx = ctx_of(v);
        let traced = explain(expr, &mut trace_ctx, &resolve_vector, Some(&options));

        if expect_error {
            if result.is_err() && traced.is_err() {
                pass += 1;
            } else {
                eprintln!("FAIL [{}] expected an error from evaluate AND explain", id);
            }
            continue;
        }

        let got = match result {
            Ok(got) => got,
            Err(e) => {
                eprintln!("FAIL [{}] raised: {}", id, e.0);
                continue;
            }
        };
        let trace = match traced {
            Ok(t) => t,
            Err(e) => {
                eprintln!("FAIL [{}] explain raised: {}", id, e.0);
                continue;
            }
        };
        let expected = v["expected"].as_f64().unwrap();
        let ok = match v.get("tolerance").and_then(|t| t.as_f64()) {
            Some(tol) => (got - expected).abs() <= tol,
            None => got == expected,
        };
        if !ok {
            eprintln!("FAIL [{}] expected {}, got {}", id, expected, got);
            continue;
        }
        if trace.value != got {
            eprintln!("FAIL [{}] explain value {} != evaluate value {}", id, trace.value, got);
            continue;
        }
        if let Some(want) = v.get("trace") {
            if !trace_matches(&trace, want) {
                eprintln!("FAIL [{}] trace mismatch", id);
                continue;
            }
        }
        pass += 1;
    }

    println!("expression-vectors: {}/{} pass", pass, total);
    assert_eq!(pass, total, "{} expression vector(s) failed", total - pass);
}
