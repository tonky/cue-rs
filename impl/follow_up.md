# Follow-up

The approved [phases 12-15](PLAN.md) own the conformance, memory and evaluator
architecture work below. Phase 12 has established the initial assertion baseline;
beneficial public API breaks are already authorized.
[CONTINUATION.md](CONTINUATION.md) summarizes the current state and next steps;
family-specific measurements below retain their historical context.

## Conformance coverage and compatibility debt

Phase 12 fixes runner policy and reporting. The legacy 504/547 result and all
43 failure names are retained for comparison; failures now return nonzero.
The new assertion matrix, pinned oracle and manifest are documented in
[tests/conformance](../tests/conformance/README.md). All 429 imports match
upstream revision 635e4bb441b29b0b8a3d754188b8edefe4012d1d byte-for-byte.

Open in phase 13: 3,383 unsupported checks (abstract/hidden/optional/pattern
observations, detailed diagnostics, goldens and configuration), 749 mismatching
observations across 173 archives, and one reference-annotation disagreement.
The 749 passing checks establish only what was actually compared. Twenty-eight
archives have all applicable checks verified; a failed package load can account
for many mismatches. The old 43 signals were not a complete defect inventory.
The migration baseline must never be presented as conformance acceptance.

## Odoo allocation cost after the cycle fix

Open performance work; phase 11 fixes runaway diagnostic growth and the
shadowing/default verdicts. The original 662-component snapshot now rejects
normally in 4.12 s at 2,184,656 KiB, about 2.08 GiB. Its original documented
cost was 2.2 GB before the partial fix introduced growing cycle messages.
The valid refs/literal variants remain at 576,956 / 348,692 KiB and match
upstream JSON. Arena retention, scope snapshots and struct cloning remain
profiling targets; this phase does not add collection or redesign retry
passes. Use CUE_SAFE_MEMORY=3G with just safe for the original, and keep the
default 2 GiB limit for smaller investigations.
The phase-14 counting-allocator probe measured 560 MB of retained heap after
loading refs and 1.92 GB after loading original. Both return to the probe's
baseline after dropping the evaluator and output. This points to request-
owned retention and allocation churn, not storage leaking beyond its owner;
precise allocation attribution remains to be done.

## Deferred generated-field evaluation

Closed by stage 3 of 03-deferred-field-conjuncts.md. A comprehension or an
embedding that constrains an already-read field is now ordinary re-derivation,
so the "deferred generated-field unification is not supported" guards are gone
and `repeated_fields.rs` asserts the upstream values instead of the refusals.

## Re-derivation cost

Mostly closed, and what is left is cheap. Phase 04 stage 3 derives only the
recipes that read a name the merge moved, in dependency order, which took the
corpus to 1.07x HEAD and `rederive_cost.rs` pins it with the evaluator's own
`derivations` and `unsettled` counters rather than with wall time.

What remains: `deps::recipe_deps` walks a field's expression at capture, so a
literal nested n deep has its subtree walked n times, and the resulting
`HashSet<String>` is allocated per field whether or not anything ever merges.
Neither shows up on the corpus or on enve. If one ever does, the walk is
memoizable per expression and a shared empty set covers the common constant
field.

Phase 05 folded the descent into merged children into the same loop as the
sweep, so a struct now descends once per sweep rather than once in total. The
corpus does not notice - a settled child returns its own id, so the extra passes
cost one confirming sweep each - but this is the loop to look at first if
re-derivation ever shows up in a profile.

`compare_values` still answers `Unknown` past a 4096-node budget, where a large
value settles rather than sweeping again. The cost is a missed propagation to a
reader of that field, not a wrong value in it; a content hash cached per
`ValueId` is the cheap way to answer properly.

## Re-derivation through lists and disjunctions

Open, and the same bug phases 03-04 exist to fix. `rederive_value` only descends
into structs, so a merge inside a list element or a disjunction branch never
re-derives its readers: `svcs: [...#Svc]` with `svcs: [{port: 9090}]` exports the
default URL beside the overridden port. HEAD is wrong here too. The fix is to
descend into `Value::List` and `Value::Disjunction` with the same copy-on-write.

## Validation re-derivation and nested verdicts

The original missing-re-derivation finding is stale as of d16afa0:
`lib.rs::validate_json` already calls `Evaluator::unify_and_rederive`, and
security-bounds tests cover overridden defaults. Do not treat that old
finding as an unfixed bug. Phase 13 still needs an operation-specific audit
of validation verdicts for nested conflicts, definitions, incomplete values
and pending dependencies; fixing the call route alone does not establish
all of those semantics.

## A forward let is dropped rather than relaxed

Open. `derive_thunk` evaluates a literal's `let` bindings in one declaration-order
pass, so `let b = a2` written above `let a2 = p` never resolves, its reader
derives to an unresolved reference, and `derive_field` keeps the pre-merge value
with no diagnostic. Upstream derives it. HEAD is wrong here too. The binding loop
wants the same relaxation the declaration loop uses, and "unresolved because the
relaxation loop has not got there" wants telling apart from "unresolved because
of the merge", which upstream reports as an error.

## Value is no longer Send

Open, needs a decision. `FieldEntry.conjuncts` reaches `Rc<Expr>` and
`Rc<ThunkEnv>`, so the public `value::Value` lost `Send`/`Sync` in phase 03.
That forecloses parallelising evaluation or the txtar runner. `Arc` plus a lock,
or an arena-side expression table, would keep the property; neither phase doc
decided this deliberately.

## rustfmt drift

Closed, as the formatting-only commit this asked for. `crates/cue-eval/src/lib.rs`,
`crates/cue-wasm/src/lib.rs` and `crates/cue-eval/src/stdlib/encoding.rs` carried
drift from the `serde_yaml_ng` swap in 2aa698d, so `just ci` failed at
`fmt-check` on a clean checkout from then until 69aa4c3. Phases 03-06 left it
alone deliberately, to keep their diffs reviewable; it was `cargo fmt --all` and
nothing else.

## A package imported twice is loaded twice

Open, cheap, and only worth doing if it shows up. `imported_packages` caches per
evaluator, and each package gets its own evaluator, so when A and B both import
C, C is loaded once per importer. HEAD does the same - it copies C into A's arena
and into B's - so phase 05 neither helps nor hurts here. Threading one cache
through `load_dir_into` would make it once per build.

## Downstream release

User-owned: update both enve workspace dependency revisions and Cargo.lock after
cue-rs publication, then rerun its policy regressions against the actual git pin.
The user explicitly requested handling this downstream step themselves. Current
local-path validation does not complete this release step.

## A bytes literal cannot interpolate

Open, and a stated limit rather than a surprise. Upstream accepts `'a\(1)b'` and
gives the bytes `a1b`; `Expr::Interpolation` carries no delimiter, so cue-rs
cannot represent an interpolation that yields bytes and phase 06 refuses it by
name instead of producing the characters `a\(1)b` as HEAD did. This is the one
corpus case phase 06 moved, `upstream_cue_testdata_interpolation_scalars.txtar`.
The fix is a delimiter on `Expr::Interpolation` plus bytes-valued concatenation
in `eval_interpolation`, which also wants the string-from-bytes rule that file
pins: invalid UTF-8 interpolated into a *string* becomes replacement characters
by the Unicode standard, one per invalid sequence, not one per byte.

## Optional struct and list fields are materialised

Closed by phase 07 stage 2. Export skips every optional field, whatever its
constraint holds, which is `cue export`'s rule - `a?: 1` exports `{}`. The
eleven-line shape test it replaced is why `b?: {x?: int}` used to appear as `{}`
and `c?: [...string]` as `[]` while the scalar `e?: int` was correctly dropped.

## A sibling reference inside the struct being defined does not resolve

Closed by phase 07 stages 3, 4 and 6. A pending declaration binds the partial
value its pass produced, an unresolved reference no longer collapses the struct
that holds it, and a pass that refines a partial counts as progress - which is
what lets a chain of them resolve a link at a time.

Two limits remain, both stated rather than hidden. **A chain longer than eight
links does not resolve**, where upstream resolves any length: the allowance is
`MAX_UNRESOLVED_DEPTH / 8`, and it cannot simply be raised, because a partial
deeper than `is_unresolved_within` walks is judged *resolved* and written with a
bottom buried inside it. Giving that walk a visited set instead of a depth budget
is what would lift both. **A non-converging value is reported as an unresolved
reference** at the link it is written on, where upstream says `structural cycle`;
naming it would mean claiming a cycle whenever the allowance runs out, which is
also what a nine-link chain looks like.

## A structural cycle is exported as a placeholder string

Closed by phase 07 stage 2. A `RecursiveRef` reaching export is refused as a
structural cycle naming its path. An optional recursive field never reaches it,
because optional fields are dropped first - which is what upstream does with
`needs?: [...#Stage]` too.

## A bottom's path is not subsumed the way upstream's is

Open, and presentation rather than verdict. `BottomReason` carries a `path` that
nothing populates consistently, so cue-rs reports
`cannot export bottom at 'a.x': _|_ (reference "nope" not found)` where upstream
reports `a.x: reference "nope" not found`. Phase 07 made the *paths* agree, which
is the part that was wrong; what is left is the envelope around them. Worth doing
once, now that the kind is on the type.

Beside it, one wording row still differs: `a: {b: c}, c: a.b` says `reference "c"
not found` where upstream says `incomplete value _`, because `declares_field`
sees only the innermost literal - an enclosing literal's environment is saved and
restored around this one. A stack of environments, or capturing the enclosing
`own_fields` into each `ThunkEnv`, would settle it.

## Exported fields are sorted, not in declaration order

Open, cosmetic, and visible in every diff a user reads. `StructValue` holds its
fields in a `BTreeMap`, so `cue-rs eval` emits `{"jobs": …, "name": …}` where
`cue export` emits `{"name": …, "jobs": …}` - upstream keeps the order the fields
were written in. The values agree; only the order does not, which is why no test
has caught it (`serde_json::Value` compares objects by key). Fixing it means an
insertion-ordered map on `StructValue`, which touches every merge.

## A disjunction is never normalised

Phase 08 drops a branch equal to one already kept, which is what the
`monorepo-go` abort needed. Upstream also drops a branch *subsumed* by another,
and cue-rs has no ordering relation to decide that with - `compare_values`
answers equal, different or unknown, not narrower. Until it does, a disjunction
of `{t: "x"} | {t: string}` keeps both branches where upstream keeps one. No
reported case needs it; it is the other half of normalisation and belongs with
whatever gives cue-rs a subsumption check.

Two smaller pieces sit behind the same work: `compare_values` answering
`Unknown` past its 4096-node budget keeps a duplicate branch, which the content
hash already wanted under "Re-derivation cost" would settle; and `ValueArena`'s
`SlotMap` never returns capacity after a rollback, so a peak is paid for the
life of the process. Neither is reachable now that the doubling is gone.

## The shape rules phase 09 left open

All four are shapes upstream preserves and cue-rs normalises. None changes a
value, and `cue fmt` leaves every one of cue-rs's outputs alone, so each costs
a one-time diff and nothing after it.

**A mixed composite settles on one side.** `{b: 1,` newline `c: 2}` has
elements on both sides of a newline; upstream reproduces the mixture, and one
bit recorded at the opening delimiter cannot. It collapses to whichever side
the first element fell on. Closing it means a position per element, which is
the whole of upstream's `RelPos` model and a phase of its own.

**Trailing comments are still dropped.** `comment_positions.rs` names the
position and has since phase 06. They are also a tabwriter cell upstream
aligns, so closing the hole adds a cell to `Rendered`, not a rule.

**An interpolated label gains parentheses.** `"\(k)": v` is written back as
`("\(k)"): v`. The parser reads CUE's two spellings — an interpolated string
label and a parenthesised dynamic one — as the same node, so the formatter has
nothing to tell them apart with. It is the same defect as the parentheses
phase 09 fixed for expressions, one level down, and wants the same answer: a
form on the label.

**Multiple attributes are one cell.** Upstream gives each its own column. No
CUE file in either repository has two attributes on a field.

**A wrapped `&` chain is not preserved.** `DisjunctionBranch::on_new_line`
covers `|` because that is where the wrapping occurs in practice — a union of
named constants. The same field on `Expr::Binary` would cover the rest.

## Closedness divergences phase 10 left open

All five are measured against `cue export` v0.16.1 and pinned in
`tests/closedness.rs` or `impl/10-closedness.md`.

**Merged closed structs allow the union of their fields.** `#A & #B & {x: 1}`
exports when `#B` does not declare `x`. Upstream records per conjunct which
closed struct admitted a field (`closeInfo`). Accepting too much is the safe
direction for enve and enact, which only need typos rejected. The test
`merged_definitions_allow_the_union_of_their_fields` flips when this closes.

**Errors inside an unused definition are not reported.** `#B: #A & {y: int}`
exports `{}`. Upstream fails with `#B.y: field not allowed`. Export does not
evaluate definitions for errors. The fix is a vet pass over definitions, which
also belongs in `cue vet`.

**A disjunction names the field once per branch.** The error reads
`no matching disjunction branch: [_|_ (z: field not allowed); ...]`. Upstream
collapses branches that fail for the same reason into one message.

**Root-embedding suppression is broad.** While a file-root embedding is
evaluated, every definition it reads stays open, not only the embedded value.
This matches the corpus (`definitions_root5`, `root7`, `root8`) and the probes,
but it is not how upstream reasons about it.

**Error paths are leaf-only.** `bogus: field not allowed` where upstream says
`a.b.bogus: field not allowed`. The unifier does not know the path it is
working at. Threading one through would help every conflict message, not only
closedness.

## A pending disjunction branch is dropped when another branch survives

`settle_disjunction` keeps a disjunction pending when no branch survived and
one of them failed on a reference that is not resolved yet. When another
branch did survive, the pending one is still dropped for good, so a later pass
cannot bring it back. `(int | [...]) & x` with `x` declared further down picks
nothing wrong today, because the list branch is the only one that could match,
but a disjunction with two viable branches, one of them pending, resolves to
the other. Upstream keeps the branch until the reference is known.

## disjselfcycle: reducedNested exports instead of failing

`upstream_cue_testdata_eval_disjselfcycle.txtar` records one error:
`issue4119.reducedNested` must fail with `x.y.0.f: undefined field: f`, and
cue-rs exports `x: y: []`. The case passed `--strict-errors` by accident until
2026-09-25: `issue4119.full` failed with a structural cycle upstream does not
report, and the harness took that for the recorded error. Keeping a pending
disjunction pending made `full` evaluate, which exposed both divergences:
- `reducedNested` still exports `y: []`, the default, where upstream reports
  the incomplete left disjunct;
- `full` exports `art.images.ko` without `accounts`, where upstream expects
  `accounts: groups: [{gid: 65532}]`. The case uses `self`, which plain
  `cue` v0.16.1 rejects without `@experiment(aliasv2)`.

## Concrete export family: remaining numeric and caller work

Phase 13 now shares export policy, preserves BigInt JSON numbers, preserves
byte octets and emits base64 for bytes. General decimal arithmetic still uses
f64, so decimal JSON decode can round; encoded float spelling also differs from
upstream. Nonfinite results error instead of becoming null. Exact YAML values
outside i128/u128 or round-trippable f64 use JSON flow syntax, not upstream's
block formatting. Address these in the numeric/encoding compatibility family.

Before enve adopts this tree, its `export_formatted` YAML branch must call
`cue_eval::export::json_to_yaml(&val)` instead of generic serde_yaml serialization
of serde_json::Value. The latter exposes the private arbitrary-precision Number
protocol. Workspace CLI/wasm are migrated and tested. The downstream checkout
has unrelated active edits and has been left unchanged, including revision pins.


## Shared evaluation inputs: remaining memory work

Shared scope frames, constant value conjuncts and session-owned exact syntax
sharing roughly halve Odoo RSS. Three capped release runs give median RSS
182068 KiB literal, 316712 KiB refs, 1084112 KiB original. Only literal meets
the approved target. Next: attribute retained allocations over all arena slots,
record every root and execution-frame lifetime, then choose reclamation points.
Whole-tree recipe sharing does not lower subtrees or cache dependency analysis;
those remain candidates. Never reuse a recipe based only on semantic value
comparison: lexical provenance still matters. The new benchmark recipe records
reproducible samples but references require explicit saved upstream JSON.

Default operand selection and later re-derivation in arithmetic are now covered
by the scalar family. The previously failing `input: *1 | int; derived: input + 1`
case is fixed, including later overrides and incomplete schemas constrained later.

## Scalar/dependency-cache family: next work

Direct dependency sets are now shared, with local let closures kept separate.
Refs (294 MiB) and literal (168 MiB) meet their targets; original still takes
about 980 MiB versus 512 MiB. Allocation attribution and explicit live execution
roots remain necessary before reclaiming intermediate arena values.

Integer/radix/SI overflow, division variants and scalar default operations are
fixed. Exact decimal storage/arithmetic still needs replacing f64. Some direct
builtin type literals are compile-invalid upstream but treated as incomplete
constraints here; distinguish these at a compilation boundary. Mixed-kind closedness,
per-file imports, closedness provenance, pending disjunctions and diagnostics
remain in phase 13. Higher-order builtin values need modeling: list.Sort currently
ignores its comparator argument, and list.Ascending is not represented as a value.
Do not globally propagate argument errors before fixing that interface.

## Plain embeddings: remaining value-model work

File/package scalar roots and plain nested embeddings now return their actual
value kind. Multiple embeddings, non-null constraints and scalar/disjunction
comprehension bodies are supported. A separate declaration accumulator preserves
empty-struct versus top and keeps embedding conflicts local.

Resolved by the scalar metadata family below: scalars with definitions,
hidden/optional fields and patterns now retain vertex metadata and embedded recipes. Examples: `{3, #unit: "kg"}`
must permit `.#unit`; `{#a+#b, #a:int, #b:int}` must re-derive after its definitions
are constrained. Do not simply drop those declarations during scalar export.
The subsequent scalar metadata family implements these examples; mixed-kind
choice closedness is addressed in the follow-ups below; general admission
provenance remains open.

## Scalar metadata and recipes (2026-09-25)

Implemented the plain-embedding follow-up: sparse arena-owned fields and recipes
on scalar/list values, scalar selectors and package selectors, lexical recipe
specialization, invalid definition/hidden-field rejection, absent optional fields,
metadata-aware equality and rollback. Basic `{3, #unit:"kg"}` and
`{#a+#b, #a:int, #b:int}` cases now work, including later constraints and defaults.

The binding-free mixed-choice boundary is now fixed: common fields are merged
into each alternative before closing. The original repro moved to
`tests/conformance/regressions/metadata/mixed-choice.txtar`; neighboring cases
cover optional/pattern fields, nested closing, open branches, embedding, operand
order, private branch fields and selectors after narrowing. A typed choice-field
view supports selectors without becoming another closed constraint.

The dependency-bearing follow-up is now fixed. The repro moved to
`tests/conformance/regressions/dynamic-choice/basic.txtar`; all 34 observations
in six reduced fixtures pass. Whole embedded expressions retain their closing
boundaries as recipe groups. They recompute under merged inputs, close using
only their own declarations, then meet outside constraints. Separate input and
selector views retain branch-local fields without freezing them as inputs.
Nested closing, changed labels, explicit struct constraints and embedding are
covered. This does not replace the general struct admission model or scheduler.

Remaining priorities: attribute retained arena allocations in original Odoo
(now about 693 MiB after scalar/recipe sharing, versus the 512 MiB target), inventory execution roots before
reclamation, and continue the 749 mismatches/3383 unsupported observations.
`builtins_matchn` nestedOK.b/c now remain available for observation instead of
being missing, but are abstract and still unverified. General conjunct admission,
exact decimals, pending evaluation, builtin contracts and diagnostics remain in
the approved phase-13 roadmap. Downstream adoption/pin changes remain separate.


## Scalar/recipe sharing follow-up (2026-09-25)

Implemented exact boolean/string constructor sharing and immutable field recipe
slices. Original Odoo's requested live heap drops 41%, peak RSS 29%; valid literal
and refs peaks drop 49% and 54%. Original remains above 512 MiB. All cached strings
are removed on owner rollback/mutation; repeated abandoned branches do not retain
text keys. Entirely unique-string workloads pay an extra owned index key; profile
that tradeoff before extending interning. Sparse metadata always has fresh owners.

The corpus exposed an existing `list.Contains` bug: it tests ValueId identity.
String sharing now makes three observations in `comprehensions_nestembed` and
`comprehensions_issue3996` match upstream, but this does not fix the builtin's
contract. `tests/conformance/open/contains-identity.txtar` is reference-backed:
Contains([7], 7) still returns false, while the string version now returns true.
Replace identity with concrete value equality in the builtin family, accounting
for optional fields, defaults, numeric equality and invalid/incomplete operands.
Do not equate the fixpoint comparator's recipe/closedness checks with CUE Equals.
List equality in the reduced issue3996 fixture also remains unsupported.

Profiled list duplicates (370,319 nodes, 72,418 distinct child-ID sequences) do
not justify another cache yet: after deduplication they would still leave the
arena in its current capacity band, and the key vectors add ownership overhead.
Focus next on retained struct/field storage and root-safe reclamation, including
saved evaluator frames and externally held ValueIds before collecting any node.
