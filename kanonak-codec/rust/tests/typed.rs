//! Typed-surface conformance: hand-written GENERATED-STYLE structs for the
//! embedded-vectors probe schema, driven through KanonakNode / Ref<T> /
//! to_node, asserted against the SAME golden vectors the node contract is
//! gated by. Also the executable spec for what the Rust SDK generator must
//! emit: structs flatten KanonakNode, object properties are
//! Option<Vec<Ref<T>>> (or Option<Ref<T>> for single), wire names ride
//! #[serde(rename)], every Option carries skip_serializing_if.

use kanonak_codec::{canonical_form, content_hash, to_node, KanonakNode, KanonakResource, Ref};
use serde::Serialize;
use serde_json::Value as J;
use std::fs;
use std::path::PathBuf;

// -- Generated-style model for probe.example.com/schema@1.0.0 ----------------

macro_rules! resource {
    ($t:ty) => {
        impl KanonakResource for $t {
            fn kanonak_node(&self) -> &KanonakNode {
                &self.node
            }
            fn kanonak_node_mut(&mut self) -> &mut KanonakNode {
                &mut self.node
            }
        }
    };
}

#[derive(Serialize, Default)]
struct Order {
    #[serde(flatten)]
    node: KanonakNode,
    #[serde(rename = "note", skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(rename = "items", skip_serializing_if = "Option::is_none")]
    items: Option<Vec<Ref<LineItem>>>,
    #[serde(rename = "customer", skip_serializing_if = "Option::is_none")]
    customer: Option<Vec<Ref<Customer>>>,
}
resource!(Order);

/// Same $type, single-valued customer — the wire/hash contract is carried by
/// $type + rename, not the struct name; exercises the bare embedded form.
#[derive(Serialize, Default)]
struct OrderSingleCustomer {
    #[serde(flatten)]
    node: KanonakNode,
    #[serde(rename = "note", skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(rename = "customer", skip_serializing_if = "Option::is_none")]
    customer: Option<Ref<Customer>>,
}
resource!(OrderSingleCustomer);

#[derive(Serialize, Default)]
struct LineItem {
    #[serde(flatten)]
    node: KanonakNode,
    #[serde(rename = "sku", skip_serializing_if = "Option::is_none")]
    sku: Option<String>,
    #[serde(rename = "qty", skip_serializing_if = "Option::is_none")]
    qty: Option<i64>,
}
resource!(LineItem);

#[derive(Serialize, Default)]
struct Customer {
    #[serde(flatten)]
    node: KanonakNode,
    #[serde(rename = "name", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(rename = "address", skip_serializing_if = "Option::is_none")]
    address: Option<Vec<Ref<Address>>>,
}
resource!(Customer);

#[derive(Serialize, Default)]
struct Address {
    #[serde(flatten)]
    node: KanonakNode,
    #[serde(rename = "city", skip_serializing_if = "Option::is_none")]
    city: Option<String>,
}
resource!(Address);

#[derive(Serialize, Default)]
struct Person {
    #[serde(flatten)]
    node: KanonakNode,
    #[serde(rename = "name", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}
resource!(Person);

#[derive(Serialize, Default)]
struct Account {
    #[serde(flatten)]
    node: KanonakNode,
    #[serde(rename = "accountCode", skip_serializing_if = "Option::is_none")]
    account_code: Option<String>,
    #[serde(rename = "seats", skip_serializing_if = "Option::is_none")]
    seats: Option<i64>,
    #[serde(rename = "rate", skip_serializing_if = "Option::is_none")]
    rate: Option<f64>,
    #[serde(rename = "active", skip_serializing_if = "Option::is_none")]
    active: Option<bool>,
    #[serde(rename = "owner", skip_serializing_if = "Option::is_none")]
    owner: Option<Ref<Person>>,
    #[serde(rename = "tags", skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
}
resource!(Account);

// -- Harness ------------------------------------------------------------------

const SCHEMA_NS: &str = "probe.example.com/schema@1.0.0";
const DATA: &str = "probe.example.com/data@1.0.0";

fn envelope(id: &str, type_local: &str) -> KanonakNode {
    KanonakNode {
        id: Some(format!("{}/{}", DATA, id)),
        type_uri: Some(format!("{}/{}", SCHEMA_NS, type_local)),
        ..Default::default()
    }
}

fn read_doc(file: &str) -> J {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("vectors");
    p.push(file);
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

fn check(doc: &J, case_id: &str, nodes: &[kanonak_codec::Node]) {
    let case = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"].as_str() == Some(case_id))
        .unwrap_or_else(|| panic!("vector case {} not found", case_id));
    let schema = doc["schema"].clone();
    let pkg = case["pkg"].clone();

    let form = canonical_form(nodes, &schema, &pkg).unwrap();
    assert_eq!(
        form,
        case["expectedCanonicalForm"].as_str().unwrap(),
        "canonical form mismatch for {}",
        case_id
    );
    let hash = content_hash(nodes, &schema, &pkg).unwrap();
    assert_eq!(
        hash,
        case["expectedHash"].as_str().unwrap(),
        "hash mismatch for {}",
        case_id
    );
}

fn node<T: Serialize>(typed: &T, doc: &J) -> kanonak_codec::Node {
    to_node(typed, &doc["schema"]).unwrap()
}

#[test]
fn typed_embedded_vectors() {
    let doc = read_doc("codec-vectors-embedded.json");

    check(
        &doc,
        "embedded-named-in-list",
        &[node(
            &Order {
                node: envelope("o1", "Order"),
                note: Some("A".into()),
                items: Some(vec![Ref::embed_named(
                    LineItem {
                        sku: Some("X".into()),
                        qty: Some(1),
                        ..Default::default()
                    },
                    "first",
                )]),
                ..Default::default()
            },
            &doc,
        )],
    );

    check(
        &doc,
        "embedded-unnamed-positional",
        &[node(
            &Order {
                node: envelope("o1", "Order"),
                note: Some("A".into()),
                items: Some(vec![Ref::embed(LineItem {
                    sku: Some("X".into()),
                    qty: Some(1),
                    ..Default::default()
                })]),
                ..Default::default()
            },
            &doc,
        )],
    );

    check(
        &doc,
        "embedded-explicit-type",
        &[node(
            &Order {
                node: envelope("o1", "Order"),
                note: Some("A".into()),
                items: Some(vec![Ref::embed_named(
                    LineItem {
                        node: KanonakNode {
                            type_uri: Some(format!("{}/LineItem", SCHEMA_NS)),
                            ..Default::default()
                        },
                        sku: Some("X".into()),
                        qty: Some(1),
                    },
                    "first",
                )]),
                ..Default::default()
            },
            &doc,
        )],
    );

    check(
        &doc,
        "embedded-list-order",
        &[node(
            &Order {
                node: envelope("o1", "Order"),
                items: Some(vec![
                    Ref::embed_named(
                        LineItem {
                            sku: Some("X".into()),
                            qty: Some(1),
                            ..Default::default()
                        },
                        "a",
                    ),
                    Ref::embed_named(
                        LineItem {
                            sku: Some("Y".into()),
                            qty: Some(2),
                            ..Default::default()
                        },
                        "b",
                    ),
                ]),
                ..Default::default()
            },
            &doc,
        )],
    );

    check(
        &doc,
        "embedded-nested",
        &[node(
            &Order {
                node: envelope("o1", "Order"),
                customer: Some(vec![Ref::embed_named(
                    Customer {
                        name: Some("Ada".into()),
                        address: Some(vec![Ref::embed_named(
                            Address {
                                city: Some("Austin".into()),
                                ..Default::default()
                            },
                            "home",
                        )]),
                        ..Default::default()
                    },
                    "cust",
                )]),
                ..Default::default()
            },
            &doc,
        )],
    );

    check(
        &doc,
        "single-embedded-bare",
        &[node(
            &OrderSingleCustomer {
                node: envelope("o1", "Order"),
                customer: Some(Ref::embed_named(
                    Customer {
                        name: Some("Ada".into()),
                        ..Default::default()
                    },
                    "cust",
                )),
                ..Default::default()
            },
            &doc,
        )],
    );

    check(
        &doc,
        "empty-list-emits-nothing",
        &[node(
            &Order {
                node: envelope("o1", "Order"),
                note: Some("A".into()),
                items: Some(vec![]),
                ..Default::default()
            },
            &doc,
        )],
    );
}

#[test]
fn typed_basic_vectors() {
    let doc = read_doc("codec-vectors.json");

    let alice = Person {
        node: envelope("p1", "Person"),
        name: Some("Alice".into()),
    };
    // Ref-by-instance: resolved through the target's envelope id.
    let owner = Ref::to_resource(&alice).unwrap();

    check(
        &doc,
        "basic-scalars-ref-list",
        &[
            node(&alice, &doc),
            node(
                &Account {
                    node: envelope("a1", "Account"),
                    account_code: Some("paul".into()),
                    seats: Some(5),
                    rate: Some(1.5),
                    active: Some(true),
                    owner: Some(owner),
                    tags: Some(vec!["x".into(), "y".into()]),
                },
                &doc,
            ),
        ],
    );
}
