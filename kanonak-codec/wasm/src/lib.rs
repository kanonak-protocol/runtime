//! kanonak-codec — WebAssembly component (the 7th port).
//!
//! A thin WIT wrapper over the Rust reference implementation: JSON strings in,
//! JSON strings out, statically bundling `kanonak-canonical`. Nothing here is a
//! reimplementation — every call delegates to `kanonak_codec`, so the component
//! is conformant by construction and gated by the same golden vectors as the
//! six native ports.
//!
//! `serialize` accepts a `schema` argument for surface symmetry with
//! `deserialize` (the WIT contract agreed with the Platform); it is validated
//! as JSON but the underlying serialize does not consult it.

use kanonak_codec::Node;
use serde_json::Value as Json;

wit_bindgen::generate!({ path: "wit" });

use exports::kanonak::codec::codec::Guest;

struct Component;

fn parse_json(label: &str, text: &str) -> Result<Json, String> {
    serde_json::from_str(text).map_err(|e| format!("{}: invalid JSON: {}", label, e))
}

fn parse_object(label: &str, text: &str) -> Result<Node, String> {
    match parse_json(label, text)? {
        Json::Object(map) => Ok(map),
        other => Err(format!("{}: expected a JSON object, got {}", label, other)),
    }
}

fn parse_nodes(text: &str) -> Result<Vec<Node>, String> {
    let items = match parse_json("nodes", text)? {
        Json::Array(items) => items,
        other => {
            return Err(format!(
                "nodes: expected a JSON array of node objects, got {}",
                other
            ))
        }
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| match item {
            Json::Object(map) => Ok(map),
            other => Err(format!("nodes[{}]: expected a node object, got {}", i, other)),
        })
        .collect()
}

fn to_json_string(node: &Node) -> Result<String, String> {
    serde_json::to_string(node).map_err(|e| format!("failed to encode result: {}", e))
}

impl Guest for Component {
    fn content_hash(nodes: String, schema: String, pkg: String) -> Result<String, String> {
        let nodes = parse_nodes(&nodes)?;
        let schema = parse_json("schema", &schema)?;
        let pkg = parse_json("pkg", &pkg)?;
        kanonak_codec::content_hash(&nodes, &schema, &pkg).map_err(|e| e.to_string())
    }

    fn canonical_form(nodes: String, schema: String, pkg: String) -> Result<String, String> {
        let nodes = parse_nodes(&nodes)?;
        let schema = parse_json("schema", &schema)?;
        let pkg = parse_json("pkg", &pkg)?;
        kanonak_codec::canonical_form(&nodes, &schema, &pkg).map_err(|e| e.to_string())
    }

    fn serialize(node: String, schema: String) -> Result<String, String> {
        let node = parse_object("node", &node)?;
        parse_json("schema", &schema)?;
        let wire = kanonak_codec::serialize(&node).map_err(|e| e.to_string())?;
        to_json_string(&wire)
    }

    fn deserialize(node: String, schema: String) -> Result<String, String> {
        let node = parse_object("node", &node)?;
        let schema = parse_json("schema", &schema)?;
        let typed = kanonak_codec::deserialize(&node, &schema).map_err(|e| e.to_string())?;
        to_json_string(&typed)
    }
}

export!(Component);
