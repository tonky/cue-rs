# Scalar operators and dependency caching

Continued the approved compatibility/memory/architecture roadmap after the user
asked what remained and requested fixes. Kept all builds, tests, corpus workers
and benchmarks under `just safe` (2 GiB, swap disabled, deadline). No commits or
downstream revision changes.

## Investigation and implementation

Pinned-reference probes confirmed default operands were ignored, integer `/`
truncated, float division by zero escaped evaluation, unary plus accepted strings,
and div/mod/quo/rem were absent. A large SI literal triggered an actual i64
multiplication panic in the worker. Reference sign/scale probes and pinned Go
sources established the required behavior before editing.

Extracted scalar execution into operators.rs and literal decoding into number.rs.
Unique defaults are selected at concrete operand boundaries while unification
keeps constraints. Operations retain incomplete dependencies for later
re-derivation; concrete contradictions have typed errors. Exact BigInt paths cover
radix/SI literals and Euclidean/truncated integer division. `/` is floating-point
and zero divisors are checked first. Mixed comparisons preserve the integer
rather than converting it to f64; float storage still limits decimal precision.

One corpus comparison caught a regression from globally propagating every
builtin argument error: list.Sort's currently unmodeled comparator value was
previously ignored by its implementation. Removed that unrelated dispatch change;
integer division propagates its own operand errors. Higher-order builtin values
and comparator handling remain open. Direct builtin type literals also need a
proper compilation boundary to distinguish compile-time rejection from an
incomplete field reference. Reduced reference fixtures keep compile-invalid
cases separate from incomplete-reference cases.

Dependency analysis was the next inspected retention source: shared ASTs still
had individually allocated dependency sets. ExpressionStore now caches direct
identifier sets. Recipes share them unless local lets add transitive dependencies;
that expansion happens on a separate set for each lexical scope. Binding-free
recipes still retain only values. A regression with identical alias syntax but
different transitive let bindings guards against sharing the wrong closure.

## Verification

Compared all 547 unchanged corpus fixtures: 99 additional passing checks,
comprising four corrected mismatches and 95 previously unsupported error checks.
No passed check regresses and no new mismatch appears. Counts are now 616 passed,
1091 mismatch (198 archives), 3174 unsupported, one oracle disagreement, and
603 not applicable; only 25 archives are fully verified. Memory caching reproduces
the scalar report exactly. Updated the migration baseline after reviewing changes.

207 workspace tests pass, plus four explicitly-run pinned-reference integration
tests (211 unique). Clippy and formatting pass. New API regressions cover default
overrides in both merge orders, incomplete arithmetic refined by a later
constraint, integer division signs and huge operands, overflow-safe literals,
precise errors, mixed comparisons and a cyclic public-arena default. Original
corpus expected values were not changed.

Three capped release samples with saved-upstream output comparison give medians:
literal 171728 KiB / 0.38 s, refs 301516 KiB / 0.70 s, original 1003840 KiB /
2.53 s. Maxima are 171732, 301768 and 1003876 KiB. Both valid variants now meet
their 200/300 MiB targets. Original still rejects normally but is above its
512 MiB target. Benchmark data includes source/binary hashes in
`tmp/scalars-deps-odoo.json`; prior measurements are retained separately.

## Remaining scope

Compatibility still needs scalar/embedded roots, package/file binding, closedness
provenance, pending disjunctions/cycles, exact decimal arithmetic, several builtin
operators and richer typed/source diagnostics. The harness still has structural,
diagnostic and golden-output verification gaps; these remain explicitly unsupported.
Original-case retained memory still needs allocation attribution and explicit
execution roots before safe reclamation. Owned result handles, compiled program
lowering and downstream YAML migration remain open architectural work.
