# Deferred field conjuncts

A field expression is collapsed to a single `ValueId` when its struct literal is
read. Unification later replaces a sibling's value and nothing re-derives the
readers, so `{p: int | *5, c: "v\(p)"} & {p: 9}` exports `p: 9` beside `c: "v5"`.
Interpolation freezes a string; a plain reference keeps the pre-unification id
and exports that value's default. Both are the same defect, and neither raises
an error: an `enve` service preset listens on the overridden port while handing
the application the default one.

Reproduced by `crates/cue-eval/tests/defaulted_field_references.rs`, eight cases,
seven red. Every expectation was checked against upstream `cue` v0.16.1.

## Design

A field keeps the conjuncts it was built from instead of only their result.
`FieldEntry` gains `conjuncts: Vec<Conjunct>`, where a conjunct is an evaluated
value or a thunk: an `Rc<Expr>` plus the environment its struct literal captured.
Struct unification concatenates the two conjunct lists, which is what upstream
does with vertex arcs. `val` stays the cached unification, so readers, export
and the unifier are unchanged until a refresh runs.

After an evaluator-initiated unification, refresh re-forces thunks against the
merged struct and replaces the cached value. Replacement, not unification, is
required: the stale cache came from the same thunk, and `"v5" & "v9"` is bottom.
Evaluated conjuncts survive the re-force, so the `{p: 9}` override is not lost
when the `int | *5` thunk runs again.

Replaying a literal's declarations on unification would fix the same seven cases
with a smaller diff, but it cannot tell a re-derived value from an override,
repeats comprehension effects, and only approximates error shape. Conjunct lists
also subsume the deferred generated-field item in follow_up.md.

## Stage 1 - conjunct model and refresh

`value.rs`: `Conjunct::{Value, Thunk}`, a `ThunkEnv` shared per literal behind an
`Rc`, and a manual `PartialEq` for `FieldEntry` that ignores conjuncts so value
equality keeps its current meaning. Twelve construction sites in `eval.rs`,
`unify.rs` and `package.rs` move to a `FieldEntry::value` constructor.

`eval.rs`: `eval_single_decl` records a thunk beside today's eager value for each
static field of a struct literal, together with the sibling names the expression
read - `eval_expr`'s `Ident` arm already observes every resolution, so the read
set is free and makes the refresh trigger precise.

`unify.rs`: `unify_structs_inner` concatenates conjunct lists for shared keys.
No behavioural change on its own.

`eval.rs`: `Evaluator::refresh` walks a value, re-forcing the fields whose read
set meets a changed name, to a capped fixpoint. Two drivers: every field value as
`eval_field_value` produces it, and the end of `eval_decls_scoped` for a literal
that holds a comprehension, an embedding or a pattern constraint. Refreshing at
each `&` instead was tried and re-entered itself through the forced expression;
doing it once per field is what avoids that, and is also where the phase's cost
sits. A capped pass count turns genuine self-reference into the existing cycle
error instead of a hang.

This covers every failing case except the `let` binding. A nested literal is
itself a thunk, so it is re-evaluated at the merged vertex and its readers follow
- except where its reads sit in a comprehension body, which declares fields into
the enclosing struct while counting as a scope of its own.

## Stage 2 - let and alias bindings

`let q = p` is a scope binding, not a field, so a captured env still holds the
pre-merge `q`. `ThunkEnv` keeps the literal's `let` and alias declarations and
re-derives them before forcing a field that read one.

## Levelled reads

A read records the scope level that answered for the name, counted outward from
the literal that holds the field. Without it an inherited `let` resolves its
dependency at whichever literal is being forced, and a nested literal that
declares a field of the same name answers instead of the one that was written:
`let servicePort = port` beside `healthCheck: {port: ... | *servicePort}` then
re-derives forever. `ThunkEnv` keeps a parent link and the field names each
literal owns, so `owning_level` names the literal a read belongs to and the
refresh chain pairs it with that vertex.

## Staleness is content, not identity

A field is stale when a name its recipe read now resolves elsewhere. Comparing
ids alone answers "elsewhere" wrongly: evaluating one expression twice yields two
ids for one value, so every re-evaluation marks every reader stale, each re-force
rebuilds a whole definition, and that rebuild invalidates the next reader.
`enve`'s 3.2k-line schema corpus cost 4.1 GB and 4.1 s that way, against 153 MB
and 37 ms before the phase, and its test suite exhausted a 3 GB cap.

`unify::values_equivalent` compares the two values instead, everything reachable,
with a pair already under comparison counted as equal so a recursive schema
terminates and a node budget that reports a difference rather than paying an
unbounded comparison. The same corpus is then 22 ms and 19 MB - faster and
smaller than before the phase, because a re-derived field now settles on its
first pass.

## Stage 3 - retire the eager guards

`unify_decl_field` and the embedding path currently reject updates to an
already-referenced field with "deferred generated-field unification is not
supported". With conjuncts these are ordinary re-forcing. Closes the follow_up.md
item. Split out if stage 1-2 validation shows the comprehension path needs its
own loop semantics.

## Test scope

The eight existing cases, plus `out: {p: int, c: "v\(p)"} & {p: 9}`, which
upstream exports as `v9` and cue-rs rejects today. Its paired form, where `x` is
also exported, must stay an error: upstream reports an incomplete `int` there,
and the fix must re-derive the reader without making the field concrete. The
two-level override stays rejected; assert rejection, not the message, since only
the text diverges.

## Risks

Cached-value replacement could drop a value that legitimately unified from two
sides - the conjunct list carries both, and the repeated-field regressions cover
it. Refresh cost is measured as txtar wall time before and after. The `PartialEq`
change is contained by the manual impl. Cycle behaviour gets a self-reference
case beside the existing cycle tests.

## Validation

Baseline: 60 workspace tests green, 522/547 txtar, seven red in the new file.

Result: 12 cases in `defaulted_field_references.rs` green, including the two the
first pass left out and the nested-binding shape that made re-derivation diverge.
All 74 workspace tests green. 527/547 txtar, the same 20 failures as the baseline
minus five it fixes - `comprehensions_issue2171`, `comprehensions_issue4423`,
`definitions_root7`, `definitions_root8`, `eval_merge` - and none new. Clippy with
warnings denied is clean; rustfmt touches only the pre-existing drift in
`lib.rs`, `stdlib/encoding.rs` and `cue-wasm/src/lib.rs`, which this phase leaves
alone - so the `justfile`'s `ci` recipe fails at `fmt-check` until that drift is
cleaned in a commit of its own. The corpus costs 1.09 s against 0.75-0.87 s
before the phase, at 1.61 GB against 1.58 GB peak.

Downstream: `enve`'s 74 `enve-cue` tests pass against a local path dependency,
where the pre-phase engine passes the same 74. `repeated_service_fields` runs in
0.45 s against 55 s mid-phase and 0.12 s before it, `transpiler_integration` in
0.17 s against 15.7 s and 0.06 s. `examples/multi_service_posthog.cue` exports
byte-identical JSON to the pre-phase engine in 0.01 s.

A `justfile` lands with this phase - the repo has none, and AGENTS.md asks for
one - covering test, clippy, fmt and the txtar corpus.

Independent review after the phase, per the parent AGENTS.md.

## Review outcome

Independent review, 2026-09-20. It confirmed the conformance and downstream
numbers and all three stages, and found four defects the corpus does not reach.
Each was reproduced against upstream v0.16.1 and a freshly built HEAD:

1. `Read.level` counts `ThunkEnv` parents - where an expression was written -
   while `chain_frame` indexes `refresh_chain`, which is pushed once per struct
   *value* descended into. A merged struct that lands one vertex deeper resolves
   a read against the wrong frame and `force_thunk` then binds it from there, so
   `out: {top: "shadow", d: base & {p: 7}}` gives `base.inner.q` the shadowing
   value. Fix: key the chain by `Rc::ptr_eq` on the env, not by depth.
   The same mismatch in the other direction leaves a comprehension body's read
   with no frame, so a generated field never re-derives; HEAD is wrong there too,
   so this half is a gap rather than a regression.
2. `current_reads` is keyed by name alone, and every read collected while a field
   expression runs is attributed to level 0 of that field's env. A read from
   inside a nested literal - dynamic label, comprehension source, `if` condition,
   embedding - then never matches, the field stays stale, and the pass cap
   reports a cycle. Four shapes that HEAD and upstream accept now fail.
3. `FieldEntry::value` leaves the conjunct list empty rather than holding a
   `Conjunct::Value`, so `merge_conjuncts` drops that side entirely: a field from
   a stdlib call or an imported package loses its value when re-forced beside a
   thunk. `Conjunct::Value` is in fact never constructed today.
4. The pass cap converts a dependency chain longer than 16 into a cycle error.
   Terminate on convergence instead, with the cap only as a backstop.

Also open, in the same area: `force_thunk` does not restore `current_env`;
`entry_is_stale` skips a read whenever any enclosing `let` shares its name rather
than when the read resolved to that binding; `force_field` decides `changed` by
id, which defeats copy-on-write; and `refresh` is `pub` with no caller. The
refresh pass is 1.4x the corpus wall time, which the same dirty-tracking work
would address.

These are defects of this phase, not of the model it introduces. The fix is a
rework of how a read is recorded and resolved, planned as phase 04.
