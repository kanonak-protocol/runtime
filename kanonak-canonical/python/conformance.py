"""Drives the shared golden vectors through the Python kanonak-canonical port.

    python conformance.py <vectors-dir>
"""

import json
import sys
from pathlib import Path

from kanonak_canonical import (
    Carrier,
    CoordinateVersion,
    Embedded,
    KList,
    Package,
    RawScalar,
    Reference,
    Statement,
    Subject,
    TypedScalar,
    canonical_form,
    canonical_hash,
    canonical_scalar_lexical,
    carrier_of,
    lenient_versionless_key,
    local_name,
    parse_coordinate,
    versionless_key,
)


def decode_value(v):
    if "lit" in v:
        c = carrier_of(v["datatype"])
        return TypedScalar(c, v["lit"]) if c is not None else RawScalar(v["lit"])
    if "raw" in v:
        return RawScalar(v["raw"])
    if "ref" in v:
        return Reference(v["ref"])
    if "embed" in v:
        e = v["embed"]
        return Embedded(e.get("name"), decode_statements(e))
    if "list" in v:
        return KList([decode_value(x) for x in v["list"]])
    raise ValueError(f"decode: unknown value shape {v}")


def decode_statements(node):
    return [Statement(s["predicate"], decode_value(s["value"])) for s in node.get("statements", [])]


def decode_subjects(inp):
    return Package([Subject(s["uri"], decode_statements(s)) for s in inp["subjects"]])


def run_lexical(path):
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    total = pas = fail = 0
    for v in doc["vectors"]:
        total += 1
        vid, carrier, inp = v["id"], Carrier(v["carrier"]), v["input"]
        expect_error = v.get("expectError", False)
        try:
            actual = canonical_scalar_lexical(carrier, inp)
            if expect_error:
                fail += 1
                print(f"  FAIL [{vid}] expected error, got '{actual}'")
            elif actual == v["expected"]:
                pas += 1
            else:
                fail += 1
                print(f"  FAIL [{vid}] expected '{v['expected']}', got '{actual}'")
        except Exception as e:  # noqa: BLE001
            if expect_error:
                pas += 1
            else:
                fail += 1
                print(f"  FAIL [{vid}] threw: {e}")
    print(f"lexical-vectors: {pas}/{total} pass, {fail} fail")
    return fail


def run_full_form(path):
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    total = pas = fail = 0
    for v in doc["vectors"]:
        total += 1
        vid = v["id"]
        try:
            pkg = decode_subjects(v["input"])
            form = canonical_form(pkg)
            digest = canonical_hash(pkg)
            ok = True
            if form != v["expectedCanonicalForm"]:
                ok = False
                print(f"  FAIL [{vid}] form\n    expected: {v['expectedCanonicalForm']}\n    actual:   {form}")
            if digest != v["expectedHash"]:
                ok = False
                print(f"  FAIL [{vid}] hash expected {v['expectedHash']} got {digest}")
            pas += ok
            fail += not ok
        except Exception as e:  # noqa: BLE001
            fail += 1
            print(f"  FAIL [{vid}] threw: {e}")
    print(f"full-form-vectors: {pas}/{total} pass, {fail} fail")
    return fail


def run_coordinate(path):
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    total = pas = fail = 0
    for v in doc["parseVectors"]:
        total += 1
        vid, inp = v["id"], v["input"]
        if v.get("expectError", False):
            try:
                got = parse_coordinate(inp)
                fail += 1
                print(f"  FAIL [{vid}] expected error, got {got}")
            except Exception:  # noqa: BLE001
                pas += 1
            continue
        e = v["expected"]
        try:
            got = parse_coordinate(inp)
            ev = e["version"]
            want_version = None if ev is None else CoordinateVersion(ev["major"], ev["minor"], ev["patch"])
            want_key = f"{e['publisher']}/{e['package']}/{e['name']}"
            ok = (got.publisher == e["publisher"] and got.package == e["package"]
                  and got.name == e["name"] and got.version == want_version
                  and versionless_key(inp) == want_key and local_name(inp) == e["name"])
            if ok:
                pas += 1
            else:
                fail += 1
                print(f"  FAIL [{vid}] expected {e}, got {got}")
        except Exception as ex:  # noqa: BLE001
            fail += 1
            print(f"  FAIL [{vid}] threw: {ex}")
    for v in doc["lenientKeyVectors"]:
        total += 1
        got = lenient_versionless_key(v["input"])
        if got == v["expected"]:
            pas += 1
        else:
            fail += 1
            print(f"  FAIL [{v['id']}] expected {v['expected']!r}, got {got!r}")
    print(f"coordinate-vectors: {pas}/{total} pass, {fail} fail")
    return fail


def main():
    vdir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "vectors"
    fails = run_lexical(vdir / "lexical-vectors.json")
    fails += run_full_form(vdir / "full-form-vectors.json")
    fails += run_coordinate(vdir / "coordinate-vectors.json")
    print("\nALL VECTORS PASS" if fails == 0 else f"\n{fails} VECTOR(S) FAILED")
    sys.exit(0 if fails == 0 else 1)


if __name__ == "__main__":
    main()
