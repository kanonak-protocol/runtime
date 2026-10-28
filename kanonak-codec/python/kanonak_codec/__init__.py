"""kanonak-codec — the generic, ontology-independent codec runtime (Python port).

Given a ``CodecSchema`` (the per-package metadata a generated SDK embeds) and a
set of typed nodes, it builds the canonical input model and content-addresses it
via ``kanonak-canonical`` (the same content-form the TypeScript reference and the
``kanonak hash`` CLI produce). It also (de)serializes the normalized-JSON wire
form. Self-contained: carriers come from the schema's datatype URIs, and the
resolved foundation URIs are embedded by the generator, so hashing needs no
runtime ontology resolution.

A node is a plain ``dict`` (the ``$``-envelope plus alias-collapsed local-name
fields); a generated Pydantic model serializes to one via ``model_dump``.

The typed SDK-facing surface (0.3.0) — ``to_node``, ``ref``, ``embed`` — binds
a generated model to that same node contract, duck-typed so the runtime stays
stdlib-only: anything with ``model_dump`` (a Pydantic model) or any mapping
dumps to its normalized-JSON wire form and maps onto the node contract through
the SAME split ``deserialize`` defines. An object property's value is EXACTLY
ONE of a reference to a named resource (``ref``) or an embedded node
(``embed``); the choice between the arms is authorial and HASH-RELEVANT, so it
is explicit here, never inferred. An embedded value's authored dict-key rides
``$name`` and is likewise hash-relevant.

Reading data typed at an earlier COMPATIBLE version of the schema's own package
(0.6.1, runtime#28): ``deserialize``, ``type_matches`` and ``enum_member``
resolve a type or member URI through the exact versioned key first, then the
same publisher/package/name at the highest schema version that can read the
written one (``is_readable_by`` from ``kanonak-canonical``). Hashing stays
exact-version only. Every rejection message ends in a bracketed kind:
``[unknown-type]``, ``[newer-version]``, ``[other-major]``,
``[other-minor-line]``, ``[hash-needs-exact-version]``.

``serialize_exact`` and ``deserialize_exact`` (runtime#54) write and read each
integer and decimal value in its JSON form, as kanonak-protocol 4.11.0's
``numbers-in-json`` gives it, refusing what JSON may already have rounded
(``[json-number-form]``). They replace ``serialize`` and ``deserialize``, which
are deprecated.
"""

from __future__ import annotations

import warnings
from collections.abc import Mapping
from dataclasses import dataclass
from decimal import Decimal
from enum import Enum
from typing import Any, Callable, Dict, List, Optional

from kanonak_canonical import (
    Coordinate,
    CoordinateVersion,
    Embedded,
    KList,
    Package,
    RawScalar,
    Reference,
    Statement,
    Subject,
    TypedScalar,
    canonical_decimal,
    canonical_form as _canonical_form,
    canonical_hash as _canonical_hash,
    canonical_integer,
    carrier_of,
    is_readable_by,
    lenient_versionless_key,
    parse_coordinate,
)

# The public API — what a 1.x release keeps compatible (runtime#29). Everything
# else here is an implementation detail and may change in any release.
__all__ = [
    "EnumMemberMatch",
    "build_package",
    "canonical_form",
    "content_hash",
    "deserialize",
    "deserialize_exact",
    "embed",
    "enum_member",
    "ref",
    "serialize",
    "serialize_exact",
    "to_node",
    "type_matches",
]

# The reserved ``$``-envelope keys, which never become statements/predicates.
# ``$name`` (0.2.0) carries an embedded value's authored dict-key — hash-relevant.
# ``$types`` (0.4.0, runtime#10) carries a multi-typed node's FULL type set.
ENVELOPE_KEYS = {"$type", "$types", "$id", "$name", "$contentHash", "$version", "$extra"}


def _compare_utf8(a: str, b: str) -> int:
    """Lexicographic comparison by UTF-8 byte sequence (== code-point order)."""
    ab, bb = a.encode("utf-8"), b.encode("utf-8")
    return (ab > bb) - (ab < bb)


def _validated_types(mapping: Mapping, where: str) -> Optional[List[str]]:
    """Validate a node-or-embedded's ``$types`` envelope (0.4.0, runtime#10) and
    return the validated set, or ``None`` when the node is single-typed.
    Invariants: sorted by UTF-8 bytes, at least two members, no duplicates, and
    ``$type`` (the dispatch key, chosen by the schema layer's primary rule) is a
    member. Enforced wherever the envelope is touched — serialize, deserialize,
    and canonicalization — so a producer fails at emit time and a reader never
    masks a nondeterministic emitter by silently repairing the set."""
    raw = mapping.get("$types")
    if raw is None:
        return None
    if not isinstance(raw, list) or any(not isinstance(m, str) or not m for m in raw):
        raise ValueError(f"{where}: $types must be a list of non-empty type URIs")
    if len(raw) < 2:
        raise ValueError(
            f"{where}: $types with {len(raw)} member(s) is forbidden — a "
            "single-typed node carries only $type (a second encoding of the "
            "same content would be hash-ambiguous)"
        )
    for prev, cur in zip(raw, raw[1:]):
        cmp = _compare_utf8(prev, cur)
        if cmp == 0:
            raise ValueError(f"{where}: $types carries duplicate member {cur}")
        if cmp > 0:
            raise ValueError(
                f"{where}: $types is not sorted by UTF-8 bytes ({prev} sorts "
                f"after {cur}) — ordering is the producer's job, never the reader's"
            )
    primary = mapping.get("$type")
    if not isinstance(primary, str) or primary not in raw:
        raise ValueError(
            f"{where}: $type ({primary!r}) must be present and a member of $types"
        )
    return raw


def _assert_types_envelopes(value: Any, where: str) -> None:
    """Recursively validate every ``$types`` envelope in a wire value (the node
    itself and any embedded node at any depth). Shared by ``serialize`` (the
    producer throws at emit time) and ``deserialize`` (the strict reader rejects
    rather than repairs)."""
    if isinstance(value, list):
        for i, item in enumerate(value):
            _assert_types_envelopes(item, f"{where}[{i}]")
        return
    if isinstance(value, dict):
        if "$types" in value:
            _validated_types(value, where)
        for key, item in value.items():
            if key != "$types":
                _assert_types_envelopes(item, f"{where}.{key}")


def _assert_reference_values(value: Any, where: str) -> None:
    """Every reference value (``{"$ref": uri}``) in a wire value, at any depth,
    must address a NAMED resource: ``publisher/package[@version]/name``, no URI
    fragment (runtime#6). Fragments address embedded resources for navigation
    and deep-linking; they are not part of the reference graph, and accepting
    one here would establish that capability by accident. Shared by
    ``serialize``, ``deserialize`` and canonicalization, so all three reject the
    same input."""
    if isinstance(value, list):
        for i, item in enumerate(value):
            _assert_reference_values(item, f"{where}[{i}]")
        return
    if isinstance(value, dict):
        uri = value.get("$ref")
        if isinstance(uri, str):
            _assert_named_reference(uri, where)
        for key, item in value.items():
            _assert_reference_values(item, f"{where}.{key}")


def _assert_named_reference(uri: str, where: str) -> None:
    if "#" in uri:
        raise ValueError(
            f"{where}: reference value '{uri}' contains a URI fragment. A reference addresses a named "
            "resource (publisher/package[@version]/name); fragments are a navigation convention, not "
            "part of the reference graph [fragment-reference]"
        )


def _lexical(value: Any) -> str:
    """The raw lexical token of a scalar — the input the canonical form normalizes."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return value
    return str(value)


# ---------------------------------------------------------------------------
# Compatible-version lookup (0.6.1, runtime#28)
# ---------------------------------------------------------------------------
#
# A schema class may carry an optional ``ancestors`` list: every superclass of
# the class, transitively, by durable VERSIONED URI. It is what lets
# ``type_matches`` accept a subclass node for a superclass check. Additive and
# canonicalization-INERT: a schema without it is still valid, and absent, a
# type match is by the class itself only — stricter, never looser.


class _Unreadable(Exception):
    """``_class_for``'s miss: the message ends in the bracketed kind."""


def _coordinate_of(uri: Any) -> Optional[Coordinate]:
    """The parsed coordinate (STRICT grammar), or None when ``uri`` is not one."""
    if not isinstance(uri, str):
        return None
    try:
        return parse_coordinate(uri)
    except ValueError:
        return None


def _key_of(c: Coordinate) -> str:
    return f"{c.publisher}/{c.package}/{c.name}"


def _version_tuple(v: CoordinateVersion) -> "tuple[int, int, int]":
    return (v.major, v.minor, v.patch)


def _format_version(v: CoordinateVersion) -> str:
    return f"{v.major}.{v.minor}.{v.patch}"


def _incompatible_version(type_uri: str, w: CoordinateVersion, r: CoordinateVersion) -> str:
    ws, rs = _format_version(w), _format_version(r)
    if w.major != r.major:
        return (
            f"{type_uri} is written at {ws}, a different major version than this "
            f"codec's schema ({rs}) [other-major]"
        )
    if w.major == 0 and w.minor != r.minor:
        return (
            f"{type_uri} is written at {ws}; below 1.0.0 a different minor is a "
            f"different version line than this codec's schema ({rs}) [other-minor-line]"
        )
    return (
        f"{type_uri} is written at {ws}, newer than this codec's schema ({rs}); "
        "upgrade the codec to read it [newer-version]"
    )


def _class_for(schema: Dict[str, Any], type_uri: Any) -> Dict[str, Any]:
    """THE class lookup (runtime#28). A node is typed with the version of the
    class its producer's import resolved to; a codec generated from a later
    COMPATIBLE version of the same package must still read it. So: the exact
    versioned key first, then the class with the same publisher, package and
    name whose version can read the written one — ``is_readable_by`` from
    ``kanonak-canonical``, the protocol's single compatibility rule. When
    nothing matches, raises ``_Unreadable`` whose message ends in a bracketed
    kind: ``[unknown-type]``, ``[newer-version]`` (the data may use terms this
    schema lacks), ``[other-major]``, or ``[other-minor-line]`` (below 1.0.0
    the minor is the incompatible line)."""
    classes = schema["classes"]
    if isinstance(type_uri, str):
        exact = classes.get(type_uri)
        if exact is not None:
            return exact
    written = _coordinate_of(type_uri)
    if written is None or written.version is None:
        raise _Unreadable(f"no schema for type {type_uri} [unknown-type]")
    key = _key_of(written)

    readable: Optional[Dict[str, Any]] = None
    readable_version: Optional[CoordinateVersion] = None
    nearest: Optional[CoordinateVersion] = None
    for uri, cls in classes.items():
        c = _coordinate_of(uri)
        if c is None or c.version is None or _key_of(c) != key:
            continue
        if is_readable_by(written.version, c.version):
            if readable_version is None or _version_tuple(c.version) > _version_tuple(readable_version):
                readable, readable_version = cls, c.version
        elif nearest is None or _version_tuple(c.version) > _version_tuple(nearest):
            nearest = c.version
    if readable is not None:
        return readable
    if nearest is None:
        raise _Unreadable(f"no schema for type {type_uri} [unknown-type]")
    raise _Unreadable(_incompatible_version(type_uri, written.version, nearest))


def _hash_class_for(schema: Dict[str, Any], type_uri: str, what: str) -> Dict[str, Any]:
    """The class to HASH a node or embedded value with: the exact versioned
    class only. A content hash is computed over the producer's predicate and
    type URIs, versions included, so a node written at an earlier compatible
    version cannot be re-hashed with a later schema — its predicates would
    carry the later version and the hash would differ. When only a compatible
    class exists, say so (``[hash-needs-exact-version]``) rather than produce a
    different hash or the bare "no schema" error."""
    exact = schema["classes"].get(type_uri)
    if exact is not None:
        return exact
    try:
        cls = _class_for(schema, type_uri)
    except _Unreadable:
        raise ValueError(f"No schema for {what} {type_uri} [unknown-type]") from None
    raise ValueError(
        f"Cannot hash {what} {type_uri}: this codec's schema has it at {cls['typeUri']}, "
        "and a content hash is computed with the producer's schema version — "
        "hashing needs the exact class [hash-needs-exact-version]"
    )


def type_matches(node: Mapping, class_uri: str, schema: Dict[str, Any]) -> bool:
    """Whether a node is an instance of the class ``class_uri`` — what a
    generated type guard asks (runtime#28). Each of the node's types
    (``$types``, else its ``$type``) is resolved through the same compatible
    lookup ``deserialize`` uses, so a node written at an earlier compatible
    version satisfies the later version's guard; it matches when that class IS
    ``class_uri`` or has it among its ``ancestors``. Classes compare by
    versionless identity. A type this schema cannot read (unknown, newer,
    another major) never matches. Raises ``ValueError`` when ``class_uri`` is
    not a class coordinate."""
    target = _coordinate_of(class_uri)
    if target is None:
        raise ValueError(f"type_matches: {class_uri!r} is not a class coordinate")
    target_key = _key_of(target)

    def matches(uri: Any) -> bool:
        c = _coordinate_of(uri)
        return c is not None and _key_of(c) == target_key

    members = node.get("$types")
    if members is None:
        primary = node.get("$type")
        members = [primary] if primary else []
    for member in members:
        try:
            cls = _class_for(schema, member)
        except _Unreadable:
            continue
        if matches(cls["typeUri"]):
            return True
        if any(matches(a) for a in cls.get("ancestors") or []):
            return True
    return False


@dataclass(frozen=True)
class EnumMemberMatch:
    """An enumeration member found by ``enum_member``."""

    enum_type: str
    """The enumeration class's durable URI (its key in ``schema["enums"]``)."""
    uri: str
    """The member's durable URI as this schema keys it."""
    member: Dict[str, Any]
    """The member entry (``{"label": ...}``)."""


def enum_member(schema: Dict[str, Any], ref: str) -> Optional[EnumMemberMatch]:
    """The enumeration member a ``{"$ref": ...}`` names (runtime#28): the exact
    versioned key first, then the same member at a version that can read the
    written one — so a member referenced at an earlier compatible version of
    the package resolves against a later schema. None when this schema has no
    such member, which, as for ``enums`` itself, means "not mine", never
    "invalid"."""
    enums = schema.get("enums") or {}
    for enum_type, en in enums.items():
        member = en["members"].get(ref)
        if member is not None:
            return EnumMemberMatch(enum_type, ref, member)
    written = _coordinate_of(ref)
    if written is None or written.version is None:
        return None
    key = _key_of(written)
    best: Optional[EnumMemberMatch] = None
    best_version: Optional[CoordinateVersion] = None
    for enum_type, en in enums.items():
        for uri, member in en["members"].items():
            c = _coordinate_of(uri)
            if c is None or c.version is None or _key_of(c) != key:
                continue
            if not is_readable_by(written.version, c.version):
                continue
            if best_version is None or _version_tuple(c.version) > _version_tuple(best_version):
                best, best_version = EnumMemberMatch(enum_type, uri, member), c.version
    return best


def _value(prop: Dict[str, Any], raw: Any, schema: Dict[str, Any]):
    if prop["kind"] == "object":
        # A node: a reference (``{"$ref"}``) or an embedded resource.
        if isinstance(raw, dict):
            if "$ref" in raw:
                return Reference(raw["$ref"])
            return _embedded_value(prop, raw, schema)
        raise ValueError(
            f"Object property {prop['predicate']} expects a reference "
            "({'$ref': ...}) or an embedded node (a dict), got "
            f"{type(raw).__name__}"
        )
    carrier = carrier_of(prop["datatype"])
    if carrier is None:
        return RawScalar(_lexical(raw))
    return TypedScalar(carrier, _lexical(raw))


def _embedded_value(prop: Dict[str, Any], mapping: Dict[str, Any], schema: Dict[str, Any]) -> Embedded:
    """Canonicalize an embedded value (0.2.0): a dict with no ``$id``, an optional
    ``$name`` (the authored dict-key — hash-relevant), an optional ``$type``, and
    schema-mapped fields. An explicit ``$type`` emits a type statement inside the
    embedded (hash-relevant even when it equals the range-derived type); without
    it, fields map via the containing property's ``range`` and NO type statement
    is emitted — range-derived typing is inference only."""
    if "$id" in mapping:
        raise ValueError(
            f"An embedded value under {prop['predicate']} must not carry $id — "
            "to point at a named resource, pass a reference ({'$ref': ...})."
        )
    types = _validated_types(mapping, f"Embedded value under {prop['predicate']}")
    explicit_type = mapping.get("$type") if isinstance(mapping.get("$type"), str) else None
    cls_uri = explicit_type if explicit_type is not None else prop.get("range")
    if cls_uri is None:
        raise ValueError(
            f"Cannot map embedded value under {prop['predicate']}: it carries "
            "no $type and the property declares no range."
        )
    cls = _hash_class_for(schema, cls_uri, "embedded type")

    statements = _field_statements(mapping, cls, schema)
    if types is not None:
        # A multi-typed embedded ($types implies an explicit $type): one type
        # statement per member, in $types (UTF-8 sorted) order — all hash-relevant.
        for member in types:
            statements.append(Statement(schema["typePredicate"], Reference(member)))
    elif explicit_type is not None:
        statements.append(Statement(schema["typePredicate"], Reference(explicit_type)))
    name = mapping.get("$name")
    name = name if isinstance(name, str) and name else None
    return Embedded(name, statements)


def _field_statements(source: Dict[str, Any], cls: Dict[str, Any], schema: Dict[str, Any]) -> List[Statement]:
    """The statements for one node-or-embedded's modeled fields + its ``$extra`` —
    everything except the type triple (subjects always carry one; embeddeds only
    when explicitly typed)."""
    statements: List[Statement] = []
    for key, raw in source.items():
        if key in ENVELOPE_KEYS or raw is None:
            continue
        prop = cls["props"].get(key)
        if prop is None:
            raise _undeclared_statement(cls, f'"{key}"')
        if isinstance(raw, list):
            # An empty list contributes NO statement — absent and empty are
            # identical at the canonical layer (the wire serialize still
            # preserves the empty list).
            if not raw:
                continue
            statements.append(Statement(prop["predicate"], KList([_value(prop, x, schema) for x in raw])))
        else:
            statements.append(Statement(prop["predicate"], _value(prop, raw, schema)))

    # Open-world extras ride through serialize and deserialize, but this codec
    # can't type them, so it refuses to hash them rather than hash them as text.
    extra = source.get("$extra")
    if extra:
        for predicate, raw in extra.items():
            if raw is None:
                continue
            raise _undeclared_statement(cls, f"$extra statement {predicate}")
    return statements


def _undeclared_statement(cls: Dict[str, Any], what: str) -> ValueError:
    """A codec holds one package's schema, so it can't know the range of a
    predicate another package declares on its classes (open-world augmentation).
    Hashing such a statement as untyped text would give a hash that differs from
    ``kanonak hash``'s without a word, so hashing refuses it, naming it
    (``[hash-undeclared-statement]``). Serializing and deserializing carry it."""
    return ValueError(
        f"Cannot hash {cls['typeUri']}'s {what}: this codec's schema doesn't declare it, so it can't type its value. "
        "Hash it with the SDK, over the graph [hash-undeclared-statement]"
    )


def _statements(node: Dict[str, Any], schema: Dict[str, Any]) -> List[Statement]:
    types = _validated_types(node, f"Node {node.get('$id', '(no $id)')}")
    _assert_reference_values(node, f"Node {node.get('$id', '(no $id)')}")
    type_uri = node.get("$type")
    if not type_uri:
        raise ValueError("node is missing $type")
    cls = _hash_class_for(schema, type_uri, "type")

    # The rdf:type triple(s) every subject carries: one per $types member for a
    # multi-typed node (in $types' UTF-8 sorted order), else the single $type.
    statements: List[Statement] = [
        Statement(schema["typePredicate"], Reference(member))
        for member in (types if types is not None else [type_uri])
    ]
    statements.extend(_field_statements(node, cls, schema))
    return statements


def build_package(nodes: List[Dict[str, Any]], schema: Dict[str, Any], pkg: Dict[str, Any]) -> Package:
    """Build the canonical input model: a subject per node + the synthesized
    package-wrapper subject (raw label + ``Package`` type), exactly the subject
    set ``kanonak hash`` produces for the equivalent authored package."""
    subjects: List[Subject] = []
    for node in nodes:
        if not node.get("$id"):
            raise ValueError("node is missing $id")
        subjects.append(Subject(node["$id"], _statements(node, schema)))

    pkg_uri = f"{pkg['publisher']}/{pkg['packageName']}@{pkg['version']}/{pkg['packageName']}"
    pkg_statements: List[Statement] = []
    if pkg.get("label") is not None:
        pkg_statements.append(Statement(schema["labelPredicate"], RawScalar(pkg["label"])))
    pkg_statements.append(Statement(schema["typePredicate"], Reference(schema["packageTypeUri"])))
    subjects.append(Subject(pkg_uri, pkg_statements))
    return Package(subjects)


def canonical_form(nodes: List[Dict[str, Any]], schema: Dict[str, Any], pkg: Dict[str, Any]) -> str:
    """The canonical form (the ``{subjects:[...]}`` JSON) of a package from nodes."""
    return _canonical_form(build_package(nodes, schema, pkg))


def content_hash(nodes: List[Dict[str, Any]], schema: Dict[str, Any], pkg: Dict[str, Any]) -> str:
    """The ``sha256:`` content hash of a package from nodes — matches ``kanonak hash``."""
    return _canonical_hash(build_package(nodes, schema, pkg))


def serialize(node: Dict[str, Any]) -> Dict[str, Any]:
    """Deprecated: use ``serialize_exact``, which writes integer and decimal
    values in their JSON forms (runtime#54).

    Serialize a typed node to its normalized-JSON wire form. ``$extra`` entries
    ride as sibling fields after the modeled ones; a modeled field wins a name
    collision (``[JsonExtensionData]`` semantics)."""
    warnings.warn(
        "serialize is deprecated: use serialize_exact, which writes integer and "
        "decimal values in their JSON forms (runtime#54)",
        DeprecationWarning,
        stacklevel=2,
    )
    return _serialize(node)


def _serialize(node: Dict[str, Any]) -> Dict[str, Any]:
    # Producer-side $types validation, at every depth — fail closest to the bug.
    _assert_types_envelopes(node, f"serialize {node.get('$id') or node.get('$type') or '(node)'}")
    _assert_reference_values(node, f"serialize {node.get('$id') or node.get('$type') or '(node)'}")
    out: Dict[str, Any] = {}
    for key, value in node.items():
        if key == "$extra" or value is None:
            continue
        out[key] = value
    extra = node.get("$extra")
    if extra:
        for key, value in extra.items():
            if value is not None and key not in out:
                out[key] = value
    return out


def deserialize(json_obj: Dict[str, Any], schema: Dict[str, Any]) -> Dict[str, Any]:
    """Deprecated: use ``deserialize_exact``, which reads integer and decimal
    values exactly (runtime#54).

    Parse normalized JSON into a typed node. ``$``-envelope keys and fields
    modeled on the node's ``$type`` stay top-level; every other key is collected
    into ``$extra`` so a strongly-typed consumer round-trips it losslessly.

    Requires a class in the schema that can read ``$type``: the exact class, or
    the same class at a compatible later version (runtime#28 — see
    ``_class_for``). The node keeps the ``$type`` it was WRITTEN with; the
    producer's bytes are the producer's."""
    warnings.warn(
        "deserialize is deprecated: use deserialize_exact, which reads integer "
        "and decimal values exactly (runtime#54)",
        DeprecationWarning,
        stacklevel=2,
    )
    return _deserialize(json_obj, schema)


def _deserialize(json_obj: Dict[str, Any], schema: Dict[str, Any]) -> Dict[str, Any]:
    type_uri = json_obj.get("$type")
    if not isinstance(type_uri, str):
        raise ValueError("Cannot deserialize: missing string $type")
    # Reader-side $types validation, at every depth: an unsorted / singleton /
    # duplicate / non-member set is REJECTED, never silently repaired —
    # determinism belongs to the producer, and a lenient reader would mask a
    # nondeterministic emitter.
    _assert_types_envelopes(json_obj, f"deserialize {json_obj.get('$id') or type_uri}")
    _assert_reference_values(json_obj, f"deserialize {json_obj.get('$id') or type_uri}")
    try:
        cls = _class_for(schema, type_uri)
    except _Unreadable as err:
        raise ValueError(f"Cannot deserialize: {err}") from None

    node: Dict[str, Any] = {"$type": type_uri}
    extra: Dict[str, Any] = {}
    for key, value in json_obj.items():
        if key == "$type":
            continue
        if key.startswith("$") or key in cls["props"]:
            node[key] = value
        else:
            extra[key] = value
    if extra:
        node["$extra"] = extra
    return node


def _wire_dict(typed: Any) -> Dict[str, Any]:
    """The normalized-JSON wire dict of a typed value: a generated Pydantic
    model dumps via ``model_dump(by_alias=True, exclude_none=True)`` (aliases
    are the wire names; absence is the data model's optionality — nulls never
    ride the wire), and any mapping passes through as a plain ``dict``."""
    dump = getattr(typed, "model_dump", None)
    if callable(dump):
        return dump(by_alias=True, exclude_none=True)
    if isinstance(typed, Mapping):
        return dict(typed)
    raise TypeError(
        "Expected a typed model (with model_dump) or a mapping, got "
        f"{type(typed).__name__}"
    )


def to_node(typed: Any, schema: Dict[str, Any]) -> Dict[str, Any]:
    """A typed instance's codec node (the dictionary contract). The bridge is
    native serde, not reflection: the instance dumps to its normalized-JSON
    wire form (envelope-as-data + ``ref``/``embed`` values), and the wire form
    maps onto the node contract through the SAME split ``deserialize`` defines
    — the typed path and the dictionary path are one contract, not two."""
    return _deserialize(_wire_dict(typed), schema)


def ref(target: Any) -> Dict[str, Any]:
    """The reference arm of an object property: ``{"$ref": uri}`` — a
    reference to a named resource by its canonical URI (a ``str``) or by the
    instance itself (resolved through its ``$id``). The target must already
    carry its identity; an embedded (id-less) value cannot be referenced."""
    if isinstance(target, str):
        if not target:
            raise ValueError("A reference needs a canonical URI.")
        return {"$ref": target}
    target_id = _wire_dict(target).get("$id")
    if not isinstance(target_id, str) or not target_id:
        raise ValueError(
            "ref(target) requires a resource with a non-empty $id — "
            "to carry the value inline instead, use embed(...)."
        )
    return {"$ref": target_id}


def embed(value: Any, name: Optional[str] = None) -> Dict[str, Any]:
    """The embedded arm of an object property: the value itself, carried
    inline (derived identity, no ``$id``). ``name`` is the authored dict-key
    and rides ``$name`` — HASH-RELEVANT. An explicit ``$type`` on the value
    also rides through (it emits a type statement inside the embedded)."""
    wire = _wire_dict(value)
    if "$id" in wire:
        raise ValueError(
            "An embedded value must not carry $id — to point at the named "
            "resource instead, use ref(...)."
        )
    if name is not None:
        wire["$name"] = name
    return wire


# ---------------------------------------------------------------------------
# The JSON forms of integer and decimal values (kanonak-protocol 4.11.0's
# ``numbers-in-json``, runtime#54). A JSON number can't carry every integer or
# decimal exactly in every language, so a wide integer and a decimal travel as
# a string of their digits, and a reader refuses a number that may already have
# been rounded.
# ---------------------------------------------------------------------------

_XSD = "kanonak.org/core-xsd"

# Past it a double can't tell a whole number from its neighbour: 2^53 + 1 reads as 2^53.
_MAX_SAFE_INTEGER = 2**53 - 1


class _Form(Enum):
    WIDE = "wide"
    DECIMAL = "decimal"
    NARROW = "narrow"


@dataclass(frozen=True)
class _JsonNumberForm:
    """How JSON carries a datatype's values: a wide integer as a string, read
    from a string or a whole number within ±(2^53 − 1); a decimal as a string
    only; a narrow integer as a number only. Each inside its datatype's range."""

    form: _Form
    minimum: Optional[int] = None
    maximum: Optional[int] = None


_JSON_NUMBER_FORMS: Dict[str, _JsonNumberForm] = {
    f"{_XSD}/integer": _JsonNumberForm(_Form.WIDE),
    f"{_XSD}/nonNegativeInteger": _JsonNumberForm(_Form.WIDE, minimum=0),
    f"{_XSD}/positiveInteger": _JsonNumberForm(_Form.WIDE, minimum=1),
    f"{_XSD}/nonPositiveInteger": _JsonNumberForm(_Form.WIDE, maximum=0),
    f"{_XSD}/negativeInteger": _JsonNumberForm(_Form.WIDE, maximum=-1),
    f"{_XSD}/long": _JsonNumberForm(_Form.WIDE, -(2**63), 2**63 - 1),
    f"{_XSD}/unsignedLong": _JsonNumberForm(_Form.WIDE, 0, 2**64 - 1),
    f"{_XSD}/decimal": _JsonNumberForm(_Form.DECIMAL),
    f"{_XSD}/int": _JsonNumberForm(_Form.NARROW, -(2**31), 2**31 - 1),
    f"{_XSD}/short": _JsonNumberForm(_Form.NARROW, -(2**15), 2**15 - 1),
    f"{_XSD}/byte": _JsonNumberForm(_Form.NARROW, -(2**7), 2**7 - 1),
    f"{_XSD}/unsignedInt": _JsonNumberForm(_Form.NARROW, 0, 2**32 - 1),
    f"{_XSD}/unsignedShort": _JsonNumberForm(_Form.NARROW, 0, 2**16 - 1),
    f"{_XSD}/unsignedByte": _JsonNumberForm(_Form.NARROW, 0, 2**8 - 1),
}

_NumberConversion = Callable[[Dict[str, Any], _JsonNumberForm, Any], Any]


def _json_number_error(prop: Dict[str, Any], why: str) -> ValueError:
    return ValueError(f"{prop['predicate']}: {why} [json-number-form]")


def _describe_json(value: Any) -> str:
    if isinstance(value, str):
        return f"the string {value!r}"
    return f"{type(value).__name__} {value!r}"


def _in_range(prop: Dict[str, Any], form: _JsonNumberForm, n: int) -> int:
    if (form.minimum is not None and n < form.minimum) or (form.maximum is not None and n > form.maximum):
        raise _json_number_error(prop, f"{n} is outside {prop['datatype']}'s range")
    return n


def _integer_of_lexical(prop: Dict[str, Any], lexical: str) -> int:
    try:
        canonical = canonical_integer(lexical)
    except ValueError:
        raise _json_number_error(prop, f"{lexical!r} is not an integer lexical form") from None
    return int(canonical)


def _decimal_of_lexical(prop: Dict[str, Any], lexical: str) -> str:
    try:
        return canonical_decimal(lexical)
    except ValueError:
        raise _json_number_error(prop, f"{lexical!r} is not a decimal lexical form") from None


def _whole_number(value: Any) -> Optional[int]:
    """An ``int``, or a ``float`` or ``Decimal`` holding a whole number, as an
    ``int``; None for anything else. A ``bool`` is an ``int`` in Python, and never
    a number here. A ``Decimal``, as ``json.loads(text, parse_float=Decimal)``
    gives, is judged exactly, so ``1.0000000000000000001`` isn't taken for 1."""
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, float) and value.is_integer():
        return int(value)
    if isinstance(value, Decimal) and value.is_finite() and value == value.to_integral_value():
        return int(value)
    return None


def _safe_integer(value: Any) -> Optional[int]:
    """A whole number within ±(2^53 − 1), which no JSON reader can have rounded;
    None for anything else, however exactly Python's own reader held it."""
    n = _whole_number(value)
    return n if n is not None and abs(n) <= _MAX_SAFE_INTEGER else None


def _read_json_number(prop: Dict[str, Any], form: _JsonNumberForm, value: Any) -> Any:
    """A value as JSON carries it, to its exact type here: an integer an ``int``,
    a decimal a ``Decimal`` of its canonical digits."""
    if form.form is _Form.DECIMAL:
        if not isinstance(value, str):
            raise _json_number_error(
                prop,
                "a decimal is read from a JSON string only, so it can't have been rounded; "
                f"got {_describe_json(value)}",
            )
        return Decimal(_decimal_of_lexical(prop, value))
    if form.form is _Form.WIDE:
        n = _integer_of_lexical(prop, value) if isinstance(value, str) else _safe_integer(value)
        if n is None:
            raise _json_number_error(
                prop,
                "a wide integer is read from a string, or from a whole JSON number within "
                f"±(2^53 − 1); got {_describe_json(value)}",
            )
        return _in_range(prop, form, n)
    n = _whole_number(value)
    if n is None:
        raise _json_number_error(
            prop, f"a narrow integer is read from a whole JSON number only; got {_describe_json(value)}"
        )
    return _in_range(prop, form, n)


def _write_json_number(prop: Dict[str, Any], form: _JsonNumberForm, value: Any) -> Any:
    """A value of its exact type here, to its JSON form: a wide integer and a
    decimal as a string of canonical digits, a narrow integer as an ``int``."""
    if form.form is _Form.DECIMAL:
        if isinstance(value, Decimal):
            return _decimal_of_lexical(prop, format(value, "f"))
        if isinstance(value, str):
            return _decimal_of_lexical(prop, value)
        raise _json_number_error(
            prop,
            "pass a decimal as a decimal.Decimal or a string of its digits, which hold it exactly; "
            f"got {_describe_json(value)}",
        )
    if form.form is _Form.WIDE:
        if isinstance(value, str):
            n = _integer_of_lexical(prop, value)
        elif isinstance(value, float):
            n = _safe_integer(value)
        else:
            n = _whole_number(value)
        if n is None:
            raise _json_number_error(
                prop,
                "pass a wide integer as an int, a string, or a whole float within ±(2^53 − 1); "
                f"got {_describe_json(value)}",
            )
        return str(_in_range(prop, form, n))
    n = _whole_number(value)
    if n is None:
        raise _json_number_error(prop, f"pass a narrow integer as a whole number; got {_describe_json(value)}")
    return _in_range(prop, form, n)


def _json_number_form_of(prop: Dict[str, Any]) -> Optional[_JsonNumberForm]:
    datatype = prop.get("datatype")
    if not isinstance(datatype, str):
        return None
    key = lenient_versionless_key(datatype)
    return None if key is None else _JSON_NUMBER_FORMS.get(key)


def _class_or_raise(schema: Dict[str, Any], type_uri: Any, what: str) -> Dict[str, Any]:
    try:
        return _class_for(schema, type_uri)
    except _Unreadable as err:
        raise ValueError(f"Cannot convert {what}: {err}") from None


def _convert_fields(
    fields: Dict[str, Any], cls: Dict[str, Any], schema: Dict[str, Any], convert: _NumberConversion
) -> Dict[str, Any]:
    """Each modeled field's value, through lists and embedded values, with its
    integers and decimals converted. ``$extra``, the envelope and references
    are left as they are."""
    out = dict(fields)
    for name, prop in cls["props"].items():
        value = out.get(name)
        if value is not None:
            out[name] = _convert_value(prop, value, schema, convert)
    return out


def _convert_value(prop: Dict[str, Any], value: Any, schema: Dict[str, Any], convert: _NumberConversion) -> Any:
    if isinstance(value, list):
        return [_convert_value(prop, item, schema, convert) for item in value]
    if prop["kind"] == "object":
        if not isinstance(value, dict) or "$ref" in value:
            return value
        cls_uri = value["$type"] if isinstance(value.get("$type"), str) else prop.get("range")
        if cls_uri is None:
            raise ValueError(
                f"Cannot convert the embedded value under {prop['predicate']}: it carries "
                "no $type and the property declares no range."
            )
        cls = _class_or_raise(schema, cls_uri, f"the embedded value under {prop['predicate']}")
        return _convert_fields(value, cls, schema, convert)
    form = _json_number_form_of(prop)
    return value if form is None else convert(prop, form, value)


def serialize_exact(node: Dict[str, Any], schema: Dict[str, Any]) -> Dict[str, Any]:
    """Serialize a typed node to its normalized-JSON wire form, as ``serialize``
    does, writing each integer and decimal value in its JSON form
    (``numbers-in-json``, runtime#54): a wide integer (an ``int``, a lexical
    ``str``, or a whole ``float`` within ±(2^53 − 1)) and a decimal (a
    ``Decimal`` or a lexical ``str``) as a string of canonical digits, a narrow
    integer as an ``int``. Anything else, or a value outside its datatype's
    range, is refused with a ``ValueError`` ending ``[json-number-form]``."""
    type_uri = node.get("$type")
    if not isinstance(type_uri, str):
        raise ValueError("Cannot serialize: missing string $type")
    cls = _class_or_raise(schema, type_uri, f"node {node.get('$id') or type_uri}")
    return _serialize(_convert_fields(node, cls, schema, _write_json_number))


def deserialize_exact(json_obj: Dict[str, Any], schema: Dict[str, Any]) -> Dict[str, Any]:
    """Parse normalized JSON into a typed node, as ``deserialize`` does, reading
    each integer and decimal value exactly (``numbers-in-json``, runtime#54): a
    wide integer to an ``int``, from a string or a whole number within
    ±(2^53 − 1); a decimal to a ``Decimal``, from a string only; a narrow integer
    to an ``int``, from a whole number only. A value JSON may already have
    rounded, or one outside its datatype's range, is refused with a
    ``ValueError`` ending ``[json-number-form]``."""
    node = _deserialize(json_obj, schema)
    cls = _class_or_raise(schema, node["$type"], f"node {node.get('$id') or node['$type']}")
    return _convert_fields(node, cls, schema, _read_json_number)
