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
    canonical_decimal, canonical_form as canonical_form_pkg, canonical_hash as canonical_hash_pkg,
    canonical_integer, carrier_of, is_readable_by, lenient_versionless_key, parse_coordinate,
    versionless_key, CanonError, CoordinateVersion, Package, Statement, Subject, Value,
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
///
/// `#[non_exhaustive]`: a later 1.x release may add a variant, so a `match`
/// outside this crate needs a wildcard arm. Branch on the message's bracketed
/// kind (`[unknown-type]`, `[fragment-reference]`, ...) rather than the variant.
#[derive(Debug)]
#[non_exhaustive]
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

/// An enumeration member found by [`enum_member`]. `#[non_exhaustive]`: read
/// its fields; a later 1.x release may add one.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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

    let mut statements = field_statements(map, cls_uri, props, schema)?;
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
    class_uri: &str,
    props: &Json,
    schema: &Json,
) -> Result<Vec<Statement>, CodecError> {
    let mut out: Vec<Statement> = Vec::new();

    for (key, raw) in source.iter() {
        if ENVELOPE_KEYS.contains(&key.as_str()) || raw.is_null() {
            continue;
        }
        match props.get(key) {
            None => return undeclared_statement(class_uri, &format!("\"{}\"", key)),
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

    // Open-world extras ride through serialize and deserialize, but this codec
    // can't type them, so it refuses to hash them rather than hash them as text.
    if let Some(extra) = source.get("$extra") {
        let extra = extra
            .as_object()
            .ok_or_else(|| CodecError::Malformed("$extra must be an object".into()))?;
        for (predicate, raw) in extra.iter() {
            if raw.is_null() {
                continue;
            }
            return undeclared_statement(class_uri, &format!("$extra statement {}", predicate));
        }
    }
    Ok(out)
}

/// A codec holds one package's schema, so it can't know the range of a
/// predicate another package declares on its classes (open-world augmentation).
/// Hashing such a statement as untyped text would give a hash that differs from
/// `kanonak hash`'s without a word, so hashing refuses it, naming it
/// (`[hash-undeclared-statement]`). Serializing and deserializing carry it.
fn undeclared_statement<T>(class_uri: &str, what: &str) -> Result<T, CodecError> {
    err(format!(
        "Cannot hash {}'s {}: this codec's schema doesn't declare it, so it can't type its value. \
         Hash it with the SDK, over the graph [hash-undeclared-statement]",
        class_uri, what
    ))
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
    out.extend(field_statements(node, type_uri, props, schema)?);
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
#[deprecated(
    note = "use serialize_exact, which also writes each integer and decimal value in its JSON form"
)]
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
#[deprecated(
    note = "use deserialize_exact, which also reads each integer and decimal value exactly"
)]
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

// The JSON forms of integer and decimal values (kanonak-protocol 4.11.0's
// `numbers-in-json`, runtime#54). A JSON number can't carry every integer or
// decimal exactly in every language, so a wide integer and a decimal travel as
// a string of their digits, and a reader refuses a number that may already
// have been rounded.

/// How JSON carries a datatype's values.
#[derive(Clone, Copy)]
enum JsonNumberForm {
    /// Written as a string; read from a string, or from a whole number within
    /// ±(2^53 − 1).
    Wide,
    /// Written as a string; read from a string only.
    Decimal,
    /// Written as a number; read from a whole number only.
    Narrow,
}

/// THE table of the datatypes JSON carries as a number or a string of digits,
/// by versionless key: each one's form, and its range's bounds where it has
/// them. Every bound fits in `i128`.
fn json_number_form(key: &str) -> Option<(JsonNumberForm, Option<i128>, Option<i128>)> {
    use JsonNumberForm::{Decimal, Narrow, Wide};
    Some(match key {
        "kanonak.org/core-xsd/integer" => (Wide, None, None),
        "kanonak.org/core-xsd/nonNegativeInteger" => (Wide, Some(0), None),
        "kanonak.org/core-xsd/positiveInteger" => (Wide, Some(1), None),
        "kanonak.org/core-xsd/nonPositiveInteger" => (Wide, None, Some(0)),
        "kanonak.org/core-xsd/negativeInteger" => (Wide, None, Some(-1)),
        "kanonak.org/core-xsd/long" => (Wide, Some(i64::MIN.into()), Some(i64::MAX.into())),
        "kanonak.org/core-xsd/unsignedLong" => (Wide, Some(0), Some(u64::MAX.into())),
        "kanonak.org/core-xsd/decimal" => (Decimal, None, None),
        "kanonak.org/core-xsd/int" => (Narrow, Some(i32::MIN.into()), Some(i32::MAX.into())),
        "kanonak.org/core-xsd/short" => (Narrow, Some(i16::MIN.into()), Some(i16::MAX.into())),
        "kanonak.org/core-xsd/byte" => (Narrow, Some(i8::MIN.into()), Some(i8::MAX.into())),
        "kanonak.org/core-xsd/unsignedInt" => (Narrow, Some(0), Some(u32::MAX.into())),
        "kanonak.org/core-xsd/unsignedShort" => (Narrow, Some(0), Some(u16::MAX.into())),
        "kanonak.org/core-xsd/unsignedByte" => (Narrow, Some(0), Some(u8::MAX.into())),
        _ => return None,
    })
}

/// A modeled field whose datatype JSON carries as a number or a string of
/// digits.
struct NumberField<'a> {
    predicate: &'a str,
    datatype: &'a str,
    form: JsonNumberForm,
    min: Option<i128>,
    max: Option<i128>,
}

impl NumberField<'_> {
    fn refuse<T>(&self, why: impl std::fmt::Display) -> Result<T, CodecError> {
        err(format!("{}: {} [json-number-form]", self.predicate, why))
    }

    fn integer_of_lexical(&self, lexical: &str) -> Result<String, CodecError> {
        canonical_integer(lexical)
            .or_else(|_| self.refuse(format!("{:?} is not an integer lexical form", lexical)))
    }

    fn decimal_of_lexical(&self, lexical: &str) -> Result<String, CodecError> {
        canonical_decimal(lexical)
            .or_else(|_| self.refuse(format!("{:?} is not a decimal lexical form", lexical)))
    }

    /// An integer's canonical digits, refused outside the datatype's range.
    /// Every bound fits in `i128`, so digits too long for one lie past the
    /// bound on their sign's side, if there is one.
    fn in_range(&self, digits: String) -> Result<String, CodecError> {
        let outside = match digits.parse::<i128>() {
            Ok(n) => {
                self.min.map_or(false, |min| n < min) || self.max.map_or(false, |max| n > max)
            }
            Err(_) if digits.starts_with('-') => self.min.is_some(),
            Err(_) => self.max.is_some(),
        };
        if outside {
            return self.refuse(format!("{} is outside {}'s range", digits, self.datatype));
        }
        Ok(digits)
    }

    /// An integer as a node here holds it: a number where its datatype bounds
    /// it on both sides, so it fits an `i64` or a `u64`; else a string of its
    /// canonical digits, since no Rust integer holds every value.
    fn held(&self, digits: String) -> Result<Json, CodecError> {
        if self.min.is_none() || self.max.is_none() {
            return Ok(Json::String(digits));
        }
        digits
            .parse::<i64>()
            .map(Json::from)
            .or_else(|_| digits.parse::<u64>().map(Json::from))
            .or_else(|_| self.refuse(format!("{} fits neither an i64 nor a u64", digits)))
    }
}

/// The JSON number form of a modeled field's datatype, or `None` when JSON
/// carries its values as they stand.
fn number_field<'a>(name: &str, prop: &'a Json) -> Result<Option<NumberField<'a>>, CodecError> {
    let datatype = match prop.get("datatype").and_then(|d| d.as_str()) {
        Some(datatype) => datatype,
        None => return Ok(None),
    };
    let (form, min, max) =
        match lenient_versionless_key(datatype).and_then(|key| json_number_form(&key)) {
            Some(found) => found,
            None => return Ok(None),
        };
    let predicate = prop
        .get("predicate")
        .and_then(|p| p.as_str())
        .ok_or_else(|| CodecError::Malformed(format!("prop {} is missing 'predicate'", name)))?;
    Ok(Some(NumberField {
        predicate,
        datatype,
        form,
        min,
        max,
    }))
}

/// A JSON number's value as an integer's canonical digits, when it is a whole
/// number.
fn whole_digits(value: &Json) -> Option<String> {
    let n = match value {
        Json::Number(n) => n,
        _ => return None,
    };
    if let Some(i) = n.as_i64() {
        return Some(i.to_string());
    }
    if let Some(u) = n.as_u64() {
        return Some(u.to_string());
    }
    let f = n.as_f64().filter(|f| f.fract() == 0.0)?;
    canonical_integer(&format!("{:.0}", f)).ok()
}

/// Whether an integer lies within ±(2^53 − 1), where a double holds every
/// whole number exactly.
fn within_double_precision(digits: &str) -> bool {
    const MAX_SAFE_INTEGER: i128 = (1 << 53) - 1;
    digits
        .parse::<i128>()
        .map_or(false, |n| n.abs() <= MAX_SAFE_INTEGER)
}

fn describe_json(value: &Json) -> String {
    let kind = match value {
        Json::Null => return "null".to_string(),
        Json::Bool(_) => "the boolean",
        Json::Number(_) => "the number",
        Json::String(_) => "the string",
        Json::Array(_) => "the list",
        Json::Object(_) => "the object",
    };
    format!("{} {}", kind, value)
}

type NumberConversion = fn(&NumberField, &Json) -> Result<Json, CodecError>;

/// A value as JSON carries it, to the type a node here holds it as.
fn read_json_number(field: &NumberField, value: &Json) -> Result<Json, CodecError> {
    match field.form {
        JsonNumberForm::Decimal => match value {
            Json::String(s) => Ok(Json::String(field.decimal_of_lexical(s)?)),
            other => field.refuse(format!(
                "a decimal is read from a JSON string only, so it can't have been rounded; got {}",
                describe_json(other)
            )),
        },
        JsonNumberForm::Wide => {
            let digits = match value {
                Json::String(s) => Some(field.integer_of_lexical(s)?),
                Json::Number(_) => whole_digits(value).filter(|d| within_double_precision(d)),
                _ => None,
            };
            match digits {
                Some(digits) => field.held(field.in_range(digits)?),
                None => field.refuse(format!(
                    "a wide integer is read from a string, or from a whole JSON number within \
                     ±(2^53 − 1); got {}",
                    describe_json(value)
                )),
            }
        }
        JsonNumberForm::Narrow => match whole_digits(value) {
            Some(digits) => field.held(field.in_range(digits)?),
            None => field.refuse(format!(
                "a narrow integer is read from a whole JSON number only; got {}",
                describe_json(value)
            )),
        },
    }
}

/// A value as a node here holds it, to its JSON form.
fn write_json_number(field: &NumberField, value: &Json) -> Result<Json, CodecError> {
    match field.form {
        JsonNumberForm::Decimal => match value {
            Json::String(s) => Ok(Json::String(field.decimal_of_lexical(s)?)),
            other => field.refuse(format!(
                "pass a decimal as a string of its digits, which holds it exactly; got {}",
                describe_json(other)
            )),
        },
        JsonNumberForm::Wide => {
            let digits = match value {
                Json::String(s) => Some(field.integer_of_lexical(s)?),
                Json::Number(n) => {
                    whole_digits(value).filter(|d| !n.is_f64() || within_double_precision(d))
                }
                _ => None,
            };
            match digits {
                Some(digits) => Ok(Json::String(field.in_range(digits)?)),
                None => field.refuse(format!(
                    "pass a wide integer as a string of its digits, an i64 or u64 number, or a \
                     whole number within ±(2^53 − 1); got {}",
                    describe_json(value)
                )),
            }
        }
        JsonNumberForm::Narrow => match whole_digits(value) {
            Some(digits) => field.held(field.in_range(digits)?),
            None => field.refuse(format!(
                "pass a narrow integer as a whole number; got {}",
                describe_json(value)
            )),
        },
    }
}

/// The modeled fields of the class `type_uri` names, found as [`deserialize`]
/// finds it.
fn props_to_convert<'a>(
    schema: &'a Json,
    type_uri: &str,
    what: &str,
) -> Result<&'a Map<String, Json>, CodecError> {
    let (cls_uri, cls) = class_for(classes_of(schema)?, type_uri)
        .map_err(|e| CodecError::Malformed(format!("Cannot convert {}: {}", what, e)))?;
    cls.get("props")
        .and_then(|p| p.as_object())
        .ok_or_else(|| CodecError::Malformed(format!("class {} is missing 'props'", cls_uri)))
}

/// Each modeled field's value, through lists and embedded values, with its
/// integers and decimals converted. `$extra`, the envelope and references are
/// left as they are.
fn convert_fields(
    fields: &Map<String, Json>,
    props: &Map<String, Json>,
    schema: &Json,
    convert: NumberConversion,
) -> Result<Map<String, Json>, CodecError> {
    let mut out = fields.clone();
    for (name, prop) in props {
        if let Some(value) = out.get_mut(name).filter(|v| !v.is_null()) {
            *value = convert_value(name, prop, value, schema, convert)?;
        }
    }
    Ok(out)
}

fn convert_value(
    name: &str,
    prop: &Json,
    value: &Json,
    schema: &Json,
    convert: NumberConversion,
) -> Result<Json, CodecError> {
    if let Json::Array(items) = value {
        return items
            .iter()
            .map(|item| convert_value(name, prop, item, schema, convert))
            .collect::<Result<Vec<Json>, CodecError>>()
            .map(Json::Array);
    }
    if prop.get("kind").and_then(|k| k.as_str()) == Some("datatype") {
        return match number_field(name, prop)? {
            Some(field) => convert(&field, value),
            None => Ok(value.clone()),
        };
    }
    match value {
        Json::Object(map) if !map.contains_key("$ref") => {
            let cls_uri = map
                .get("$type")
                .and_then(|t| t.as_str())
                .or_else(|| prop.get("range").and_then(|r| r.as_str()))
                .ok_or_else(|| {
                    CodecError::Malformed(format!(
                        "Cannot convert the embedded value under {}: it carries no $type and \
                         the property declares no range.",
                        name
                    ))
                })?;
            let props =
                props_to_convert(schema, cls_uri, &format!("the embedded value under {}", name))?;
            Ok(Json::Object(convert_fields(map, props, schema, convert)?))
        }
        _ => Ok(value.clone()),
    }
}

fn convert_node(
    node: &Node,
    schema: &Json,
    convert: NumberConversion,
) -> Result<Node, CodecError> {
    let type_uri = node
        .get("$type")
        .and_then(|t| t.as_str())
        .ok_or_else(|| {
            CodecError::Malformed("Cannot convert a node: missing string $type".into())
        })?;
    let what = format!(
        "node {}",
        node.get("$id").and_then(|i| i.as_str()).unwrap_or(type_uri)
    );
    convert_fields(node, props_to_convert(schema, type_uri, &what)?, schema, convert)
}

/// Serialize a node to normalized JSON, as [`serialize`] does, writing each
/// integer and decimal value in its JSON form (kanonak-protocol 4.11.0's
/// `numbers-in-json`, runtime#54): a wide integer (`integer`, its four
/// sign-restricted forms, `long`, `unsignedLong`) and a decimal as a string of
/// its canonical digits, a narrow integer (`int` and narrower) as a number.
/// Takes a wide integer as a string of digits, an `i64` or `u64` number, or a
/// whole number within ±(2^53 − 1); a decimal as a string of digits; a narrow
/// integer as a whole number. Anything else, or a value outside its datatype's
/// range, is refused with an error ending `[json-number-form]`.
#[allow(deprecated)]
pub fn serialize_exact(node: &Node, schema: &Json) -> Result<Node, CodecError> {
    serialize(&convert_node(node, schema, write_json_number)?)
}

/// Parse normalized JSON into a node, as [`deserialize`] does, reading each
/// integer and decimal value exactly (kanonak-protocol 4.11.0's
/// `numbers-in-json`, runtime#54): a wide integer from a string, or from a
/// whole JSON number within ±(2^53 − 1); a decimal from a string only; a
/// narrow integer from a whole number only. The node holds `integer`, its four
/// sign-restricted forms and `decimal` as a string of canonical digits, and
/// `long`, `unsignedLong` and the narrow integers as a number. A value JSON may
/// already have rounded, or one outside its datatype's range, is refused with
/// an error ending `[json-number-form]`. It judges the `Value`s it's given:
/// without serde_json's `arbitrary_precision`, `1.0000000000000000001` is
/// already the `f64` `1.0`, so a caller that must refuse it parses with it.
#[allow(deprecated)]
pub fn deserialize_exact(json_obj: &Node, schema: &Json) -> Result<Node, CodecError> {
    convert_node(&deserialize(json_obj, schema)?, schema, read_json_number)
}
