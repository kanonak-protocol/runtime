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
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any, Dict, List, Optional

from kanonak_canonical import (
    Embedded,
    KList,
    Package,
    RawScalar,
    Reference,
    Statement,
    Subject,
    TypedScalar,
    canonical_form as _canonical_form,
    canonical_hash as _canonical_hash,
    carrier_of,
)

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


def _lexical(value: Any) -> str:
    """The raw lexical token of a scalar — the input the canonical form normalizes."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return value
    return str(value)


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
    cls = schema["classes"].get(cls_uri)
    if cls is None:
        raise ValueError(f"no schema for embedded type {cls_uri}")

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
            statements.append(Statement(key, RawScalar(_lexical(raw))))
            continue
        if isinstance(raw, list):
            # An empty list contributes NO statement — absent and empty are
            # identical at the canonical layer (the wire serialize still
            # preserves the empty list).
            if not raw:
                continue
            statements.append(Statement(prop["predicate"], KList([_value(prop, x, schema) for x in raw])))
        else:
            statements.append(Statement(prop["predicate"], _value(prop, raw, schema)))

    extra = source.get("$extra")
    if extra:
        for predicate, raw in extra.items():
            if raw is None:
                continue
            statements.append(Statement(predicate, RawScalar(_lexical(raw))))
    return statements


def _statements(node: Dict[str, Any], schema: Dict[str, Any]) -> List[Statement]:
    types = _validated_types(node, f"Node {node.get('$id', '(no $id)')}")
    type_uri = node.get("$type")
    if not type_uri:
        raise ValueError("node is missing $type")
    cls = schema["classes"].get(type_uri)
    if cls is None:
        raise ValueError(f"no schema for type {type_uri}")

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
    """Serialize a typed node to its normalized-JSON wire form. ``$extra`` entries
    ride as sibling fields after the modeled ones; a modeled field wins a name
    collision (``[JsonExtensionData]`` semantics)."""
    # Producer-side $types validation, at every depth — fail closest to the bug.
    _assert_types_envelopes(node, f"serialize {node.get('$id') or node.get('$type') or '(node)'}")
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
    """Parse normalized JSON into a typed node. ``$``-envelope keys and fields
    modeled on the node's ``$type`` stay top-level; every other key is collected
    into ``$extra`` so a strongly-typed consumer round-trips it losslessly."""
    type_uri = json_obj.get("$type")
    if not isinstance(type_uri, str):
        raise ValueError("Cannot deserialize: missing string $type")
    # Reader-side $types validation, at every depth: an unsorted / singleton /
    # duplicate / non-member set is REJECTED, never silently repaired —
    # determinism belongs to the producer, and a lenient reader would mask a
    # nondeterministic emitter.
    _assert_types_envelopes(json_obj, f"deserialize {json_obj.get('$id') or type_uri}")
    cls = schema["classes"].get(type_uri)
    if cls is None:
        raise ValueError(f"Cannot deserialize: no schema for type {type_uri}")

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
    return deserialize(_wire_dict(typed), schema)


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
