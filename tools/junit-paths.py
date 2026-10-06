#!/usr/bin/env python3
"""Adds a `file` attribute (repo-relative source path) to every testcase of a cargo-nextest JUnit
report so `gintrack spec ingest` can map tests to `path#symbol` trace refs.

nextest names unit tests `<module path>::<test>` with classname `<crate>` and integration tests
`<test>` with classname `<crate>::<binary>`. We resolve:

  unit test  `edit::state::tests::x`  -> crates/<crate>/src/edit/state.rs (or .../state/mod.rs)
  integration `<crate>::roundtrip`    -> crates/<crate>/tests/roundtrip.rs

Usage: tools/junit-paths.py target/nextest/ci/junit.xml [repo-root]   (rewrites the file in place)
"""
import os
import sys
import xml.etree.ElementTree as ET


def unit_file(root, crate, name):
    parts = name.split("::")
    if "tests" in parts:
        parts = parts[: parts.index("tests")]
    else:
        parts = parts[:-1]
    base = os.path.join("crates", crate, "src")
    while parts:
        for cand in (
            os.path.join(base, *parts) + ".rs",
            os.path.join(base, *parts, "mod.rs"),
        ):
            if os.path.isfile(os.path.join(root, cand)):
                return cand
        parts = parts[:-1]
    for cand in ("lib.rs", "main.rs"):
        if os.path.isfile(os.path.join(root, base, cand)):
            return os.path.join(base, cand)
    return None


def main():
    path = sys.argv[1]
    root = sys.argv[2] if len(sys.argv) > 2 else "."
    tree = ET.parse(path)
    for case in tree.iter("testcase"):
        classname = case.get("classname", "")
        crate, _, binary = classname.partition("::")
        if binary:
            f = os.path.join("crates", crate, "tests", binary + ".rs")
            f = f if os.path.isfile(os.path.join(root, f)) else None
        else:
            f = unit_file(root, crate, case.get("name", ""))
        if f:
            case.set("file", f.replace(os.sep, "/"))
            # The symbol must be the bare test name for integration tests and the in-file path for
            # unit tests; gintrack uses `name` as written.
    tree.write(path, encoding="utf-8", xml_declaration=True)


if __name__ == "__main__":
    main()
