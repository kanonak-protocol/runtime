//! The typed SDK-facing surface (0.3.0): the `$`-envelope as data, the explicit
//! reference-or-embedded union, and the serde bridge from a typed model to the
//! node contract. A generated struct composes [`KanonakNode`] via
//! `#[serde(flatten)]`, types its object properties as [`Ref<T>`], and binds
//! through [`to_node`] — native serde, no reflection, one contract with the
//! dictionary path.

use crate::{deserialize, CodecError, Node};
use serde::de::DeserializeOwned;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

/// The `$`-envelope as data — composed into a generated struct via
/// `#[serde(flatten)]`, so an instance carries its own identity and serializes
/// straight to the normalized-JSON wire form. `extra` holds open-world
/// assertions outside the type-model, keyed by predicate URI; being the
/// innermost flatten map, it also collects unknown wire fields on deserialize.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KanonakNode {
    /// The resource's canonical URI. Required to form a subject.
    #[serde(rename = "$id", skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,

    /// The durable class URI — the value of the synthesized type triple.
    #[serde(rename = "$type", skip_serializing_if = "Option::is_none", default)]
    pub type_uri: Option<String>,

    /// A multi-typed node's FULL type set (0.4.0, runtime#10) — present only
    /// when the node carries more than one type statement. Sorted by UTF-8
    /// bytes, at least two members, no duplicates, `$type` a member; each
    /// member emits one type statement in canonical form. Exposed ONLY as the
    /// `$types` envelope — deliberately no unprefixed `types` accessor, because
    /// an ontology can model a property literally named `types`; the `$` prefix
    /// exists to avoid exactly that collision.
    #[serde(rename = "$types", skip_serializing_if = "Option::is_none", default)]
    pub types: Option<Vec<String>>,

    /// An embedded value's authored dict-key — HASH-RELEVANT (serialized into
    /// the canonical form). Only meaningful when the instance is used as an
    /// embedded value; `None` for subjects.
    #[serde(rename = "$name", skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,

    /// Package provenance on read; ignored if echoed back on write.
    #[serde(
        rename = "$contentHash",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub content_hash: Option<String>,

    /// Package provenance on read; ignored if echoed back on write.
    #[serde(rename = "$version", skip_serializing_if = "Option::is_none", default)]
    pub version: Option<String>,

    /// Open-world assertions outside the type-model, keyed by predicate URI.
    /// Rides the wire as sibling fields (flatten semantics).
    #[serde(flatten)]
    pub extra: Map<String, Json>,
}

/// Implemented by generated typed structs (over their flattened
/// [`KanonakNode`]) so the runtime can read/write an instance's envelope —
/// what lets [`Ref::to_resource`] resolve identity and
/// [`Ref::embed_named`] carry the authored dict-key.
pub trait KanonakResource {
    fn kanonak_node(&self) -> &KanonakNode;
    fn kanonak_node_mut(&mut self) -> &mut KanonakNode;
}

/// An object property's value: EXACTLY ONE of a reference to a named resource
/// (its canonical URI) or an embedded node (the value itself, carried inline —
/// derived identity, no `$id`). The typed twin of the wire form's
/// `{"$ref": uri}` vs embedded-node distinction; the choice between the arms
/// is authorial and hash-relevant, so it is explicit here, never inferred.
#[derive(Debug, Clone, PartialEq)]
pub enum Ref<T> {
    /// A reference to a named resource by its canonical URI.
    Reference(String),
    /// An embedded value, carried inline.
    Embedded(T),
}

impl<T> Ref<T> {
    /// A reference to a named resource by its canonical URI.
    pub fn to(uri: impl Into<String>) -> Self {
        Ref::Reference(uri.into())
    }

    /// A reference to a named resource by the instance itself — resolved
    /// through the target's envelope `id`. The target must already carry its
    /// identity; an embedded (id-less) value cannot be referenced.
    pub fn to_resource(target: &impl KanonakResource) -> Result<Self, CodecError> {
        match target.kanonak_node().id.as_deref() {
            Some(id) if !id.is_empty() => Ok(Ref::Reference(id.to_string())),
            _ => Err(CodecError::Malformed(
                "Ref::to_resource requires a resource with a non-empty envelope id — \
                 to carry the value inline instead, use Ref::embed."
                    .into(),
            )),
        }
    }

    /// An embedded value, carried inline (derived identity, no `$id`).
    pub fn embed(value: T) -> Self {
        Ref::Embedded(value)
    }

    /// An embedded value with its authored dict-key name (hash-relevant).
    pub fn embed_named(mut value: T, name: impl Into<String>) -> Self
    where
        T: KanonakResource,
    {
        value.kanonak_node_mut().name = Some(name.into());
        Ref::Embedded(value)
    }

    /// True when this is the reference arm.
    pub fn is_reference(&self) -> bool {
        matches!(self, Ref::Reference(_))
    }
}

impl<T: Serialize> Serialize for Ref<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Ref::Reference(uri) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("$ref", uri)?;
                map.end()
            }
            Ref::Embedded(value) => value.serialize(serializer),
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Ref<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Json::deserialize(deserializer)?;
        if let Some(uri) = value.get("$ref").and_then(|r| r.as_str()) {
            return Ok(Ref::Reference(uri.to_string()));
        }
        serde_json::from_value(value)
            .map(Ref::Embedded)
            .map_err(serde::de::Error::custom)
    }
}

/// A typed instance's codec node (the dictionary contract). The bridge is
/// native serde: the instance serializes to its normalized-JSON wire form
/// (envelope-as-data + [`Ref<T>`] values), and the wire form maps onto the
/// node contract through the SAME split [`deserialize`] defines — one
/// contract, not two.
pub fn to_node<T: Serialize>(typed: &T, schema: &Json) -> Result<Node, CodecError> {
    let wire = serde_json::to_value(typed)
        .map_err(|e| CodecError::Malformed(format!("typed value failed to serialize: {}", e)))?;
    match wire.as_object() {
        Some(map) => deserialize(map, schema),
        None => Err(CodecError::Malformed(
            "a typed instance must serialize to a JSON object (the wire node form)".into(),
        )),
    }
}
