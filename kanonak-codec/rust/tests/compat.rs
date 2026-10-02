//! The 0.6.1 compatibility file (`codec-vectors-compat.json`, runtime#28):
//! deserialize / type_matches / enum_member / hashing over a node typed at an
//! earlier compatible version of the schema's package. Mirrors the TypeScript
//! reference runner (`runCompatFile`). A rejection must END in the bracketed
//! kind the vector names — the one part of an error message every port
//! reproduces.

use kanonak_codec::{
    content_hash, deserialize, enum_member, type_matches, CodecError, KanonakNode, Node,
};
use serde_json::{json, Value as J};
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

/// Whether `result` is a rejection ending in `[kind]`; reports otherwise.
fn rejected<T: std::fmt::Debug>(result: Result<T, CodecError>, kind: &str, id: &str) -> bool {
    match result {
        Ok(value) => {
            eprintln!(
                "[{}] expected a [{}] rejection, got a value: {:?}",
                id, kind, value
            );
            false
        }
        Err(e) => {
            let message = e.to_string();
            let ok = message.ends_with(&format!("[{}]", kind));
            if !ok {
                eprintln!("[{}] expected [{}], got: {}", id, kind, message);
            }
            ok
        }
    }
}

#[test]
fn codec_vectors_compat() {
    let doc = read_doc("codec-vectors-compat.json");
    let schemas = &doc["schemas"];
    let pkg = &doc["pkg"];
    let mut total = 0usize;
    let mut failures = 0usize;

    // --- deserialize: the exact class, else the same class at a version that
    //     can read the written one; the node keeps its WRITTEN $type ---
    let deserialize_cases = doc["deserializeCases"].as_array().unwrap();
    assert!(!deserialize_cases.is_empty(), "no deserializeCases");
    for c in deserialize_cases {
        total += 1;
        let id = c["id"].as_str().unwrap();
        let schema = &schemas[c["schema"].as_str().unwrap()];
        let input = as_node(&c["input"]);
        let ok = match c["expectError"].as_str() {
            Some(kind) => rejected(deserialize(&input, schema), kind, id),
            None => match deserialize(&input, schema) {
                Ok(got) => {
                    let got = J::Object(got);
                    let same = got == c["expected"];
                    if !same {
                        eprintln!("[{}] expected {} got {}", id, c["expected"], got);
                    }
                    same
                }
                Err(e) => {
                    eprintln!("[{}] expected a node, got error: {}", id, e);
                    false
                }
            },
        };
        if !ok {
            failures += 1;
        }
    }

    // --- type_matches: what a generated type guard asks, subclass-aware ---
    let type_matches_cases = doc["typeMatchesCases"].as_array().unwrap();
    assert!(!type_matches_cases.is_empty(), "no typeMatchesCases");
    for c in type_matches_cases {
        total += 1;
        let id = c["id"].as_str().unwrap();
        let schema = &schemas[c["schema"].as_str().unwrap()];
        let expected = c["expected"].as_bool().unwrap();
        match type_matches(
            &as_node(&c["node"]),
            c["classUri"].as_str().unwrap(),
            schema,
        ) {
            Ok(got) if got == expected => {}
            Ok(got) => {
                failures += 1;
                eprintln!("[{}] type_matches expected {} got {}", id, expected, got);
            }
            Err(e) => {
                failures += 1;
                eprintln!("[{}] type_matches errored: {}", id, e);
            }
        }
        let typed: KanonakNode = serde_json::from_value(c["node"].clone()).unwrap();
        match typed.type_matches(c["classUri"].as_str().unwrap(), schema) {
            Ok(got) if got == expected => {}
            other => {
                failures += 1;
                eprintln!(
                    "[{}] KanonakNode::type_matches expected {} got {:?}",
                    id, expected, other
                );
            }
        }
    }

    // --- enum_member: exact key, else the same member at a readable version ---
    let enum_member_cases = doc["enumMemberCases"].as_array().unwrap();
    assert!(!enum_member_cases.is_empty(), "no enumMemberCases");
    for c in enum_member_cases {
        total += 1;
        let id = c["id"].as_str().unwrap();
        let schema = &schemas[c["schema"].as_str().unwrap()];
        let got = match enum_member(schema, c["ref"].as_str().unwrap()) {
            Some(m) => json!({
                "enumType": m.enum_type,
                "uri": m.uri,
                "label": m.member.get("label").cloned().unwrap_or(J::Null),
            }),
            None => J::Null,
        };
        if got != c["expected"] {
            failures += 1;
            eprintln!(
                "[{}] enum_member expected {} got {}",
                id, c["expected"], got
            );
        }
    }

    // --- hashing: exact-version only ---
    let hash_cases = doc["hashCases"].as_array().unwrap();
    assert!(!hash_cases.is_empty(), "no hashCases");
    for c in hash_cases {
        total += 1;
        let id = c["id"].as_str().unwrap();
        let schema = &schemas[c["schema"].as_str().unwrap()];
        let nodes: Vec<Node> = c["nodes"].as_array().unwrap().iter().map(as_node).collect();
        let ok = match c["expectError"].as_str() {
            Some(kind) => rejected(content_hash(&nodes, schema, pkg), kind, id),
            None => match content_hash(&nodes, schema, pkg) {
                Ok(got) => {
                    let expected = c["expectedHash"].as_str().unwrap();
                    if got != expected {
                        eprintln!("[{}] hash expected {} got {}", id, expected, got);
                    }
                    got == expected
                }
                Err(e) => {
                    eprintln!("[{}] expected a hash, got error: {}", id, e);
                    false
                }
            },
        };
        if !ok {
            failures += 1;
        }
    }

    assert_eq!(
        failures, 0,
        "codec-vectors-compat.json: {} of {} case(s) failed",
        failures, total
    );
    println!("codec-vectors-compat.json: {}/{} pass", total, total);
}

/// The `ancestors` field is optional and canonicalization-inert: a class
/// without it still parses, and without it a type match is by the class
/// itself only (a subclass node does not match its superclass check).
#[test]
fn ancestors_absent_is_stricter_never_looser() {
    let doc = read_doc("codec-vectors-compat.json");
    let mut schema = doc["schemas"]["five-one"].clone();
    let terminal = "example.com/vocab@5.1.0/TerminalApplication";
    let app = "example.com/vocab@5.1.0/Application";
    let node = as_node(&json!({ "$type": "example.com/vocab@5.0.0/TerminalApplication" }));
    assert!(type_matches(&node, app, &schema).unwrap());

    schema["classes"][terminal]
        .as_object_mut()
        .unwrap()
        .remove("ancestors");
    assert!(!type_matches(&node, app, &schema).unwrap());
    assert!(type_matches(&node, terminal, &schema).unwrap());
    assert!(deserialize(&node, &schema).is_ok());
}

/// A class URI that is not a coordinate is a caller error, not a "no".
#[test]
fn type_matches_rejects_a_non_coordinate_class() {
    let doc = read_doc("codec-vectors-compat.json");
    let schema = &doc["schemas"]["five-one"];
    let node = as_node(&json!({ "$type": "example.com/vocab@5.1.0/Application" }));
    assert!(type_matches(&node, "not a coordinate", schema).is_err());
}
