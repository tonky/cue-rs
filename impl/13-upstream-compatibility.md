# Phase 13: remove upstream incompatibilities

Status: roadmap approved; depends on phase 12's assertion-level baseline.

## First implementation family: concrete serialization boundaries

The verified baseline found a large integer exported as a string and divergent
bytes/encoder behavior. Source inspection also found duplicated JSON traversal:
the builtin converter ignores optionality and defaults. Before changing root
unification, consolidate concrete serialization into an observation module.
Preserve exact integer tokens, encode bytes as base64 in JSON, reject nonfinite floating-point results, and use the same default/optional policy for exports and
JSON/YAML builtins. JSON decoding must retain integer tokens as BigInt.

The harness's arbitrary_precision feature also exposes serde_json::Number's
private serialization protocol to generic YAML serializers. Introduce an explicit
JSON-to-YAML adapter and route workspace callers through it. Represent ordinary
numbers through Serde's numeric methods; if YAML's serializer cannot represent
an exact number, emit the document as JSON (a YAML 1.2 flow representation), never
round it or expose internal marker maps. Document the corresponding enve call-site
migration; its current dirty checkout and revision pins remain untouched.

This family needs API-level regressions for large positive/negative integers,
byte sequences, defaults/ambiguity, optional fields, open lists, JSON round trips
and YAML scalar types. The pinned oracle exports the known prefix of an open
list, so that existing behavior must be preserved. Pin reference behavior with the existing oracle. Compare
the full matrix and Odoo workloads before accepting the family. General decimal
arithmetic and exact YAML presentation remain separate work.

## Goal

Resolve the current imported-corpus failure signals and semantic failures
uncovered by the corrected harness. Preserve all original cases. A case is
closed with an upstream-backed regression and a recorded explanation, not
by broadening an error heuristic or dropping an assertion.

Fixes are grouped by invariant, not one conditional per fixture. Each stage
starts with minimal reproductions and an operation-correct oracle result.
Detailed design is refined from phase 12's verified inventory before editing
each affected subsystem.

## Stage order and ownership

1. **Compilation, loading and top-level values.**
   Correct package assembly, file-scoped aliases, module metadata and scope
   binding. Handle scalar embeddings without assuming every file or embedded
   expression is a struct. Audit optional versus required field syntax,
   label aliases and source spans in cue-syntax as failures require.
   Current signals include packages_embed/issue398/sub, export_000..003,
   eval_basictypes and scalars_emptystruct/embed_bound.
2. **Evaluation state and propagation.**
   Distinguish a dependency still waiting, a valid abstract constraint,
   a terminal contradiction and an execution resource limit. Fix forward
   lets, partial branches, dependency changes through lists/disjunctions,
   comprehension sources and field existence checks. Preserve valid cycles
   constrained by a later conjunct. A pass budget is not evidence of a
   semantic cycle or of successful convergence.
3. **Unification, defaults and closedness.**
   Track which conjunct admits a field rather than only a union of allowed
   names. Preserve viable pending disjuncts; check defaults, branch
   elimination and subsumption against the oracle. Separate value equivalence
   from recipe identity and unresolved comparison. Add cycle-safe graph
   tests and operand-order cases where the language requires commutativity.
4. **Builtins, interpolation and validation.**
   Cover bytes interpolation, incomplete arguments to encoders/templates,
   argument defaults and selected stdlib behavior identified by the matrix.
   Audit validation of nested fields, definitions and pending values using
   the appropriate validation options, not root-bottom checks alone.
5. **Diagnostics and remaining corpus cases.**
   Carry typed categories and source/path information from the failing
   operation to the API and CLI. Check all expected diagnostic paths rather
   than whichever error was returned first. Audit cases the old runner passed
   for the wrong reason. Keep formatting compatibility separate from
   evaluation semantics.

## Tests and release gates

- Every bug has a minimal regression plus its original corpus assertion.
- Test both valid and invalid neighbors, declaration/operand order, imported
  scope, merging, incomplete values and cycles where relevant.
- Each family must improve the verified baseline without new semantic
  regressions. Run the affected regressions, then workspace tests, formatting
  and Clippy; run the full corpus at family boundaries.
- Run Odoo original/refs/literal and representative enve/enact workloads.
  Track memory/time throughout this phase; stop to fix a regression rather
  than defer all performance consequences until phase 14.
- Do not promise Go's exact internal allocation counts, pointer-sharing IDs
  or debug printer output as Rust compatibility. Report such checks outside
  the portable semantics denominator.

## Completion criteria

All 43 original signals are explained and resolved under their correct
operations. All portable semantic checks in the selected imported corpus
are implemented and pass against the recorded upstream revision; any newly
discovered incompatibility remains open work, not a silently accepted
baseline. The regression suite and downstream integrations pass.

The target revision and experiment matrix come from phase 12. This does not
claim support for every future upstream language extension or complete
coverage of upstream's entire repository.

## Concrete serialization outcome (2026-09-25)

Implemented the shared `cue_eval::export` boundary. Function argument default
selection now also requires a unique default; several defaults previously picked
the first before the encoder could reject the ambiguity. Bytes literals carry
`BytesLit { value: Vec<u8>, form }`, preserving `\xff` as one octet rather than
UTF-8 for U+00FF. The formatter preserves octets through reparsing; non-UTF-8
literals fall back to a quoted escaped spelling.

Pinned-reference regressions pass for exports, encoders and ambiguous calls.
The unchanged 547-fixture matrix improves two checks with no check regressions:
515 -> 517 passed, 1096 -> 1095 mismatches, 3270 -> 3269 unsupported. The changed
checks are `definitions_exclude/regress.cue:0009` (unique-default handling now
exposes the expected value) and `crypto_sha256/:0001` (base64 byte export).
The reviewed migration baseline records these outcomes; 25/547 entire archives
remain fully verified. General decimal arithmetic, floating-point presentation
inside encoded strings and exact YAML presentation for enormous numbers remain
open. Nonfinite floats now error instead of silently exporting JSON null; this
is not arbitrary-precision decimal support.

Odoo outputs still match saved upstream JSON for refs and literal; original
still rejects. Release runs: literal 0.68 s / 349288 KiB; refs 1.16 s /
577436 KiB; original 4.04 s / 2185604 KiB. These are effectively unchanged from
phase 11 and do not satisfy phase 14's reduction targets.

Caller migration: workspace CLI and wasm YAML entrypoints use
`cue_eval::export::json_to_yaml`. When enve adopts this checkout, change its
`crates/enve-cue/src/evaluator.rs` `export_formatted` YAML branch from
`serde_yaml_ng::to_string(&val)` to `cue_eval::export::json_to_yaml(&val)`.
Its active dirty checkout and dependency pins have not been modified. Enact
consumes enve's CLI. AST consumers constructing `Expr::Bytes` must now provide
`BytesLit` rather than `StringLit`.

## Next family: scalar operands and integer arithmetic

Investigation after shared-input work confirms: arithmetic/unary operations do
not resolve defaults; integer `/` truncates; float division by zero produces a
nonfinite value; unary plus accepts strings; div/mod/quo/rem are absent; radix
literals use i64 and scaled literals can panic on i64 multiplication. Pinned
reference probes verify defaults, ambiguity error categories, Euclidean versus
truncated signs and integer-kind SI literals. Fractional multipliers must give
an exact integer (1.5Ki works; 1.1Ki is rejected by upstream compilation).

Extract operator execution from evaluator orchestration. Select unique defaults
only for operations needing operands, preserving constraints in unification and
bounds. Ambiguous/no-default choices remain incomplete; pending/bottom operands
retain their cause. Guard cycles when following public arena disjunctions. Keep
re-derivation recipes so a later override updates an operation that read a
default. Calls and interpolation use the same operand selection.

Use BigInt end-to-end for radix/scaled integers and div/mod/quo/rem. `/` produces
floating division, matching the pinned evaluator's float kind. Reject zero
divisors before calculation and remove fabricated zero/one conversion fallbacks.
Compare mixed integers/floats without rounding the integer through f64. Ordinary
float arithmetic still uses f64: exact decimal arithmetic and values outside its
range remain open, with range failures reported honestly. Cover all divisor-sign
combinations, large integers, default overrides, invalid neighbors and ambiguity
with API tests and pinned-reference regressions; compare the whole matrix and
Odoo before accepting the family. This refines already-approved phases 13/15.

Reference: pinned `internal/core/adt/decimal.go` and `cue/literal/num.go`; public
operator semantics at https://cuelang.org/docs/reference/spec/ and
https://cuelang.org/docs/howto/use-the-built-in-functions-div-mod-quo-rem/.

## Scalar family outcome (2026-09-25)

Implemented operator/default selection in `operators.rs` and numeric literal
parsing in `number.rs`. Defaults are selected at arithmetic/unary/call/interpolation
boundaries; ambiguous choices remain incomplete and arithmetic results still
re-derive after a later input constraint. Integer `/` no longer truncates.
Division-by-zero, invalid unary plus and integer builtin argument errors are
reported during evaluation. `div`/`mod` and `quo`/`rem` preserve their respective
sign rules with BigInt. Radix and exact SI literals no longer narrow through i64;
large SI multiplication no longer panics. Mixed comparisons avoid converting a
large integer to f64 before comparing it.

Compared with the preceding baseline: 99 additional passing checks, consisting
of four corrected mismatches and 95 previously unclassified error observations.
No passed check regresses and no new mismatch is introduced. Current counts:
616 passed, 1091 mismatches across 198 archives, 3174 unsupported, one reference
annotation disagreement and 603 not applicable. Still 25 complete archives;
unsupported structural/diagnostic/golden checks prevent the other whole-case
passes. All 547 original fixtures are preserved.

Reduced cases cover signs, defaults/overrides, incomplete schemas later
constrained, precise error categories, large radix/SI integers and division.
The default resolver also terminates on a cyclic disjunction constructed through
the public arena. The wider workspace, reference controls and capped Odoo runs
pass. Detailed validation and memory changes are in the scalar/dependency-cache
worklog.

Limits: float storage/arithmetic still uses f64, including decimal decoding;
nonfinite/range failures remain implementation limitations, not CUE conflicts.
Some direct uses of builtin type literals are rejected during upstream compilation
where cue-rs evaluates them as incomplete constraints. Abstract field references
and direct type literals must be distinguished by a future compilation boundary.
Compound/list equality, byte/string operators, scalar roots, per-file bindings,
closedness, pending branches and diagnostic paths remain separate work.

## Scalar roots and plain embeddings: first slice

Pinned-reference probes confirm file roots and nested declaration blocks can be
scalars, lists, defaults or abstract constraints. Empty `{}` remains a struct;
`{_}` remains top. Multiple embeddings meet in either declaration order, and a
scalar beside a regular field is a conflict local to that value. `!=null` accepts
non-null structs, lists and scalar kinds, including when used as an embedding.

Introduce a private declaration accumulator that keeps the structural body and
embedded constraints separate until the declaration block is finished. Preserve
an explicit struct embedding as a structural constraint even if it has no fields.
Continue using existing scope/relaxation/re-derivation for declarations. Return a
ValueId from root/nested declaration evaluation, and migrate both package loading
paths so neither allocates an unconditional root struct. Preserve typed bottoms
for embedding contradictions rather than aborting observations of sibling fields.

This is an incremental part of the approved top-level-values stage. Scalars with
definitions, hidden/optional fields and patterns require persistent metadata plus
embedding recipes on value vertices; that larger model is not implemented in
this slice. Keep those existing failures explicit, never erase metadata to make
JSON pass. Test plain roots, nested values, defaults, lets, multiple constraints,
empty-struct versus top, invalid neighbors, package entrypoints, and non-null
bounds against the pinned oracle. Run the complete matrix and capped memory gates.

Full-corpus review extended this slice to comprehension bodies: they can yield
scalars and disjunctions too. Merge the declaration accumulator directly into
its parent, avoiding an extra retained arena struct per iteration. A pending
embedded constraint must retry with the enclosing comprehension; an unbound
definition on a literal's final pass must remain pending for its outer retry
loop. Reference-backed cases cover forward lets, nested forward definitions,
closed struct branches and a scalar/struct disjunction narrowed by regular fields.

### Plain embedding outcome (2026-09-25)

Implemented and verified: scalar/list/default/constraint roots; ordinary nested
embeddings; non-null bounds across value kinds; multiple constraints; local typed
conflicts; scalar/disjunction comprehension bodies; forward embedding retries.
The original export_000..003 observations now pass. All 547 fixtures are unchanged.

109 additional verified passes, with no previous pass regressing or new mismatch:
725 passed, 756 mismatches across 178 archives, 3400 unsupported; 27 complete
archives. Of the 335 removed mismatches, 105 now pass and 230 now reach unsupported
abstract observations instead of failing package loading. Four formerly unsupported
checks also pass. Do not count the 230 as compatibility successes.

214 workspace tests plus five explicit pinned-reference controls pass; Clippy and
formatting clean. Odoo median RSS: literal 171960 KiB, refs 301952 KiB, original
1003600 KiB; outputs/verdict unchanged, all under 2 GiB. This slice preserves the
memory improvements but does not reduce original Odoo to its 512 MiB target.
Scalar metadata and embedded recipe re-derivation remain open. The worklog records
validation, including the changed legacy heuristic failures.

## Scalar fields and embedded recipes

Pinned probes confirm that scalar/list values retain definitions, hidden fields,
optional fields and patterns; those do not themselves impose struct kind when
an explicit value is embedded. `{3, #x:1} & {#x:1}` conflicts (the right side is a
struct), while `{3, #x:1} & {_, #x:1}` retains the scalar. Embedded arithmetic must
be re-derived against merged metadata, including overriding defaults in either
operand order and applying an additional scalar constraint afterwards.

Retain a sparse, arena-owned metadata record only for values requiring it. A
ValueId identifies both its semantic payload and its retained fields/embedded
conjuncts. Plain value reads remain payload reads, keeping scalar operators and
builtin argument handling consistent without wrapping every scalar match. Add
an explicit field view for selectors/observations. Arena rollback removes metadata
with its owning node; copied values must preserve it where identity is retained.
Ordinary struct-only Odoo workloads must not gain per-node metadata allocations.

Unification merges retained fields and concatenates embedded conjuncts, preserving
the distinction between an explicit top and an implicit struct. The evaluator
re-derives embedded recipes in their original lexical environment under the merged
fields, then checks all scalar constraints. A cached incomplete/contradictory
recipe result must not become an irreversible conjunct. Equality and branch
comparison include retained fields; export reads only the semantic payload.
Audit closing, pending traversal, selectors, observation, defaults and rollback.
This refines the approved value-store/evaluation boundaries; broader scheduling
and branch lifecycle work remains in phases 13/15.

The metadata review also pins a remaining mixed-kind choice/closedness case in
`tests/conformance/regressions/metadata/mixed-choice.txtar` (originally under
`open/`, fixed by the next slice below). At this stage, homogeneous struct choices
merge common fields before closing; a mixed scalar/struct definition still needs
branch-specific admission provenance. Keep this visible under stage 3 rather
than opening all scalar metadata bodies, which would admit invalid fields.

### Scalar metadata outcome (2026-09-25)

Implemented sparse arena metadata, payload/field views, lexical embedded recipes,
scalar/list selectors and package selectors, metadata-preserving copies/closing,
metadata-aware equality and rollback. Cached recipe results can be replaced after
merging their inputs; they do not collapse a parent or eliminate a choice before
re-evaluation. Contradictory definition/hidden fields reject the value; absent
optional constraints do not. All-struct choices incorporate common fields before
closing. List index errors exposed by valid list embeddings now carry Conflict.

The unchanged corpus gains 20 passing checks (two prior mismatches and 18 prior
unsupported observations), with no regressed pass or new mismatch. Current gate:
745 passed, 754 mismatches across 176 archives, 3382 unsupported, one reference
annotation disagreement and 603 not applicable. Still 27 whole archives verified.
220 workspace tests and six explicit reference controls pass; Clippy and formatting
clean. Odoo outputs/verdict preserved under 2 GiB, median RSS 171508/301760/1003120
KiB for literal/refs/original. Mixed scalar/struct choice admission and the wider
closedness/scheduling work remain open; see follow_up.md and the metadata worklog.

## Mixed choice admission: binding-free normalization

The saved mixed-choice repro and new pinned-reference neighbors confirm that a
struct branch and the fields declared beside it form one closing boundary.
Common optional fields must be admitted in every struct branch; fields from two
different alternatives must not be pooled. Open branches and nested closing must
retain their existing meanings. Top with metadata must still reject undeclared
fields when read through a definition.

For embedded choices with constant conjuncts or unshadowed predeclared type
bindings, distribute common fields into the alternatives
before closing. Each alternative retains its own constant conjunct and fields.
Keep a distinct selector-only common-field view on the resulting disjunction;
it must never act as an extra closed conjunct during unification. Model this
separately from embedded recipes so the invariant is explicit. Preserve the view
through copying, closing, opening for embedding and re-derivation, and remove it
with its arena owner during rollback.

Do not distribute a whole-expression thunk by copying it into every branch or by
replacing it with the currently evaluated branch: either can change future default
overrides. Dependency-bearing mixed choices remain a tracked follow-up requiring
branch recipe provenance. This is a bounded refinement of approved stages 13/15.


### Mixed choice outcome (2026-09-25)

The original saved repro and all 29 observations in four reduced fixtures now
match the pinned reference. These cover both operand orders, separate branch
admission, optional/pattern fields, nested closed structs, open branches,
embedding, branch-private metadata, common selectors after narrowing and lexical
shadow/default specialization controls. The original repro is now a passing
regression; a new dynamic-choice repro records the remaining branch-recipe gap.

`MetadataSource` distinguishes embedded conjuncts from a selector-only choice
view. Unification distributes the actual branch constraints and merges the view
separately. Closing/opening, re-derivation, semantic comparison and rollback
preserve that distinction. Constant alternatives retain their own conjuncts;
unshadowed builtin type references can normalize, while changing bindings keep
their recipes. Payload and field-selector APIs stay the same. `UnifyContext` gains a
normalization recursion set; callers constructing it directly should use its
constructor or `Default`. Cyclic and over-deep choice graphs terminate under
the shared normalization guard.

The 547-fixture corpus has exactly the preceding observation statuses: 745 passed,
754 mismatches, 3382 unsupported, one oracle disagreement and 603 not applicable;
27 whole archives verified. No fixture hashes or baseline outcomes changed.
223 workspace tests plus six explicit reference controls pass. This fix addresses
the reduced open case, not additional original-corpus assertions.

Final capped release Odoo medians: 171900/301868/1003648 KiB and
0.38/0.68/2.49 seconds for literal/refs/original, with saved reference outputs
and rejection verdict preserved. RSS is effectively unchanged. The legacy
harness remains 520/547 with the same 27 failure names. Detailed logs and the
remaining dynamic-choice case are recorded in the mixed-choice worklog.

## Dynamic mixed choices: retain recipe closing boundaries

The next approved-stage refinement retains the whole embedded expression rather
than manufacturing branch recipes from cached alternatives. Separate the current
materialized value from the recipe group that produced it. A closed recipe group
records its original common field declarations and whether closing is recursive,
outer-only or recursive with the outer boundary reopened for embedding.

On a later meet, merge the recipe inputs and retain each closed group as a
conjunct. Re-evaluate that group against the merged inputs, project those inputs
onto its original common declarations, join them to its freshly evaluated value,
and apply its closing boundary before meeting outside conjuncts. Incoming fields
must not expand that group's admission. Preserve the group even when its cached
payload is a struct or bottom; defaults and branch values can still change.
Selectors on a materialized struct read its actual fields; recipe inputs remain
separate. Fixed choices keep the previous normalization path.

Reference probes cover scalar and struct specialization in both operand orders,
source isolation, competing branches, extra/conflicting fields, nested closing,
open branches, embedding and composed definitions. Validate the existing metadata
and full corpus gates plus capped Odoo measurements. This extends approved phases
13/15; general scheduling and arena reclamation remain separate work.

### Dynamic choice outcome (2026-09-25)

Implemented closed recipe groups and separate recipe-input/materialized-field
views. Re-derivation retains the whole expression, refreshes a group's original
declarations, applies its closing boundary, then meets outside conjuncts. Cached
structs and bottoms keep their recipes. Explicit `{}` and regular declarations
preserve a struct-kind constraint without freezing their current field values.
Re-evaluated embedding thunks reopen their outer value before the receiving
literal adds its fields and closes; the original corpus caught that requirement.

The saved repro is now `regressions/dynamic-choice/basic.txtar`. All 34 observations
in six reduced fixtures pass, covering scalar/struct/default specialization,
operand order, source isolation, dynamic labels, nested closing, open branches,
patterns, independent closed definitions, embedding, explicit struct constraints
and branch-private selectors. Two API tests supplement the reference controls.

Full corpus: 746 passed, 751 mismatches across 174 archives, 3384 unsupported,
one oracle disagreement and 603 not applicable. Still 27/547 complete archives.
One former mismatch now passes (`definitions_033_Issue_#153/in.cue:0001`). Two
`builtins_matchn` observations moved from missing paths to unsupported abstract
values; they are not verified successes. No previous pass regressed and no new
mismatch appeared. All 547 fixture hashes remain unchanged; baseline reviewed.
225 workspace tests plus six explicit reference controls pass; Clippy/formatting
clean. Legacy remains 520/547 with exactly the same 27 failure names.

Three capped release Odoo runs per variant preserve outputs/verdict. Median RSS
171824/301952/1003900 KiB, median time 0.38/0.69/2.51 s for literal/refs/original.
Memory is effectively unchanged, including original's unmet 512 MiB target.


## Outcomes exposed by scalar sharing (2026-09-25)

Phase 14's string sharing makes three observations match upstream: the nested
comprehension `New_infra` and its package export, and issue3996's `out`. Investigation
found `list.Contains` compares arena IDs. These particular string operands now
share an ID; the builtin contract remains wrong for separately allocated equal
values. The new `tests/conformance/open/contains-identity.txtar` confirms that
Contains([7], 7) still incorrectly returns false. Keep that under the builtin
family; do not replace it with the evaluator's fixpoint comparator without
accounting for concrete equality, optional fields, defaults and incomplete values.

Reviewed full corpus: 749 passed, 749 mismatches across 173 archives, 3383
unsupported, one oracle disagreement and 603 not applicable; 28/547 fully verified
archives. All other statuses/paths/operations and all 547 fixture hashes are
unchanged. The original 43-signal matrix is regenerated from this reviewed report.
