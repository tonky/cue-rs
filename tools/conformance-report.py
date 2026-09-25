#!/usr/bin/env python3
"""Summarize a reviewed report; optionally record an explicit migration baseline.

No CUE fixtures or expected values are generated or rewritten. A baseline
records verification debt and mismatches, not permission to call them passes.
"""
import argparse
from collections import Counter
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("report", type=Path)
parser.add_argument("--write-baseline", action="store_true")
args = parser.parse_args()
report = json.loads(args.report.read_text())
counts = Counter(check["status"] for case in report["cases"] for check in case["checks"])
if any(counts[s] for s in ("crash", "resource_limit", "invalid_fixture")):
    raise SystemExit("Resolve infrastructure failures before recording a baseline")
if args.write_baseline:
    cases = [{**case, "checks": [{**check, "detail": ""} for check in case["checks"]]} for case in report["cases"]]
    data = '{\n  "oracle_revision": ' + json.dumps(report["oracle_revision"]) + ',\n  "cases": [\n'
    data += ',\n'.join('    ' + json.dumps(case, separators=(',', ':')) for case in cases)
    Path("tests/conformance/baseline.json").write_text(data + '\n  ]\n}\n')

legacy = json.loads(Path("tests/conformance/legacy-signals.json").read_text())
by_name = {case["case"]: case for case in report["cases"]}
lines = ["# The 43 legacy failure signals", "",
         "These are the original phase-11 failure names, retained without deleting or",
         "rewriting fixtures. Counts describe observations, not independent bugs: one",
         "package-load failure can fail many checks. Unsupported adapters are verification",
         "debt, not semantic passes.", "", "| Case | New check outcomes |", "| --- | --- |"]
for entry in legacy:
    case = by_name[entry["case"]]
    status = Counter(check["status"] for check in case["checks"])
    lines.append('| ' + entry["case"] + ' | ' + ', '.join(f'{n} {key}' for key, n in sorted(status.items())) + ' |')
lines += ["", "Original complaints are in `legacy-signals.json`. All 547 cases and check IDs",
          "are recorded in `baseline.json`; regenerate full diagnostics with",
          "`just safe just conformance` (written to `tmp/conformance-report.json`).", "",
          "Whole-file missing-error complaints are not preserved as assertions. Inline",
          "errors are checked at their annotated paths: incomplete definitions can coexist",
          "with a valid export. Scalar-root and package-loading differences remain visible",
          "under package/export operations.", "",
          "Phase 13 must address confirmed observation mismatches and the individually",
          "recorded abstract-value, diagnostic, golden and configuration adapter debt.",
          "This matrix does not declare every old signal to be an evaluator bug."]
Path("tests/conformance/legacy-signals.md").write_text('\n'.join(lines) + '\n')
print(json.dumps(dict(sorted(counts.items())), indent=2))
print("Cases with mismatches:", sum(any(c["status"] == "mismatch" for c in case["checks"]) for case in report["cases"]))
