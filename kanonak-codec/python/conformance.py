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

from kanonak_codec import canonical_form, content_hash, serialize, deserialize  # noqa: E402

VECTORS = os.path.join(_HERE, "..", "vectors", "codec-vectors.json")


def main() -> int:
    with io.open(VECTORS, encoding="utf-8") as fh:
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

    print(f"\n{passed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
