# Pinned reference adapter

This test-only Go executable uses CUE revision
`635e4bb441b29b0b8a3d754188b8edefe4012d1d` with evaluator v3. All 429 imported
archives match that revision byte-for-byte. `go.mod` and `go.sum` pin its
source/dependencies; it is not a dependency of the Rust evaluator.

From the repository root:

```sh
just safe just oracle-build
just safe just oracle-test
just safe just conformance-test
just safe just conformance
```

The last command intentionally fails while semantic mismatches or unsupported
verification remain. `just safe just conformance-baseline` checks the recorded
migration baseline; its success is **not** conformance acceptance. Use
`just safe just conformance-family definitions` or pass one archive to
`just conformance` for narrower investigations.

The adapter loads root CUE files as a package with isolated overlays, preserving
module metadata and local imported packages. It reads inline attributes using
the pinned parser. It checks literal equality, kinds, closedness and selected
error properties against the reference before requesting a Rust observation.
It also requests concrete JSON export where upstream supports it. The Rust
worker receives independent requests; it cannot set its own pass verdict.

JSON comparison is limited to exported JSON and observation protocol data.
Numbers compare as exact decimals without rounding through f64. Structural
CUE equality is a separate operation: defaults, disjunctions, abstract values,
optional/hidden/definition fields and patterns cannot be reduced to JSON.
Unimplemented structural checks, detailed diagnostics and configuration/golden
adapters are reported as unsupported. They prevent a case from passing.
Reference assertion disagreements are reported separately from Rust mismatches.
Upstream pointer-sharing/debug/statistics checks are outside portable semantics.

Each subprocess has a deadline. Run under `just safe` for the process-tree
memory limit; the per-process deadline is not a memory boundary. Reports are
written to `tmp/conformance-report.json`. The runner verifies revision and
archive hashes; the standard recipes also require the provenance manifest.
Fixtures and expected results are never rewritten by a run.

To reproduce provenance from the pinned source checkout:

```sh
python3 tools/conformance-manifest.py /path/to/pinned/cue
```

That command refuses imports without an exact upstream byte match and rewrites
only the manifest. Review manifest changes whenever fixtures change. The
legacy `test-txtar --strict-errors` command remains available for historical
comparison and currently exits nonzero for 27 failures (520/547). The original
43 failure signals remain in `tests/conformance/legacy-signals.json`.
