# Continuation checkpoint — 2026-09-25

The user approved phases 12–15: upstream compatibility, lower memory use and an
architectural refactor. Beneficial public Rust API breaks are permitted. Phase 12
has a usable oracle and migration gate; phases 13–15 remain in progress. See
[PLAN.md](PLAN.md), [SOURCE_MAP.md](../SOURCE_MAP.md) and the dated worklogs for
implementation details. This checkpoint covers accumulated work since d16afa0.

## Current behavior and verification

The oracle pins CUE revision `635e4bb441b29b0b8a3d754188b8edefe4012d1d`, evaluator
v3. All 547 original fixture hashes remain unchanged: 429 upstream imports and
118 local fixtures. The current migration baseline records 749 passed checks,
749 mismatches across 173 archives, 3383 unsupported checks, one oracle mismatch
and 603 non-portable checks. Only 28 archives are fully verified. The legacy
runner reports 520/547 with 27 failure names; the original 43-signal inventory
is preserved. Neither migration-baseline success nor legacy success establishes
complete CUE conformance.

The oracle disagreement is `upstream_cue_testdata_eval_required.txtar`, check
`issue3918.cue:0046`, error_paths at `issue3918.noFunction.x`. Keep it separate
from Rust mismatches; do not alter the fixture to make the disagreement disappear.

Workspace verification covers 231 tests with all features, including six new
sharing regressions, plus six additional real-reference controls. Clippy across
all targets/features and formatting passed. Strict corpus runs intentionally
exit nonzero while mismatches or unsupported verification remain.

Implemented families include exact BigInt export/arithmetic and byte octets,
shared JSON/YAML export, scalar operand defaults, scalar/package embeddings,
retained scalar/list fields and recipes, and mixed-choice closing groups that
recompute in the proper lexical context. Architectural boundaries now separate
operators, number decoding, declaration construction, metadata, scopes, expression
storage and export; this is not yet a complete Program/Session/Result design.

## Memory checkpoint

Three sequential release samples per Odoo variant, with saved upstream JSON
comparison for both valid variants:

| Variant | Median RSS before latest sharing | Current median / max RSS | Median time |
| --- | ---: | ---: | ---: |
| literal | 171824 KiB | 88232 / 88396 KiB | 0.35 s |
| refs | 301952 KiB | 137964 / 138344 KiB | 0.61 s |
| original | 1003900 KiB | 709968 / 710304 KiB | 2.31 s |

Original still rejects at `pipeline.components.account.name` with the same
incomplete cycle diagnostic. Its approximately 693 MiB peak remains above the
512 MiB target. Literal and refs meet their 200/300 MiB targets. All three median
runtimes improved; preserve the phase-14 maximum 20% runtime-regression budget.

Durable evidence, including every sample, binary/source hashes and output hashes:

- [Before scalar sharing](measurements/2026-09-25/odoo-before-scalar-sharing.json).
- [After scalar sharing](measurements/2026-09-25/odoo-after-scalar-sharing.json).
- [All-slot original profile](measurements/2026-09-25/original-arena-profile.json).

These are historical samples from one host and the external sibling repro, not
portable performance guarantees. Absolute paths identify that host. Raw scratch
logs and the external counting-allocator probe remain ignored under `tmp/`.

Original's requested live heap fell from 1,057,495,287 to 628,738,961 bytes in the
latest family, while node count fell from 2,752,463 to 887,813 and capacity from
4,194,303 to 1,048,575. This 41% heap reduction differs from the 29% peak-RSS saving.
Allocation attribution walks all arena entries, including unreachable work; it
allocates scratch sets and must not be used as the production RSS benchmark.
The external allocator probe returned to its 577-byte baseline after dropping
evaluator/output. No retained-after-drop leak was demonstrated for that workload.

Sharing already implemented: captured scope snapshots, exact immutable recipe
syntax, direct dependency sets, binding-free value conjuncts, unchanged scalar
meet operands, boolean/string constructors and immutable field recipe slices.

## Next memory investigation

Attribute retained structs and their field maps before choosing the next change.
Original retains 342,897 structs with 1,993,493 field entries, 1,252,899 distinct
recipe sequences, 168,554 captured environments and 305,742 distinct frames.
Only two boolean nodes and 4,833 string nodes remain. The string index owns a
second text copy per unique value (170,466 bytes here); entirely unique-string
workloads need separate measurement before extending interning.

Do not add a list cache just because lists are numerous. The profile has 370,319
list nodes but 72,418 distinct child-ID sequences; removing duplicates alone
would leave slot capacity in its current band, while an index retains key vectors.
Measure net ownership and runtime for any proposed replacement.

Before collecting or rolling back successful work, complete this root inventory:

| Owner or edge | What reclamation must account for |
| --- | --- |
| Public results and external `ValueId`s | Callers can hold IDs outside the evaluator; current API cannot enumerate them. Establish result ownership before automatic collection. |
| Active scopes and placeholders | Every bound value and recursive target that evaluation can still resolve. |
| Saved execution state | Rust stack locals in thunk derivation, declaration construction, retries, unification and package loading; tracing only `Evaluator` fields is insufficient. |
| Values and sparse metadata | Fields, definitions, hidden fields, patterns, lists/tails, alternatives, bounds, validators, recursive targets, metadata inputs/views and grouped conjuncts. |
| Recipes and captured environments | Value conjuncts, all captured scope bindings and imported package IDs; shared frame identity must avoid duplicate tracing. |
| Imports and package loading | Current/file-captured imports and parent execution state while the arena is moved into a sub-evaluator. |
| Closed-copy and scalar caches | Define weak-cache tracing/eviction explicitly. Generational validity alone is not an ownership policy. |
| Allocation trail | Supports branch rollback; it is not a semantic root set. Marking all trail entries would retain everything. |

`ExpressionStore` contains syntax/dependency data, not evaluated results; verify
that distinction remains true as lowering evolves. Partial reachability numbers
from the scratch probe omit private and active execution roots and must not be
treated as a live/dead classification. General expansion/work budgets, synthetic
scaling, import diamonds and repeated-request measurements remain phase-14 work.

## Compatibility work to pick up

1. `list.Contains` currently compares arena IDs. Shared strings incidentally made
   three observations pass in `comprehensions_nestembed` and `issue3996`; this is
   not a complete builtin fix. The reference-backed
   [open repro](../tests/conformance/open/contains-identity.txtar) shows
   `list.Contains([7], 7)` still returning false. Implement concrete value equality
   with defaults, numeric equality and optional-field semantics considered. The
   evaluator's fixpoint comparator includes metadata/closedness and is not a
   drop-in CUE Equals implementation. Issue3996's reduced list-equality case is
   also still unsupported.
2. Continue general field-admission provenance and pending evaluation/scheduling.
   Mixed-choice recipe groups fix a specific boundary; they do not replace those
   models. Preserve lexical scope and closing provenance during later merges.
3. Replace f64 for exact decimal semantics. BigInt already fixes integer parsing,
   overflow and integer operations; that does not establish exact decimal math.
4. Complete builtin contracts, including higher-order values (`list.Sort` ignores
   its comparator and `list.Ascending` is not modeled as a value). Avoid blanket
   argument-error propagation until those contracts are established.
5. Extend structural observation and typed diagnostics to retire unsupported
   checks honestly. A resource limit or unknown comparison must not become a
   semantic contradiction or a successful assertion.

Use phase 13's family workflow: minimize a case, check the pinned oracle, cover
neighbors/ordering/imported scopes, then compare the full observation matrix.
Review any changed status before updating the baseline. Move fixed open repros
to `regressions/` and include them in real-reference controls.

## Invariants and API changes to preserve

- Semantic equality cannot deduplicate recipes: identical values can retain
  different lexical bindings or closing boundaries.
- Scope snapshots are immutable; only the active frame copies on write. Shared
  syntax/direct dependencies do not share lexical let closures or evaluated results.
- Metadata input declarations and materialized selector views remain separate.
  Closing groups recompute using their own declarations, close, then meet outside
  constraints. Cached bottoms with a recipe can still resolve later.
- `ValueArena::bool` and `string` may return shared IDs. `alloc` is always fresh,
  including metadata owners. `get_mut` changes all aliases and removes a string
  cache entry before mutation; rollback removes keys with their owners. Boolean
  cache hits validate generation/payload. Cache membership must never add metadata
  to a shared scalar. `FieldEntry::conjuncts` is now `Rc<[Conjunct]>`.
- `ValueId` has a slot generation but no store identity. Owned results or borrowed
  views, typed evaluation states, resolver boundaries and structured diagnostics
  remain phase-15 work. No `Send`/`Sync` promise is implied by `Rc`-based ownership.

Enve is the direct Rust consumer; enact consumes its CLI. Their adoption/integration
still needs checking, and no downstream pins were changed. Enve's YAML branch
must use `cue_eval::export::json_to_yaml` instead of generic Serde serialization
of arbitrary-precision JSON numbers. Byte AST constructors now take `BytesLit`
with octets. Respect the user's dirty sibling checkout and keep publication/pin
changes separate from local compatibility verification.

## Safe reproduction

Run heavy commands sequentially through `just safe`: the default process-tree
cap is 2 GiB, swap is disabled, timeout is 180 seconds, and build/test jobs are
limited to one. Do not raise the cap for the current Odoo variants. If user
cgroups are unavailable, establish an equivalent process-tree memory boundary
before evaluating; a filesystem sandbox alone does not bound RAM.

```sh
just safe cargo test --workspace --all-features
just safe cargo clippy --workspace --all-targets --all-features -- -D warnings
just safe cargo fmt --all -- --check
just safe just oracle-build
just safe just oracle-test
just safe just conformance-test
just safe just conformance-baseline
just safe just conformance
just safe cargo build --release -p cue-cli
just safe python3 tools/benchmark-odoo.py ../cue-rs-odoo-repro --reference-dir tmp/odoo-measurements --output tmp/odoo-benchmark.json
just safe cargo run --release -p cue-eval --example memory_profile --features memory-profile -- ../cue-rs-odoo-repro/original
```

The benchmark reference directory must contain independently generated upstream
`literal.upstream.json` and `refs.upstream.json` for the same repro sources. Those
local files and the sibling repro are not part of this repository; recover them
before claiming a new reference-checked measurement. Without `--reference-dir`,
the benchmark checks repeated-output consistency only. Use the durable source
hash to detect a changed workload, and rebuild the CLI after source changes.
