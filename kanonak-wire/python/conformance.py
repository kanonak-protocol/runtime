"""Drives the shared wire vectors through the Python kanonak-wire port.

Read vectors run an op-script against a hex buffer asserting values or
required errors {kind, offset}; write vectors run writer ops asserting exact
output bytes.

    python conformance.py <vectors-dir>
"""

import json
import sys
from pathlib import Path

from kanonak_wire import WireError, WireReader, WireWriter

# Python has arbitrary-precision ints, floats, and surrogate-capable strings:
# all capabilities present.
CAPABILITIES = {"wide-numeric-params", "dynamic-numeric", "utf16-strings"}

counts = {"pass": 0, "fail": 0, "skipped": 0}


def fail_case(vid, msg):
    counts["fail"] += 1
    print("{0}: {1}".format(vid, msg), file=sys.stderr)


def check_error(vid, op_name, e, want):
    if not isinstance(e, WireError):
        fail_case(vid, "{0}: threw a non-WireError: {1}".format(op_name, e))
        return False
    if e.kind != want["kind"]:
        fail_case(
            vid,
            "{0}: expected error kind {1}, got {2} ({3})".format(
                op_name, want["kind"], e.kind, e
            ),
        )
        return False
    if "offset" in want and e.offset != want["offset"]:
        fail_case(
            vid,
            "{0}: expected error offset {1}, got {2}".format(
                op_name, want["offset"], e.offset
            ),
        )
        return False
    return True


def run_read_op(r, op):
    name = op["op"]
    if name == "u8":
        return r.u8()
    if name == "u16be":
        return r.u16_be()
    if name == "u32be":
        return r.u32_be()
    if name == "bytes":
        return bytes(r.bytes(op["n"])).hex()
    if name == "uuid":
        return r.uuid()
    if name == "utf8":
        return r.utf8(op["n"])
    if name == "lenPrefixedBytes16":
        return bytes(r.len_prefixed_bytes16()).hex()
    if name == "rest":
        return bytes(r.rest()).hex()
    if name == "remaining":
        return r.remaining()
    if name == "expectEnd":
        r.expect_end()
        return None
    raise RuntimeError("conformance: unknown read op '{0}'".format(name))


def run_write_op(w, op):
    name = op["op"]
    if name == "u8":
        w.u8(op["value"])
    elif name == "u16be":
        w.u16_be(op["value"])
    elif name == "u32be":
        w.u32_be(op["value"])
    elif name == "bytes":
        w.bytes(bytes.fromhex(op["hex"]))
    elif name == "uuid":
        w.uuid(op["value"])
    elif name == "utf8":
        if "utf16CodeUnits" in op:
            s = "".join(chr(u) for u in op["utf16CodeUnits"])
        else:
            s = op["value"]
        w.utf8(s)
    elif name == "lenPrefixedBytes16":
        w.len_prefixed_bytes16(bytes.fromhex(op["hex"]))
    else:
        raise RuntimeError("conformance: unknown write op '{0}'".format(name))


def run_read_vector(v):
    r = WireReader(bytes.fromhex(v["bytes"]))
    for op in v["ops"]:
        if "expectError" in op:
            try:
                run_read_op(r, op)
            except Exception as e:  # noqa: BLE001
                if not check_error(v["id"], op["op"], e, op["expectError"]):
                    return False
            else:
                fail_case(
                    v["id"],
                    "{0}: expected {1}, got a value".format(
                        op["op"], op["expectError"]["kind"]
                    ),
                )
                return False
            break  # an error op ends the script
        try:
            got = run_read_op(r, op)
        except Exception as e:  # noqa: BLE001
            fail_case(v["id"], "{0}: threw {1}".format(op["op"], e))
            return False
        if "expected" in op and got != op["expected"]:
            fail_case(
                v["id"],
                "{0}: expected {1!r}, got {2!r}".format(op["op"], op["expected"], got),
            )
            return False
    return True


def run_write_vector(v):
    w = WireWriter()
    for op in v["ops"]:
        if "expectError" in op:
            try:
                run_write_op(w, op)
            except Exception as e:  # noqa: BLE001
                if not check_error(v["id"], op["op"], e, op["expectError"]):
                    return False
            else:
                fail_case(
                    v["id"],
                    "{0}: expected {1}, got success".format(
                        op["op"], op["expectError"]["kind"]
                    ),
                )
                return False
            break  # an error op ends the script
        try:
            run_write_op(w, op)
        except Exception as e:  # noqa: BLE001
            fail_case(v["id"], "{0}: threw {1}".format(op["op"], e))
            return False
    if "expectedBytes" in v:
        got = w.to_bytes().hex()
        if got != v["expectedBytes"]:
            fail_case(
                v["id"], "expected bytes {0}, got {1}".format(v["expectedBytes"], got)
            )
            return False
    return True


def main():
    vdir = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else Path(__file__).resolve().parent.parent / "vectors"
    )
    data = json.loads((vdir / "wire-vectors.json").read_text(encoding="utf-8"))

    for v in data["readVectors"]:
        if v.get("requires") and v["requires"] not in CAPABILITIES:
            counts["skipped"] += 1
            continue
        if run_read_vector(v):
            counts["pass"] += 1

    for v in data["writeVectors"]:
        if v.get("requires") and v["requires"] not in CAPABILITIES:
            counts["skipped"] += 1
            continue
        if run_write_vector(v):
            counts["pass"] += 1

    total = len(data["readVectors"]) + len(data["writeVectors"])
    print(
        "wire-vectors: {0}/{1} pass ({2} skipped)".format(
            counts["pass"], total, counts["skipped"]
        )
    )
    if counts["fail"] > 0:
        print("{0} VECTOR(S) FAILED".format(counts["fail"]), file=sys.stderr)
        sys.exit(1)
    print("ALL VECTORS PASS")


if __name__ == "__main__":
    main()
