# Phase 14: reduce evaluation memory and allocation churn

Status: roadmap approved; implement against phase 12's semantic gate.

## Measured starting point

Phase 11 fixed a non-converging cycle diagnostic. The remaining Odoo cost is
retained state and allocation churn, measured at d16afa0 using a temporary
counting allocator and independent peak-RSS measurements.

| Metric | Valid refs | Invalid original |
| --- | ---: | ---: |
| Live requested heap after loading | 560,017,858 B | 1,920,130,355 B |
| Total requested allocation bytes through loading | 2,731,395,553 B | 7,346,730,147 B |
| Allocation/reallocation calls through loading | 33,320,418 | 94,103,146 |
| Re-derivations | 12 | 48,707 |
| Unsettled structs | 0 | 0 |
| Values reachable through semantic contents | 34,561 | 30,421 |
| Values reachable including recipes and captured values | 101,113 | 412,813 |
| Distinct reachable recipe environments | 11,218 | 41,833 |
| Bindings retained by those environments | 820,873 | 1,941,538 |
| Expression nodes in distinct reachable thunk AST roots | 223,869 | 936,245 |

The valid case exports 981,976 bytes of compact JSON. Dropping its evaluator
leaves approximately 7 MB of allocated JSON objects; dropping the output
returns to the approximately 600-byte probe baseline. The invalid case also
returns to baseline. This experiment does not show an allocator leak
persisting beyond evaluator ownership.

Counts of reachable values exclude evaluator-private roots and unreachable
arena entries; they are not a complete live/dead classification. Requested
heap bytes exclude allocator overhead and are not RSS. Allocation profiling
is diagnostic, not the reference for timing performance.

Current uninstrumented optimized RSS: refs 576,956 KiB, literal 348,692 KiB,
original 2,184,656 KiB. The original is invalid and must still be rejected.

## Stage 1: reproducible allocation and liveness profiles

- Add opt-in evaluation statistics for arena allocations, live slots,
  retained capacity, recipe/env counts, captures, derivations and branch work.
  Avoid production-cost instrumentation when not enabled.
- Attribute allocation stacks using an available profiler, or isolate
  capture/clone/merge operations with counters where tooling is unavailable.
- Inventory every arena root: public results, execution frames, recipes,
  imports, recursive targets and caches. Document lifetime and rollback
  invariants before reclaiming any node.
- Add reproducible release benchmarks for all Odoo variants, scaled
  components, import diamonds, repeated merges, disjunction width, deep
  references and repeated requests in one process. Record time, RSS, total
  allocation bytes and retained bytes separately.

## Stage 2: share immutable inputs

- Compile/lower source expressions once and reference them through typed
  expression IDs or shared immutable storage. Stop cloning an expression's
  entire subtree into each field recipe.
- Represent captured lexical frames as shared immutable snapshots with
  narrowly scoped overlays. Preserve the environment a recipe was written
  in; merging must not rebind a name from another lexical scope.
- Intern symbols where the profile justifies it and reuse immutable
  dependency sets. Cache dependency analysis per compiled expression.
- Make package identity and per-file import bindings explicit, enabling
  safe reuse of a package loaded through multiple import paths.

## Stage 3: reduce intermediates and bound ownership

- Reuse unchanged immutable values and avoid full struct/conjunct clones
  when a merge changes a small subset of fields.
- Reclaim abandoned work at proven lifetime boundaries. Evaluate region
  ownership or traced reclamation after stage 1 identifies all roots.
  Raw rollback across a successful evaluation is not safe: IDs can escape
  through scopes, recipes and caches.
- Separate compiled program lifetime, one evaluation request, speculative
  branches and returned results. A completed export should not require
  retaining all recipes and intermediate results.
- Budget large expansions and work explicitly. Return a resource-limit
  error without disguising it as an ordinary CUE conflict or incomplete
  value. Logical engine budgets complement external process memory caps;
  they are not an exact guarantee on all allocator RSS.

## Proposed acceptance budgets

On the same release-build benchmark host, with semantic results unchanged:

- Valid refs: peak RSS at or below 300 MiB.
- Valid literal: peak RSS at or below 200 MiB.
- Original invalid input: normal diagnostic within 512 MiB.
- No more than 20% median time regression for any of these cases, with
  repeated measurements and the same oracle/binary build conditions.
- Repeated evaluations release request-owned storage; growth cannot track
  the number of completed requests after outputs are dropped.
- Deterministic allocation/capture growth tests complement environment-
  dependent RSS thresholds. Synthetic scaling must not show quadratic scope
  capture or uncontrolled disjunction multiplication.

These are proposed targets for approval, not achieved results. If the
profile invalidates the budgets or design, report evidence and revise the
plan rather than calling a raised cap a memory optimization.

Continue using just safe: 2 GiB by default and 3 GiB only for the known
baseline original. Keep swap disabled and command deadlines in place.

## First ownership change: shared scope frames

After the concrete-export family establishes the updated semantic gate,
replace `Vec<HashMap<String, ValueId>>` captures with a stack of shared immutable
`ScopeFrame` snapshots. Source inspection found only two map mutation sites:
`insert_binding` and `remove_binding`; both target the active last frame.
Snapshot/capture/re-derivation currently clone the full stack's maps. Make a
frame clone share its map, and copy only the active frame on an actual mutation.
Equal insertions and missing removals must avoid copying. Encapsulate the map
behind a type with read-only public access and crate-private writes; do not
introduce shared interior mutation.

Invariants: a captured name keeps the ValueId it had at capture time; writing
or removing a name cannot change existing captures; stack order, push/pop,
struct-scope depth, dynamic retries and recipe imports remain identical. No
ValueId is reclaimed and no cache/root lifetime changes in this step. Frames
own IDs, not arena values, so sharing cannot create an Rc ownership cycle.
Existing shadowing, imports, forward-reference and repeated-merge regressions
plus explicit snapshot isolation tests guard the invariant. Re-run the whole
matrix and measure all Odoo variants before accepting memory improvements.

The first shared-frame measurement reduces RSS by about 30%: literal 245208 KiB,
refs 397912 KiB, original 1524724 KiB. All outputs/verdicts match and the 547-case
gate is unchanged. No target is yet met. The next inspected retention source is
`eval_field_value`: every constant also retains a cloned AST and its environment.
`recipe_deps` conservatively walks every identifier (including nested literals,
patterns, loops, interpolation and transitive lets). An empty dependency set
means the expression cannot observe changed bindings. Store that result as the
existing `Conjunct::Value` variant, preserving it as a constraint in later
merges; retain full thunks for all expressions with any dependency. This avoids
retaining dead captures without arena reclamation or weakening dependency rules.
Test that constant constraints still survive merges beside recomputed fields,
and that dependent fields keep their lexical captures. Measure and compare again.

## Expression sharing design

Recipe AST roots are already `Rc<Expr>`, but `eval_field_value` creates a fresh
deep clone on every evaluation of the same template. Introduce an evaluator-owned
`ExpressionStore` that interns immutable expressions by exact AST equality.
Use derived `Eq`/`Hash` on the syntax tree, including literal kinds, defaults,
labels and formatting metadata; hashes only index candidates and equality must
confirm a match. This is syntax identity, not semantic value equivalence.
Environments, imports and transitive-let dependency sets remain attached to each
thunk independently; identical syntax in different lexical scopes must not share
bindings or results. No evaluated value is memoized.

The store owns one Rc per distinct submitted tree for the evaluation session;
thunks share those trees. Session drop releases store ownership; imported
thunks may keep their trees alive after the imported evaluator has returned.
Expressions contain syntax only, with no arena IDs or backreferences, so no
ownership cycles arise. This is an incremental immutable-input boundary, not a
complete lowered Program/ExprId IR or a long-lived global cache. Test repeated
syntax under different scopes and imported aliases, check the full matrix, and
measure before accepting it. Arena reclamation remains deferred until all live
roots and execution frames have an explicit ownership model.

## Measured ownership result (2026-09-25)

Implemented shared immutable scope frames, binding-free value conjuncts and a
session-owned exact-syntax store. Added capture isolation and merge/recipe
regressions. All 200 ordinary workspace tests and three pinned-reference
integration controls pass; all 547 corpus outcomes match the reviewed phase-13
baseline. No fixture or expected result was changed for memory work.

`tools/benchmark-odoo.py` / `just benchmark-odoo` now records binary/source hashes,
three sequential samples, RSS, elapsed time, exit verdicts and stable JSON
hashes. Optional saved upstream JSON comparison was enabled for this run.
All three variants now run under the default 2 GiB cap.

| Variant | Before this family, KiB (single run) | After, KiB (median of 3) | After maximum, KiB | Before / after seconds |
| --- | ---: | ---: | ---: | ---: |
| literal | 349288 | 182068 | 182516 | 0.68 / 0.41 |
| refs | 577436 | 316712 | 316804 | 1.16 / 0.72 |
| original | 2185604 | 1084112 | 1084156 | 4.04 / 2.62 |

Valid outputs match saved upstream JSON; original gives the same normal
rejection. The literal target is met in these samples. Refs remains about
309 MiB against 300 MiB, and original about 1.03 GiB against 512 MiB. The
phase is therefore not complete. These comparisons use a historical single-run
baseline, not a repeated before/after median experiment.

Next ownership work needs allocation attribution across all arena slots and an
explicit live-root/execution-frame inventory before reclaiming intermediates.
The current store shares complete AST roots, not all subtrees, and still walks
syntax and allocates dependency sets per recipe. Dependency-analysis caching
and a compiled expression representation remain candidates. Do not introduce
blind rollback or semantic-equality-based recipe deduplication: both can lose
retained IDs or lexical provenance.

The counting-allocator probe confirms release after ownership ends. Requested
heap after loading: refs 360296561 B, original 1118213213 B; total allocation
requests: 1508689480 B / 17711895 calls and 4501335126 B / 54486478 calls.
Dropping evaluator then output returns to the probe's 573/577-byte baseline.
These are requested heap bytes, not RSS, and include reserved arena capacity.
Reachable recipe syntax shrinks to 212 / 214 distinct roots (1742 / 1771 AST
expression nodes) from 223869 / 936245 expression nodes in phase 12. These walks
still do not classify all arena slots or evaluator-private roots.

## Cache recipe dependency analysis

After scalar compatibility corrections, inspect the remaining immutable recipe
inputs: every recipe still creates its own HashSet of dependency names, even
when its AST is shared. Extend the session expression store with cached direct
dependency sets. A recipe with no referenced local let bindings can share the
cached set. If it references local lets/aliases, compute their transitive closure
from a copy; never mutate the cached direct set or share a closure across lexical
environments merely because expression syntax matches. Empty dependency sets
continue to use value conjuncts without interning constant trees.

The dependency walk stays conservative and its public behavior is preserved.
Test shared syntax with different let closures, subsequent merges and capture
isolation. Compare the whole corpus and repeated Odoo measurements; this is
immutable-input sharing, not evaluated-value caching or arena reclamation.


## Dependency-cache measurements

The cache preserves every scalar-family corpus outcome exactly. Three release
samples give median/max KiB: literal 171728/171732, refs 301516/301768, original
1003840/1003876. Median seconds are 0.38, 0.70, 2.53. Saved upstream JSON still
matches for valid variants and original rejects normally. Literal and refs meet
the approved 200/300 MiB targets. Original remains about 980 MiB, above 512 MiB;
phase 14 is not complete. These runs and all builds/tests used the default 2 GiB
process-tree cap. See the scalar/dependency-cache worklog for verification.

Plain embedding family gate: three capped release samples per Odoo variant,
with saved upstream JSON comparison. Median literal 171960 KiB / 0.38 s,
refs 301952 KiB / 0.68 s, original 1003600 KiB / 2.47 s. Effectively unchanged
from the dependency-cache results. Comprehension construction state merges
directly, avoiding an extra arena allocation for each completed body.
`tmp/embedding-odoo.json` records hashes and all samples. Original remains above
the 512 MiB target; the two valid variants still meet theirs.

Scalar metadata family gate: three release samples per variant under the same
2 GiB cap, with upstream JSON comparison. Median/max RSS (KiB): literal
171508/171684, refs 301760/301920, original 1003120/1003352. Median times:
0.41/0.70/2.65 seconds (previous family 0.38/0.68/2.47). RSS is effectively
unchanged; these timing samples are slightly slower, without a controlled
same-run before/after attribution. Sparse metadata avoids increasing each arena
slot's size, and payload comparison avoids allocating an annotation-free copy.
Report: `tmp/metadata-odoo.json`. Original reclamation remains open.

## Mixed choice normalization gate (2026-09-25)

Normalizing fixed scalar/struct alternatives adds sparse selector views only
where common fields accompany a choice. It adds no metadata to ordinary values.
Normalization has an active-pair and depth guard, tested with a branching cycle
and an over-deep acyclic graph through the public mutable arena. Reclamation of
intermediate arena nodes remains separate work.

Final release, three sequential runs per variant with saved reference JSON:
literal median 171900 KiB / 0.38 s (max 172068 KiB), refs 301868 KiB / 0.68 s
(max 302096 KiB), original 1003648 KiB / 2.49 s (max 1003916 KiB).
Outputs/verdict are unchanged. Compared with the preceding metadata family,
RSS is effectively unchanged; original still exceeds its 512 MiB target.
Report: `tmp/mixed-choice-odoo.json`.

## Dynamic recipe groups gate (2026-09-25)

Whole-expression closing groups and branch-local selector views remain sparse
arena metadata. Three capped final release runs per variant preserve outputs and
verdict. Literal median 171824 KiB / 0.38 s (max 171964 KiB); refs 301952 KiB /
0.69 s (max 302188 KiB); original 1003900 KiB / 2.51 s (max 1004056 KiB).
RSS is effectively unchanged from the fixed-choice family. Original remains
about 980 MiB; allocation attribution/root inventory and reclamation are still
needed to reach 512 MiB. Report: `tmp/dynamic-choice-odoo.json`.

## Current allocation attribution

Continue the approved profiling stage with an opt-in `memory-profile` feature.
Walk every allocated arena slot (not only the exported graph), count value kinds,
slot capacity, field/conjunct storage and distinct captured environments/frames.
Use the existing external counting-allocator probe for requested heap ownership;
keep RSS/time acceptance on the uninstrumented release CLI. This is read-only
profiling: no node reclamation until all execution roots are accounted for.
Choose the next representation change from these measurements, preserving values,
recipes, scopes, arena generations and rollback behavior.


The first measured optimization reuses the left operand when two plain concrete
scalars unify successfully. Metadata dispatch remains ahead of this path, so
lexical recipes are still combined. This matches existing identity behavior for
top, equal types and type/concrete meets, avoids new aliases beyond the arena's
existing shared-value model, and does not introduce interning caches or change
rollback lifetimes. The original profile contains 1,171,789 booleans among
2,752,463 slots (capacity 4,194,303), making redundant scalar meets a substantial
candidate. Measure this independently before changing the arena representation.


Boolean constructor sharing reduces original Odoo to 1,456,539 nodes and
804,981,011 requested live heap bytes; only two boolean nodes remain. These weak
cache IDs are checked against the generational arena and current payload on every
hit. Fresh `alloc` and metadata allocations keep their existing behavior; public
mutation of shared graph nodes is documented explicitly.

Next, share field recipe sequences as immutable `Rc<[Conjunct]>` slices. The
profile counts 1,993,493 field entries and 3,510,987 conjunct capacity across their
cloned vectors. Copying a field should share its exact recipe sequence and lexical
environments; appending constraints constructs a new sequence. This preserves
conjunct order and duplicate constraints, with no semantic deduplication. Add
snapshot-isolation coverage for field copies and merged constraints; count unique
recipe allocations in the profile. Keep sparse metadata recipe ownership intact.


The duplicate profile finds 573,559 string nodes containing only 4,833 distinct
texts. Extend constructor sharing to exact strings, with an arena-local index.
Remove the index entry before mutable access and when rollback removes its owner;
never index fresh `alloc` or metadata owners. The index owns a second copy of each
unique text (a tradeoff for workloads of entirely distinct strings), but does not
retain keys from abandoned branches. Test nested rollback, reused slots, mutation,
cloned arenas and separate scalar metadata. Empty lists number only 348, so adding
another singleton cache there is not justified by this profile.


## Scalar and field recipe sharing gate (2026-09-25)

Implemented equal-scalar meet reuse, shared boolean/string constructors, borrowed
string lookup before allocation, and immutable field recipe slices. Fresh arena
allocations and metadata owners are never interned. String cache entries are
removed on mutable access and owner rollback; boolean hits validate both their
generation and current payload. Graph mutation affects existing aliases, with
fresh `alloc` available for independent ownership. No nodes are reclaimed outside
existing speculative rollback.

| Original Odoo allocation metric | Before | After |
| --- | ---: | ---: |
| Allocated slots | 2,752,463 | 887,813 |
| Arena capacity | 4,194,303 | 1,048,575 |
| Requested heap after loading | 1,057,495,287 B | 628,738,961 B |
| Total requested allocations through loading | 4,429,501,560 B | 3,485,110,751 B |
| Allocation/reallocation calls through loading | 52,706,067 | 46,787,482 |
| FieldEntry size | 40 B | 32 B |

There are 4,833 string cache keys using 170,466 bytes of text capacity. Derivations
stay at 48,707 with zero unsettled structs. Dropping the evaluator and output
returns the external allocator probe to its 577-byte baseline. This is request
ownership evidence, not proof that arena intermediates have bounded lifetimes.

Uninstrumented release, three sequential samples per variant, saved upstream
JSON checked for both valid cases and original diagnostic unchanged:

| Variant | Previous median KiB | Median KiB | Maximum KiB | Median seconds |
| --- | ---: | ---: | ---: | ---: |
| literal | 171824 | 88232 | 88396 | 0.35 |
| refs | 301952 | 137964 | 138344 | 0.61 |
| original | 1003900 | 709968 | 710304 | 2.31 |

Peak RSS medians improve by 49%, 54%, and 29%; all median runtimes improve.
The original remains about 693 MiB, above 512 MiB. Requested live heap and peak
RSS are different measurements: the 41% live-heap reduction is not the RSS saving.
Remaining work includes live-root inventory, retained struct/field storage and
safe reclamation, with general work budgets/scaling still open.

The full corpus has three new passing observations in two fixtures, traced to
the pre-existing identity-based `list.Contains` implementation becoming correct
for shared strings. This is not a complete builtin fix; a reference-backed open
integer counterexample is recorded. No existing observation regressed, all fixture
hashes are unchanged, and the three outcomes were explicitly reviewed before
updating the migration baseline. The legacy 27 failure names remain unchanged.

Evidence: `tmp/ram-sharing-odoo.json`, `tmp/ram-sharing-corpus.json`, allocation
probe logs `tmp/ram-before-profile.log` and `tmp/ram-list-duplicates-profile.log`.
Reproduce allocation attribution using the checked-in `memory_profile` example
under `just safe` with `--features memory-profile`. The profile walks all slots;
its scratch allocations must not be included in production RSS acceptance.
