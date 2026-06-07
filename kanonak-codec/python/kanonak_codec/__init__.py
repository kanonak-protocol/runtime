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
"""

from __future__ import annotations

from typing import Any, Dict, List

from kanonak_canonical import (
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

ENVELOPE_KEYS = {"$type", "$id", "$contentHash", "$version", "$extra"}


def _lexical(value: Any) -> str:
    """The raw lexical token of a scalar — the input the canonical form normalizes."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return value
    return str(value)


def _value(prop: Dict[str, Any], raw: Any):
    if prop["kind"] == "object":
        if isinstance(raw, dict) and "$ref" in raw:
            return Reference(raw["$ref"])
        raise ValueError(
            "Embedded object values are not yet supported by the codec runtime; "
            "pass a reference ({'$ref': ...})."
        )
    carrier = carrier_of(prop["datatype"])
    if carrier is None:
        return RawScalar(_lexical(raw))
    return TypedScalar(carrier, _lexical(raw))


def _statements(node: Dict[str, Any], schema: Dict[str, Any]) -> List[Statement]:
    type_uri = node.get("$type")
    if not type_uri:
        raise ValueError("node is missing $type")
    cls = schema["classes"].get(type_uri)
    if cls is None:
        raise ValueError(f"no schema for type {type_uri}")

    statements: List[Statement] = [Statement(schema["typePredicate"], Reference(type_uri))]
    for key, raw in node.items():
        if key in ENVELOPE_KEYS or raw is None:
            continue
        prop = cls["props"].get(key)
        if prop is None:
            statements.append(Statement(key, RawScalar(_lexical(raw))))
            continue
        if isinstance(raw, list):
            statements.append(Statement(prop["predicate"], KList([_value(prop, x) for x in raw])))
        else:
            statements.append(Statement(prop["predicate"], _value(prop, raw)))

    extra = node.get("$extra")
    if extra:
        for predicate, raw in extra.items():
            if raw is None:
                continue
            statements.append(Statement(predicate, RawScalar(_lexical(raw))))
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
