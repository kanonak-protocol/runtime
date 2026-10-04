//! kanonak-codec — the generic, ontology-independent codec runtime (Rust port).
//!
//! Given a `CodecSchema` (the per-package metadata a generated SDK embeds) and a
//! set of typed nodes, it builds the canonical input model and content-addresses
//! it via `kanonak-canonical` (the same content-form the Python/TypeScript
//! references and the `kanonak hash` CLI produce). It also (de)serializes the
//! normalized-JSON wire form. Self-contained: carriers come from the schema's
//! datatype URIs, and the resolved foundation URIs are embedded by the generator,
//! so hashing needs no runtime ontology resolution.
//!
//! A node is a plain JSON object (`serde_json::Map<String, serde_json::Value>`) —
//! the `$`-envelope plus alias-collapsed local-name fields. A generated typed
//! model serializes to one. Note: `serde_json::Value` (the node field model) is
//! distinct from `kanonak_canonical::Value` (the canonical-input value enum).

use kanonak_canonical::{
    canonical_form as canonical_form_pkg, canonical_hash as canonical_hash_pkg, carrier_of,
    is_readable_by, parse_coordinate, versionless_key, CanonError, CoordinateVersion, Package,
    Statement, Subject, Value,
};
use serde_json::{Map, Value as Json};

mod typed;
pub use typed::{to_node, KanonakNode, KanonakResource, Ref};

/// The reserved `$`-envelope keys, which never become statements/predicates.
/// `$name` (0.2.0) carries an embedded value's authored dict-key — hash-relevant.
/// `$types` (0.4.0, runtime#10) carries a multi-typed node's FULL type set.
const ENVELOPE_KEYS: [&str; 7] = [
    "$type",
    "$types",
    "$id",
    "$name",
    "$contentHash",
    "$version",
    "$extra",
];

/// A node is a JSON object.
pub type Node = Map<String, Json>;

/// Errors raised by the codec runtime. Fails loudly — no fallbacks.
#[derive(Debug)]
pub enum CodecError {
    /// A node, schema, or package context was malformed.
    Malformed(String),
    /// The underlying canonical library rejected a lexical/value.
    Canon(CanonError),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::Malformed(m) => write!(f, "{}", m),
            CodecError::Canon(e) => write!(f, "{}", e.0),
        }
    }
}

impl std::error::Error for CodecError {}

impl From<CanonError> for CodecError {
    fn from(e: CanonError) -> Self {
        CodecError::Canon(e)
    }
}

fn err<T>(msg: impl Into<String>) -> Result<T, CodecError> {
    Err(CodecError::Malformed(msg.into()))
}

/// The raw lexical token of a scalar — the input the canonical form normalizes.
/// bool -> "true"/"false"; string -> the string; number -> its plain decimal
/// string (serde_json's `Number::to_string()` gives "5" / "1.5", never
/// scientific notation). The canonical crate re-normalizes from there.
fn lexical(value: &Json) -> String {
    match value {
        Json::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Json::String(s) => s.clone(),
        Json::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// The schema's `classes` map. A class entry may carry an optional
/// `ancestors` list (0.6.1, runtime#28): every superclass, transitively, by
/// durable VERSIONED URI — what lets [`type_matches`] accept a subclass node
/// for a superclass check. Additive and canonicalization-INERT: a schema
/// without it is still valid, and absent, a type match is by the class itself
/// only — stricter, never looser.
fn classes_of(schema: &Json) -> Result<&Map<String, Json>, CodecError> {
    schema
        .get("classes")
        .and_then(|c| c.as_object())
        .ok_or_else(|| CodecError::Malformed("schema is missing 'classes'".into()))
}

/// A coordinate's versionless key and version, or `None` when the string is
/// not a STRICTLY well-formed coordinate or carries no version (never errors).
fn versioned_coordinate(uri: &str) -> Option<(String, CoordinateVersion)> {
    let c = parse_coordinate(uri).ok()?;
    let version = c.version?;
    Some((format!("{}/{}/{}", c.publisher, c.package, c.name), version))
}

/// The versionless identity of a coordinate (versioned or not), or `None`
/// when the string is not one.
fn identity_of(uri: &str) -> Option<String> {
    versionless_key(uri).ok()
}

fn version_order(v: CoordinateVersion) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn format_version(v: CoordinateVersion) -> String {
    format!("{}.{}.{}", v.major, v.minor, v.patch)
}

/// THE class lookup (runtime#28). A node is typed with the version of the
/// class its producer's import resolved to; a codec generated from a later
/// COMPATIBLE version of the same package must still read it. So: the exact
/// versioned key first, then the class with the same publisher, package and
/// name whose version can read the written one — `is_readable_by` from
/// `kanonak-canonical`, the protocol's single compatibility rule, pinned in
/// every port. Returns the class's schema key and entry. When nothing
/// matches, the error says why, ending in a bracketed kind the conformance
/// vectors pin: `[unknown-type]`, `[newer-version]` (the data may use terms
/// this schema lacks), `[other-major]`, or `[other-minor-line]` (below 1.0.0
/// the minor is the incompatible line).
fn class_for<'a>(
    classes: &'a Map<String, Json>,
    type_uri: &str,
) -> Result<(&'a str, &'a Json), String> {
    if let Some((key, cls)) = classes.get_key_value(type_uri) {
        return Ok((key.as_str(), cls));
    }
    let unknown = || format!("no schema for type {} [unknown-type]", type_uri);
    let (key, written) = match versioned_coordinate(type_uri) {
        Some(found) => found,
        None => return Err(unknown()),
    };

    let mut readable: Option<(&'a str, &'a Json, CoordinateVersion)> = None;
    let mut nearest: Option<CoordinateVersion> = None;
    for (uri, cls) in classes.iter() {
        let version = match versioned_coordinate(uri) {
            Some((k, v)) if k == key => v,
            _ => continue,
        };
        if is_readable_by(written, version) {
            if readable.map_or(true, |(_, _, best)| {
                version_order(version) > version_order(best)
            }) {
                readable = Some((uri.as_str(), cls, version));
            }
        } else if nearest.map_or(true, |n| version_order(version) > version_order(n)) {
            nearest = Some(version);
        }
    }
    if let Some((uri, cls, _)) = readable {
        return Ok((uri, cls));
    }
    match nearest {
        Some(r) => Err(incompatible_version(type_uri, written, r)),
        None => Err(unknown()),
    }
}

fn incompatible_version(type_uri: &str, w: CoordinateVersion, r: CoordinateVersion) -> String {
    let (ws, rs) = (format_version(w), format_version(r));
    if w.major != r.major {
        return format!(
            "{} is written at {}, a different major version than this codec's schema ({}) \
             [other-major]",
            type_uri, ws, rs
        );
    }
    if w.major == 0 && w.minor != r.minor {
        return format!(
            "{} is written at {}; below 1.0.0 a different minor is a different version line \
             than this codec's schema ({}) [other-minor-line]",
            type_uri, ws, rs
        );
    }
    format!(
        "{} is written at {}, newer than this codec's schema ({}); upgrade the codec to read it \
         [newer-version]",
        type_uri, ws, rs
    )
}

/// The class to HASH a node or embedded value with: the exact versioned class
/// only. A content hash is computed over the producer's predicate and type
/// URIs, versions included, so a node written at an earlier compatible
/// version cannot be re-hashed with a later schema — its predicates would
/// carry the later version and the hash would differ. When only a compatible
/// class exists, say so (`[hash-needs-exact-version]`) rather than produce a
/// different hash or the bare "no schema" error.
fn hash_class_for<'a>(
    schema: &'a Json,
    type_uri: &str,
    what: &str,
) -> Result<&'a Json, CodecError> {
    let classes = classes_of(schema)?;
    if let Some(cls) = classes.get(type_uri) {
        return Ok(cls);
    }
    match class_for(classes, type_uri) {
        Ok((found, _)) => err(format!(
            "Cannot hash {} {}: this codec's schema has it at {}, and a content hash is \
             computed with the producer's schema version — hashing needs the exact class \
             [hash-needs-exact-version]",
            what, type_uri, found
        )),
        Err(_) => err(format!(
            "no schema for {} {} [unknown-type]",
            what, type_uri
        )),
    }
}

/// Whether a node is an instance of the class `class_uri` — what a generated
/// type guard asks (runtime#28). Each of the node's types (`$types`, else its
/// `$type`) is resolved through the same compatible lookup [`deserialize`]
/// uses, so a node written at an earlier compatible version satisfies the
/// later version's guard; it matches when that class IS `class_uri` or has it
/// among its `ancestors`. Classes compare by versionless identity. A type this
/// schema cannot read (unknown, newer, another major) never matches. Errors
/// when `class_uri` is not a class coordinate or the schema is malformed.
/// For a typed instance, [`KanonakNode::type_matches`] asks the same question
/// of its envelope.
pub fn type_matches(node: &Node, class_uri: &str, schema: &Json) -> Result<bool, CodecError> {
    let members: Vec<&str> = match node.get("$types") {
        None | Some(Json::Null) => node
            .get("$type")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty())
            .into_iter()
            .collect(),
        Some(Json::Array(items)) => items
            .iter()
            .map(|t| t.as_str())
            .collect::<Option<Vec<&str>>>()
            .ok_or_else(|| {
                CodecError::Malformed("type_matches: $types must be a list of type URIs".into())
            })?,
        Some(_) => return err("type_matches: $types must be a list of type URIs"),
    };
    type_matches_members(&members, class_uri, schema)
}

/// The one type-match walk, over a node's type members (`$types`, else
/// `$type`), shared by the dictionary and typed surfaces.
pub(crate) fn type_matches_members(
    members: &[&str],
    class_uri: &str,
    schema: &Json,
) -> Result<bool, CodecError> {
    let target = identity_of(class_uri).ok_or_else(|| {
        CodecError::Malformed(format!(
            "type_matches: '{}' is not a class coordinate",
            class_uri
        ))
    })?;
    let classes = classes_of(schema)?;

    for &member in members {
        let (key, cls) = match class_for(classes, member) {
            Ok(found) => found,
            Err(_) => continue,
        };
        let type_uri = cls
            .get("typeUri")
            .and_then(|t| t.as_str())
            .ok_or_else(|| CodecError::Malformed(format!("class {} is missing 'typeUri'", key)))?;
        if identity_of(type_uri).as_deref() == Some(target.as_str()) {
            return Ok(true);
        }
        match cls.get("ancestors") {
            None | Some(Json::Null) => {}
            Some(Json::Array(ancestors)) => {
                for ancestor in ancestors {
                    let ancestor = ancestor.as_str().ok_or_else(|| {
                        CodecError::Malformed(format!(
                            "class {}: 'ancestors' must be a list of class URIs",
                            key
                        ))
                    })?;
                    if identity_of(ancestor).as_deref() == Some(target.as_str()) {
                        return Ok(true);
                    }
                }
            }
            Some(_) => {
                return err(format!(
                    "class {}: 'ancestors' must be a list of class URIs",
                    key
                ))
            }
        }
    }
    Ok(false)
}

/// An enumeration member found by [`enum_member`].
#[derive(Debug, Clone, PartialEq)]
pub struct EnumMemberMatch {
    /// The enumeration class's durable URI (its key in the schema's `enums`).
    pub enum_type: String,
    /// The member's durable URI as this schema keys it.
    pub uri: String,
    /// The member's schema entry (`{"label": ...}`).
    pub member: Json,
}

/// The enumeration member a `{"$ref": …}` names (runtime#28): the exact
/// versioned key first, then the same member at a version that can read the
/// written one — so a member referenced at an earlier compatible version of
/// the package resolves against a later schema. `None` when this schema has
/// no such member, which, as for `enums` itself, means "not mine", never
/// "invalid".
pub fn enum_member(schema: &Json, reference: &str) -> Option<EnumMemberMatch> {
    let enums = schema.get("enums").and_then(|e| e.as_object())?;
    for (enum_type, e) in enums.iter() {
        if let Some(member) = e.get("members").and_then(|m| m.get(reference)) {
            return Some(EnumMemberMatch {
                enum_type: enum_type.clone(),
                uri: reference.to_string(),
                member: member.clone(),
            });
        }
    }
    let (key, written) = versioned_coordinate(reference)?;
    let mut best: Option<(&str, &str, &Json, CoordinateVersion)> = None;
    for (enum_type, e) in enums.iter() {
        let members = match e.get("members").and_then(|m| m.as_object()) {
            Some(members) => members,
            None => continue,
        };
        for (uri, member) in members.iter() {
            let version = match versioned_coordinate(uri) {
                Some((k, v)) if k == key => v,
                _ => continue,
            };
            if !is_readable_by(written, version) {
                continue;
            }
            if best.map_or(true, |(_, _, _, b)| {
                version_order(version) > version_order(b)
            }) {
                best = Some((enum_type.as_str(), uri.as_str(), member, version));
            }
        }
    }
    best.map(|(enum_type, uri, member, _)| EnumMemberMatch {
        enum_type: enum_type.to_string(),
        uri: uri.to_string(),
        member: member.clone(),
    })
}

/// Validate a node-or-embedded's `$types` envelope (0.4.0, runtime#10) and
/// return the validated set, or `None` when the node is single-typed.
/// Invariants: sorted by UTF-8 bytes, at least two members, no duplicates, and
/// `$type` (the dispatch key, chosen by the schema layer's primary rule) is a
/// member. Enforced wherever the envelope is touched — serialize, deserialize,
/// and canonicalization — so a producer fails at emit time and a reader never
/// masks a nondeterministic emitter by silently repairing the set.
fn validated_types(
    map: &Map<String, Json>,
    where_: &str,
) -> Result<Option<Vec<String>>, CodecError> {
    let raw = match map.get("$types") {
        None | Some(Json::Null) => return Ok(None),
        Some(v) => v,
    };
    let items = match raw.as_array() {
        Some(items) => items,
        None => {
            return err(format!(
                "{}: $types must be a list of non-empty type URIs",
                where_
            ))
        }
    };
    let mut types: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        match item.as_str() {
            Some(s) if !s.is_empty() => types.push(s.to_string()),
            _ => {
                return err(format!(
                    "{}: $types must be a list of non-empty type URIs",
                    where_
                ))
            }
        }
    }
    if types.len() < 2 {
        return err(format!(
            "{}: $types with {} member(s) is forbidden — a single-typed node carries \
             only $type (a second encoding of the same content would be hash-ambiguous)",
            where_,
            types.len()
        ));
    }
    for pair in types.windows(2) {
        match pair[0].as_bytes().cmp(pair[1].as_bytes()) {
            std::cmp::Ordering::Equal => {
                return err(format!(
                    "{}: $types carries duplicate member {}",
                    where_, pair[1]
                ))
            }
            std::cmp::Ordering::Greater => {
                return err(format!(
                    "{}: $types is not sorted by UTF-8 bytes ({} sorts after {}) — \
                     ordering is the producer's job, never the reader's",
                    where_, pair[0], pair[1]
                ))
            }
            std::cmp::Ordering::Less => {}
        }
    }
    match map.get("$type").and_then(|t| t.as_str()) {
        Some(primary) if types.iter().any(|t| t == primary) => Ok(Some(types)),
        other => err(format!(
            "{}: $type ({:?}) must be present and a member of $types",
            where_, other
        )),
    }
}

/// Recursively validate every `$types` envelope in a wire value (the node
/// itself and any embedded node at any depth). Shared by [`serialize`] (the
/// producer fails at emit time) and [`deserialize`] (the strict reader rejects
/// rather than repairs).
fn assert_types_envelopes(value: &Json, where_: &str) -> Result<(), CodecError> {
    match value {
        Json::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_types_envelopes(item, &format!("{}[{}]", where_, i))?;
            }
            Ok(())
        }
        Json::Object(map) => assert_types_envelopes_map(map, where_),
        _ => Ok(()),
    }
}

fn assert_types_envelopes_map(map: &Map<String, Json>, where_: &str) -> Result<(), CodecError> {
    if map.contains_key("$types") {
        validated_types(map, where_)?;
    }
    for (key, value) in map.iter() {
        if key != "$types" {
            assert_types_envelopes(value, &format!("{}.{}", where_, key))?;
        }
    }
    Ok(())
}

/// Every reference value (`{"$ref": uri}`) in a wire value, at any depth, must
/// address a NAMED resource: `publisher/package[@version]/name`, no URI fragment
/// (runtime#6). Fragments address embedded resources for navigation and
/// deep-linking; they are not part of the reference graph, and accepting one
/// here would establish that capability by accident. Shared by [`serialize`],
/// [`deserialize`] and canonicalization, so all three reject the same input.
fn assert_reference_values(value: &Json, where_: &str) -> Result<(), CodecError> {
    match value {
        Json::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_reference_values(item, &format!("{}[{}]", where_, i))?;
            }
            Ok(())
        }
        Json::Object(map) => assert_reference_values_map(map, where_),
        _ => Ok(()),
    }
}

fn assert_reference_values_map(map: &Map<String, Json>, where_: &str) -> Result<(), CodecError> {
    if let Some(Json::String(uri)) = map.get("$ref") {
        assert_named_reference(uri, where_)?;
    }
    for (key, value) in map.iter() {
        assert_reference_values(value, &format!("{}.{}", where_, key))?;
    }
    Ok(())
}

fn assert_named_reference(uri: &str, where_: &str) -> Result<(), CodecError> {
    if uri.contains('#') {
        return err(format!(
            "{}: reference value '{}' contains a URI fragment. A reference addresses a named \
             resource (publisher/package[@version]/name); fragments are a navigation convention, \
             not part of the reference graph [fragment-reference]",
            where_, uri
        ));
    }
    Ok(())
}

/// Build a single canonical `Value` for one (non-list) field datum, per its
/// schema prop.
fn build_value(prop: &Json, raw: &Json, schema: &Json) -> Result<Value, CodecError> {
    let kind = prop
        .get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| CodecError::Malformed("schema prop is missing 'kind'".into()))?;
    if kind == "object" {
        // A node: a reference (`{"$ref"}`) or an embedded resource.
        if let Some(reference) = raw.get("$ref").and_then(|r| r.as_str()) {
            return Ok(Value::Reference(reference.to_string()));
        }
        if let Some(map) = raw.as_object() {
            return embedded_value(prop, map, schema);
        }
        return err(format!(
            "Object property expects a reference ({{\"$ref\": ...}}) or an embedded \
             node (a map), got {}",
            raw
        ));
    }
    let datatype = prop
        .get("datatype")
        .and_then(|d| d.as_str())
        .ok_or_else(|| CodecError::Malformed("datatype prop is missing 'datatype'".into()))?;
    match carrier_of(datatype) {
        None => Ok(Value::Raw(lexical(raw))),
        Some(carrier) => Ok(Value::Typed {
            carrier,
            lexical: lexical(raw),
        }),
    }
}

/// Canonicalize an embedded value (0.2.0): a map with no `$id`, an optional
/// `$name` (the authored dict-key — hash-relevant), an optional `$type`, and
/// schema-mapped fields. An explicit `$type` emits a type statement inside the
/// embedded (hash-relevant even when it equals the range-derived type); without
/// it, fields map via the containing property's `range` and NO type statement is
/// emitted — range-derived typing is inference only.
fn embedded_value(
    prop: &Json,
    map: &Map<String, Json>,
    schema: &Json,
) -> Result<Value, CodecError> {
    if map.contains_key("$id") {
        return err(
            "An embedded value must not carry $id — to point at a named resource, \
             pass a reference ({\"$ref\": ...}).",
        );
    }
    let types = validated_types(map, "Embedded value")?;
    let explicit_type = map.get("$type").and_then(|t| t.as_str());
    let cls_uri = match explicit_type.or_else(|| prop.get("range").and_then(|r| r.as_str())) {
        Some(uri) => uri,
        None => {
            return err(
                "Cannot map embedded value: it carries no $type and the property \
                 declares no range.",
            )
        }
    };
    let cls = hash_class_for(schema, cls_uri, "embedded type")?;
    let props = cls
        .get("props")
        .ok_or_else(|| CodecError::Malformed(format!("class {} is missing 'props'", cls_uri)))?;

    let mut statements = field_statements(map, props, schema)?;
    if let Some(members) = types {
        // A multi-typed embedded ($types implies an explicit $type): one type
        // statement per member, in $types (UTF-8 sorted) order — all hash-relevant.
        let type_predicate = schema
            .get("typePredicate")
            .and_then(|p| p.as_str())
            .ok_or_else(|| CodecError::Malformed("schema is missing 'typePredicate'".into()))?;
        for member in members {
            statements.push(Statement {
                predicate: type_predicate.to_string(),
                value: Value::Reference(member),
            });
        }
    } else if let Some(type_uri) = explicit_type {
        let type_predicate = schema
            .get("typePredicate")
            .and_then(|p| p.as_str())
            .ok_or_else(|| CodecError::Malformed("schema is missing 'typePredicate'".into()))?;
        statements.push(Statement {
            predicate: type_predicate.to_string(),
            value: Value::Reference(type_uri.to_string()),
        });
    }
    let name = map
        .get("$name")
        .and_then(|n| n.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Ok(Value::Embedded { name, statements })
}

/// The statements for one node-or-embedded's modeled fields + its `$extra` —
/// everything except the type triple (subjects always carry one; embeddeds only
/// when explicitly typed).
fn field_statements(
    source: &Map<String, Json>,
    props: &Json,
    schema: &Json,
) -> Result<Vec<Statement>, CodecError> {
    let mut out: Vec<Statement> = Vec::new();

    for (key, raw) in source.iter() {
        if ENVELOPE_KEYS.contains(&key.as_str()) || raw.is_null() {
            continue;
        }
        match props.get(key) {
            None => out.push(Statement {
                predicate: key.clone(),
                value: Value::Raw(lexical(raw)),
            }),
            Some(prop) => {
                let predicate =
                    prop.get("predicate")
                        .and_then(|p| p.as_str())
                        .ok_or_else(|| {
                            CodecError::Malformed(format!("prop {} is missing 'predicate'", key))
                        })?;
                let value = match raw.as_array() {
                    Some(items) => {
                        // An empty list contributes NO statement — absent and empty
                        // are identical at the canonical layer (the wire serialize
                        // still preserves the empty list).
                        if items.is_empty() {
                            continue;
                        }
                        let mut list = Vec::with_capacity(items.len());
                        for item in items {
                            list.push(build_value(prop, item, schema)?);
                        }
                        Value::List(list)
                    }
                    None => build_value(prop, raw, schema)?,
                };
                out.push(Statement {
                    predicate: predicate.to_string(),
                    value,
                });
            }
        }
    }

    if let Some(extra) = source.get("$extra") {
        let extra = extra
            .as_object()
            .ok_or_else(|| CodecError::Malformed("$extra must be an object".into()))?;
        for (predicate, raw) in extra.iter() {
            if raw.is_null() {
                continue;
            }
            out.push(Statement {
                predicate: predicate.clone(),
                value: Value::Raw(lexical(raw)),
            });
        }
    }
    Ok(out)
}

/// The statements for one subject node: the rdf:type triple(s), then its fields.
fn statements(node: &Node, schema: &Json) -> Result<Vec<Statement>, CodecError> {
    let types = {
        let id = node
            .get("$id")
            .and_then(|i| i.as_str())
            .unwrap_or("(no $id)");
        let where_ = format!("Node {}", id);
        let types = validated_types(node, &where_)?;
        assert_reference_values_map(node, &where_)?;
        types
    };
    let type_uri = node
        .get("$type")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CodecError::Malformed("node is missing $type".into()))?;

    let cls = hash_class_for(schema, type_uri, "type")?;
    let props = cls
        .get("props")
        .ok_or_else(|| CodecError::Malformed(format!("class {} is missing 'props'", type_uri)))?;

    let type_predicate = schema
        .get("typePredicate")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed("schema is missing 'typePredicate'".into()))?;

    // The rdf:type triple(s) every subject carries: one per $types member for a
    // multi-typed node (in $types' UTF-8 sorted order), else the single $type.
    let members = types.unwrap_or_else(|| vec![type_uri.to_string()]);
    let mut out: Vec<Statement> = members
        .into_iter()
        .map(|member| Statement {
            predicate: type_predicate.to_string(),
            value: Value::Reference(member),
        })
        .collect();
    out.extend(field_statements(node, props, schema)?);
    Ok(out)
}

/// Build the canonical input model: a subject per node + the synthesized
/// package-wrapper subject (raw label + `Package` type), exactly the subject set
/// `kanonak hash` produces for the equivalent authored package.
pub fn build_package(nodes: &[Node], schema: &Json, pkg: &Json) -> Result<Package, CodecError> {
    let mut subjects: Vec<Subject> = Vec::with_capacity(nodes.len() + 1);
    for node in nodes {
        let id = node
            .get("$id")
            .and_then(|i| i.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| CodecError::Malformed("node is missing $id".into()))?;
        subjects.push(Subject {
            uri: id.to_string(),
            statements: statements(node, schema)?,
        });
    }

    let publisher = pkg
        .get("publisher")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed("pkg is missing 'publisher'".into()))?;
    let package_name = pkg
        .get("packageName")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed("pkg is missing 'packageName'".into()))?;
    let version = pkg
        .get("version")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed("pkg is missing 'version'".into()))?;

    let pkg_uri = format!(
        "{}/{}@{}/{}",
        publisher, package_name, version, package_name
    );

    let type_predicate = schema
        .get("typePredicate")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed("schema is missing 'typePredicate'".into()))?;
    let label_predicate = schema
        .get("labelPredicate")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed("schema is missing 'labelPredicate'".into()))?;
    let package_type_uri = schema
        .get("packageTypeUri")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed("schema is missing 'packageTypeUri'".into()))?;

    let mut pkg_statements: Vec<Statement> = Vec::new();
    if let Some(label) = pkg.get("label") {
        if !label.is_null() {
            let label = label
                .as_str()
                .ok_or_else(|| CodecError::Malformed("pkg label must be a string".into()))?;
            pkg_statements.push(Statement {
                predicate: label_predicate.to_string(),
                value: Value::Raw(label.to_string()),
            });
        }
    }
    pkg_statements.push(Statement {
        predicate: type_predicate.to_string(),
        value: Value::Reference(package_type_uri.to_string()),
    });
    subjects.push(Subject {
        uri: pkg_uri,
        statements: pkg_statements,
    });

    Ok(Package { subjects })
}

/// The canonical form (the `{subjects:[...]}` JSON) of a package from nodes.
pub fn canonical_form(nodes: &[Node], schema: &Json, pkg: &Json) -> Result<String, CodecError> {
    Ok(canonical_form_pkg(&build_package(nodes, schema, pkg)?)?)
}

/// The `sha256:` content hash of a package from nodes — matches `kanonak hash`.
pub fn content_hash(nodes: &[Node], schema: &Json, pkg: &Json) -> Result<String, CodecError> {
    Ok(canonical_hash_pkg(&build_package(nodes, schema, pkg)?)?)
}

/// Serialize a typed node to its normalized-JSON wire form. `$extra` entries ride
/// as sibling fields after the modeled ones; a modeled field wins a name
/// collision (`[JsonExtensionData]` semantics). No `$extra` key on the wire.
/// Fallible since 0.4.0: an invalid `$types` envelope (at any depth) is a
/// producer bug and fails at emit time, as does a fragment reference (runtime#6).
pub fn serialize(node: &Node) -> Result<Node, CodecError> {
    let where_ = node
        .get("$id")
        .or_else(|| node.get("$type"))
        .and_then(|v| v.as_str())
        .unwrap_or("(node)");
    assert_types_envelopes_map(node, &format!("serialize {}", where_))?;
    assert_reference_values_map(node, &format!("serialize {}", where_))?;
    let mut out = Map::new();
    for (key, value) in node.iter() {
        if key == "$extra" || value.is_null() {
            continue;
        }
        out.insert(key.clone(), value.clone());
    }
    if let Some(extra) = node.get("$extra").and_then(|e| e.as_object()) {
        for (key, value) in extra.iter() {
            if !value.is_null() && !out.contains_key(key) {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(out)
}

/// Parse normalized JSON into a typed node. `$`-envelope keys and fields modeled
/// on the node's `$type` stay top-level; every other key is collected into
/// `$extra` so a strongly-typed consumer round-trips it losslessly. Requires
/// `$type` (the one field that cannot be inferred) and a class in the schema
/// that can read it: the exact class, or the same class at a compatible later
/// version (runtime#28 — see [`type_matches`] for the same rule). The node
/// keeps the `$type` it was WRITTEN with; the producer's bytes are the
/// producer's.
pub fn deserialize(json_obj: &Node, schema: &Json) -> Result<Node, CodecError> {
    let type_uri = json_obj
        .get("$type")
        .and_then(|t| t.as_str())
        .ok_or_else(|| CodecError::Malformed("Cannot deserialize: missing string $type".into()))?;

    // Reader-side $types validation, at every depth: an unsorted / singleton /
    // duplicate / non-member set is REJECTED, never silently repaired —
    // determinism belongs to the producer, and a lenient reader would mask a
    // nondeterministic emitter.
    {
        let where_ = json_obj
            .get("$id")
            .and_then(|i| i.as_str())
            .unwrap_or(type_uri);
        assert_types_envelopes_map(json_obj, &format!("deserialize {}", where_))?;
        assert_reference_values_map(json_obj, &format!("deserialize {}", where_))?;
    }

    let (cls_uri, cls) = class_for(classes_of(schema)?, type_uri)
        .map_err(|e| CodecError::Malformed(format!("Cannot deserialize: {}", e)))?;
    let props = cls
        .get("props")
        .ok_or_else(|| CodecError::Malformed(format!("class {} is missing 'props'", cls_uri)))?;

    let mut node = Map::new();
    node.insert("$type".to_string(), Json::String(type_uri.to_string()));
    let mut extra = Map::new();
    for (key, value) in json_obj.iter() {
        if key == "$type" {
            continue;
        }
        if key.starts_with('$') || props.get(key).is_some() {
            node.insert(key.clone(), value.clone());
        } else {
            extra.insert(key.clone(), value.clone());
        }
    }
    if !extra.is_empty() {
        node.insert("$extra".to_string(), Json::Object(extra));
    }
    Ok(node)
}
