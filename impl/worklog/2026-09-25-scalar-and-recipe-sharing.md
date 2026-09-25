# Scalar and field recipe sharing

Continued approved phase 14 at the user's request to reduce RAM, keeping every
build, test, evaluator run and benchmark under `just safe`: 2 GiB process-tree
memory, no swap, one build/test job and 180-second timeout. No cap was raised.

The all-slot profile attributed original Odoo's arena to 1.17 million booleans,
698 thousand strings and nearly two million field entries. Reusing successful
scalar meet operands alone removed 131 thousand nodes but did not cross the
arena's capacity threshold. Sharing boolean and exact string constructors reduced
the arena from 2,752,463 nodes to 887,813, with only two booleans and 4,833 strings.
Lookup accepts borrowed text, so a cache hit need not allocate a temporary string.

Field copies now share immutable `Rc<[Conjunct]>` sequences. Adding constraints
builds a new sequence, preserving order, duplicates and lexical environments.
FieldEntry shrinks from 40 to 32 bytes. No semantic equivalence is used to dedupe
recipes, and sparse metadata always retains a fresh owner. Public graph mutation
affects aliases; fresh `alloc` remains available. String mutation and rollback
evict index keys, while boolean cache hits check the generation and payload.

Retained requested heap falls from 1,057,495,287 to 628,738,961 bytes (41%), total
requested allocations from 4.43 to 3.49 GB, and allocation calls from 52.7 to 46.8
million. Derivations stay at 48,707. The external counting allocator returns to
its 577-byte baseline after evaluator/output drop. Unique-string workloads pay
for a second owned text key; Odoo's index has 170,466 bytes of text capacity.
List caching was investigated but not implemented: the likely saving does not
justify key-vector ownership or another mutable-cache invariant at this point.

Three uninstrumented release samples per variant:

| Variant | Previous median RSS | New median / maximum RSS | Time before → after |
| --- | ---: | ---: | ---: |
| literal | 171824 KiB | 88232 / 88396 KiB | 0.38 → 0.35 s |
| refs | 301952 KiB | 137964 / 138344 KiB | 0.69 → 0.61 s |
| original | 1003900 KiB | 709968 / 710304 KiB | 2.51 → 2.31 s |

Both valid JSON outputs match the saved upstream outputs. Original rejects with
the identical diagnostic. The valid variants are below their 200/300 MiB budgets;
original is about 693 MiB, still above 512 MiB. Phase 14 remains open: inventory
execution roots, including saved frames and external IDs, before reclaiming
intermediates. Requested live heap and peak RSS must remain separate metrics.

Verification:

- Workspace tests with all features passed, plus the final added cache-growth
  regression: 231 distinct workspace tests. Six additional real-reference
  controls passed (nine conformance integration tests including three duplicates).
- Six new sharing tests cover rollback/generation reuse, public mutation, cloned
  arenas, unchanged scalar meets, separate metadata owners, field recipe snapshot
  isolation and index growth across 128 abandoned branches.
- Clippy across the workspace/all targets/all features and formatting passed.
  The checked-in opt-in profiling example ran successfully against original Odoo.
- Full corpus: 749 passed, 749 mismatches, 3383 unsupported, one oracle mismatch,
  603 not applicable; 28/547 fully verified archives. No prior observation
  regressed; all 547 fixture hashes are unchanged.
- Three new passes were investigated before baseline update. They arise because
  existing `list.Contains` compares arena IDs and shared strings now match. This
  does not establish general builtin correctness. The reference-backed open
  `contains-identity.txtar` retains an integer counterexample. Baseline/43-signal
  matrix and current status documents were updated explicitly after review.
- Legacy strict runner remains 520/547 with exactly the same 27 failure names.

Reports/logs are under `tmp/ram-*`; acceptance reports are
`tmp/ram-sharing-odoo.json` and `tmp/ram-sharing-corpus.json`. The profiler is
opt-in (`memory-profile`) and separate from production RSS measurement.
No commits, publication, or downstream pin changes were made.
