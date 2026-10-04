#!/usr/bin/env python3
"""The release plan, and the two compatibility rules that need no language
toolchain (runtime#29).

A member's BASELINE is its last release: the highest `kanonak-<member>/go/v*`
tag. Every release run pushes that tag after the registries publish, so it
marks the source each registry shipped. The DECLARED version is in the
committed manifests, which preflight already holds to parity, so this reads
the Rust manifest. Comparing the two gives the change a release may carry:

  none      declared == baseline: nothing releases, so nothing may change
  fix       a patch from 1.0.0: no API addition, no break
  addition  a minor from 1.0.0, or a patch below 1.0.0: additions, no break
  breaking  a new major, or a new minor below 1.0.0, or no baseline yet

That mirrors isReadableBy in kanonak-canonical: below 1.0.0 the minor is the
incompatible line. Each language's API-diff check asks this script what is
allowed and fails when the code changed more than that.

Subcommands:
  plan                 print every member's baseline, declared version, allowed change
  allowed MEMBER       print the allowed change for one member
  baseline MEMBER      print the baseline version (empty when none)
  vectors              a vector case published in a release never changes within
                       a major (it may only be added to); exit 1 on a violation
  swift                the Swift root tag is a release train over all four members:
                       derive its version from theirs and check the declared one
  swift-baseline       print the last root Swift tag (e.g. v0.9.6; empty when none)
  swift-allowed        print the change the declared Swift version allows
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from typing import Optional, Tuple

MEMBERS = ("canonical", "codec", "expression", "wire")
Version = Tuple[int, int, int]


def git(*args: str) -> str:
    return subprocess.run(("git",) + args, check=True, capture_output=True, encoding="utf-8").stdout


def parse(v: str) -> Version:
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", v.strip())
    if not m:
        raise SystemExit(f"release_plan: '{v}' is not major.minor.patch")
    return int(m[1]), int(m[2]), int(m[3])


def fmt(v: Version) -> str:
    return "%d.%d.%d" % v


def declared(member: str) -> Version:
    text = open(f"kanonak-{member}/rust/Cargo.toml", encoding="utf-8").read()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        raise SystemExit(f"release_plan: no version in kanonak-{member}/rust/Cargo.toml")
    return parse(m[1])


def baseline(member: str) -> Optional[Version]:
    prefix = f"kanonak-{member}/go/v"
    versions = []
    for tag in git("tag", "--list", prefix + "*").split():
        try:
            versions.append(parse(tag[len(prefix):]))
        except SystemExit:
            continue
    return max(versions) if versions else None


def change_kind(base: Optional[Version], new: Version, what: str) -> str:
    if base is None:
        return "breaking"
    if new < base:
        raise SystemExit(
            f"release_plan: {what} declares {fmt(new)}, BEHIND its last release {fmt(base)}; "
            "a reverted manifest or an out-of-band release. Reconcile before releasing."
        )
    if new == base:
        return "none"
    if new[0] != base[0]:
        return "breaking"
    if new[0] == 0:
        return "breaking" if new[1] != base[1] else "addition"
    return "addition" if new[1] != base[1] else "fix"


def allowed(member: str) -> str:
    return change_kind(baseline(member), declared(member), f"kanonak-{member}")


# --- vectors -----------------------------------------------------------------

DESCRIPTIVE = {"description", "why"}


def is_case_list(value) -> bool:
    return isinstance(value, list) and bool(value) and all(isinstance(x, dict) and "id" in x for x in value)


def without_descriptive(case: dict) -> dict:
    return {k: v for k, v in case.items() if k not in DESCRIPTIVE}


def content(case: dict) -> dict:
    return {k: v for k, v in case.items() if k not in DESCRIPTIVE and k != "id"}


def compare(base, head, path: str, problems: list) -> None:
    """Everything the baseline published must survive unchanged: a case (a dict
    in an id-keyed list) exactly, apart from its prose; a container may gain
    keys and a case list may gain cases."""
    if is_case_list(base):
        if not isinstance(head, list):
            problems.append(f"{path}: the case list is gone")
            return
        by_id = {c.get("id"): c for c in head if isinstance(c, dict)}
        bodies = [content(c) for c in head if isinstance(c, dict)]
        for case in base:
            other = by_id.get(case["id"])
            if other is None:
                # A renamed case is the same expectation under a new id.
                if content(case) not in bodies:
                    problems.append(f"{path}[{case['id']}]: case removed")
            elif without_descriptive(case) != without_descriptive(other):
                problems.append(f"{path}[{case['id']}]: case changed")
        return
    if isinstance(base, dict):
        if not isinstance(head, dict):
            problems.append(f"{path}: changed shape")
            return
        for key, value in base.items():
            if path == "" and key in DESCRIPTIVE:
                continue
            if key not in head:
                problems.append(f"{path}.{key}: removed")
            else:
                compare(value, head[key], f"{path}.{key}", problems)
        return
    if base != head:
        problems.append(f"{path or '(file)'}: changed")


def check_vectors() -> int:
    failures = 0
    for member in MEMBERS:
        base = baseline(member)
        kind = allowed(member)
        if base is None:
            print(f"vectors   kanonak-{member}: no release yet, nothing frozen")
            continue
        if kind == "breaking":
            print(f"vectors   kanonak-{member}: {fmt(base)} -> {fmt(declared(member))} is a new major line; the old cases are not binding")
            continue
        tag = f"kanonak-{member}/go/v{fmt(base)}"
        files = [f for f in git("ls-tree", "-r", "--name-only", tag, "--", f"kanonak-{member}/vectors/").split() if f.endswith(".json")]
        problems: list = []
        for f in files:
            old = json.loads(git("show", f"{tag}:{f}"))
            try:
                new = json.load(open(f, encoding="utf-8"))
            except FileNotFoundError:
                problems.append(f"{f}: removed")
                continue
            compare(old, new, "", file_problems := [])
            problems += [f"{f}{p}" for p in file_problems]
        if problems:
            failures += 1
            for p in problems:
                print(
                    f"::error::VECTOR CHANGED kanonak-{member} {p} — published in {fmt(base)}, and a published "
                    f"case never changes within a major (runtime#29). Add a new case instead, or release a new major."
                )
        else:
            print(f"OK        vectors       kanonak-{member}: every case published in {fmt(base)} is unchanged ({len(files)} files)")
    return 1 if failures else 0


# --- swift -------------------------------------------------------------------

def swift_declared(text: str) -> Version:
    m = re.search(r'^\s*swift_package_version:\s*"([^"]+)"', text, re.M)
    if not m:
        raise SystemExit("release_plan: no swift_package_version in release-targets.yml")
    return parse(m[1])


def member_versions(text: str) -> dict:
    block = re.search(r"^\s*go_module_versions:\s*\n((?:\s+\w+:\s*\"[^\"]+\"\s*\n)+)", text, re.M)
    if not block:
        raise SystemExit("release_plan: no go_module_versions block in release-targets.yml")
    return {k: parse(v) for k, v in re.findall(r"(\w+):\s*\"([^\"]+)\"", block[1])}


def swift_last_tag() -> Optional[Tuple[Version, str]]:
    tags = []
    for tag in git("tag", "--list", "v*").split():
        try:
            tags.append((parse(tag[1:]), tag))
        except SystemExit:
            continue
    return max(tags) if tags else None


def swift_allowed() -> str:
    last = swift_last_tag()
    want = swift_declared(open("release-targets.yml", encoding="utf-8").read())
    return change_kind(last[0] if last else None, want, "the Swift package")


def check_swift() -> int:
    head_text = open("release-targets.yml", encoding="utf-8").read()
    want = swift_declared(head_text)
    found = swift_last_tag()
    if not found:
        print(f"OK        swift         no root tag yet; v{fmt(want)} will be the first")
        return 0
    last, tag = found
    then = member_versions(git("show", f"{tag}:release-targets.yml"))
    now = member_versions(head_text)
    kinds = {m: change_kind(then.get(m), now[m], f"kanonak-{m}") for m in now}
    broke = any(k == "breaking" for k in kinds.values())
    added = any(k == "addition" for k in kinds.values())
    fixed = any(k == "fix" for k in kinds.values())
    if last[0] == 0:
        # Below 1.0.0 the minor is the incompatible line, as for the members;
        # the train reaches 1.0.0 together with the last member to get there.
        if (broke or added or fixed) and all(v[0] >= 1 for v in now.values()):
            derived = (1, 0, 0)
        elif broke:
            derived = (0, last[1] + 1, 0)
        elif added or fixed:
            derived = (0, last[1], last[2] + 1)
        else:
            derived = last
    elif broke:
        derived = (last[0] + 1, 0, 0)
    elif added:
        derived = (last[0], last[1] + 1, 0)
    elif fixed:
        derived = (last[0], last[1], last[2] + 1)
    else:
        derived = last
    detail = ", ".join(f"{m} {fmt(then[m]) if m in then else '-'}->{fmt(now[m])} ({kinds[m]})" for m in now)
    if want != derived:
        print(
            f"::error::SWIFT VERSION swift_package_version is {fmt(want)} but the members since {tag} derive "
            f"{fmt(derived)} ({detail}). The root tag is one release train over all four products: a major if "
            f"any member broke, a minor if any added, else a patch (runtime#29). Set it to {fmt(derived)}."
        )
        return 1
    print(f"OK        swift         v{fmt(want)} derives from the members since {tag} ({detail})")
    return 0


def main(argv: list) -> int:
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    cmd = argv[0]
    if cmd == "plan":
        for m in MEMBERS:
            base = baseline(m)
            print(f"kanonak-{m:<11} baseline {fmt(base) if base else '-':<8} declared {fmt(declared(m)):<8} allowed {allowed(m)}")
        return 0
    if cmd == "allowed" and len(argv) == 2:
        print(allowed(argv[1]))
        return 0
    if cmd == "baseline" and len(argv) == 2:
        base = baseline(argv[1])
        print(fmt(base) if base else "")
        return 0
    if cmd == "vectors":
        return check_vectors()
    if cmd == "swift":
        return check_swift()
    if cmd == "swift-baseline":
        found = swift_last_tag()
        print(found[1] if found else "")
        return 0
    if cmd == "swift-allowed":
        print(swift_allowed())
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
