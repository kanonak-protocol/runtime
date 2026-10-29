//! Drives the shared codec golden vectors through the Rust kanonak-codec port.

use kanonak_canonical::local_name;
use kanonak_codec::{canonical_form, content_hash, deserialize, serialize, CodecError, Node};
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
    run_types_file("codec-vectors-types.json");
}

/// The reference-values file (runtime#6): a `{"$ref"}` must address a named
/// resource; a URI fragment is rejected on all three surfaces, at any depth.
/// Same format and contract as the $types file.
#[test]
fn codec_vectors_references() {
    run_types_file("codec-vectors-references.json");
}

/// The JSON forms of integer and decimal values (runtime#54):
/// `serialize` and `deserialize` against the shared vectors. A
/// read case's field is rendered back to its canonical lexical form; a write
/// case's value is built as this port holds it.
#[test]
fn codec_vectors_json_numbers() {
    let file = "codec-vectors-json-numbers.json";
    let doc = read_doc(file);
    let schema = &doc["schema"];
    let class = doc["class"].as_str().unwrap();
    let read_cases = doc["readCases"].as_array().unwrap();
    let write_cases = doc["writeCases"].as_array().unwrap();
    assert!(!read_cases.is_empty() && !write_cases.is_empty(), "{}: no cases", file);
    let mut pass = 0;

    for case in read_cases {
        let field = case["field"].as_str().unwrap();
        let text = format!(
            "{{\"$type\": {}, {}: {}}}",
            J::from(class),
            J::from(field),
            case["json"].as_str().unwrap()
        );
        let json: Node = serde_json::from_str(&text).unwrap();
        let read = deserialize(&json, schema).map(|node| lexical_of(&J::Object(node)[field]));
        if json_number_case_holds("read", case, read, &case["expected"]) {
            pass += 1;
        }
    }
    for case in write_cases {
        let field = case["field"].as_str().unwrap();
        let mut node = Node::new();
        node.insert("$type".into(), J::from(class));
        node.insert(field.into(), held_value(schema, class, field, &case["value"]));
        let expected = match case["json"].as_str() {
            Some(text) => serde_json::from_str(text).unwrap(),
            None => J::Null,
        };
        let written = serialize(&node, schema).map(|wire| J::Object(wire)[field].clone());
        if json_number_case_holds("write", case, written, &expected) {
            pass += 1;
        }
    }

    let total = read_cases.len() + write_cases.len();
    println!("{}: {}/{} pass", file, pass, total);
    assert_eq!(pass, total, "{}: {} case(s) failed", file, total - pass);
}

/// Whether a json-numbers case came out as it says: refused with an error
/// ending `[json-number-form]` under `expectError`, else `expected`.
fn json_number_case_holds(
    what: &str,
    case: &J,
    outcome: Result<J, CodecError>,
    expected: &J,
) -> bool {
    let id = case["id"].as_str().unwrap();
    let expect_error = case["expectError"].as_bool() == Some(true);
    match outcome {
        Err(e) if expect_error && e.to_string().ends_with("[json-number-form]") => true,
        Err(e) => {
            eprintln!("FAIL [{} {}] threw {}", what, id, e);
            false
        }
        Ok(got) if expect_error => {
            eprintln!("FAIL [{} {}] expected a refusal, got {}", what, id, got);
            false
        }
        Ok(got) if &got == expected => true,
        Ok(got) => {
            eprintln!("FAIL [{} {}] expected {}, got {}", what, id, expected, got);
            false
        }
    }
}

/// A value read back as its canonical lexical form, item by item and field by
/// field.
fn lexical_of(value: &J) -> J {
    match value {
        J::Array(items) => J::Array(items.iter().map(lexical_of).collect()),
        J::Object(fields) => J::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), lexical_of(v)))
                .collect(),
        ),
        J::String(_) => value.clone(),
        other => J::String(other.to_string()),
    }
}

/// The datatypes this port holds as a number; it holds every other integer and
/// decimal as a string of its digits.
const HELD_AS_NUMBERS: [&str; 8] = [
    "long",
    "unsignedLong",
    "int",
    "short",
    "byte",
    "unsignedInt",
    "unsignedShort",
    "unsignedByte",
];

/// This port's value for a field of `class`, from its canonical lexical form:
/// a list item by item, an embedded value field by field under its range.
fn held_value(schema: &J, class: &str, field: &str, lexical: &J) -> J {
    let prop = &schema["classes"][class]["props"][field];
    match lexical {
        J::Array(items) => J::Array(
            items
                .iter()
                .map(|item| held_value(schema, class, field, item))
                .collect(),
        ),
        J::Object(fields) => {
            let range = prop["range"].as_str().unwrap();
            J::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), held_value(schema, range, k, v)))
                    .collect(),
            )
        }
        J::String(text) => {
            let datatype = local_name(prop["datatype"].as_str().unwrap()).unwrap();
            if HELD_AS_NUMBERS.contains(&datatype.as_str()) {
                J::Number(text.parse().unwrap())
            } else {
                lexical.clone()
            }
        }
        other => panic!("{}: a write case's value is a lexical form, got {}", field, other),
    }
}

fn run_types_file(file: &str) {
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

        if case["expectError"].as_bool() == Some(true) {
            if canonical_form(&nodes, &schema, &pkg).is_ok() {
                fails += 1;
                eprintln!("FAIL [{}] expected canonicalize to reject, it did not", id);
            }
            if nodes.iter().all(|n| serialize(n, &schema).is_ok()) {
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
            let wire = serialize(node, &schema).unwrap();
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
            if &J::Object(serialize(&back, &schema).unwrap()) != &expected_serialize[i] {
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

    assert_eq!(fails, 0, "{}: {} check(s) failed", file, fails);
    println!(
        "{}: {} case(s), all checks pass",
        file,
        doc["cases"].as_array().unwrap().len()
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

        if let Some(kind) = case.get("expectHashError").and_then(|k| k.as_str()) {
            // Serializing still carries what hashing refuses (runtime#37).
            let suffix = format!("[{}]", kind);
            for (what, refused) in [
                ("canonical form", canonical_form(&nodes, &schema, &pkg).err().map(|e| e.to_string())),
                ("hash", content_hash(&nodes, &schema, &pkg).err().map(|e| e.to_string())),
            ] {
                match refused {
                    Some(message) if message.ends_with(&suffix) => {}
                    Some(message) => {
                        fails += 1;
                        eprintln!("FAIL [{}] {} expected {}, got: {}", id, what, suffix, message);
                    }
                    None => {
                        fails += 1;
                        eprintln!("FAIL [{}] expected the {} to refuse with {}, it did not", id, what, suffix);
                    }
                }
            }
        } else {
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
        }

        let expected_serialize = case["expectedSerialize"].as_array().unwrap();
        for (i, node) in nodes.iter().enumerate() {
            let actual = J::Object(serialize(node, &schema).unwrap());
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
