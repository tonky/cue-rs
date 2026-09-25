# Conformance baseline

The 547 fixtures are unchanged. `manifest.json` records SHA-256 hashes, source
paths/revisions, package files, language/experiment metadata and assertion kinds.
All 429 imports exactly match upstream commit
`635e4bb441b29b0b8a3d754188b8edefe4012d1d`; the other 118 fixtures have local
provenance. The oracle uses that revision's v3 evaluator.

`baseline.json` records check identities, observation paths and outcomes for all
cases. It is a migration gate, **not a conformance pass**. It contains no generated
expected CUE values. Run `just safe just conformance` to produce a full report
with diagnostic details; strict acceptance fails while mismatches, unsupported
checks or oracle disagreements remain. Run `just safe just conformance-baseline`
to detect changes from the recorded outcomes. Crashes and resource failures
cannot satisfy a baseline, even when they match an earlier result.

The old 43 failure names remain in `legacy-signals.json` with their original
complaints; `legacy-signals.md` maps each to the new observations. The legacy
runner currently reports 520/547 and exits nonzero for failures. New checked failures
can occur in cases the legacy runner passed.

`passed` means the particular observation was compared. `unsupported` means
verification remains incomplete, not that cue-rs necessarily implements the
language feature incorrectly. A case passes only if all applicable checks pass
and at least one check was performed. Debug/pointer-sharing/allocation checks are
reported as `not_applicable` and are outside portable language semantics.
Multiple failed checks in one archive can share one evaluator defect.

After reviewing a changed full report, record its migration outcomes explicitly:

```sh
python3 tools/conformance-report.py tmp/conformance-report.json --write-baseline
```

That command never changes fixtures or expected CUE values. It does not run
as part of tests or CI. See `tools/conformance-oracle/README.md` for building
the test-only reference engine and running one case or a family.

`regressions/` holds reduced phase-13 cases separate from the unchanged corpus.
The real-reference integration tests run these through the pinned adapter with
`just safe just conformance-test`.

`open/` holds reduced, reference-backed cases that still fail or lack verification.
They are not passing regression controls and are not included in the unchanged
547-fixture baseline. Each is linked from the implementation follow-up list.
