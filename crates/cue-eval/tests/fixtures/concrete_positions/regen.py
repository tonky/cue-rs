#!/usr/bin/env python3
"""Regenerate the concrete_positions goldens from the official `cue` binary.

    python3 crates/cue-eval/tests/fixtures/concrete_positions/regen.py

Goldens were produced with cue v0.17.1. For every `<name>.cue` this writes
`<name>.json` (the `cue export` output) or `<name>.err` (cue's error text)
when cue rejects the file. `comparisons.json` is the operator x value-kind
matrix: each entry is evaluated as `v: <lhs> <op> <rhs>` on its own.
"""
import concurrent.futures
import itertools
import json
import os
import pathlib
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent

# One representative per value kind, plus defaults, incomplete and bottom
# values, so every operator is exercised over every kind pairing.
VALUES = [
    "null",
    "true",
    "false",
    "1",
    "2.5",
    '"a"',
    '""',
    "'a'",
    "[]",
    '["a"]',
    "[1, 2]",
    "{}",
    "{a: 1}",
    "{a: 2}",
    "{a: 1, b?: 2}",
    "*1 | 2",
    "*[1] | [...int]",
    "1 | 2",
    "int",
    "[int]",
    "{a: int}",
    "1 / 0",
]
OPS = ["==", "!=", "<", "<=", ">", ">=", "=~", "!~"]
# Same-kind pairs where equality is decided by content.
EXTRA = [
    ("1", "==", "1.0"),
    ('["a"]', "==", '["a"]'),
    ("[1, 2]", "==", "[1, 2]"),
    ("[1, 2]", "==", "[2, 1]"),
    ("[[1]]", "==", "[[1]]"),
    ("[*1 | 2]", "==", "[1]"),
    ("{a: 1}", "==", "{a: 1}"),
    ("{a: {b: 1}}", "==", "{a: {b: 1}}"),
    ("{a: 1, _h: 2}", "==", "{a: 1}"),
    ("{a: 1, #d: 2}", "==", "{a: 1}"),
    ('"a"', "=~", '"^a"'),
    ('"a"', "!~", '"^b"'),
    ('"a"', "<", '"b"'),
    ("'a'", "<", "'b'"),
]


def run_cue(path, *args):
    proc = subprocess.run(
        ["cue", "export", str(path), "--out", "json", *args],
        capture_output=True,
        text=True,
    )
    if proc.returncode == 0:
        return json.loads(proc.stdout), None
    return None, proc.stderr.strip()


def fixture(path):
    value, error = run_cue(path)
    json_path, err_path = path.with_suffix(".json"), path.with_suffix(".err")
    for stale in (json_path, err_path):
        stale.unlink(missing_ok=True)
    if error is None:
        json_path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    else:
        err_path.write_text(error + "\n")


def comparison(case):
    lhs, op, rhs = case
    with tempfile.NamedTemporaryFile("w", suffix=".cue", delete=False) as f:
        f.write(f"v: ({lhs}) {op} ({rhs})\n")
    try:
        value, error = run_cue(f.name, "-e", "v")
    finally:
        os.unlink(f.name)
    entry = {"lhs": lhs, "op": op, "rhs": rhs}
    if error is None:
        entry["value"] = value
    else:
        entry["error"] = error.splitlines()[0]
    return entry


def main():
    cases = list(itertools.product(VALUES, OPS, VALUES))
    cases = [(l, o, r) for l, o, r in cases] + EXTRA
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count()) as pool:
        list(pool.map(fixture, sorted(HERE.glob("*.cue"))))
        entries = list(pool.map(comparison, cases))
    (HERE / "comparisons.json").write_text(json.dumps(entries, indent=1) + "\n")


if __name__ == "__main__":
    main()
