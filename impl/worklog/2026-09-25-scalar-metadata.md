# Scalar metadata and embedded recipes

## Scope and design

Continuation of approved phases 13/15 after the plain-embedding family. Pinned
probes confirm `{3,#unit:"kg"}` exports 3 and retains `.#unit`; scalar/list values
also retain hidden/optional fields and patterns. `{#x:1}` imposes struct kind,
whereas `{_,#x:1}` can constrain a scalar's metadata. Embedded expressions such
as `{#x+1,#x:*1|int}` must update when #x is constrained, including through imports,
nested merges and choices.

ValueId now owns a semantic payload plus optional sparse arena metadata. `get`
reads the payload; `fields` exposes its retained fields. Metadata holds an arena
struct ID and the original embedded conjuncts. A side map keeps ordinary arena
slots unchanged in size. Rollback removes the metadata with its owning node.
Closing/copying/package selection preserve that ownership. Scalar operators,
list indexing and builtin argument handling continue reading normal payloads.

Unification combines metadata fields and retains recipes separately from their
cached values. The evaluator re-derives the recipes in their lexical environment
under the merged fields. Cached failures do not prematurely collapse parents or
eliminate speculative branches. Metadata-aware equality keeps distinct field
sets separate. Re-evaluation compares payloads without allocating a throwaway
copy. All-struct choices absorb common fields before closing.

API note: low-level `unify` retains recipes but does not execute them; use
Evaluator::unify_and_rederive for values whose cached results depend on lexical
inputs. The raw mutable arena API remains low-level; the planned owned result/
handle migration is not complete.

## Validation and rejected intermediates

- Six new API tests cover payload/selector consistency, specialization/source
  isolation, lexical scope and partial self-reference, nested/disjunctive recipes,
  invalid metadata neighbors and arena rollback.
- Eight new pinned-reference fixtures cover those cases plus imported metadata,
  optional constraints, closed choices and indexed-list errors.
- 220 ordinary workspace tests pass. All six ignored reference controls were
  explicitly run: 226 unique tests total. The reference binary reports nine
  passes, including three ordinary controls already counted in the workspace.
- Clippy with warnings denied and formatting/diff whitespace checks pass.
- All 547 original corpus hashes preserved. Final report:
  `tmp/metadata-corpus-final.json`. 20 new passes: two mismatches and 18 unsupported
  checks now pass, with no previous pass regressing and no new mismatch.
  Counts: 745 passed, 754 mismatches across 176 archives, 3382 unsupported,
  one reference-annotation disagreement, 603 not applicable; 27 whole archives.
- Initial corpus review caught a closed all-struct choice losing common optional
  fields; fixed before accepting the family. A newly reachable out-of-range list
  access exposed an untyped error; it now reports Conflict and has a reference
  regression. The earlier `metadata-corpus.json` is not the accepted baseline.

Logs: `tmp/metadata-workspace.log`, `tmp/metadata-oracle-final.log`,
`tmp/metadata-clippy.log`, `tmp/metadata-corpus-final.log`.

## Memory and legacy observations

All evaluator/build/test runs stayed under `just safe`: 2 GiB, no swap, bounded
process-tree runtime. Three optimized runs per Odoo variant still produce the
saved upstream JSON for literal/refs; original rejects normally. Median/max RSS
(KiB): literal 171508/171684, refs 301760/301920, original 1003120/1003352.
Median times: 0.41/0.70/2.65 seconds. RSS is unchanged in practical terms; timings
are slightly slower than the preceding samples. Original still exceeds its
512 MiB target. `tmp/metadata-odoo.json` records hashes and all measurements.

Legacy strict results are 520/547, one additional failure name (`comprehensions_issue4448`)
relative to the plain-embedding family. The reference export and the observable
port assertion pass, with no conformance regression. The legacy runner expects
any error from an archive that records an incomplete unused definition; working
hidden-field selection now lets export succeed, so it complains about the missing
error. Keep the legacy result visible rather than changing the heuristic or
breaking the valid export. Its four unsupported observations remain verification
debt. Log: `tmp/metadata-legacy.log`.

## Remaining work

The reference-backed mixed scalar/struct closed-choice repro is retained at
`tests/conformance/open/metadata-mixed-choice.txtar`. Common metadata and a struct
branch need their own field-admission provenance; the current merge incorrectly
restricts the branch's fields. Do not solve this by globally opening metadata or
unioning admission across distinct conjuncts. Broader pending-branch scheduling,
closedness, per-file bindings, exact decimals and diagnostic adapters remain in
phase 13. Phase 14 still needs allocation attribution and live-root/lifetime work.
No commit, publication or downstream pin changes were made.
