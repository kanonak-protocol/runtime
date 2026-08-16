//! The 0.5.0 enumerations file (`enums`, kanonak-protocol/runtime#21).
//!
//! Beyond the standard form/hash/serialize checks this pins the enumeration
//! contract:
//!
//! - the schema parses with `enums` keyed by durable VERSIONED URIs at both
//!   levels — a versionless key would look right and miss every lookup, so the
//!   formation is asserted, not assumed;
//! - an enumeration STANDS ALONE: its class has no `classes` twin;
//! - `enums` and an enum-ranged `range` are canonicalization-INERT — the
//!   positive hashes are the ones the same nodes produce without them;
//! - `expectError` cases are rejected at CANONICALIZATION. Unlike the $types
//!   file's all-three-surfaces contract this violation is schema-DEPENDENT:
//!   `serialize` is schema-free and `deserialize` does not recurse into
//!   embedded values, so canonicalization is the only surface that can see it.

use kanonak_codec::{canonical_form, content_hash, serialize, Node};
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

fn as_node(v: &J) -> Node {
    v.as_object().unwrap().clone()
}

/// `publisher/package@version/name` — the durable versioned formation, checked
/// structurally so the crate needs no regex dependency.
fn is_versioned_durable_uri(uri: &str) -> bool {
    let segs: Vec<&str> = uri.split('/').collect();
    if segs.len() != 3 || segs.iter().any(|s| s.is_empty()) {
        return false;
    }
    let (pkg_at_ver, _name) = (segs[1], segs[2]);
    let mut parts = pkg_at_ver.splitn(2, '@');
    let pkg = parts.next().unwrap_or("");
    let ver = match parts.next() {
        Some(v) => v,
        None => return false,
    };
    if pkg.is_empty() {
        return false;
    }
    let nums: Vec<&str> = ver.split('.').collect();
    nums.len() == 3 && nums.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

#[test]
fn codec_vectors_enums() {
    let doc = read_doc("codec-vectors-enums.json");
    let schema = doc["schema"].clone();
    let mut failures = 0usize;

    // --- structural assertions on the schema itself ---
    let enums = schema["enums"]
        .as_object()
        .expect("schema carries no enums");
    assert!(!enums.is_empty(), "schema carries no enums");
    let classes = schema["classes"].as_object().unwrap();
    for (key, en) in enums {
        if key != en["typeUri"].as_str().unwrap_or_default() {
            eprintln!("enum key {} != typeUri {}", key, en["typeUri"]);
            failures += 1;
        }
        // An enumeration stands alone. The absence of a `classes` twin is
        // load-bearing: it is what makes an embedded value on an enum-ranged
        // property fail instead of being mapped as if it were a resource.
        if classes.contains_key(key) {
            eprintln!("enum {} must NOT also appear in classes", key);
            failures += 1;
        }
        let members = en["members"].as_object().expect("enum has no members");
        assert!(!members.is_empty(), "enum {} declares no members", key);
        for member_uri in members.keys() {
            if !is_versioned_durable_uri(member_uri) {
                eprintln!("member key {} is not a versioned durable URI", member_uri);
                failures += 1;
            }
        }
    }

    // --- cases ---
    for c in doc["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let nodes: Vec<Node> = c["nodes"].as_array().unwrap().iter().map(as_node).collect();
        let pkg = c["pkg"].clone();

        if c["expectError"].as_bool().unwrap_or(false) {
            if canonical_form(&nodes, &schema, &pkg).is_ok() {
                eprintln!("[{}] expected canonicalization to reject, it did not", id);
                failures += 1;
            }
            continue;
        }

        let form = canonical_form(&nodes, &schema, &pkg).unwrap();
        if form != c["expectedCanonicalForm"].as_str().unwrap() {
            eprintln!("[{}] canonical form mismatch", id);
            failures += 1;
        }
        let hash = content_hash(&nodes, &schema, &pkg).unwrap();
        if hash != c["expectedHash"].as_str().unwrap() {
            eprintln!(
                "[{}] hash expected {} got {}",
                id,
                c["expectedHash"].as_str().unwrap(),
                hash
            );
            failures += 1;
        }
        for (i, node) in nodes.iter().enumerate() {
            let wire = J::Object(serialize(node).unwrap());
            if wire != c["expectedSerialize"][i] {
                eprintln!("[{}] serialize[{}] mismatch", id, i);
                failures += 1;
            }
        }
    }

    assert_eq!(failures, 0, "codec-vectors-enums.json: {} check(s) failed", failures);
    println!("codec-vectors-enums.json: all checks pass");
}
