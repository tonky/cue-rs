#!/usr/bin/env python3
"""Record byte provenance without modifying any corpus fixture.

Usage: python3 tools/conformance-manifest.py PINNED_UPSTREAM_CHECKOUT
The checkout must correspond to REVISION. Every imported file must match.
"""
import hashlib
import json
from pathlib import Path
import re
import sys

REVISION = "635e4bb441b29b0b8a3d754188b8edefe4012d1d"
root = Path(sys.argv[1])
index = {}
for subdir in ("cue/testdata", "pkg"):
    for path in (root / subdir).rglob("*.txtar"):
        name = "upstream_" + path.relative_to(root).as_posix().replace("/", "_")
        index.setdefault(name, []).append(path)
cases = []
for path in sorted(Path("tests/testdata").glob("*.txtar")):
    data = path.read_bytes()
    candidates = index.get(path.name, [])
    match = next((p for p in candidates if p.read_bytes() == data), None)
    if path.name.startswith("upstream_") and match is None:
        raise SystemExit(f"no byte-identical upstream source: {path}")
    text = data.decode()
    sections = re.findall(r"^-- (.*?) --$", text, re.M)
    header = text.split("-- ", 1)[0]
    cases.append({
        "case": path.name,
        "sha256": hashlib.sha256(data).hexdigest(),
        "source_path": match.relative_to(root).as_posix() if match else None,
        "source_revision": REVISION if match else None,
        "input_files": [s for s in sections if not s.startswith("out/")],
        "output_sections": [s for s in sections if s.startswith("out/")],
        "configuration_directives": [s for s in header.splitlines() if s.startswith("#") and not s.startswith("# ")],
        "language_versions": sorted(set(re.findall(r'language:\s*version:\s*"([^"]+)"', text))),
        "experiments": sorted(set(re.findall(r'@experiment\(([^)]+)\)', text))),
        "assertion_kinds": sorted(set(re.findall(r'@test\(\s*([\w:]+)', text))),
    })
Path("tests/conformance/manifest.json").write_text(json.dumps({"oracle_revision": REVISION, "cases": cases}, indent=2) + "\n")
print(f"Recorded {len(cases)} fixtures; {sum(c['source_path'] is not None for c in cases)} exact upstream matches")
