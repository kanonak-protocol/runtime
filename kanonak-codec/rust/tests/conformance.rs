//! Drives the shared codec golden vectors through the Rust kanonak-codec port.

use kanonak_codec::{canonical_form, content_hash, serialize, Node};
use serde_json::Value as J;
use std::fs;
use std::path::PathBuf;

fn vectors_path() -> PathBuf {
    // tests run from the crate root (rust/); vectors are at ../vectors.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("vectors");
    p.push("codec-vectors.json");
    p
}

fn read_doc() -> J {
    serde_json::from_str(&fs::read_to_string(vectors_path()).unwrap()).unwrap()
}

fn as_node(v: &J) -> Node {
    v.as_object().unwrap().clone()
}

#[test]
fn codec_vectors() {
    let doc = read_doc();
    let schema = doc["schema"].clone();
    let mut fails = 0;

    for case in doc["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let pkg = case["pkg"].clone();
        let nodes: Vec<Node> = case["nodes"].as_array().unwrap().iter().map(as_node).collect();

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
            let actual = J::Object(serialize(node));
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

    assert_eq!(fails, 0, "{} codec vector check(s) failed", fails);
}
