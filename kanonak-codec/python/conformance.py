"""Conformance: drive kanonak_codec with the shared codec vectors and assert the
canonical form, content hash, and normalized-JSON serialize all match the
authoritative (TypeScript-generated) expected values."""

import io
import json
import os
import sys

# Resolve the sibling canonical port + the shared vectors without installing.
_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_HERE, "..", "..", "kanonak-canonical", "python"))

from kanonak_codec import (  # noqa: E402
    canonical_form,
    content_hash,
    deserialize,
    embed,
    ref,
    serialize,
    to_node,
)

VECTOR_FILES = [
    os.path.join(_HERE, "..", "vectors", "codec-vectors.json"),
    os.path.join(_HERE, "..", "vectors", "codec-vectors-embedded.json"),
]


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
    for vectors in VECTOR_FILES:
        p, f = run_file(vectors)
        passed += p
        failed += f

    p, f = run_typed()
    passed += p
    failed += f

    print(f"\n{passed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
