# Shared evaluation inputs

Continued approved phase 14 against phase 13's reviewed assertion baseline.
Read allocation measurements and mutation sites before changing ownership.

1. Encapsulated lexical maps in `ScopeFrame`. Captures share maps; active-frame
   insertion/removal copies shared storage first. No-op writes avoid a copy.
   Original Odoo RSS fell from 2185604 to 1524724 KiB; refs from 577436 to 397912.
2. Binding-free expressions now keep `Conjunct::Value` instead of cloned syntax,
   dependency sets and captured environments. Constraints still participate in
   later merges. The dependency walk remains conservative. Original fell to
   1414520 KiB; refs to 373140.
3. Added session-owned `ExpressionStore` using exact AST equality and hash lookup.
   Repeated syntax shares storage but environments/imports/results remain
   separate. AST Eq/Hash preserves literal kinds, defaults and source-form
   metadata. This is syntax identity, not CUE semantic equivalence or a value
   cache. Let expressions use the same store.

Tests cover capture isolation, no-op writes, constant constraints surviving
merges, dependent nested fields retaining lexical captures, and identical
syntax under different bindings. Existing import/shadowing/forward-reference
and re-derivation regressions all pass. An initial new test used arithmetic on
an unresolved default, exposing an existing phase-13 numeric/default gap; the
ownership test uses a direct dependent reference instead. That compatibility
gap remains tracked, rather than being altered as part of memory optimization.

Added a sequential Odoo benchmark with binary/source hashes and optional saved
upstream comparisons. Three final runs under the default 2 GiB cap give median
RSS/time: literal 182068 KiB / 0.41 s; refs 316712 KiB / 0.72 s; original
1084112 KiB / 2.62 s. Maximum RSS across the runs: 182516, 316804 and 1084156 KiB.
Valid outputs equal saved upstream JSON; original rejects normally. Only the
literal target is met. Original memory is roughly halved, but still above the
512 MiB target; refs is just above 300 MiB. Historical baselines are single
runs, so these are not controlled repeated before/after median comparisons.

200 workspace tests plus three real-reference integration tests pass. Full
547-case corpus outcomes are unchanged from phase 13; formatting and Clippy
are clean. Source map now documents current ownership and known limits instead
of claiming all corpus cases pass or that handles identify their arena.
No commits, downstream checkout edits or dependency-pin changes were made.

Next: attribute retained arena allocations and make execution roots explicit
before attempting reclamation. Further immutable-program lowering and cached
dependency analysis remain open. All three requested roadmap goals remain
active; the concrete-export family and these ownership steps are completed
increments, not completion of phases 13-15.

Final allocator probe: refs retains 360296561 requested bytes after loading
(from 560017858), original 1118213213 (from 1920130355). Total allocation
requests fall to 1.51 GB / 17.7 million calls and 4.50 GB / 54.5 million calls.
Both return to their sub-kilobyte starting allocation count after evaluator
and output drop. Requested bytes include untouched capacity and are not RSS.
Legacy strict harness remains 504/547 with the same 43 failure names; this
preserves their investigation history and is not claimed as conformance.
