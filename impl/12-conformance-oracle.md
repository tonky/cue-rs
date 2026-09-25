# Phase 12: trustworthy conformance results

Status: implemented and validated through the initial assertion baseline.
Unsupported adapters are individually recorded debt for phase 13; full
conformance acceptance intentionally remains failing.

## Purpose and scope

Make upstream compatibility measurable before changing evaluator behavior.
The current 504/547 count is a legacy harness result, not semantic parity.
This phase changes the test harness and its CLI integration; it introduces
only the evaluator observation interfaces necessary to test actual behavior.

The user permits beneficial public API changes. Enve is a direct Rust
consumer; enact currently executes enve cue export. Cover both boundaries.

## Evidence at d16afa0

- The corpus has 429 upstream-prefixed archives and 118 other archives.
  The legacy strict runner reports 43 failures in the imported subset.
- 314 archives contain @test(eq), 174 contain @test(err), 28 contain
  @test(closed), and 8 contain @test(allows). The runner ignores these
  field-level assertions.
- 380 archives contain out/eval/stats. These are implementation statistics,
  not expected evaluated values.
- The runner exports files individually through one reused evaluator instead
  of reproducing each fixture's package and test operation. A first unrelated
  error can satisfy an entire expected-error archive.
- Errors can also be recorded in out/evalalpha, not only out/errors.txt.
  The basictypes fixture is an example that the current classifier misses.
- A directory run reports failures but exits successfully. A case without
  recorded errors can be evaluated twice in strict mode.
- Compared with cached cuelang.org/go v0.16.1, only 14 imported archives match
  byte-for-byte, 384 differ, and 31 have no corresponding path. This does not
  establish the imported revision or prove the differing cases changed
  semantics. Several contain newer inline-test annotations.
- builtins_056_issue314 expects incompleteness on fields inside abstract
  definitions while a concrete instance is valid. Requiring whole-file JSON
  export to fail is the wrong observation for those assertions.

## Stage 1: provenance, operations and an honest runner

1. Preserve all existing fixtures and content hashes. Record original path,
   source revision when recoverable, language version, experiment flags,
   package layout, assertion kinds and intended operation in a case manifest.
   Find the matching upstream source revision before claiming its behavior as
   the oracle. Installed v0.16.1 is a provisional executable reference only.
   Unrecoverable provenance is explicit, never silently rewritten.
2. Separate operations: parsing/compilation, partial evaluation, validation,
   concrete export and formatting. Keep incomplete schemas distinct from
   invalid configurations. Treat module files as module metadata.
3. Move runner policy out of cue-cli into cue-test-harness. Preserve the
   syntax -> evaluator dependency direction: use a backend interface or a
   separate integration target so production cue-eval does not depend on its
   harness. Avoid a circular normal/dev dependency.
4. Model results explicitly: passed assertions, semantic mismatches,
   unsupported checks, oracle/version mismatches, crashes and resource limits.
   None of the last four can count as a pass.
5. Return nonzero for unexpected failures and incomplete verification in
   strict acceptance mode. Provide a separately named baseline-comparison
   mode during migration: it detects regressions but does not claim full
   conformance. Reports retain stable case IDs and per-assertion coverage.

## Stage 2: assertion adapters and the reference engine

- Build a pinned upstream adapter under the same external process limits.
  Use public CUE APIs where sufficient. For newer inline directives, inspect
  the matching upstream runner before implementing their meaning.
- Start with concrete value equality and expected errors at a selected path.
  Compare canonical typed values, not pretty-print spacing or map order.
  Error checks match the selected operation, path and stable category.
- Extend to abstract values, defaults, optional/required/hidden fields,
  definitions, closedness, kinds and allowed-field probes. Abstract equality
  must not be reduced to JSON export or evaluated by cue-rs on both sides.
- Keep backend-specific pointer-sharing and allocation statistics separate
  from portable language semantics. An unimplemented semantic directive is
  visible verification debt, not a skip that turns the fixture green.
- Cover multi-file packages and imports in isolated local fixture trees.
  Compare sets of diagnostics so a wrong sibling error cannot satisfy a test.

## Stage 3: baseline and gates

- Generate an assertion-level matrix for all 547 archives. Reclassify the
  original 43 signals into confirmed engine defects, adapter defects and
  version/configuration mismatches, keeping each case accounted for.
- Add end-to-end negative controls: a wrong value, missing expected error,
  wrong path/category, unsupported directive, subprocess timeout and bad
  fixture must each be visible and must fail strict acceptance.
- Record coverage and verified values separately from execution counts.
  No automatic expected-output updates from cue-rs results.
- Add just recipes for a single case, an incompatibility family, the full
  corpus and baseline comparisons, all compatible with just safe.

## Completion criteria

Every case has a declared operation and verification status. The runner
cannot report a pass without checking its supported semantic assertions;
unsupported assertions and unresolved provenance are listed individually.
Strict acceptance fails for remaining debt; the regression gate can compare
an explicitly recorded baseline while phase 13 removes it. A value mismatch
and an error at the wrong path demonstrably fail the harness.

No claim that there are exactly 43 semantic bugs survives this phase:
stronger checks can uncover failures currently counted as passes.

## Implemented result

The matching upstream revision is 635e4bb441b29b0b8a3d754188b8edefe4012d1d:
all 429 imports match byte-for-byte. The manifest records all 547 archive hashes,
original paths, source revisions, package files and configuration metadata.
The Go reference adapter pins that commit and evaluator v3. It is test tooling,
not a dependency of cue-eval.

Runner policy moved into cue-test-harness with a subprocess protocol; cue-cli
provides the Rust observation adapter. Each child has a deadline and the common
just safe workflow supplies the process-tree memory boundary. Archive extraction
rejects unsafe/duplicate paths, preserves newlines and excludes output files from
inputs. Legacy checks execute once, retain their 504/547 result and exit nonzero
on failure.

Supported observations include literal trees, concrete JSON export, kinds,
closedness and selected-path error presence/categories. Error-path checks are
extracted and reference-checked, but the Rust adapter explicitly refuses to
claim full diagnostic paths. Abstract/default/hidden/optional/pattern equality,
detailed diagnostics, legacy golden formats and configuration adapters remain
recorded as unsupported. Backend-specific debug/sharing/stats are outside the
portable denominator. No unchecked assertion becomes a pass.

The full matrix has 515 passed checks, 1,096 mismatches, 3,270 unsupported checks,
one oracle disagreement and 603 non-portable checks. Mismatches occur in 201
archives; 25 archives have all applicable checks verified. This does not mean
there are 1,096 independent engine bugs or 522 semantic failures. The original
43 signals remain individually mapped in tests/conformance/legacy-signals.md.

One incomplete-error annotation in eval_required expects _foo.out while the
reference records issue3918.noFunction._foo.out. Upstream's inline runner skips
that path check when Value.Err omits an incomplete bottom. We retain the
reference disagreement rather than blaming cue-rs or accepting it silently.

Negative controls cover wrong values, missing errors, wrong paths/categories,
unsupported checks, timeout, crash, malformed fixture, changed provenance,
baseline overwrite and changed observation paths. Real-reference controls cover
multi-file packages, selected errors and a large-number export mismatch.
189 workspace tests pass; two additional reference-dependent integration tests
and the Go adapter tests pass. Clippy and formatting are clean. Validation uses
2 GiB process-tree caps and disabled swap; no evaluator semantics were changed.

The fixture/op/check inventory and strict exit behavior meet this phase's gate.
Phase 13 must improve both adapter coverage and engine behavior; it cannot use
the migration baseline as a substitute for conformance acceptance.
