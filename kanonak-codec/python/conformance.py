"""Conformance: drive kanonak_codec with the shared codec vectors and assert the
canonical form, content hash, and normalized-JSON serialize all match the
authoritative (TypeScript-generated) expected values."""

import io
import json
import re
import os
import sys
import warnings
from decimal import Decimal

# Resolve the sibling canonical port + the shared vectors without installing.
_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_HERE, "..", "..", "kanonak-canonical", "python"))

from kanonak_codec import (  # noqa: E402
    canonical_form,
    content_hash,
    deserialize,
    deserialize_exact,
    embed,
    enum_member,
    ref,
    serialize,
    serialize_exact,
    to_node,
    type_matches,
)

VECTOR_FILES = [
    os.path.join(_HERE, "..", "vectors", "codec-vectors.json"),
    os.path.join(_HERE, "..", "vectors", "codec-vectors-embedded.json"),
]

TYPES_VECTOR_FILE = os.path.join(_HERE, "..", "vectors", "codec-vectors-types.json")
REFERENCES_VECTOR_FILE = os.path.join(_HERE, "..", "vectors", "codec-vectors-references.json")
ENUMS_VECTOR_FILE = os.path.join(_HERE, "..", "vectors", "codec-vectors-enums.json")
COMPAT_VECTOR_FILE = os.path.join(_HERE, "..", "vectors", "codec-vectors-compat.json")
JSON_NUMBERS_VECTOR_FILE = os.path.join(_HERE, "..", "vectors", "codec-vectors-json-numbers.json")


def _expect_form_and_hash(case, cid, nodes, schema, pkg, passed, failed):
    form = canonical_form(nodes, schema, pkg)
    if form != case["expectedCanonicalForm"]:
        failed += 1
        print(f"FAIL [{cid}] canonical form\n  got: {form}\n  exp: {case['expectedCanonicalForm']}")
    else:
        passed += 1

    h = content_hash(nodes, schema, pkg)
    if h != case["expectedHash"]:
        failed += 1
        print(f"FAIL [{cid}] hash\n  got: {h}\n  exp: {case['expectedHash']}")
    else:
        passed += 1
    return passed, failed


def run_file(vectors: str) -> "tuple[int, int]":
    with io.open(vectors, encoding="utf-8") as fh:
        data = json.load(fh)
    schema = data["schema"]
    passed = 0
    failed = 0

    for case in data["cases"]:
        cid = case["id"]
        nodes = case["nodes"]
        pkg = case["pkg"]

        if "expectHashError" in case:
            # Serializing still carries what hashing refuses (runtime#37).
            kind = case["expectHashError"]
            for what, run in (("canonical form", canonical_form), ("hash", content_hash)):
                try:
                    run(nodes, schema, pkg)
                    failed += 1
                    print(f"FAIL [{cid}] expected the {what} to refuse with [{kind}], it did not")
                except ValueError as err:
                    if str(err).endswith(f"[{kind}]"):
                        passed += 1
                    else:
                        failed += 1
                        print(f"FAIL [{cid}] {what} expected [{kind}], got: {err}")
        else:
            passed, failed = _expect_form_and_hash(case, cid, nodes, schema, pkg, passed, failed)

        for i, node in enumerate(nodes):
            got = serialize(node)
            exp = case["expectedSerialize"][i]
            if got != exp:
                failed += 1
                print(f"FAIL [{cid}] serialize[{i}]\n  got: {got}\n  exp: {exp}")
            else:
                passed += 1
            # deserialize(serialize(node)) recovers the modeled + $extra split.
            back = deserialize(got, schema)
            if back.get("$type") != node.get("$type"):
                failed += 1
                print(f"FAIL [{cid}] deserialize[{i}] $type")
            else:
                passed += 1

    return passed, failed


def run_types_file(vectors: str) -> "tuple[int, int]":
    """The 0.4.0 multi-typed-subjects file (runtime#10). Beyond the standard
    form/hash/serialize checks it exercises the $types contract: expectError
    cases must be rejected on ALL THREE surfaces — serialize (the producer
    fails at emit time), deserialize (the reader rejects, never repairs), and
    canonicalization — and positive cases must round-trip:
    deserialize(serialize(x)) preserves $types exactly and re-canonicalizes to
    the same hash."""
    with io.open(vectors, encoding="utf-8") as fh:
        data = json.load(fh)
    schema = data["schema"]
    passed = 0
    failed = 0

    for case in data["cases"]:
        cid = case["id"]
        nodes = case["nodes"]
        pkg = case["pkg"]

        if case.get("expectError"):
            surfaces = [
                ("canonicalize", lambda: canonical_form(nodes, schema, pkg)),
                ("serialize", lambda: [serialize(n) for n in nodes]),
                ("deserialize", lambda: [deserialize(n, schema) for n in nodes]),
            ]
            ok = True
            for what, run in surfaces:
                try:
                    run()
                except ValueError:
                    continue
                ok = False
                print(f"FAIL [{cid}] expected {what} to reject, it did not")
            if ok:
                passed += 1
            else:
                failed += 1
            continue

        form = canonical_form(nodes, schema, pkg)
        if form != case["expectedCanonicalForm"]:
            failed += 1
            print(f"FAIL [{cid}] canonical form\n  got: {form}\n  exp: {case['expectedCanonicalForm']}")
        else:
            passed += 1

        h = content_hash(nodes, schema, pkg)
        if h != case["expectedHash"]:
            failed += 1
            print(f"FAIL [{cid}] hash\n  got: {h}\n  exp: {case['expectedHash']}")
        else:
            passed += 1

        ser = [serialize(n) for n in nodes]
        if ser != case["expectedSerialize"]:
            failed += 1
            print(f"FAIL [{cid}] serialize\n  got: {ser}\n  exp: {case['expectedSerialize']}")
        else:
            passed += 1

        round_tripped = [deserialize(s, schema) for s in ser]
        rt_ser = [serialize(n) for n in round_tripped]
        rt_hash = content_hash(round_tripped, schema, pkg)
        if rt_ser != case["expectedSerialize"] or rt_hash != case["expectedHash"]:
            failed += 1
            print(f"FAIL [{cid}] round-trip\n  got: {rt_ser} @ {rt_hash}\n  exp: {case['expectedSerialize']} @ {case['expectedHash']}")
        else:
            passed += 1

    return passed, failed


def run_enums_file(vectors: str) -> "tuple[int, int]":
    """The 0.5.0 enumerations file (``enums``, runtime#21). Beyond the standard
    form/hash/serialize checks it pins the enumeration contract: the schema
    parses with ``enums`` keyed by durable VERSIONED URIs at both levels (a
    versionless key would look right and miss every lookup); an enumeration
    STANDS ALONE, with no ``classes`` twin; ``enums`` and an enum-ranged
    ``range`` are canonicalization-INERT; and expectError cases are rejected at
    CANONICALIZATION. Unlike the $types file's all-three-surfaces contract this
    violation is schema-DEPENDENT — serialize is schema-free and deserialize
    does not recurse into embedded values, so canonicalization is the only
    surface that can see it."""
    with io.open(vectors, encoding="utf-8") as fh:
        data = json.load(fh)
    schema = data["schema"]
    passed = 0
    failed = 0

    member_re = re.compile(r"^[^/]+/[^/@]+@\d+\.\d+\.\d+/[^/]+$")
    enums = schema.get("enums") or {}
    shape_ok = bool(enums)
    if not enums:
        print("FAIL [enums] schema carries no enums")
    for key, en in enums.items():
        if key != en.get("typeUri"):
            print("FAIL [enums] key {} != typeUri {}".format(key, en.get("typeUri")))
            shape_ok = False
        if key in schema["classes"]:
            print("FAIL [enums] {} must NOT also appear in classes".format(key))
            shape_ok = False
        for member_uri in en.get("members", {}):
            if not member_re.match(member_uri):
                print("FAIL [enums] member key {} is not a versioned durable URI".format(member_uri))
                shape_ok = False
    if shape_ok:
        passed += 1
    else:
        failed += 1

    for case in data["cases"]:
        cid = case["id"]
        nodes = case["nodes"]
        pkg = case["pkg"]

        if case.get("expectError"):
            try:
                canonical_form(nodes, schema, pkg)
            except ValueError:
                passed += 1
                continue
            failed += 1
            print("FAIL [{}] expected canonicalization to reject, it did not".format(cid))
            continue

        form = canonical_form(nodes, schema, pkg)
        hashed = content_hash(nodes, schema, pkg)
        ser = [serialize(n) for n in nodes]
        ok = True
        if form != case["expectedCanonicalForm"]:
            ok = False
            print("FAIL [{}] canonical form".format(cid))
            print("  got: {}".format(form))
            print("  exp: {}".format(case["expectedCanonicalForm"]))
        if hashed != case["expectedHash"]:
            ok = False
            print("FAIL [{}] hash got {} exp {}".format(cid, hashed, case["expectedHash"]))
        if ser != case["expectedSerialize"]:
            ok = False
            print("FAIL [{}] serialize mismatch".format(cid))
        if ok:
            passed += 1
        else:
            failed += 1

    print("{}: {} passed, {} failed".format(os.path.basename(vectors), passed, failed))
    return passed, failed


def run_compat_file(vectors: str) -> "tuple[int, int]":
    """The 0.6.1 compatibility file (runtime#28): deserialize / type_matches /
    enum_member / hashing over a node typed at an earlier compatible version of
    the schema's package. A rejection must end in the bracketed kind the vector
    names — the one part of an error message every port reproduces."""
    with io.open(vectors, encoding="utf-8") as fh:
        data = json.load(fh)
    schemas = data["schemas"]
    pkg = data["pkg"]
    passed = 0
    failed = 0

    def rejected(run, kind: str, cid: str) -> bool:
        try:
            run()
        except ValueError as err:
            if str(err).endswith("[{}]".format(kind)):
                return True
            print("FAIL [{}] expected [{}], got: {}".format(cid, kind, err))
            return False
        print("FAIL [{}] expected a [{}] rejection, got a value".format(cid, kind))
        return False

    for case in data["deserializeCases"]:
        cid = case["id"]
        schema = schemas[case["schema"]]
        if case.get("expectError"):
            ok = rejected(lambda: deserialize(case["input"], schema), case["expectError"], cid)
        else:
            got = deserialize(case["input"], schema)
            ok = got == case["expected"]
            if not ok:
                print("FAIL [{}] deserialize\n  got: {}\n  exp: {}".format(cid, got, case["expected"]))
        if ok:
            passed += 1
        else:
            failed += 1

    for case in data["typeMatchesCases"]:
        got = type_matches(case["node"], case["classUri"], schemas[case["schema"]])
        if got is case["expected"]:
            passed += 1
        else:
            failed += 1
            print("FAIL [{}] type_matches expected {} got {}".format(case["id"], case["expected"], got))

    for case in data["enumMemberCases"]:
        m = enum_member(schemas[case["schema"]], case["ref"])
        got = None if m is None else {"enumType": m.enum_type, "uri": m.uri, "label": m.member.get("label")}
        if got == case["expected"]:
            passed += 1
        else:
            failed += 1
            print("FAIL [{}] enum_member expected {} got {}".format(case["id"], case["expected"], got))

    for case in data["hashCases"]:
        cid = case["id"]
        schema = schemas[case["schema"]]
        if case.get("expectError"):
            ok = rejected(lambda: content_hash(case["nodes"], schema, pkg), case["expectError"], cid)
        else:
            got = content_hash(case["nodes"], schema, pkg)
            ok = got == case["expectedHash"]
            if not ok:
                print("FAIL [{}] hash got {} exp {}".format(cid, got, case["expectedHash"]))
        if ok:
            passed += 1
        else:
            failed += 1

    print("{}: {} passed, {} failed".format(os.path.basename(vectors), passed, failed))
    return passed, failed


def run_json_numbers_file(vectors: str) -> "tuple[int, int]":
    """The JSON forms of integer and decimal values (runtime#54): serialize_exact
    and deserialize_exact against the shared vectors. A value read must be this
    port's exact type for its datatype, a ``Decimal`` for a decimal and an
    ``int`` for every integer, holding the expected canonical lexical form; a
    value written must be the expected JSON, an ``int`` or a ``str``."""
    with io.open(vectors, encoding="utf-8") as fh:
        data = json.load(fh)
    schema = data["schema"]
    props = schema["classes"][data["class"]]["props"]
    decimal_uri = data["datatypes"]["decimal"]
    passed = 0
    failed = 0

    def props_of(prop):
        return schema["classes"][prop["range"]]["props"]

    def exact(prop, lexical):
        if isinstance(lexical, list):
            return [exact(prop, item) for item in lexical]
        if prop["kind"] == "object":
            return {k: exact(props_of(prop)[k], v) for k, v in lexical.items()}
        return Decimal(lexical) if prop["datatype"] == decimal_uri else int(lexical)

    def lexical_of(prop, value):
        if isinstance(value, list):
            return [lexical_of(prop, item) for item in value]
        if prop["kind"] == "object" and isinstance(value, dict):
            return {k: lexical_of(props_of(prop)[k], v) for k, v in value.items()}
        want = Decimal if prop.get("datatype") == decimal_uri else int
        if type(value) is not want:
            return "<{} {!r}>".format(type(value).__name__, value)
        return format(value, "f") if want is Decimal else str(value)

    def as_json(value):
        return json.dumps(value, sort_keys=True, default=lambda v: "<{} {!r}>".format(type(v).__name__, v))

    def refused(err, cid):
        if str(err).endswith("[json-number-form]"):
            return True
        print("FAIL [{}] expected [json-number-form], got: {}".format(cid, err))
        return False

    # Each read case twice: as json.loads parses by default, and with parse_float=Decimal, which keeps a JSON number
    # exact, so a non-whole one is refused rather than rounded to a whole one first.
    read_cases = [(case, parse) for parse in (None, Decimal) for case in data["readCases"]]
    edge = {"id": "integer-number-not-whole-past-double-precision", "field": "integer",
            "json": "1.0000000000000000001", "expectError": True}
    read_cases.append((edge, Decimal))
    for case, parse_float in read_cases:
        cid = "read " + case["id"] + (" (Decimal)" if parse_float else "")
        field = case["field"]
        obj = json.loads('{{"$type": {}, {}: {}}}'.format(json.dumps(data["class"]), json.dumps(field), case["json"]),
                         parse_float=parse_float)
        try:
            got = lexical_of(props[field], deserialize_exact(obj, schema)[field])
        except ValueError as err:
            ok = bool(case.get("expectError")) and refused(err, cid)
            if not case.get("expectError"):
                print("FAIL [{}] threw {}".format(cid, err))
        else:
            ok = not case.get("expectError") and got == case["expected"]
            if not ok:
                print("FAIL [{}] expected {}, got {}".format(cid, "a refusal" if case.get("expectError") else case["expected"], got))
        if ok:
            passed += 1
        else:
            failed += 1

    for case in data["writeCases"]:
        cid = "write " + case["id"]
        field = case["field"]
        node = {"$type": data["class"], field: exact(props[field], case["value"])}
        try:
            got = serialize_exact(node, schema)[field]
        except ValueError as err:
            ok = bool(case.get("expectError")) and refused(err, cid)
            if not case.get("expectError"):
                print("FAIL [{}] threw {}".format(cid, err))
        else:
            ok = not case.get("expectError") and as_json(got) == as_json(json.loads(case["json"]))
            if not ok:
                print("FAIL [{}] expected {}, got {}".format(cid, "a refusal" if case.get("expectError") else case["json"], as_json(got)))
        if ok:
            passed += 1
        else:
            failed += 1

    print("{}: {} passed, {} failed".format(os.path.basename(vectors), passed, failed))
    return passed, failed


def run_deprecations() -> "tuple[int, int]":
    """serialize and deserialize warn that they're deprecated (runtime#54), and
    nothing that replaces them, or builds on them, does."""
    with io.open(JSON_NUMBERS_VECTOR_FILE, encoding="utf-8") as fh:
        data = json.load(fh)
    schema = data["schema"]
    node = {"$type": data["class"], "int": 1}
    checks = [
        ("serialize", True, lambda: serialize(node)),
        ("deserialize", True, lambda: deserialize(node, schema)),
        ("serialize_exact", False, lambda: serialize_exact(node, schema)),
        ("deserialize_exact", False, lambda: deserialize_exact(node, schema)),
        ("to_node", False, lambda: to_node(node, schema)),
    ]
    passed = 0
    failed = 0
    for name, deprecated, run in checks:
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            run()
        where = [w.filename for w in caught if issubclass(w.category, DeprecationWarning)]
        if where == ([__file__] if deprecated else []):
            passed += 1
        else:
            failed += 1
            print("FAIL [deprecation] {}: expected {}, got DeprecationWarnings at {}".format(
                name, "one at its caller" if deprecated else "none", where))
    return passed, failed


def run_types_typed() -> "tuple[int, int]":
    """Typed-surface $types cases (0.4.0, runtime#10): the multi-typed set rides
    the wire dict as $types only (no unprefixed accessor) and reproduces the
    same golden vectors through to_node/embed."""
    schema_ns = "probe.example.com/schema@1.0.0"
    data_ns = "probe.example.com/data@1.0.0"

    def t(local: str) -> str:
        return f"{schema_ns}/{local}"

    def d(local: str) -> str:
        return f"{data_ns}/{local}"

    with io.open(TYPES_VECTOR_FILE, encoding="utf-8") as fh:
        doc = json.load(fh)

    checks = [
        ("covered-redundant-set", [
            {"$type": t("ClassDef"), "$types": [t("AnnotatedDef"), t("ClassDef")],
             "$id": d("w1"), "note": "A"},
        ]),
        ("embedded-multi-typed-named", [
            {"$type": t("Bundle"), "$id": d("b1"),
             "parts": embed({"$type": t("PartDef"),
                             "$types": [t("PartDef"), t("SealedDef")],
                             "size": 2}, name="first")},
        ]),
        ("types-in-list-items", [
            {"$type": t("Bundle"), "$id": d("b1"),
             "parts": [embed({"$type": t("PartDef"),
                              "$types": [t("PartDef"), t("SealedDef")],
                              "size": 1}, name="a"),
                       embed({"$type": t("PartDef"), "size": 2}, name="b")]},
        ]),
    ]

    passed = 0
    failed = 0
    schema = doc["schema"]
    for cid, typed in checks:
        case = next(c for c in doc["cases"] if c["id"] == cid)
        nodes = [to_node(x, schema) for x in typed]

        form = canonical_form(nodes, schema, case["pkg"])
        if form != case["expectedCanonicalForm"]:
            failed += 1
            print(f"FAIL [typed:{cid}] canonical form\n  got: {form}\n  exp: {case['expectedCanonicalForm']}")
        else:
            passed += 1

        h = content_hash(nodes, schema, case["pkg"])
        if h != case["expectedHash"]:
            failed += 1
            print(f"FAIL [typed:{cid}] hash\n  got: {h}\n  exp: {case['expectedHash']}")
        else:
            passed += 1

    return passed, failed


def run_typed() -> "tuple[int, int]":
    """The typed surface (to_node/ref/embed) reproducing the golden vectors:
    plain-dict fixtures (standing in for a generated Pydantic model's
    model_dump output) flow through the helpers and must land on the SAME
    expected canonical form + hash the node contract is gated by — the typed
    path and the dictionary path are one contract, not two."""
    schema_ns = "probe.example.com/schema@1.0.0"
    data = "probe.example.com/data@1.0.0"

    def t(local: str) -> str:
        return f"{schema_ns}/{local}"

    def d(local: str) -> str:
        return f"{data}/{local}"

    with io.open(VECTOR_FILES[0], encoding="utf-8") as fh:
        basic = json.load(fh)
    with io.open(VECTOR_FILES[1], encoding="utf-8") as fh:
        emb = json.load(fh)

    person = {"$type": t("Person"), "$id": d("p1"), "name": "Alice"}

    def account(owner: "dict") -> "dict":
        return {
            "$type": t("Account"), "$id": d("a1"),
            "accountCode": "paul", "seats": 5, "rate": 1.5, "active": True,
            "owner": owner, "tags": ["x", "y"],
        }

    checks = [
        (emb, "embedded-named-in-list", [
            {"$type": t("Order"), "$id": d("o1"), "note": "A",
             "items": [embed({"sku": "X", "qty": 1}, name="first")]},
        ]),
        (emb, "embedded-unnamed-positional", [
            {"$type": t("Order"), "$id": d("o1"), "note": "A",
             "items": [embed({"sku": "X", "qty": 1})]},
        ]),
        # The embedded dict carries an explicit $type — it emits a type
        # statement inside the embedded (hash-relevant).
        (emb, "embedded-explicit-type", [
            {"$type": t("Order"), "$id": d("o1"), "note": "A",
             "items": [embed({"$type": t("LineItem"), "sku": "X", "qty": 1},
                             name="first")]},
        ]),
        (emb, "embedded-list-order", [
            {"$type": t("Order"), "$id": d("o1"),
             "items": [embed({"sku": "X", "qty": 1}, name="a"),
                       embed({"sku": "Y", "qty": 2}, name="b")]},
        ]),
        (emb, "embedded-nested", [
            {"$type": t("Order"), "$id": d("o1"),
             "customer": [embed({"name": "Ada",
                                 "address": [embed({"city": "Austin"},
                                                   name="home")]},
                                name="cust")]},
        ]),
        (emb, "single-embedded-bare", [
            {"$type": t("Order"), "$id": d("o1"),
             "customer": embed({"name": "Ada"}, name="cust")},
        ]),
        (emb, "empty-list-emits-nothing", [
            {"$type": t("Order"), "$id": d("o1"), "note": "A", "items": []},
        ]),
        # The 0.1.0 basic case through the typed path: ref by canonical URI...
        (basic, "basic-scalars-ref-list", [person, account(ref(d("p1")))]),
        # ...and ref by the instance itself — resolved through its $id.
        (basic, "basic-scalars-ref-list", [person, account(ref(person))]),
    ]

    passed = 0
    failed = 0
    for doc, cid, typed in checks:
        schema = doc["schema"]
        case = next(c for c in doc["cases"] if c["id"] == cid)
        pkg = case["pkg"]
        nodes = [to_node(x, schema) for x in typed]

        form = canonical_form(nodes, schema, pkg)
        if form != case["expectedCanonicalForm"]:
            failed += 1
            print(f"FAIL [typed:{cid}] canonical form\n  got: {form}\n  exp: {case['expectedCanonicalForm']}")
        else:
            passed += 1

        h = content_hash(nodes, schema, pkg)
        if h != case["expectedHash"]:
            failed += 1
            print(f"FAIL [typed:{cid}] hash\n  got: {h}\n  exp: {case['expectedHash']}")
        else:
            passed += 1

    return passed, failed


def main() -> int:
    passed = 0
    failed = 0
    # These vectors drive serialize and deserialize, deprecated since runtime#54
    # and still kept to them.
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        for vectors in VECTOR_FILES:
            p, f = run_file(vectors)
            passed += p
            failed += f

        p, f = run_types_file(TYPES_VECTOR_FILE)
        passed += p
        failed += f

        p, f = run_types_file(REFERENCES_VECTOR_FILE)
        passed += p
        failed += f

        p, f = run_enums_file(ENUMS_VECTOR_FILE)
        passed += p
        failed += f

        p, f = run_compat_file(COMPAT_VECTOR_FILE)
        passed += p
        failed += f

    with warnings.catch_warnings():
        warnings.simplefilter("error", DeprecationWarning)
        p, f = run_typed()
        passed += p
        failed += f

        p, f = run_types_typed()
        passed += p
        failed += f

        p, f = run_json_numbers_file(JSON_NUMBERS_VECTOR_FILE)
        passed += p
        failed += f

    p, f = run_deprecations()
    passed += p
    failed += f

    print(f"\n{passed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
