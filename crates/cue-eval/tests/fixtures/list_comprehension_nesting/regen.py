#!/usr/bin/env python3
"""Regenerate the list_comprehension_nesting goldens from the official `cue` binary.

    python3 crates/cue-eval/tests/fixtures/list_comprehension_nesting/regen.py

Goldens were produced with cue v0.17.1. For every `<name>.cue` this writes
`<name>.json` (the `cue export` output) or `<name>.err` (cue's error text)
when cue rejects the file.
"""
import json
import pathlib
import subprocess

HERE = pathlib.Path(__file__).resolve().parent


def fixture(path):
    proc = subprocess.run(
        ["cue", "export", path.name, "--out", "json"],
        capture_output=True,
        text=True,
        cwd=HERE,
    )
    json_path, err_path = path.with_suffix(".json"), path.with_suffix(".err")
    for stale in (json_path, err_path):
        stale.unlink(missing_ok=True)
    if proc.returncode == 0:
        value = json.loads(proc.stdout)
        json_path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    else:
        err_path.write_text(proc.stderr.strip() + "\n")


def main():
    for path in sorted(HERE.glob("*.cue")):
        fixture(path)


if __name__ == "__main__":
    main()
