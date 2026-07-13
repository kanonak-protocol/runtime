//! Drives the shared codec golden vectors through the Rust kanonak-codec port.

use kanonak_codec::{canonical_form, content_hash, deserialize, serialize, Node};
use serde_json::Value as J;
use std::fs;
use std::path::PathBuf;

fn vectors_path(file: &str) -> PathBuf {
    // tests run from the crate root (rust/); vectors are at ../vectors.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("vectors");
    p.push(file);
    p
}

fn read_doc(file: &str) -> J {
    serde_json::from_str(&fs::read_to_string(vectors_path(file)).unwrap()).unwrap()
}

fn as_node(v: &J) -> Node {
    v.as_object().unwrap().clone()
}

#[test]
fn codec_vectors() {
    run_file("codec-vectors.json");
}

#[test]
fn codec_vectors_embedded() {
    run_file("codec-vectors-embedded.json");
}

/// The 0.4.0 multi-typed-subjects file (runtime#10). Beyond the standard
/// form/hash/serialize checks it exercises the $types contract: expectError
/// cases must be rejected on ALL THREE surfaces — serialize (the producer
/// fails at emit time), deserialize (the reader rejects, never repairs), and
/// canonicalization — and positive cases must round-trip:
/// deserialize(serialize(x)) preserves $types exactly and re-canonicalizes to
/// the same hash.
#[test]
fn codec_vectors_types() {
    let doc = read_doc("codec-vectors-types.json");
    let schema = doc["schema"].clone();
    let mut fails = 0;

    for case in doc["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let pkg = case["pkg"].clone();
        let nodes: Vec<Node> = case["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(as_node)
            .collect();

        if case["expectError"].as_bool() == Some(true) {
            if canonical_form(&nodes, &schema, &pkg).is_ok() {
                fails += 1;
                eprintln!("FAIL [{}] expected canonicalize to reject, it did not", id);
            }
            if nodes.iter().all(|n| serialize(n).is_ok()) {
                fails += 1;
                eprintln!("FAIL [{}] expected serialize to reject, it did not", id);
            }
            if nodes.iter().all(|n| deserialize(n, &schema).is_ok()) {
                fails += 1;
                eprintln!("FAIL [{}] expected deserialize to reject, it did not", id);
            }
            continue;
        }

        let form = canonical_form(&nodes, &schema, &pkg).unwrap();
        let exp_form = case["expectedCanonicalForm"].as_str().unwrap();
        if form != exp_form {
            fails += 1;
            eprintln!(
                "FAIL [{}] form\n  expected: {}\n  actual:   {}",
                id, exp_form, form
            );
        }

        let hash = content_hash(&nodes, &schema, &pkg).unwrap();
        let exp_hash = case["expectedHash"].as_str().unwrap();
        if hash != exp_hash {
            fails += 1;
            eprintln!("FAIL [{}] hash expected {} got {}", id, exp_hash, hash);
        }

        let expected_serialize = case["expectedSerialize"].as_array().unwrap();
        let mut round_tripped: Vec<Node> = Vec::with_capacity(nodes.len());
        for (i, node) in nodes.iter().enumerate() {
            let wire = serialize(node).unwrap();
            if &J::Object(wire.clone()) != &expected_serialize[i] {
                fails += 1;
                eprintln!(
                    "FAIL [{}] serialize[{}]\n  expected: {}\n  actual:   {}",
                    id,
                    i,
                    expected_serialize[i],
                    J::Object(wire.clone())
                );
            }
            let back = deserialize(&wire, &schema).unwrap();
            if &J::Object(serialize(&back).unwrap()) != &expected_serialize[i] {
                fails += 1;
                eprintln!("FAIL [{}] round-trip serialize[{}] mismatch", id, i);
            }
            round_tripped.push(back);
        }
        let rt_hash = content_hash(&round_tripped, &schema, &pkg).unwrap();
        if rt_hash != exp_hash {
            fails += 1;
            eprintln!(
                "FAIL [{}] round-trip hash expected {} got {}",
                id, exp_hash, rt_hash
            );
        }
    }

    assert_eq!(
        fails, 0,
        "codec-vectors-types.json: {} check(s) failed",
        fails
    );
}

fn run_file(file: &str) {
    let doc = read_doc(file);
    let schema = doc["schema"].clone();
    let mut fails = 0;

    for case in doc["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let pkg = case["pkg"].clone();
        let nodes: Vec<Node> = case["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(as_node)
            .collect();

        let form = canonical_form(&nodes, &schema, &pkg).unwrap();
        let exp_form = case["expectedCanonicalForm"].as_str().unwrap();
        if form != exp_form {
            fails += 1;
            eprintln!(
                "FAIL [{}] form\n  expected: {}\n  actual:   {}",
                id, exp_form, form
            );
        }

        let hash = content_hash(&nodes, &schema, &pkg).unwrap();
        let exp_hash = case["expectedHash"].as_str().unwrap();
        if hash != exp_hash {
            fails += 1;
            eprintln!("FAIL [{}] hash expected {} got {}", id, exp_hash, hash);
        }

        let expected_serialize = case["expectedSerialize"].as_array().unwrap();
        for (i, node) in nodes.iter().enumerate() {
            let actual = J::Object(serialize(node).unwrap());
            let expected = &expected_serialize[i];
            if &actual != expected {
                fails += 1;
                eprintln!(
                    "FAIL [{}] serialize[{}]\n  expected: {}\n  actual:   {}",
                    id, i, expected, actual
                );
            }
        }
    }

    assert_eq!(fails, 0, "{}: {} codec vector check(s) failed", file, fails);
}
