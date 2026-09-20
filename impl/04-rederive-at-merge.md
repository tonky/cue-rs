# Re-derive at the merge

Phase 03 fixed the bug and cost ~1050 lines to do it. About 150 of those are the
fix: a field keeps the conjuncts it was built from, so the recipe survives the
merge that overrides one of its siblings. The other ~540 are a refresh pass that
walks values *after* the fact and tries to work out which fields went stale -
levels, a vertex chain, a read set, a pass cap, a deep value comparison. All four
defects the independent review found live in that layer.

It exists for one reason: `unify` is a free function over the arena with no
evaluator, so phase 03 could not re-derive *at* the merge and reconstructed its
consequences afterwards, from a value graph that has lost its lexical structure.
Levels are that layer rebuilding scope information the evaluator's own scope
stack already holds.

This phase deletes the layer. The evaluator owns the merges that produce an
override, so a merged struct re-derives its recipes there, by ordinary
evaluation in an ordinary scope.

## Design

After a merge the evaluator itself initiates - `BinaryOp::Unify`, and the end of
a literal whose declarations can change a field after a sibling has read it: a
comprehension, an embedding or a pattern constraint - the result is re-derived:

For each field of the merged struct that has a thunk conjunct, evaluate the
conjuncts again and replace the cached value with their unification. Replacement,
not unification, for the thunk's own result: the cached value came from the same
thunk, and `"v5" & "v9"` is bottom. A `Conjunct::Value` contributes its value, so
an override keeps its place.

A thunk is evaluated with the scope stack it captured, plus one frame holding the
merged struct's values for the names *this literal declares*. Only those: a
literal that reads `policy` without declaring it means the `policy` in the scope
it was written in, not one a repeated declaration contributed. The literal's own
`let` and alias bindings are evaluated in that frame first, so a binding sees the
merged value too.

Nested literals need no special case. A field whose expression is a struct
literal re-evaluates that literal from its expression, in the scope that now
holds the merged values - which is why the levels, the vertex chain and the
inherited-`let` read sets all go: the scope stack is the scope.

Recursion follows the merge, not the value graph. Once every side of a merge
carries a conjunct, a field that was merged has two or more of them and a field
that was not has one, so the pass descends exactly where the unifier merged
something.

Deriving one field can change what the field beside it reads, so a struct sweeps
until nothing moves. "Moves" is content: deriving one expression twice yields two
ids for one value, so a sweep that compared ids would never settle -
`values_equivalent` decides it. A chain of n references costs n sweeps, and the
cap is a backstop that leaves the values as they are rather than inventing a
cycle the file does not have. A `visiting` set covers a cyclic value graph, and a
struct no merge touched is skipped before any of this: if no field has a second
conjunct, no recipe's inputs moved.

## What is deleted

`Read`, `LetReads`, `LetBinding.level`, `ThunkEnv::{parent, ancestor,
owning_level, binds_let}`, `Thunk.reads`, `expand_let_reads`, `refresh`,
`refresh_value`, `refresh_value_inner`, `refresh_struct`, `refresh_children`,
`vertex_frame`, `stale_fields`, `entry_is_stale`, `resolve_read`, `chain_frame`,
`refresh_chain`, `current_reads`, `MAX_REFRESH_PASSES`, and the read-recording
arm in `eval_expr`. `values_equivalent` stays, as the convergence test rather
than as a staleness test.

`ThunkEnv` keeps what a re-derivation actually needs: the captured scope stack,
the literal's own `let`/alias declarations, and the set of field names it
declares - the last still learned at run time for a dynamic label or a generated
field, which is the one piece of mutation-after-capture that remains.

## Stage 1 - re-derivation

The above, plus `FieldEntry::value(val)` holding `Conjunct::Value(val)` instead
of an empty list, which is review finding 3 and the invariant the recursion rule
rests on: the conjuncts reproduce `val` everywhere, including a struct from a
stdlib call or an imported package.

## Stage 2 - cost

`eval_field_value` refreshing after every field is what made phase 03 1.34x the
corpus wall time. Re-derivation runs per merge instead, which is fewer sites and
a narrower walk, so stage 2 was measurement first: the corpus and the enve suite
against a freshly built HEAD.

The measurement asked for one thing. Re-derivation entered every merged struct,
including the many that hold nothing to derive, which left the corpus still at
1.34x. Skipping a struct in which no field has a second conjunct - the same
condition that makes the recursion rule work - takes it to 1.15x, and the rest
is the walk that actually re-derives something.

## Test scope

The twelve cases in `defaulted_field_references.rs` stay green, and the six
regressions the review found become cases, each checked against upstream v0.16.1:
the extra-vertex shadowing (`out: {top: "shadow", d: base & {p: 7}}`), the
comprehension body (`gen: {for i in [1, 2] {"k\(i)": p}}` under an override), the
four nested-literal shapes - dynamic label, comprehension source, embedding, `if`
condition - a reader over a conjunct-less struct via `yaml.Unmarshal`, and a
dependency chain longer than phase 03's cap.

## Risks

Re-deriving a whole literal costs more per merge than re-forcing one field, and
a merge inside a large definition pays for the definition. The conjunct-count
gate is what keeps it to the structs a merge actually touched; the corpus and
enve measure whether that is enough.

An expression that re-derives to an unresolved reference keeps its previous
value, as the relaxation loop does, so a forward reference is not turned into a
bottom by being derived again.

The frame holds only the names the literal declares, which is what keeps repeated
declarations resolving in their own lexical scope. That invariant carries the
phase; `repeated_fields.rs` already pins it.

## Validation

Every workspace test green, no txtar regression against 527/547 - the five cases
phase 03 fixed stay fixed - Clippy with warnings denied, rustfmt on changed
lines, enve's 74 tests against a local path dependency, and corpus and enve
timings against a freshly built HEAD binary. Independent review after the phase.

Result: 79 workspace tests green, including the twelve in
`defaulted_field_references.rs` and five new ones in `rederived_at_merge.rs` that
carry the review's six shapes. 527/547 txtar, the same twenty failures as phase
03 and as HEAD-minus-five. Clippy with warnings denied is clean, and rustfmt
reports only the two pre-existing drift files this phase does not touch.

All four review defects are fixed by construction - there is no level to
mismatch, no read set to key by name, `FieldEntry::value` carries its
`Conjunct::Value`, and convergence replaced the pass cap. So is f1b, a
comprehension body that never re-derived, which HEAD gets wrong too.

Cost: the corpus runs in 0.84-0.86 s against 0.74 s for a freshly built HEAD, at
1.62 GB against 1.58 GB peak - 1.15x wall, 2.7% memory, where phase 03 was 1.34x.
Downstream, `enve`'s prepared schema corpus evaluates in 32 ms at 21 MB, against
4.1 GB before the staleness fix; its 74 `enve-cue` tests pass in 3.26 s, and
`examples/multi_service_posthog.cue` exports byte-identical JSON to HEAD.

Net against HEAD, phases 03 and 04 together: 693 added and 107 removed across the
four engine files, plus 381 lines of new tests.

## Review outcome

Independent review, 2026-09-20. It confirmed the deletion list, the conformance
and cost numbers and review finding 3, and found two defects that block the
phase and four gaps. I reproduced all six against upstream v0.16.1 and a freshly
built HEAD.

**Blockers.**

1. Budget exhaustion in `values_equivalent` turns the sweep loop into 256 full
   re-derivations. The budget reports "different" when it runs out, which was
   the safe direction for phase 03's staleness test and is the unsafe one for a
   convergence test: a field larger than 4096 nodes always reports movement, so
   `rederive_struct` runs the cap and each pass copies the whole subtree into an
   arena that only frees on rollback. A 600-field static literal under one
   override - `base & {p: 9}`, no comprehension, no stdlib - goes from HEAD's
   0.01 s and 8.6 MB to 13.4 s and 6.0 GB, and aborts under a 6 GB cap. This is
   the failure the worklog's process note was written about, reachable from an
   ordinary config file.
2. `bind_struct_fields` calls `note_field_name` for every field of the target
   struct, so an embedded struct's fields and a comprehension's generated fields
   enter `own_fields` as if the literal had written them - which breaks the one
   invariant the phase rests on. `{top: "outer", mix: {top: "inner"},
   base: {c: top, mix, p: int | *1}, out: base & {p: 9}}` gives `base.c: "outer"`
   and `out.c: "inner"`: unifying `{p: 9}` silently rewrote a field that has
   nothing to do with `p`. Upstream and HEAD both say "outer". The same with a
   generated name. This is phase-03 finding 2 re-entering through another door.

**Gaps**, all of which HEAD shares, so none is a regression:

3. `rederive_value` returns non-structs unchanged, so a merge inside a list
   element or a disjunction branch never re-derives. `#Svc: {port: int | *8080,
   url: "...\(port)"}` with `svcs: [...#Svc]` and `svcs: [{port: 9090}]` still
   exports the 8080 URL - the phase's own motivating shape, through a list.
4. `validate_json` (`cue-rs vet`, `cue-wasm`) calls the free `unify` and never
   re-derives, and its verdict is exactly inverted: correct data is rejected and
   stale data is accepted.
5. The sweep cap returns a value it knows is unsettled, with no diagnostic. A
   301-link chain exports the pre-merge value for the far end and the merged one
   16 links in. Sweep count also depends on `BTreeMap` key order rather than on
   the dependency graph.
6. A `let` that forward-references another `let` is dropped rather than relaxed,
   so its reader silently keeps the pre-merge value.

Also noted: `Value` is no longer `Send`/`Sync` - phase 03's `Rc<Expr>` and
`Rc<ThunkEnv>` reach it through `FieldEntry.conjuncts`, which forecloses
parallel evaluation and is a public API change neither doc mentions; `ThunkEnv`
derives `Clone` while carrying the `RefCell` whose point is to be shared;
`eval_field_value` deep-clones the AST and the whole scope stack per literal
whether or not anything merges; `values_equivalent` compares
`pattern_constraints` positionally, a second order-dependent path into blocker 1.

The two blockers are defects of this phase's implementation, not of its model.

## Stage 3 - the blocker fixes

Both blockers came from the same place: re-derivation had no idea which recipes
the merge could have affected, so it ran all of them and let a value comparison
sort out the result afterwards.

**Which names a recipe reads is lexical**, so it is answered from the expression
rather than from the value graph. `deps.rs` collects every identifier an
expression mentions, with the literal's `let` bindings followed through, and a
`Thunk` carries that set. Nothing is subtracted - not a comprehension's loop
variable, not a nested literal's own bindings - because over-collecting costs a
derivation that produces the same value while under-collecting would keep a
stale one. This is not phase 03's read set returning: there is no level, no
chain, no recording during evaluation and nothing resolved at run time, only the
free names of an expression.

A merged struct then seeds its sweep with the names the merge gave a second
recipe to, and a sweep derives only the fields whose recipes read something that
moved. The 600-field literal beside one override now derives nothing at all,
because nothing in it mentions the overridden name.

**Fields derive in dependency order.** The graph is already there in the same
dep sets, so a Kahn ordering puts a field after the fields it reads, and a name
that moves counts inside the sweep that moved it. A chain settles in one sweep
whichever way its names sort, where before it advanced one link per sweep in
`BTreeMap` order and a chain longer than the cap was silently truncated.

**The comparison reports what it could establish.** `compare_values` returns
`Equal`, `Different` or `Unknown`, the last when the node budget runs out.
Phase 03 collapsed `Unknown` into `Different` because for a staleness test that
was the cautious answer; for a convergence test it is the reckless one - it is
what made a struct of large literals sweep to the cap. `Unknown` now settles,
and the derived value is kept either way.

**`own_fields` holds only what the literal wrote.** `bind_struct_fields` makes
an embedding's or a comprehension's fields visible to the declarations that
follow, which it must, but no longer records them as names this literal
declares.

`Evaluator::derivations` and `Evaluator::unsettled` count the recipes a pass ran
and the structs it gave up on, so `rederive_cost.rs` can assert that one
override over a 600-field literal costs a handful of derivations and that no
struct reaches the cap. Wall time would have been flaky; these are exact.

### Stage 3 result

83 workspace tests green. 527/547 txtar, the same twenty failures as HEAD minus
the five this work fixes, none new. Clippy with warnings denied is clean, and
rustfmt still reports only the two pre-existing drift files.

The blocker case - 600 static fields beside one override - goes from 13.4 s and
6.0 GB, aborting under a 6 GB cap, to 0.03 s and 46 MB against HEAD's 0.01 s and
8.6 MB. The 301-link chain and both embedding shapes now match upstream.

Cost fell rather than rose: the corpus runs at 0.80-0.81 s against HEAD's
0.73-0.79 s over three runs each - 1.07x, where the stage-1 implementation was
1.15x - at 1.60 GB against 1.58 GB. Deriving only what the merge could have
changed pays for the dep sets several times over. Downstream, enve's 74
`enve-cue` tests are green, its prepared 3.3k-line corpus evaluates in 33 ms at
20 MB, and the posthog example is byte-identical to HEAD.

Four gaps from the review stay open and are filed in follow_up.md: re-derivation
does not descend into list elements or disjunction branches, `validate_json`
never re-derives and its verdict is inverted, a forward `let` is dropped rather
than relaxed, and `Value` is no longer `Send`. None is a regression; all four
are HEAD's behaviour too.
