# Re-derive at the merge

Phase 03 fixed the bug and the independent review found four defects in it, all
in the same layer: a refresh pass that walked values after a merge and tried to
reconstruct which fields had gone stale. The user's question - "this all just to
fix the original bug?" - was the right one. About 150 of phase 03's lines are the
fix; the other ~540 were that layer. This phase deletes it.

## Why the layer existed

`unify` is a free function over the arena with no evaluator, so phase 03 could
not re-derive *at* the merge. It reconstructed the consequences afterwards, from
a value graph that has already lost its lexical structure - hence levels, a
vertex chain, read sets. Every one of those is the layer rebuilding scope
information the evaluator's own scope stack still holds.

The evaluator does own the merges that produce an override: `BinaryOp::Unify`,
and the end of a literal holding a comprehension, an embedding or a pattern
constraint. Re-deriving there is ordinary evaluation in an ordinary scope.

## What was built

A merged struct re-derives each field that has a thunk conjunct: evaluate the
conjuncts again, replace the cached value with their unification. A thunk runs
with the scope stack its literal captured, plus one frame holding the merged
struct's values **for the names that literal declares**. Only those - a literal
that reads `policy` without declaring it means the `policy` where it was written,
not one a repeated declaration contributed. That single rule is what replaced
levels, the vertex chain and the inherited-`let` read sets.

Nested literals need no case of their own. A field whose expression is a struct
literal re-evaluates that literal from its expression, in a scope that now holds
the merged values.

Recursion follows the merge rather than the value graph: with every side of a
merge carrying a conjunct, a field that was merged has two or more and a field
that was not has one, so the walk descends exactly where the unifier merged.

## Two things the plan did not have

Convergence, not one pass. The design said a single top-down pass would do, with
a guard against re-entry. It does not: deriving one field changes what the field
beside it reads, and a first implementation advanced a 17-link reference chain by
exactly one link per merge, in `BTreeMap` key order. A struct now sweeps until
nothing moves, with `MAX_REDERIVE_SWEEPS` as a backstop that leaves values alone
rather than inventing a cycle. "Moves" has to be content - deriving one
expression twice yields two ids for one value - so `values_equivalent` survived
phase 03 after all, as the convergence test rather than the staleness test.

The conjunct-count gate. Re-derivation first entered every merged struct,
including the many with nothing to derive, and the corpus stayed at phase 03's
1.34x. Skipping a struct in which no field has a second conjunct - the same
condition the recursion rule rests on - takes it to 1.15x.

## Trade-offs

A whole literal is re-derived per merge where phase 03 re-forced one field, so a
merge inside a large definition pays for the definition. The gate keeps that to
structs a merge actually touched, and the measured 1.15x is the answer; per-field
dirty tracking is the next lever if it is ever needed, and follow_up.md carries
it.

An expression that re-derives to an unresolved reference keeps its previous
value, as the relaxation loop does, so a forward reference is not turned into a
bottom by being derived again.

`FieldEntry::value` now carries `Conjunct::Value(val)` rather than an empty list -
review finding 3, and the invariant the recursion rule needs: the conjuncts
reproduce `val` everywhere, including a struct from a stdlib call or an imported
package, so `merge_conjuncts` cannot drop a side.

## Validation

79 workspace tests green - the twelve in `defaulted_field_references.rs`, and
five new ones in `rederived_at_merge.rs` carrying the review's six shapes: the
extra-vertex shadowing, a generated field under an override, the four
nested-literal reads (dynamic label, comprehension source, embedding, `if`
condition), a reader over a conjunct-less struct via `yaml.Unmarshal`, and a
17-link chain. All four review defects are fixed by construction, as is f1b, a
comprehension body that never re-derived and that HEAD gets wrong too.

527/547 txtar, the same twenty failures as phase 03; the five it fixed stay
fixed. Clippy with warnings denied is clean. rustfmt reports only the two
pre-existing drift files.

Cost against a freshly built HEAD: corpus 0.84-0.86 s against 0.74 s at 1.62 GB
against 1.58 GB - 1.15x wall and 2.7% memory, where phase 03 was 1.34x.
Downstream, in the isolated enve copy on path dependencies: 74 `enve-cue` tests
green in 3.26 s, the prepared schema corpus at 32 ms and 21 MB, and the posthog
example byte-identical to HEAD.

The tracked diff went from 1048 lines added in phase 03 to 711, and the deleted
names are listed in 04-rederive-at-merge.md.

## Process note

Every experiment in this phase ran under `ulimit -v` with a timeout, after phase
03's diagnosis took the machine's memory out from under the user. That stays the
default for anything that evaluates a schema corpus.

Phases 03 and 04 land as a single change. Not committed; the enve pin stays
user-owned.

## Stage 3 - what the review found, and the fix

The independent review confirmed the deletion list, the numbers and the four
phase-03 defects, and found two blockers. I reproduced both against upstream
v0.16.1 and a freshly built HEAD before accepting them.

One was a memory blowup worse than anything phase 03 had. `values_equivalent`
reported "different" when its 4096-node budget ran out, which was the cautious
answer for a staleness test and the reckless one for a convergence test: a field
larger than the budget always reported movement, so every merged struct ran all
256 sweeps and each sweep copied the whole subtree into an arena that only frees
on rollback. Six hundred static fields beside one `& {p: 9}` - no comprehension,
no stdlib - went from HEAD's 0.01 s and 8.6 MB to 13.4 s and 6.0 GB, aborting
under a 6 GB cap. The same failure the process note above was written about, out
of an ordinary config file.

The other was `bind_struct_fields` recording every field of the target struct in
`own_fields`, including an embedding's and a comprehension's, which broke the one
invariant the phase rests on. Unifying `{p: 9}` silently rewrote a field that
read an outer name of the same label.

The second was a line. The first was not a budget to tune: the pass had no idea
which recipes a merge could have affected, so it ran all of them and let a value
comparison sort it out afterwards. The fix is to answer that question where it is
actually cheap.

## Reads, lexically

Which names a recipe reads is a property of its expression, so `deps.rs`
collects every identifier the expression mentions, follows the literal's `let`
bindings through, and hands the `Thunk` that set. Nothing is subtracted. A
merged struct seeds its sweep with the names the merge gave a second recipe to,
and a sweep derives only the fields that read something that moved.

It is worth being plain that this is a read set again, after the phase deleted
one. What it is not is phase 03's: there is no level, no vertex chain, no
recording during evaluation and nothing resolved at run time. It is the free
names of an expression, computed once, over-approximate by construction, and
wrong only in the direction that costs a derivation rather than a value.

The same sets give the dependency graph for free, so fields derive in Kahn order
and a name that moves counts inside the sweep that moved it. A chain settles in
one sweep whichever way its names sort - it used to advance one link per sweep in
`BTreeMap` order, and a chain longer than the cap was silently truncated.

`compare_values` now returns `Equal`, `Different` or `Unknown`, and `Unknown`
settles. The derived value is kept either way, so the cost of an oversized
comparison is a missed propagation, never a discarded value.

## What that cost

It was cheaper, not dearer. The corpus runs at 0.80-0.81 s against HEAD's
0.73-0.79 s over three runs each - 1.07x, where stage 1 was 1.15x and phase 03
was 1.34x - at 1.60 GB against 1.58 GB. Deriving only what a merge could have
changed pays for the dep sets several times over. The blocker case is 0.03 s and
46 MB. 83 workspace tests green, 527/547 txtar with the same five fixed and none
new, enve's 74 tests green, its prepared corpus 33 ms at 20 MB, posthog
byte-identical to HEAD.

`Evaluator::derivations` and `unsettled` are new public counters, and
`rederive_cost.rs` asserts on them: one override over a 600-field literal costs
a handful of derivations, and no struct reaches the sweep cap. The review's first
blind spot was exactly this, and a wall-time assertion would have been flaky.

## What stays open

Four gaps, none a regression, all filed in follow_up.md: re-derivation does not
descend into list elements or disjunction branches, so a `[...#Svc]` schema still
shows the original bug; `validate_json` never re-derives and its verdict is
inverted, which is the one surface where that certifies bad data; a forward `let`
is dropped rather than relaxed; and `Value` lost `Send` to phase 03's `Rc`s.
