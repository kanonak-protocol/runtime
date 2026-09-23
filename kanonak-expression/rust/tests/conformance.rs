//! Drives the shared parity vectors through the Rust kanonak-expression port —
//! BOTH files: `expression-vectors.json` (v1 — must pass UNCHANGED under the
//! v2 kernel; the numeric-regression gate) and `expression-vectors-2.json`
//! (the value-domain extension). Every vector runs through `evaluate` AND
//! `explain` and their values must agree; `env` bindings and `expected` are
//! Values (numbers, strings, arrays, `{"ref": …}` objects); vectors with a
//! `trace` assert the verdict tree structurally.

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

/// JSON → Value, the vector-file encoding: numbers, strings, arrays (lists),
/// `{"ref": …}` objects (refs). Booleans normalize to 1/0 per the domain.
fn value_of(v: &J) -> EvalValue {
    match v {
        J::Number(n) => EvalValue::Num(n.as_f64().unwrap()),
        J::String(s) => EvalValue::Str(s.clone()),
        J::Bool(b) => EvalValue::Num(if *b { 1.0 } else { 0.0 }),
        J::Array(items) => EvalValue::List(items.iter().map(value_of).collect()),
        J::Object(o) => match o.get("ref").and_then(J::as_str) {
            Some(r) => EvalValue::Ref(r.to_string()),
            None => panic!("unrepresentable vector value: {v}"),
        },
        J::Null => panic!("null is not a Value"),
    }
}

struct Ctx {
    env: HashMap<String, EvalValue>,
    ref_env: HashMap<String, String>,
    /// A host graph for the caller's `tx.PropertyRead` leaf: ref URI → property → Value.
    graph: HashMap<String, HashMap<String, EvalValue>>,
}

/// Conformance resolve hook: a `tx.VarRef` returns `env[varName]` (error if
/// absent); a `tx.PropertyRead` is a host graph read — the documented
/// caller-leaf shape, the kernel never touches a graph. PropertyRead is the
/// reference engine's convention, pinned by the vectors so every port's harness
/// agrees: `readSource` is evaluated through the handed-back `recurse` (so a
/// loopVar bound by an enclosing iterator is visible — runtime#25) and MUST
/// yield a ref; then graph[ref][readProp] is absent → the empty list, several
/// values → a list, one value → itself. Any other unknown leaf is an error.
fn resolve_vector(
    node: &J,
    ctx: &mut Ctx,
    recurse: &mut dyn FnMut(&J, &mut Ctx) -> Result<EvalValue, ExpressionError>,
) -> Result<EvalValue, ExpressionError> {
    let typ = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if typ == "kanonak.org/transformations/VarRef" {
        let name = node
            .get("varName")
            .and_then(|n| n.as_str())
            .ok_or_else(|| ExpressionError("VarRef missing varName".into()))?;
        match ctx.env.get(name) {
            Some(v) => Ok(v.clone()),
            None => Err(ExpressionError(format!("unbound variable '{name}'"))),
        }
    } else if typ == "kanonak.org/transformations/PropertyRead" {
        let source_node = node
            .get("readSource")
            .ok_or_else(|| ExpressionError("PropertyRead missing readSource".into()))?;
        let uri = match recurse(source_node, ctx)? {
            EvalValue::Ref(u) => u,
            other => return Err(ExpressionError(format!("PropertyRead over a non-ref: {other:?}"))),
        };
        let prop = node
            .get("readProp")
            .and_then(|p| p.as_str())
            .ok_or_else(|| ExpressionError("PropertyRead missing readProp".into()))?;
        Ok(ctx
            .graph
            .get(&uri)
            .and_then(|props| props.get(prop))
            .cloned()
            .unwrap_or(EvalValue::List(Vec::new())))
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
            env.insert(k.clone(), value_of(val));
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
    let mut graph = HashMap::new();
    if let Some(obj) = v.get("graph").and_then(|g| g.as_object()) {
        for (uri, props) in obj {
            let mut m = HashMap::new();
            if let Some(pm) = props.as_object() {
                for (prop, val) in pm {
                    m.insert(prop.clone(), value_of(val));
                }
            }
            graph.insert(uri.clone(), m);
        }
    }
    Ctx { env, ref_env, graph }
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

/// Deep Value equality — the vector-comparison rule (lists compare
/// structurally so a list-valued `expected` can be asserted; NOT the
/// runtime's `Equals` primitive).
fn values_deep_equal(a: &EvalValue, b: &EvalValue) -> bool {
    match (a, b) {
        (EvalValue::Num(x), EvalValue::Num(y)) => x == y,
        (EvalValue::Str(x), EvalValue::Str(y)) => x == y,
        (EvalValue::Ref(x), EvalValue::Ref(y)) => x == y,
        (EvalValue::List(x), EvalValue::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_deep_equal(a, b))
        }
        _ => false,
    }
}

/// Structural equality of a produced verdict tree against the vector's expected
/// JSON tree, including absent-vs-present refs.
fn trace_matches(got: &TraceNode, want: &J) -> bool {
    if want.get("type").and_then(|t| t.as_str()) != Some(got.typ.as_str()) {
        return false;
    }
    match want.get("value") {
        Some(w) => {
            if !values_deep_equal(&got.value, &value_of(w)) {
                return false;
            }
        }
        None => return false,
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

fn run_file(name: &str) -> (usize, usize) {
    let doc = read(name);
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
        let result = evaluate_with_options(expr, &mut ctx, &resolve_vector, &options);
        let mut trace_ctx = ctx_of(v);
        let traced = explain(expr, &mut trace_ctx, &resolve_vector, Some(&options));

        if expect_error {
            if result.is_err() && traced.is_err() {
                pass += 1;
            } else {
                eprintln!("FAIL [{name}/{id}] expected an error from evaluate AND explain");
            }
            continue;
        }

        let got = match result {
            Ok(got) => got,
            Err(e) => {
                eprintln!("FAIL [{name}/{id}] raised: {}", e.0);
                continue;
            }
        };
        let trace = match traced {
            Ok(t) => t,
            Err(e) => {
                eprintln!("FAIL [{name}/{id}] explain raised: {}", e.0);
                continue;
            }
        };
        let expected = value_of(&v["expected"]);
        let ok = match v.get("tolerance").and_then(|t| t.as_f64()) {
            Some(tol) => match (&got, &expected) {
                (EvalValue::Num(g), EvalValue::Num(e)) => (g - e).abs() <= tol,
                _ => false,
            },
            None => values_deep_equal(&got, &expected),
        };
        if !ok {
            eprintln!("FAIL [{name}/{id}] expected {expected:?}, got {got:?}");
            continue;
        }
        if !values_deep_equal(&trace.value, &got) {
            eprintln!(
                "FAIL [{name}/{id}] explain value {:?} != evaluate value {:?}",
                trace.value, got
            );
            continue;
        }
        if let Some(want) = v.get("trace") {
            if !trace_matches(&trace, want) {
                eprintln!("FAIL [{name}/{id}] trace mismatch");
                continue;
            }
        }
        pass += 1;
    }

    println!("{name}: {pass}/{total} pass");
    (pass, total)
}

#[test]
fn expression_vectors() {
    // v1 vectors are the regression gate: every one passes unchanged under v2.
    let (p1, t1) = run_file("expression-vectors.json");
    let (p2, t2) = run_file("expression-vectors-2.json");
    let (p3, t3) = run_align_file("expression-alignment-vectors.json");
    assert_eq!(p1, t1, "{} v1 vector(s) failed", t1 - p1);
    assert_eq!(p2, t2, "{} v2 vector(s) failed", t2 - p2);
    assert_eq!(p3, t3, "{} alignment vector(s) failed", t3 - p3);
}

/// Structural equality of an aligned tree against the vector's expected tree.
fn aligned_matches(got: &AlignedNode, want: &J) -> bool {
    let typ = want.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if got.expr.get("type").and_then(|t| t.as_str()) != Some(typ) || got.trace.typ != typ {
        return false;
    }
    if got.operand.as_deref() != want.get("operand").and_then(|o| o.as_str()) {
        return false;
    }
    if got.index.map(|i| i as u64) != want.get("index").and_then(|i| i.as_u64()) {
        return false;
    }
    match (&got.element, want.get("element")) {
        (None, None) => {}
        (Some(el), Some(w)) => {
            if Some(el.loop_var.as_str()) != w.get("loopVar").and_then(|l| l.as_str()) {
                return false;
            }
            match w.get("value") {
                Some(v) if values_deep_equal(&el.value, &value_of(v)) => {}
                _ => return false,
            }
        }
        _ => return false,
    }
    match want.get("value") {
        Some(v) if values_deep_equal(&got.trace.value, &value_of(v)) => {}
        _ => return false,
    }
    let want_children: &[J] = want.get("children").and_then(|c| c.as_array()).map(|a| a.as_slice()).unwrap_or(&[]);
    if want_children.len() != got.children.len() {
        return false;
    }
    got.children.iter().zip(want_children.iter()).all(|(g, w)| aligned_matches(g, w))
}

/// The alignment vectors: explain `expr`, then `align` — the pairing must be
/// exactly the expected tree, or an error where one is expected.
fn run_align_file(name: &str) -> (usize, usize) {
    let doc = read(name);
    let vectors = doc["vectors"].as_array().unwrap();
    let mut pass = 0;
    for v in vectors {
        let id = v["id"].as_str().unwrap();
        let expr = &v["expr"];
        let closures = closures_of(v);
        let options = EvalOptions { closures: closures.as_ref(), resolve_ref: Some(&resolve_ref_vector) };
        let mut ctx = ctx_of(v);
        let trace = match explain(expr, &mut ctx, &resolve_vector, Some(&options)) {
            Ok(t) => t,
            Err(e) => {
                println!("{name}/{id}: explain failed: {}", e.0);
                continue;
            }
        };
        if v.get("expectError").and_then(|x| x.as_bool()).unwrap_or(false) {
            let target = v.get("alignExpr").unwrap_or(expr);
            if align(target, &trace).is_ok() {
                println!("{name}/{id}: expected align to reject the trace");
            } else {
                pass += 1;
            }
            continue;
        }
        match align(expr, &trace) {
            Ok(got) => {
                if aligned_matches(&got, &v["expected"]) {
                    pass += 1;
                } else {
                    println!("{name}/{id}: alignment mismatch");
                }
            }
            Err(e) => println!("{name}/{id}: align failed: {}", e.0),
        }
    }
    println!("{name}: {pass}/{} pass", vectors.len());
    (pass, vectors.len())
}
