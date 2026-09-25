# Phase 12 implementation

The user approved the roadmap with “go”. Beneficial Rust API breaks remain
permitted, although this phase changes test tooling and CLI integration rather
than evaluator semantics. No commit or publication was requested.

## Provenance and reference

Queried upstream revision metadata for the corpus import date, downloaded the
August 28 snapshot into tmp, and compared every import's bytes. All 429 match
635e4bb441b29b0b8a3d754188b8edefe4012d1d exactly. This resolves the earlier
v0.16.1 mismatch; no corpus fixture was replaced. Added a manifest for all 547
hashes, source paths/revisions, input files and language/experiment metadata.

The test-only Go adapter pins that commit and evaluator v3, with locked module
checksums. Inspected the matching inline runner's equality, error and package
loading semantics. Literal equality and concrete export are distinct operations;
abstract CUE equality is not approximated by JSON serialization. Unsupported
structural/diagnostic/configuration/golden checks remain individually visible.

One reference annotation disagrees with the observed incomplete error path:
issue3918.noFunction.x expects _foo.out, while the error reports
issue3918.noFunction._foo.out. The upstream inline path check is conditional
on Value.Err, which omits that incomplete bottom. The adapter inspects the
bottom's diagnostic and records an oracle disagreement instead of silently
accepting the annotation or calling it a Rust mismatch.

The pinned Value.Expr implementation panics on some builtin-produced lists.
The adapter now inspects the evaluated core's disjunction/conjunction structure
without reconstructing source conjuncts. A real list.Repeat regression covers it.

## Runner and observations

Moved legacy policy from cue-cli into cue-test-harness behind an evaluator
backend. Removed duplicate evaluation in strict mode. Legacy directory failures
and empty directories return nonzero. The archive reader preserves section
newlines, rejects duplicate/unsafe paths and excludes out/ and module metadata
from the CUE source iterator. The new package runner still materializes module
metadata and local import trees.

The harness owns comparisons, typed outcome statuses and baseline policy. The
reference and cue-rs worker run in child processes with deadlines; just safe
provides the process-tree memory cap. Child output goes to files to avoid pipe
deadlocks. Hash/revision checks reject stale provenance. Unsupported, malformed,
crashed or timed-out checks cannot count as passes. Backend-specific statistics
and sharing checks are explicitly outside portable semantics.

When the user asked what semantic_equal compared, clarified that its operands
were serde_json::Value observations, not evaluator Value nodes, and renamed it
json_values_equal. Exact decimal canonicalization treats 2, 2.0 and 20e-1 alike
without f64 rounding or exponent expansion. It preserves distinct large integers
and never equates numbers with strings. Typed observation wrappers distinguish
value forms; unknown abstract forms cannot pass through JSON export instead.

A baseline contains check identities, paths, hashes and outcomes, not generated
expected CUE values. It refuses infrastructure failures, changed observation
paths and empty reports; the CLI refuses to overwrite its own baseline. Updating
the baseline is an explicit review action, never a test side effect.

## Results and limits

All 547 archives have a recorded plan and outcome. Initial matrix:

- 515 passed checks;
- 1,096 mismatching observations across 201 archives;
- 3,270 unsupported checks;
- one reference-annotation disagreement;
- 603 implementation-only checks outside portable semantics.

Twenty-five archives have all applicable checks verified. These numbers are
not independent bug counts: one package failure can fail many observations,
and unsupported verification is not proof of incorrect evaluator behavior.
The original 43 signals are mapped individually in tests/conformance; legacy
strict results remain 504/547 with exactly the same failure names.

Phase 13 must finish missing adapters alongside compatibility work. Confirmed
new signals include large integers exported as strings, builtin output/value
differences, and package/root evaluation failures. No RAM improvement is claimed;
phase 14's original allocation targets remain outstanding.

## Validation

189 workspace tests pass, with two additional real-reference integration tests
run through just conformance-test. The pinned Go adapter suite passes, including
wrong values, wrong error paths/categories, numeric spelling, large integers,
multi-file packages, builtin lists and incomplete diagnostic paths. Rust controls
also cover missing errors, unsupported directives, process timeout/crash, bad
fixtures, stale provenance and baseline corruption/observation changes.

All runs used 2 GiB process-tree caps, disabled swap and command deadlines.
Clippy with warnings denied, formatting and diff whitespace checks pass. Strict
acceptance intentionally exits nonzero; the separately named migration baseline
reproduces the recorded outcomes. No source fixture or sibling consumer changed.
